//! A poll-based model of moq-net's send-credit line, after `SendCredit.tla`
//! (moq-dev/moq, `rs/moq-net/verify/credit/`): the fixture `kio::clock::sim`'s
//! adversarial schedules and lost-wakeup probe are measured on, with bugs to inject.
//!
//! Each subscription's track waits in the session's line for stream credit, resets the
//! lowest stream it preempts (`make_room`), and gives way to a group that outranks the one
//! it holds; each opened stream sends, then the peer frees it and returns its credit.
//! Groups arrive over time in a random order. The model drops the spec's live groups, so
//! a quiet session must have sent or given up every group.
//!
//! Every wake is targeted, through a [`WaiterList`] per track and per stream, as the
//! session's line wakes only its head, so a missing wake is a real lost wakeup rather
//! than one a broadcast channel would paper over.

use std::{
	cell::RefCell,
	collections::VecDeque,
	hash::{DefaultHasher, Hash, Hasher},
	pin::pin,
	rc::Rc,
	task::Poll,
	time::Duration,
};

use rand::{RngExt, SeedableRng, rngs::StdRng, seq::SliceRandom};

use kio::{
	Waiter, WaiterList,
	clock::{
		sim::{idle, spawn},
		sleep,
	},
};

/// A bug to inject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bug {
	/// Leaving the line forgets to wake the new head: a lost wakeup.
	NoHeadWake,
	/// The peer freeing a stream forgets to wake the tracks asking for credit: a lost wakeup.
	NoFreeWake,
	/// Resetting a stream to make room forgets to wake its track, which may now hand out
	/// a group it was holding back: a lost wakeup a later wake of that track can mask.
	NoResetWake,
	/// A track the peer's free woke opens without checking it still heads the line: an
	/// ordering bug, seen only when it runs before the track that overtook it, which the
	/// free woke first.
	StealCredit,
}

/// The session to model.
#[derive(Clone, Copy, Debug)]
pub struct Fixture {
	pub subs: usize,
	pub groups: usize,
	/// Bit `s`: subscription `s` sends at the higher priority.
	pub high: u32,
	/// Bit `s`: subscription `s` sends its groups oldest first.
	pub asc: u32,
	pub max_streams: u32,
	/// Drive the tracks as children of one [`kio::Tasks`] set rather than as tasks.
	pub children: bool,
	pub bug: Option<Bug>,
}

impl Fixture {
	/// Three subscriptions, the first at the higher priority, three groups each, over one
	/// stream of credit.
	pub const SMALL: Self = Self {
		subs: 3,
		groups: 3,
		high: 0b001,
		asc: 0b100,
		max_streams: 1,
		children: false,
		bug: None,
	};

	/// [`SMALL`](Self::SMALL) over two streams of credit, so a subscription can hold one
	/// stream while it waits for another.
	pub const WIDE: Self = Self {
		max_streams: 2,
		..Self::SMALL
	};
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum St {
	/// Not yet arrived.
	Pending,
	/// Arrived, waiting for the cursor.
	Cached,
	/// Waiting for credit.
	Opening,
	/// Its stream is open and listed.
	Serving,
	/// Sent whole.
	Fin,
	/// Preempted.
	Reset,
	/// Freed by the peer.
	Closed,
	/// Shed or replaced without opening.
	Given,
}

/// An open's or stream's rank: priority, the oldest turn first, then its rank in its
/// subscription. Greater goes first.
type Rank = (i64, i64, i64);

#[derive(Clone, Copy, Debug, Hash)]
struct Place {
	sub: usize,
	rank: Rank,
	arrival: u64,
}

/// The draws a run's data takes, made up front so the schedule cannot change them.
struct Plan {
	/// Per subscription, its groups in arrival order, each with the delay before it.
	arrivals: Vec<Vec<(usize, Duration)>>,
	/// Per group, how long its stream sends, then how long the peer takes to free it.
	send: Vec<Vec<(Duration, Duration)>>,
}

impl Plan {
	fn new(fixture: &Fixture, seed: u64) -> Self {
		let mut rng = StdRng::seed_from_u64(seed);
		let ms = |rng: &mut StdRng, max: u64| Duration::from_millis(rng.random_range(0..=max));
		let arrivals = (0..fixture.subs)
			.map(|_| {
				let mut order: Vec<usize> = (0..fixture.groups).collect();
				order.shuffle(&mut rng);
				order.into_iter().map(|k| (k, ms(&mut rng, 3))).collect()
			})
			.collect();
		let send = (0..fixture.subs)
			.map(|_| {
				(0..fixture.groups)
					.map(|_| {
						(
							ms(&mut rng, 3) + Duration::from_millis(1),
							ms(&mut rng, 2) + Duration::from_millis(1),
						)
					})
					.collect()
			})
			.collect();
		Self { arrivals, send }
	}
}

struct Session {
	fixture: Fixture,
	st: Vec<Vec<St>>,
	arrived: Vec<VecDeque<usize>>,
	opening: Vec<Option<usize>>,
	asked: Vec<bool>,
	/// Woken by the peer freeing a stream, since the track last polled.
	granted: Vec<bool>,
	/// Published on every change, as a session kept in a kio channel would.
	changes: kio::Producer<()>,
	/// The opens waiting for credit, in line order.
	line: Vec<Place>,
	arrivals: u64,
	resets: usize,
	turn: Vec<u64>,
	turns: u64,
	opened: u32,
	closed: u32,
	tracks: Vec<WaiterList>,
	streams: Vec<Vec<WaiterList>>,
	plan: Plan,
}

impl Session {
	fn new(fixture: Fixture, plan: Plan) -> Self {
		let (subs, groups) = (fixture.subs, fixture.groups);
		Self {
			fixture,
			st: vec![vec![St::Pending; groups]; subs],
			arrived: vec![VecDeque::new(); subs],
			opening: vec![None; subs],
			asked: vec![false; subs],
			granted: vec![false; subs],
			changes: kio::Producer::new(()),
			line: Vec::new(),
			arrivals: 0,
			resets: 0,
			turn: vec![0; subs],
			turns: 0,
			opened: 0,
			closed: 0,
			tracks: (0..subs).map(|_| WaiterList::new()).collect(),
			streams: (0..subs)
				.map(|_| (0..groups).map(|_| WaiterList::new()).collect())
				.collect(),
			plan,
		}
	}

	fn prio(&self, sub: usize) -> i64 {
		i64::from(self.fixture.high & (1 << sub) != 0)
	}

	fn in_group(&self, sub: usize, group: usize) -> i64 {
		let group = group as i64;
		match self.fixture.asc & (1 << sub) != 0 {
			true => -group,
			false => group,
		}
	}

	fn rank(&self, sub: usize, group: usize) -> Rank {
		(self.prio(sub), -(self.turn[sub] as i64), self.in_group(sub, group))
	}

	fn credit(&self) -> bool {
		self.opened < self.fixture.max_streams + self.closed
	}

	fn position(&self, sub: usize) -> Option<usize> {
		self.line.iter().position(|place| place.sub == sub)
	}

	fn is_head(&self, sub: usize) -> bool {
		self.position(sub) == Some(0)
	}

	fn join(&mut self, sub: usize) {
		let group = self.opening[sub].expect("an open joins the line");
		let place = Place {
			sub,
			rank: self.rank(sub, group),
			arrival: self.arrivals,
		};
		self.arrivals += 1;
		// Higher rank first, then the earlier arrival, which every place already in line has.
		let at = self.line.partition_point(|other| other.rank >= place.rank);
		self.line.insert(at, place);
	}

	/// `Line::leave`: wake whoever is now first, if `sub` was, and owe no more resets than opens.
	fn leave(&mut self, sub: usize) {
		let Some(at) = self.position(sub) else { return };
		self.line.remove(at);
		self.resets = self.resets.min(self.line.len());
		if at == 0
			&& let Some(head) = self.line.first()
			&& self.fixture.bug != Some(Bug::NoHeadWake)
		{
			let head = head.sub;
			self.tracks[head].wake();
		}
	}

	/// Whether an open of `sub`'s `group` may reset `other`'s stream of `theirs`.
	fn preempts(&self, sub: usize, group: usize, other: usize, theirs: usize) -> bool {
		self.prio(sub) > self.prio(other) || (sub == other && self.in_group(sub, group) > self.in_group(other, theirs))
	}

	fn listed(&self) -> Vec<(usize, usize)> {
		let mut listed = Vec::new();
		for (sub, groups) in self.st.iter().enumerate() {
			for (group, st) in groups.iter().enumerate() {
				if *st == St::Serving {
					listed.push((sub, group));
				}
			}
		}
		listed
	}

	fn lowest(&self, streams: impl Iterator<Item = (usize, usize)>) -> Option<(usize, usize)> {
		streams.min_by_key(|&(sub, group)| self.rank(sub, group))
	}

	/// `Lines::make_room`: each open in line not owed a reset resets the lowest stream it preempts.
	fn make_room(&mut self) {
		// Reading every listed stream touches every subscription.
		let mut listed = self.listed();
		let owed: Vec<usize> = self.line.iter().skip(self.resets).map(|place| place.sub).collect();
		for sub in owed {
			let group = self.opening[sub].expect("an open in line");
			let candidate = self.lowest(listed.iter().copied()).and_then(|(low, theirs)| {
				if self.preempts(sub, group, low, theirs) {
					return Some((low, theirs));
				}
				let own = self.lowest(listed.iter().copied().filter(|&(other, _)| other == sub))?;
				self.preempts(sub, group, own.0, own.1).then_some(own)
			});
			let Some((other, theirs)) = candidate else { continue };
			listed.retain(|&stream| stream != (other, theirs));
			self.st[other][theirs] = St::Reset;
			self.resets += 1;
			self.streams[other][theirs].wake();
			if self.fixture.bug != Some(Bug::NoResetWake) {
				self.tracks[other].wake();
			}
		}
	}

	/// The open takes credit: its group's stream is listed, and its subscription takes a turn.
	fn open(&mut self, sub: usize) {
		let group = self.opening[sub].take().expect("an open takes credit");
		assert!(self.credit(), "WithinCredit: subscription {sub} opened without credit");
		if let Some(head) = self.line.first() {
			assert!(
				head.sub == sub,
				"line order: subscription {sub} opened while subscription {} headed the line",
				head.sub
			);
		}
		self.st[sub][group] = St::Serving;
		self.opened += 1;
		self.turns += 1;
		self.turn[sub] = self.turns;
		let (send, free) = self.plan.send[sub][group];
		drop(spawn(stream(self.handle(), sub, group, send, free)));
	}

	/// `Open::poll` for `sub`'s waiting group.
	fn poll_open(&mut self, sub: usize) {
		if self.opening[sub].is_none() {
			return;
		}
		let granted = std::mem::take(&mut self.granted[sub]);
		// The bug is the out-of-line open: a head takes the normal path, resets included.
		if self.fixture.bug == Some(Bug::StealCredit) && granted && self.credit() && !self.is_head(sub) {
			self.open(sub);
			return self.leave(sub);
		}
		if self.position(sub).is_none() && self.line.is_empty() {
			// Nobody waits: take credit if there is any, and join the line only without it.
			if self.credit() {
				return self.open(sub);
			}
			self.join(sub);
			self.asked[sub] = true;
			return self.make_room();
		}
		if self.position(sub).is_none() {
			self.join(sub);
		}
		if !self.is_head(sub) {
			return self.make_room();
		}
		if self.credit() {
			self.resets = self.resets.saturating_sub(1);
			self.open(sub);
			return self.leave(sub);
		}
		self.asked[sub] = true;
		self.make_room();
	}

	/// `TrackRun::stuck`: the waiting group ranks below every stream the subscription serves.
	fn stuck(&self, sub: usize) -> bool {
		let Some(waiting) = self.opening[sub] else { return false };
		let mut held = (0..self.fixture.groups).filter(|&group| self.st[sub][group] == St::Serving);
		let first = held.next();
		first.is_some()
			&& first
				.into_iter()
				.chain(held)
				.all(|group| self.in_group(sub, group) > self.in_group(sub, waiting))
	}

	/// Whether the cursor may hand out `group`: any while none waits; while one waits and
	/// is stuck, any when newest first, and only an older one when oldest first.
	fn admitted(&self, sub: usize, group: usize) -> bool {
		match self.opening[sub] {
			None => true,
			Some(waiting) => self.stuck(sub) && (self.fixture.asc & (1 << sub) == 0 || group < waiting),
		}
	}

	/// The cursor hands out `group`: shed if it ranks below the waiting group, or replacing
	/// it (`give_way`) to wait itself.
	fn cursor(&mut self, sub: usize, group: usize) {
		self.arrived[sub].retain(|&arrived| arrived != group);
		if let Some(waiting) = self.opening[sub] {
			if self.in_group(sub, group) < self.in_group(sub, waiting) {
				self.st[sub][group] = St::Given;
				return;
			}
			self.st[sub][waiting] = St::Given;
			self.opening[sub] = None;
			self.leave(sub);
		}
		self.st[sub][group] = St::Opening;
		self.opening[sub] = Some(group);
	}

	/// One pass of `TrackRun::poll_step`, returning whether it changed anything.
	fn step(&mut self, sub: usize) -> bool {
		let before = self.digest();
		self.poll_open(sub);
		let ready = self.arrived[sub]
			.iter()
			.copied()
			.find(|&group| self.admitted(sub, group));
		if let Some(group) = ready {
			self.cursor(sub, group);
		}
		self.digest() != before
	}

	/// Whether `sub` has nothing left to hand out or open.
	fn finished(&self, sub: usize) -> bool {
		self.st[sub]
			.iter()
			.all(|st| !matches!(st, St::Pending | St::Cached | St::Opening))
	}

	/// The model's state, to tell whether a pass changed it.
	fn digest(&self) -> u64 {
		let mut hasher = DefaultHasher::new();
		(&self.st, &self.arrived, &self.opening, &self.asked).hash(&mut hasher);
		(&self.line, self.resets, &self.turn, self.opened, self.closed).hash(&mut hasher);
		hasher.finish()
	}

	/// Publish a change, so the probe sees it.
	fn publish(&mut self) {
		if let Ok(mut changes) = self.changes.write() {
			*changes = ();
		}
	}

	/// Once quiet: every group was sent and freed or given up, no credit leaked, and each
	/// subscription's best group reached the peer or was reset for a higher priority, as
	/// the spec's `TopDelivered` allows: both end `Closed`, and nothing in its own
	/// subscription outranks it, so only a higher priority can reset it.
	fn check_quiet(&self) {
		for sub in 0..self.fixture.subs {
			for (group, &st) in self.st[sub].iter().enumerate() {
				assert!(
					matches!(st, St::Closed | St::Given),
					"Ends: quiet with subscription {sub}'s group {group} {st:?}"
				);
			}
			let top = (0..self.fixture.groups)
				.max_by_key(|&group| self.in_group(sub, group))
				.expect("a group");
			assert_eq!(
				self.st[sub][top],
				St::Closed,
				"TopDelivered: subscription {sub}'s best group was given up"
			);
		}
		assert!(self.line.is_empty(), "Ends: quiet with {:?} in line", self.line);
		assert_eq!(self.opened, self.closed, "Ends: credit outstanding once quiet");
	}

	fn handle(&self) -> Handle {
		CURRENT.with(|current| current.borrow().clone().expect("a session"))
	}
}

type Handle = Rc<RefCell<Session>>;

thread_local! {
	/// The running session, for a step that spawns a stream.
	static CURRENT: RefCell<Option<Handle>> = const { RefCell::new(None) };
}

/// `sub`'s track: steps until a pass changes nothing, then parks on its wake list.
fn track(session: Handle, sub: usize) -> impl FnMut(&Waiter) -> Poll<()> {
	move |waiter| {
		// Wakes go out once the session is released.
		let _hold = kio::hold();
		let mut session = session.borrow_mut();
		while session.step(sub) {
			session.publish();
		}
		if session.finished(sub) {
			return Poll::Ready(());
		}
		session.tracks[sub].register(waiter);
		Poll::Pending
	}
}

/// `sub`'s groups arrive at its cursor in the planned order.
async fn source(session: Handle, sub: usize) {
	let arrivals = session.borrow().plan.arrivals[sub].clone();
	for (group, delay) in arrivals {
		sleep(delay).await;
		let _hold = kio::hold();
		let mut session = session.borrow_mut();
		session.st[sub][group] = St::Cached;
		session.arrived[sub].push_back(group);
		session.tracks[sub].wake();
		session.publish();
	}
}

/// An open stream: sends for `send` unless reset first, then the peer frees it after `free`.
async fn stream(session: Handle, sub: usize, group: usize, send: Duration, free: Duration) {
	let fixture = session.borrow().fixture;
	let mut sending = pin!(sleep(send));
	let sent = kio::wait(|waiter: &Waiter| {
		let mut session = session.borrow_mut();
		if session.st[sub][group] == St::Reset {
			return Poll::Ready(false);
		}
		if waiter.poll_future(sending.as_mut()).is_ready() {
			return Poll::Ready(true);
		}
		session.streams[sub][group].register(waiter);
		Poll::Pending
	})
	.await;
	if sent {
		let _hold = kio::hold();
		let mut session = session.borrow_mut();
		session.st[sub][group] = St::Fin;
		session.tracks[sub].wake();
		session.publish();
	}

	sleep(free).await;
	let _hold = kio::hold();
	let mut session = session.borrow_mut();
	session.st[sub][group] = St::Closed;
	session.closed += 1;
	for other in 0..fixture.subs {
		if std::mem::take(&mut session.asked[other]) && fixture.bug != Some(Bug::NoFreeWake) {
			session.granted[other] = true;
			session.tracks[other].wake();
		}
	}
	session.publish();
}

/// Run the session to quiet, checking its rules.
async fn session(fixture: Fixture, plan: Plan) {
	let session = Rc::new(RefCell::new(Session::new(fixture, plan)));
	CURRENT.with(|current| *current.borrow_mut() = Some(session.clone()));
	struct Clear;
	impl Drop for Clear {
		fn drop(&mut self) {
			CURRENT.with(|current| current.borrow_mut().take());
		}
	}
	let _clear = Clear;

	for sub in 0..fixture.subs {
		drop(spawn(source(session.clone(), sub)));
	}
	if fixture.children {
		let mut tasks = kio::Tasks::new();
		for sub in 0..fixture.subs {
			tasks.push(track(session.clone(), sub));
		}
		drop(spawn(kio::wait(move |waiter: &Waiter| tasks.poll(waiter))));
	} else {
		for sub in 0..fixture.subs {
			drop(spawn(kio::wait(track(session.clone(), sub))));
		}
	}
	idle().await;
	session.borrow().check_quiet();
}

/// One run under the schedule and probe that `KIO_SIM_SCHEDULE` and `KIO_SIM_PROBE` name:
/// `data` seeds the session's arrivals and timings, and `seed` the run, its schedule
/// included. Returns the panic message, if it failed.
pub fn run(fixture: Fixture, data: u64, seed: u64) -> Option<String> {
	let plan = Plan::new(&fixture, data);
	let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
		kio::clock::sim::run_seeded(seed, session(fixture, plan));
	}));
	result.err().map(
		|panic| match (panic.downcast_ref::<&str>(), panic.downcast_ref::<String>()) {
			(Some(message), _) => message.to_string(),
			(_, Some(message)) => message.clone(),
			_ => "a panic".to_string(),
		},
	)
}
