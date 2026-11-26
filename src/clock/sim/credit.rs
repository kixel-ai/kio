//! A poll-based model of moq-net's send-credit line, after `SendCredit.tla`
//! (moq-dev/moq, `rs/moq-net/verify/credit/`): the fixture the adversarial schedules
//! and the lost-wakeup probe are measured on, with bugs to inject.
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
	collections::{HashSet, VecDeque},
	hash::{DefaultHasher, Hash, Hasher},
	pin::pin,
	rc::Rc,
	task::Poll,
	time::Duration,
};

use rand::{RngExt, SeedableRng, rngs::StdRng, seq::SliceRandom};

use super::{
	idle,
	schedule::{Choice, Config, Policy},
	spawn,
};
use crate::{Waiter, WaiterList, clock::sleep};

/// A bug to inject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Bug {
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
pub(super) struct Fixture {
	pub subs: usize,
	pub groups: usize,
	/// Bit `s`: subscription `s` sends at the higher priority.
	pub high: u32,
	/// Bit `s`: subscription `s` sends its groups oldest first.
	pub asc: u32,
	pub max_streams: u32,
	/// Drive the tracks as children of one [`Tasks`](crate::Tasks) set rather than as tasks.
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

/// What a poll touched, a bit per object, to tell which polls commute.
const CREDIT: u64 = 1;
const LINE: u64 = 2;
fn sub_bit(sub: usize) -> u64 {
	4 << sub
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

/// What a run saw, kept across runs.
#[derive(Default)]
pub(super) struct Observed {
	/// Every distinct session state reached.
	pub states: HashSet<u64>,
	/// Each poll that changed the session, the scheduler's unit of interleaving: the
	/// unit that ran it and what it touched.
	pub steps: Vec<(usize, u64)>,
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
	changes: crate::Producer<()>,
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
	observed: Rc<RefCell<Observed>>,
	/// What the poll running now has touched.
	touched: u64,
}

impl Session {
	fn new(fixture: Fixture, plan: Plan, observed: Rc<RefCell<Observed>>) -> Self {
		let (subs, groups) = (fixture.subs, fixture.groups);
		Self {
			fixture,
			st: vec![vec![St::Pending; groups]; subs],
			arrived: vec![VecDeque::new(); subs],
			opening: vec![None; subs],
			asked: vec![false; subs],
			granted: vec![false; subs],
			changes: crate::Producer::new(()),
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
			observed,
			touched: 0,
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
			self.touched |= sub_bit(head);
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
		self.touched |= (0..self.fixture.subs).map(sub_bit).fold(LINE, |all, bit| all | bit);
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
		self.touched |= CREDIT | sub_bit(sub);
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
		self.touched |= CREDIT | LINE;
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
			self.touched |= LINE;
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

	/// The model's state, for counting the distinct states a run reaches.
	fn digest(&self) -> u64 {
		let mut hasher = DefaultHasher::new();
		(&self.st, &self.arrived, &self.opening, &self.asked).hash(&mut hasher);
		(&self.line, self.resets, &self.turn, self.opened, self.closed).hash(&mut hasher);
		hasher.finish()
	}

	/// Record the state a change reached, and publish it.
	fn observe_state(&mut self) {
		if let Ok(mut changes) = self.changes.write() {
			*changes = ();
		}
		self.observed.borrow_mut().states.insert(self.digest());
	}

	/// End the poll `unit` ran, logging what it touched when it `changed` anything, so
	/// the next poll's footprint starts empty either way.
	fn end_poll(&mut self, unit: usize, changed: bool) {
		let touched = std::mem::take(&mut self.touched);
		if changed {
			self.observed.borrow_mut().steps.push((unit, touched));
		}
	}

	/// Record the change `unit`'s poll made, and publish it.
	fn observe(&mut self, unit: usize) {
		self.observe_state();
		self.end_poll(unit, true);
	}

	/// Once quiet: every group was sent and freed or given up, no credit leaked, and each
	/// subscription's best group reached the peer or was reset for a higher priority, as
	/// the spec's `TopDelivered` allows: both end `Closed`, and nothing in its own
	/// subscription outranks it, so only a higher priority can reset it.
	fn check_quiet(&self) {
		for sub in 0..self.fixture.subs {
			for group in 0..self.fixture.groups {
				let st = self.st[sub][group];
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

/// Unit ids for the step log: tracks, then sources, then streams.
fn track_unit(sub: usize) -> usize {
	sub
}
fn source_unit(fixture: &Fixture, sub: usize) -> usize {
	fixture.subs + sub
}
fn stream_unit(fixture: &Fixture, sub: usize, group: usize) -> usize {
	2 * fixture.subs + sub * fixture.groups + group
}

/// `sub`'s track: steps until a pass changes nothing, then parks on its wake list.
fn track(session: Handle, sub: usize) -> impl FnMut(&Waiter) -> Poll<()> {
	move |waiter| {
		// Wakes go out once the session is released.
		let _hold = crate::hold();
		let mut session = session.borrow_mut();
		let mut changed = false;
		while session.step(sub) {
			session.observe_state();
			changed = true;
		}
		// One footprint per poll, the last pass's reads included: the scheduler cannot
		// interleave between passes.
		session.end_poll(track_unit(sub), changed);
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
		let _hold = crate::hold();
		let mut session = session.borrow_mut();
		session.st[sub][group] = St::Cached;
		session.arrived[sub].push_back(group);
		session.touched |= sub_bit(sub);
		session.tracks[sub].wake();
		let fixture = session.fixture;
		session.observe(source_unit(&fixture, sub));
	}
}

/// An open stream: sends for `send` unless reset first, then the peer frees it after `free`.
async fn stream(session: Handle, sub: usize, group: usize, send: Duration, free: Duration) {
	let fixture = session.borrow().fixture;
	let unit = stream_unit(&fixture, sub, group);
	let mut sending = pin!(sleep(send));
	let sent = crate::wait(|waiter: &Waiter| {
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
		let _hold = crate::hold();
		let mut session = session.borrow_mut();
		session.st[sub][group] = St::Fin;
		session.touched |= sub_bit(sub);
		session.tracks[sub].wake();
		session.observe(unit);
	}

	sleep(free).await;
	let _hold = crate::hold();
	let mut session = session.borrow_mut();
	session.st[sub][group] = St::Closed;
	session.closed += 1;
	session.touched |= CREDIT | LINE;
	for other in 0..fixture.subs {
		if std::mem::take(&mut session.asked[other]) && fixture.bug != Some(Bug::NoFreeWake) {
			session.granted[other] = true;
			session.touched |= sub_bit(other);
			session.tracks[other].wake();
		}
	}
	session.observe(unit);
}

/// Run the session to quiet, checking its rules.
async fn session(fixture: Fixture, plan: Plan, observed: Rc<RefCell<Observed>>) {
	let session = Rc::new(RefCell::new(Session::new(fixture, plan, observed)));
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
		let mut tasks = crate::Tasks::new();
		for sub in 0..fixture.subs {
			tasks.push(track(session.clone(), sub));
		}
		drop(spawn(crate::wait(move |waiter: &Waiter| tasks.poll(waiter))));
	} else {
		for sub in 0..fixture.subs {
			drop(spawn(crate::wait(track(session.clone(), sub))));
		}
	}
	idle().await;
	session.borrow().check_quiet();
}

/// One run: `data` seeds the session's arrivals and timings, `config` its schedule, and
/// `seed` the schedule's draws. Returns the panic message, if it failed.
pub(super) fn run(
	fixture: Fixture,
	data: u64,
	seed: u64,
	config: Config,
	observed: &Rc<RefCell<Observed>>,
) -> Option<String> {
	let plan = Plan::new(&fixture, data);
	let observed = observed.clone();
	let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
		super::run_with(seed, config, session(fixture, plan, observed));
	}));
	result.err().map(
		|panic| match (panic.downcast_ref::<&str>(), panic.downcast_ref::<String>()) {
			(Some(message), _) => message.to_string(),
			(_, Some(message)) => message.clone(),
			_ => "a panic".to_string(),
		},
	)
}

/// Explore the schedules of one data seed depth-first by replaying choice prefixes,
/// branching only at the first `bound` choices. Returns the runs it took and the failures
/// it found, stopping at `limit` runs or the first failure when `stop`.
pub(super) fn explore(
	fixture: Fixture,
	data: u64,
	probe: bool,
	bound: usize,
	limit: usize,
	stop: bool,
	observed: &Rc<RefCell<Observed>>,
) -> (usize, Vec<String>) {
	let mut prefix: Vec<u32> = Vec::new();
	let mut failures = Vec::new();
	for runs in 1..=limit {
		let trace = Rc::new(RefCell::new(Vec::<Choice>::new()));
		let config = Config {
			policy: Policy::Replay {
				prefix: prefix.clone(),
				trace: trace.clone(),
			},
			probe,
		};
		if let Some(failure) = run(fixture, data, 0, config, observed) {
			failures.push(failure);
			if stop {
				return (runs, failures);
			}
		}
		let trace = trace.borrow();
		let Some(at) = (0..trace.len().min(bound))
			.rev()
			.find(|&at| trace[at].taken + 1 < trace[at].options)
		else {
			return (runs, failures);
		};
		prefix = trace[..at].iter().map(|choice| choice.taken).collect();
		prefix.push(trace[at].taken + 1);
	}
	(limit, failures)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn config(policy: Policy, probe: bool) -> Config {
		Config { policy, probe }
	}

	const PCT: Policy = Policy::Pct { depth: 3, steps: 200 };

	fn caught(fixture: Fixture, config: &Config, seeds: u64) -> Vec<(u64, String)> {
		let observed = Rc::new(RefCell::new(Observed::default()));
		(0..seeds)
			.filter_map(|seed| run(fixture, seed, seed, config.clone(), &observed).map(|failure| (seed, failure)))
			.collect()
	}

	/// The model holds every rule under every schedule, and the probe finds no lost wakeup.
	#[test]
	fn a_correct_session_passes_every_schedule() {
		for children in [false, true] {
			let fixture = Fixture {
				children,
				..Fixture::SMALL
			};
			for policy in [Policy::Fifo, Policy::Random, PCT] {
				let config = config(policy, true);
				assert_eq!(caught(fixture, &config, 40), [], "{config:?}, children {children}");
			}
		}
	}

	/// The probe names a lost wakeup where it happens, under the fixed order too.
	#[test]
	fn the_probe_catches_lost_wakeups() {
		for bug in [Bug::NoHeadWake, Bug::NoFreeWake] {
			for children in [false, true] {
				let fixture = Fixture {
					bug: Some(bug),
					children,
					..Fixture::SMALL
				};
				let failures = caught(fixture, &config(Policy::Fifo, true), 40);
				assert!(!failures.is_empty(), "{bug:?} slipped by, children {children}");
				for (seed, failure) in failures {
					assert!(failure.starts_with("lost wakeup"), "{bug:?} seed {seed}: {failure}");
					assert!(
						failure.contains("credit.rs"),
						"{bug:?} seed {seed} blames no track: {failure}"
					);
				}
			}
		}
	}

	/// The fixed order wakes the track that overtook first and so rarely lets the other
	/// steal the credit; adversarial schedules catch it on more seeds. Only for tasks: a
	/// task set's own rotating slot order already varies which child runs first.
	#[test]
	fn adversarial_schedules_catch_an_ordering_bug_more_often() {
		let fixture = Fixture {
			bug: Some(Bug::StealCredit),
			..Fixture::SMALL
		};
		let fifo = caught(fixture, &config(Policy::Fifo, false), 40).len();
		for policy in [Policy::Random, PCT] {
			let failures = caught(fixture, &config(policy.clone(), false), 40);
			assert!(
				failures.len() > fifo,
				"{policy:?} caught {} to FIFO's {fifo}",
				failures.len()
			);
			for (seed, failure) in failures {
				assert!(failure.starts_with("line order"), "{policy:?} seed {seed}: {failure}");
			}
		}
	}

	/// The same seed replays the same schedule, so a failure's seed reproduces it.
	#[test]
	fn a_schedule_replays_from_its_seed() {
		let fixture = Fixture {
			bug: Some(Bug::StealCredit),
			..Fixture::SMALL
		};
		for policy in [Policy::Random, PCT] {
			let config = config(policy, false);
			let runs = |seed| {
				let observed = Rc::new(RefCell::new(Observed::default()));
				let failure = run(fixture, seed, seed, config.clone(), &observed);
				let steps = std::mem::take(&mut observed.borrow_mut().steps);
				(failure, steps)
			};
			for seed in 0..10 {
				assert_eq!(runs(seed), runs(seed), "{config:?} seed {seed}");
			}
		}
	}

	/// The explorer enumerates schedules from replayed prefixes until none is left.
	#[test]
	fn the_explorer_exhausts_a_bounded_tree() {
		let observed = Rc::new(RefCell::new(Observed::default()));
		let (runs, failures) = explore(Fixture::SMALL, 0, true, 4, 10_000, false, &observed);
		assert!(runs > 1 && runs < 10_000, "explored {runs} schedules");
		assert_eq!(failures, Vec::<String>::new());
	}

	/// What caught a failure: its message up to the first colon.
	fn kind(failure: &str) -> &str {
		failure.split(':').next().unwrap_or(failure)
	}

	/// The spike's numbers, as Markdown tables: each bug's catch rate, first catching seed,
	/// and cost per seed under each schedule, with and without the probe; then the bounded
	/// explorer against random and PCT schedules on fixed data, and how many adjacent polls
	/// commute, the reduction partial-order methods could find.
	///
	/// `cargo test --release -p kio --lib credit::tests::report -- --ignored --nocapture`
	#[test]
	#[ignore = "a measurement, not a check"]
	#[expect(clippy::disallowed_methods, reason = "measures the host time each seed costs")]
	fn report() {
		use std::{collections::BTreeMap, time::Instant};

		/// Runs to the first failure on a data seed, if any.
		type Method = Box<dyn Fn(u64) -> Option<usize>>;

		std::panic::set_hook(Box::new(|_| {}));
		const SEEDS: u64 = 1000;
		let policies = [("fifo", Policy::Fifo), ("random", Policy::Random), ("pct:3:200", PCT)];
		let bugs = [
			None,
			Some(Bug::NoHeadWake),
			Some(Bug::NoFreeWake),
			Some(Bug::NoResetWake),
			Some(Bug::StealCredit),
		];

		println!(
			"| credit | bug | tracks as | schedule | probe | caught / {SEEDS} | first seed | µs per seed | caught by |"
		);
		println!("|---|---|---|---|---|---|---|---|---|");
		for (base, bug) in [Fixture::SMALL, Fixture::WIDE]
			.into_iter()
			.flat_map(|base| bugs.map(|bug| (base, bug)))
		{
			for children in [false, true] {
				for (name, policy) in &policies {
					for probe in [false, true] {
						let fixture = Fixture { bug, children, ..base };
						let config = config(policy.clone(), probe);
						let observed = Rc::new(RefCell::new(Observed::default()));
						let mut kinds = BTreeMap::<String, usize>::new();
						let mut first = None;
						let start = Instant::now();
						for seed in 0..SEEDS {
							if let Some(failure) = run(fixture, seed, seed, config.clone(), &observed) {
								first.get_or_insert(seed);
								*kinds.entry(kind(&failure).to_string()).or_default() += 1;
							}
							observed.borrow_mut().steps.clear();
						}
						let micros = start.elapsed().as_micros() / u128::from(SEEDS);
						let caught: usize = kinds.values().sum();
						let first = first.map_or("-".to_string(), |seed| seed.to_string());
						let kinds: Vec<String> = kinds.iter().map(|(kind, n)| format!("{kind} {n}")).collect();
						let drive = if children { "children" } else { "tasks" };
						println!(
							"| {} | {bug:?} | {drive} | {name} | {probe} | {caught} | {first} | {micros} | {} |",
							base.max_streams,
							kinds.join(", ")
						);
					}
				}
			}
		}

		// Runs to the first failure on each fixed data seed: the explorer's depth-first
		// replay against random and PCT draws of the schedule alone.
		const DATA: u64 = 20;
		const LIMIT: usize = 2000;
		println!();
		println!("| bug | probe | method | data seeds caught / {DATA} | median runs to catch | ms total |");
		println!("|---|---|---|---|---|---|");
		// The ordering bug: the lost wakeups fall to the first run of every method. Any
		// other failure means the injection broke more than the line order.
		let line_order = |failure: &String| {
			assert!(
				failure.starts_with("line order"),
				"the ordering bug failed otherwise: {failure}"
			);
		};
		{
			let (bug, probe) = (Bug::StealCredit, false);
			let fixture = Fixture {
				bug: Some(bug),
				..Fixture::SMALL
			};
			let observed = Rc::new(RefCell::new(Observed::default()));
			let mut methods: Vec<(String, Method)> = Vec::new();
			for bound in [8, 64] {
				let observed = observed.clone();
				methods.push((
					format!("explore, branch at the first {bound} choices"),
					Box::new(move |data| {
						let (runs, failures) = explore(fixture, data, probe, bound, LIMIT, true, &observed);
						failures.iter().for_each(line_order);
						(!failures.is_empty()).then_some(runs)
					}),
				));
			}
			for (name, policy) in policies.iter().skip(1) {
				let observed = observed.clone();
				let policy = policy.clone();
				methods.push((
					name.to_string(),
					Box::new(move |data| {
						(0..LIMIT)
							.find(|&seed| {
								let failure = run(fixture, data, seed as u64, config(policy.clone(), probe), &observed);
								failure.as_ref().inspect(|failure| line_order(failure)).is_some()
							})
							.map(|seed| seed + 1)
					}),
				));
			}
			for (name, method) in methods {
				let start = Instant::now();
				let mut found: Vec<usize> = (0..DATA).filter_map(&method).collect();
				observed.borrow_mut().steps.clear();
				found.sort_unstable();
				let median = found.get(found.len() / 2).map_or("-".to_string(), usize::to_string);
				println!(
					"| {bug:?} | {probe} | {name} | {} | {median} | {} |",
					found.len(),
					start.elapsed().as_millis()
				);
			}
		}

		// Distinct session states each method reaches on one data seed in a fixed budget.
		println!();
		// A clean run that fails would count states it reached on the way.
		let clean = |method: &str, seed: u64, failure: Option<String>| {
			assert_eq!(failure, None, "a clean session failed under {method}, seed {seed}");
		};
		println!(
			"| method (clean session, data seed 0; random and PCT for 1 s, explore until done or 20000 runs) | runs | distinct states observed | states per s |"
		);
		println!("|---|---|---|---|");
		let budget = std::time::Duration::from_secs(1);
		for bound in [8, 12, 16] {
			// The explorer exhausts its bound or runs out of budget, whichever comes first.
			let observed = Rc::new(RefCell::new(Observed::default()));
			let start = Instant::now();
			let limit = 20_000;
			let (runs, failures) = explore(Fixture::SMALL, 0, false, bound, limit, false, &observed);
			clean("explore", 0, failures.into_iter().next());
			let states = observed.borrow().states.len();
			let secs = start.elapsed().as_secs_f64();
			let exhausted = if runs < limit { "exhausted" } else { "cut off" };
			println!(
				"| explore, branch at the first {bound} choices ({exhausted} in {secs:.2} s) | {runs} | {states} | {:.0} |",
				states as f64 / secs
			);
		}
		for (name, policy) in policies.iter().skip(1) {
			let observed = Rc::new(RefCell::new(Observed::default()));
			let start = Instant::now();
			let mut runs = 0;
			while start.elapsed() < budget {
				clean(
					name,
					runs,
					run(Fixture::SMALL, 0, runs, config(policy.clone(), false), &observed),
				);
				observed.borrow_mut().steps.clear();
				runs += 1;
			}
			let states = observed.borrow().states.len();
			println!(
				"| {name} | {runs} | {states} | {:.0} |",
				states as f64 / start.elapsed().as_secs_f64()
			);
		}

		// How often two adjacent polls by different units touch disjoint state, so their
		// order cannot matter.
		let observed = Rc::new(RefCell::new(Observed::default()));
		let (mut pairs, mut independent) = (0usize, 0usize);
		for seed in 0..200 {
			clean(
				"random",
				seed,
				run(Fixture::SMALL, seed, seed, config(Policy::Random, false), &observed),
			);
			let steps = std::mem::take(&mut observed.borrow_mut().steps);
			for pair in steps.windows(2) {
				let ((a, a_touched), (b, b_touched)) = (pair[0], pair[1]);
				if a != b {
					pairs += 1;
					independent += usize::from(a_touched & b_touched == 0);
				}
			}
		}
		println!();
		println!(
			"Adjacent polls by different units that commute: {independent} of {pairs} ({:.1}%)",
			100.0 * independent as f64 / pairs as f64
		);
		let _ = std::panic::take_hook();
	}
}
