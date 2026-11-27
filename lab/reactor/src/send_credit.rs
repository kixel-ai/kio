//! `SendCredit.tla` as `#[reactor]`s over real `kio` waiters: each subscription's
//! [`TrackRun`] hands its groups out one at a time and waits in its session's line for the
//! peer's stream credit, and the [`Peer`] sends streams whole and frees them.
//!
//! The session mirrors `moq-net`'s `transport/open.rs`: the opens waiting for credit queue
//! in a line ranked as the transport sends their streams, only the head asks the transport,
//! leaving the line wakes the new head ([`Line::wake_head`]), and a waiting open resets the
//! lowest-ranked stream it preempts. `TrackRun` follows `lite/publisher.rs`'s cursor: a
//! group handed out while another waits replaces it if it outranks it (`give_way`) and is
//! shed otherwise, and while the waiting group ranks below every stream its subscription
//! serves (`stuck`), only groups that outrank it are handed out.
//!
//! The spec's names are kept: subscription `s` and group `k` count from 1.

use std::{cmp::Reverse, collections::BTreeMap, fmt::Write as _, task::Poll};

use kio::{Lock, Waiter, WaiterList};
use kio_macros::reactor;
use rand::{Rng as _, SeedableRng, rngs::StdRng};

/// A mistake to inject, each one the explorer should catch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bug {
	/// Leaving the line forgets to wake the new head.
	SkipWakeHead,
	/// `give_way` assumes the replaced open is in line, which holds only when `open` runs
	/// before `hand_out`, as the priority order does.
	GiveWayAssumesPlaced,
}

/// One subscription.
#[derive(Clone, Debug)]
pub struct Sub {
	/// At the higher send group priority.
	pub high: bool,
	/// Its groups go oldest first.
	pub asc: bool,
	/// Its group whose producer never finishes.
	pub live: Option<u64>,
}

/// A `SendCredit.tla` model's constants, plus a bug to inject.
#[derive(Clone, Debug)]
pub struct Config {
	/// `Subs`, `High`, `Asc`, `LiveSubs`, and `LiveGroup`.
	pub subs: Vec<Sub>,
	/// `G`: groups per subscription.
	pub groups: u64,
	/// `MaxStreams`: the peer's initial stream credit.
	pub max_streams: u64,
	/// The bug to inject.
	pub bug: Option<Bug>,
}

impl Config {
	/// `SendCredit.cfg`.
	pub fn base() -> Self {
		Self {
			subs: vec![
				Sub {
					high: false,
					asc: false,
					live: None,
				},
				Sub {
					high: false,
					asc: true,
					live: None,
				},
			],
			groups: 3,
			max_streams: 1,
			bug: None,
		}
	}

	/// `SendCreditLive.cfg`.
	pub fn live() -> Self {
		let mut config = Self::base();
		for sub in &mut config.subs {
			sub.live = Some(2);
		}
		config
	}

	/// `SendCreditPriority.cfg`.
	pub fn priority() -> Self {
		let mut config = Self::base();
		config.subs[0].high = true;
		config
	}

	/// `SendCreditStreams.cfg`.
	pub fn streams() -> Self {
		let mut config = Self::base();
		config.subs[0].asc = true;
		config.subs[1].asc = false;
		config.max_streams = 2;
		config
	}

	/// Every config, by its `.cfg` file's name.
	pub fn all() -> [(&'static str, Self); 4] {
		[
			("SendCredit", Self::base()),
			("SendCreditLive", Self::live()),
			("SendCreditPriority", Self::priority()),
			("SendCreditStreams", Self::streams()),
		]
	}

	/// The same config with `bug` injected.
	pub fn with(mut self, bug: Bug) -> Self {
		self.bug = Some(bug);
		self
	}

	fn prio(&self, s: usize) -> i64 {
		i64::from(self.subs[s - 1].high)
	}

	/// `InGroup`: group `k`'s rank within subscription `s`.
	fn in_group(&self, s: usize, k: u64) -> i64 {
		let k = i64::try_from(k).expect("a small group number");
		if self.subs[s - 1].asc { -k } else { k }
	}
}

/// A group's state, as the spec's `st` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum St {
	Cached,
	Opening,
	Serving,
	Fin,
	Reset,
	Closed,
	Given,
}

impl St {
	fn tla(self) -> &'static str {
		match self {
			Self::Cached => "cached",
			Self::Opening => "opening",
			Self::Serving => "serving",
			Self::Fin => "fin",
			Self::Reset => "reset",
			Self::Closed => "closed",
			Self::Given => "given",
		}
	}
}

/// `open::Rank` as the spec orders it: priority, then the oldest turn, then the rank in the
/// subscription, greater first.
type Rank = (i64, i64, i64);

/// A place in line: the highest rank first, then the earliest arrival.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
	rank: Reverse<Rank>,
	arrival: u64,
}

/// One open waiting in line.
struct Place {
	s: usize,
	k: u64,
	waiters: WaiterList,
}

/// The opens waiting for credit, as `open.rs`'s `Line`.
#[derive(Default)]
struct Line {
	waiting: BTreeMap<Key, Place>,
	arrivals: u64,
	/// Streams reset to make room whose credit has yet to reach an open: the first this many
	/// in line are owed one.
	resets: usize,
}

impl Line {
	fn head(&self) -> Option<Key> {
		self.waiting.keys().next().copied()
	}

	fn join(&mut self, rank: Rank, s: usize, k: u64) -> Key {
		let key = Key {
			rank: Reverse(rank),
			arrival: self.arrivals,
		};
		self.arrivals += 1;
		self.waiting.insert(
			key,
			Place {
				s,
				k,
				waiters: WaiterList::new(),
			},
		);
		key
	}

	/// Leave the line, waking whoever is now first so it asks the transport for credit.
	fn leave(&mut self, key: Key, bug: Option<Bug>) {
		let was_head = self.head() == Some(key);
		self.waiting.remove(&key);
		self.resets = self.resets.min(self.waiting.len());
		if was_head && bug != Some(Bug::SkipWakeHead) {
			self.wake_head();
		}
	}

	fn wake_head(&mut self) {
		if let Some(place) = self.waiting.values_mut().next() {
			place.waiters.wake();
		}
	}
}

/// What every reactor of one session shares.
struct Session {
	config: Config,
	/// `st`, by subscription, then group.
	st: Vec<Vec<St>>,
	line: Line,
	opened: u64,
	closed: u64,
	/// The opens waiting on the transport's credit, by subscription.
	asked: Vec<bool>,
	/// Where those opens wait.
	credit: WaiterList,
	turns: u64,
	turn: Vec<u64>,
	/// By subscription: woken when one of its streams finishes or is reset.
	streams: Vec<WaiterList>,
	/// The peer's: woken when a stream starts serving.
	serving: WaiterList,
	/// The peer's: woken when a stream finishes or is reset.
	ended: WaiterList,
}

impl Session {
	fn new(config: Config) -> Self {
		let subs = config.subs.len();
		let groups = usize::try_from(config.groups).expect("a small group count");
		Self {
			config,
			st: vec![vec![St::Cached; groups]; subs],
			line: Line::default(),
			opened: 0,
			closed: 0,
			asked: vec![false; subs],
			credit: WaiterList::new(),
			turns: 0,
			turn: vec![0; subs],
			streams: (0..subs).map(|_| WaiterList::new()).collect(),
			serving: WaiterList::new(),
			ended: WaiterList::new(),
		}
	}

	fn st(&self, s: usize, k: u64) -> St {
		self.st[s - 1][index(k)]
	}

	fn set(&mut self, s: usize, k: u64, st: St) {
		self.st[s - 1][index(k)] = st;
	}

	fn rank(&self, s: usize, k: u64) -> Rank {
		let turn = i64::try_from(self.turn[s - 1]).expect("a small turn");
		(self.config.prio(s), -turn, self.config.in_group(s, k))
	}

	fn credit(&self) -> bool {
		self.opened < self.config.max_streams + self.closed
	}

	fn live(&self, s: usize, k: u64) -> bool {
		self.config.subs[s - 1].live == Some(k)
	}

	/// The open takes credit: its group's stream is listed, and its send group takes a turn.
	fn open(&mut self, s: usize, k: u64) {
		self.set(s, k, St::Serving);
		self.opened += 1;
		self.turns += 1;
		self.turn[s - 1] = self.turns;
		self.serving.wake();
	}

	/// Ask the transport for credit, reporting whether this open had not asked yet.
	fn ask(&mut self, s: usize, waiter: &Waiter) -> bool {
		waiter.register(&mut self.credit);
		!std::mem::replace(&mut self.asked[s - 1], true)
	}

	/// Whether an open of `s`'s group `k` may reset `t`'s stream of group `j`.
	fn preempts(&self, (s, k): (usize, u64), (t, j): (usize, u64)) -> bool {
		self.config.prio(s) > self.config.prio(t) || (s == t && self.config.in_group(s, k) > self.config.in_group(t, j))
	}

	/// The serving streams, the lowest ranked first.
	fn listed(&self) -> Vec<(usize, u64)> {
		let mut listed: Vec<_> = self.pairs().filter(|&(s, k)| self.st(s, k) == St::Serving).collect();
		listed.sort_by_key(|&(s, k)| self.rank(s, k));
		listed
	}

	fn pairs(&self) -> impl Iterator<Item = (usize, u64)> + use<> {
		let groups = self.config.groups;
		(1..=self.config.subs.len()).flat_map(move |s| (1..=groups).map(move |k| (s, k)))
	}

	/// `Lines::make_room`: each open in line not owed a reset resets the lowest stream it
	/// preempts. Reports whether it reset any.
	fn make_room(&mut self) -> bool {
		let owed: Vec<(usize, u64)> = self
			.line
			.waiting
			.values()
			.skip(self.line.resets)
			.map(|place| (place.s, place.k))
			.collect();
		let mut listed = self.listed();
		let mut reset = false;
		for open in owed {
			let Some(&lowest) = listed.first() else { break };
			let candidate = if self.preempts(open, lowest) {
				Some(lowest)
			} else {
				listed
					.iter()
					.copied()
					.find(|&(t, _)| t == open.0)
					.filter(|&own| self.preempts(open, own))
			};
			let Some(stream) = candidate else { continue };
			listed.retain(|&other| other != stream);
			self.set(stream.0, stream.1, St::Reset);
			self.line.resets += 1;
			self.streams[stream.0 - 1].wake();
			self.ended.wake();
			reset = true;
		}
		reset
	}
}

fn index(k: u64) -> usize {
	usize::try_from(k - 1).expect("a small group number")
}

/// One subscription's `TrackRun`: its waiting open, and the cursor handing out its groups.
pub struct TrackRun {
	session: Lock<Session>,
	s: usize,
	/// The order its groups arrive in.
	order: Vec<u64>,
	/// The group waiting for credit, or 0.
	opening: u64,
	/// Its open's place in line.
	place: Option<Key>,
	bug: Option<Bug>,
}

#[reactor]
impl TrackRun {
	/// `Open::poll` for the waiting group: take credit at the head of the line, or wait in
	/// it, making room.
	#[action]
	pub fn open(&mut self, waiter: &Waiter) -> Poll<()> {
		let k = self.opening;
		if k == 0 {
			return Poll::Pending;
		}
		let s = self.s;
		let mut session = self.session.lock();
		let session = &mut *session;
		let (key, joined) = match self.place {
			// Nobody waits: take credit if there is any, and join the line only without it.
			None if session.line.waiting.is_empty() => {
				if session.credit() {
					session.open(s, k);
					self.opening = 0;
					return Poll::Ready(());
				}
				let key = session.line.join(session.rank(s, k), s, k);
				self.place = Some(key);
				waiter.register(&mut session.line.waiting.get_mut(&key).expect("just joined").waiters);
				session.ask(s, waiter);
				session.make_room();
				return Poll::Ready(());
			}
			None => (session.line.join(session.rank(s, k), s, k), true),
			Some(key) => (key, false),
		};
		self.place = Some(key);
		let place = session
			.line
			.waiting
			.get_mut(&key)
			.expect("an open's place stays in line");
		waiter.register(&mut place.waiters);

		if session.line.head() != Some(key) {
			let reset = session.make_room();
			return ready(joined || reset);
		}
		if !session.credit() {
			let asked = session.ask(s, waiter);
			let reset = session.make_room();
			return ready(joined || asked || reset);
		}
		session.open(s, k);
		session.line.resets = session.line.resets.saturating_sub(1);
		session.line.leave(key, self.bug);
		self.place = None;
		self.opening = 0;
		Poll::Ready(())
	}

	/// The cursor: hand out the next group that arrived and is admitted, replacing the
	/// waiting group if it outranks it, and shedding it otherwise.
	#[action]
	pub fn hand_out(&mut self, waiter: &Waiter) -> Poll<()> {
		let s = self.s;
		let mut session = self.session.lock();
		let session = &mut *session;
		let w = self.opening;
		let stuck = self.stuck(session);
		let asc = session.config.subs[s - 1].asc;
		let admitted = |k: u64| w == 0 || (stuck && (!asc || k < w));
		let next = self
			.order
			.iter()
			.copied()
			.find(|&k| session.st(s, k) == St::Cached && admitted(k));
		let Some(k) = next else {
			waiter.register(&mut session.streams[s - 1]);
			return Poll::Pending;
		};

		if w != 0 && session.config.in_group(s, k) < session.config.in_group(s, w) {
			session.set(s, k, St::Given);
			return Poll::Ready(());
		}
		if w != 0 {
			// `give_way`: the waiting group gives its place up.
			session.set(s, w, St::Given);
			self.opening = 0;
			let place = match self.bug {
				Some(Bug::GiveWayAssumesPlaced) => Some(
					self.place
						.take()
						.expect("`open` runs first, so a waiting open is in line"),
				),
				_ => self.place.take(),
			};
			if let Some(key) = place {
				session.line.leave(key, self.bug);
			}
		}
		session.set(s, k, St::Opening);
		self.opening = k;
		Poll::Ready(())
	}

	/// Only the group waiting for credit holds a place in line.
	#[invariant]
	pub fn placed_while_opening(&self) -> bool {
		let session = self.session.lock();
		let place = self
			.place
			.map(|key| session.line.waiting.get(&key).map(|place| (place.s, place.k)));
		match place {
			None => self.opening == 0 || session.st(self.s, self.opening) == St::Opening,
			Some(found) => found == Some((self.s, self.opening)),
		}
	}
}

impl TrackRun {
	/// `TrackRun::stuck`: the waiting group ranks below every stream the subscription serves.
	fn stuck(&self, session: &Session) -> bool {
		let w = self.opening;
		let s = self.s;
		let mut held = (1..=session.config.groups)
			.filter(|&j| session.st(s, j) == St::Serving)
			.peekable();
		w != 0 && held.peek().is_some() && held.all(|j| session.config.in_group(s, j) > session.config.in_group(s, w))
	}

	/// The loop `poll_next` generates, written by hand: the waiting open, else the cursor.
	pub fn poll_step(&mut self, waiter: &Waiter) -> Poll<()> {
		if self.open(waiter).is_ready() {
			return Poll::Ready(());
		}
		self.hand_out(waiter)
	}
}

fn ready(changed: bool) -> Poll<()> {
	if changed { Poll::Ready(()) } else { Poll::Pending }
}

/// The peer and the transport: streams are sent whole, and the peer frees each one sent
/// whole or reset, granting one more.
pub struct Peer {
	session: Lock<Session>,
	rng: StdRng,
}

#[reactor]
impl Peer {
	/// A serving stream, except a live group's, is sent whole.
	#[action]
	pub fn finish(&mut self, waiter: &Waiter) -> Poll<()> {
		let mut session = self.session.lock();
		let session = &mut *session;
		let serving: Vec<_> = session
			.pairs()
			.filter(|&(s, k)| session.st(s, k) == St::Serving && !session.live(s, k))
			.collect();
		let Some(&(s, k)) = pick(&mut self.rng, &serving) else {
			waiter.register(&mut session.serving);
			return Poll::Pending;
		};
		session.set(s, k, St::Fin);
		session.streams[s - 1].wake();
		Poll::Ready(())
	}

	/// The peer frees a stream sent whole or reset, and the opens waiting on the transport wake.
	#[action]
	pub fn free(&mut self, waiter: &Waiter) -> Poll<()> {
		let mut session = self.session.lock();
		let session = &mut *session;
		let ended: Vec<_> = session
			.pairs()
			.filter(|&(s, k)| matches!(session.st(s, k), St::Fin | St::Reset))
			.collect();
		let Some(&(s, k)) = pick(&mut self.rng, &ended) else {
			waiter.register(&mut session.ended);
			return Poll::Pending;
		};
		session.set(s, k, St::Closed);
		session.closed += 1;
		session.asked.fill(false);
		session.credit.wake();
		Poll::Ready(())
	}

	/// `WithinCredit`: never more streams open than the peer granted.
	#[invariant]
	pub fn within_credit(&self) -> bool {
		let session = self.session.lock();
		session.opened <= session.config.max_streams + session.closed
	}

	/// Never more resets owed than opens waiting to be owed one.
	#[invariant]
	pub fn resets_owed(&self) -> bool {
		let session = self.session.lock();
		session.line.resets <= session.line.waiting.len()
	}
}

fn pick<'a, T>(rng: &mut StdRng, items: &'a [T]) -> Option<&'a T> {
	if items.is_empty() {
		return None;
	}
	items.get((rng.next_u64() % items.len() as u64) as usize)
}

/// One session: a `TrackRun` per subscription and the peer.
pub struct System {
	session: Lock<Session>,
	/// Each subscription's `TrackRun`, subscription 1 first.
	pub tracks: Vec<TrackRun>,
	/// The peer.
	pub peer: Peer,
}

impl System {
	/// A session for `config`, its groups arriving in an order drawn from `seed`.
	pub fn new(config: Config, seed: u64) -> Self {
		let mut rng = StdRng::seed_from_u64(seed);
		let bug = config.bug;
		let groups = config.groups;
		let subs = config.subs.len();
		let session = Lock::new(Session::new(config));
		let tracks = (1..=subs)
			.map(|s| {
				let mut order: Vec<u64> = (1..=groups).collect();
				for i in (1..order.len()).rev() {
					order.swap(i, (rng.next_u64() % (i as u64 + 1)) as usize);
				}
				TrackRun {
					session: session.clone(),
					s,
					order,
					opening: 0,
					place: None,
					bug,
				}
			})
			.collect();
		let peer = Peer {
			session: session.clone(),
			rng,
		};
		Self { session, tracks, peer }
	}

	/// Every group's state, by subscription, as the spec names them.
	pub fn states(&self) -> Vec<Vec<&'static str>> {
		let session = self.session.lock();
		session
			.st
			.iter()
			.map(|row| row.iter().map(|st| st.tla()).collect())
			.collect()
	}

	/// The spec's `v` for this state, given which subscriptions' tasks are awake.
	pub fn tla(&self, awake: &[bool]) -> String {
		let session = self.session.lock();
		let subs = session.config.subs.len();
		let mut rank = vec!["<<>>".to_owned(); subs];
		let mut arrival = vec![0; subs];
		for (key, place) in &session.line.waiting {
			let (prio, turn, group) = key.rank.0;
			rank[place.s - 1] = format!("<<{prio}, {turn}, {group}>>");
			arrival[place.s - 1] = key.arrival;
		}
		let st = session
			.st
			.iter()
			.map(|row| tuple(row.iter().map(|st| format!("\"{}\"", st.tla()))))
			.collect::<Vec<_>>();
		let mut out = String::new();
		write!(
			out,
			"[st |-> {}, opening |-> {}, awake |-> {}, asked |-> {}, rank |-> {}, arrival |-> {}, \
			 arrivals |-> {}, resets |-> {}, turn |-> {}, turns |-> {}, opened |-> {}, closed |-> {}]",
			tuple(st),
			tuple(self.tracks.iter().map(|track| track.opening)),
			tuple(awake.iter().map(|&awake| if awake { "TRUE" } else { "FALSE" })),
			tuple(session.asked.iter().map(|&asked| if asked { "TRUE" } else { "FALSE" })),
			tuple(rank),
			tuple(arrival),
			session.line.arrivals,
			session.line.resets,
			tuple(&session.turn),
			session.turns,
			session.opened,
			session.closed,
		)
		.expect("writing to a String");
		out
	}
}

fn tuple<T: std::fmt::Display>(items: impl IntoIterator<Item = T>) -> String {
	let items: Vec<String> = items.into_iter().map(|item| item.to_string()).collect();
	format!("<<{}>>", items.join(", "))
}

/// Which loop drives each `TrackRun`.
#[derive(Clone, Copy, Debug)]
pub enum Driver {
	/// The `#[reactor]`'s `poll_next`, until it parks.
	Reactor,
	/// The hand-written [`TrackRun::poll_step`], until it parks.
	Hand,
}

/// Run a session to quiet in priority order, round robin over its reactors, returning the
/// actions fired.
pub fn drive(system: &mut System, driver: Driver) -> usize {
	let waiter = Waiter::noop();
	let mut fired = 0;
	loop {
		let before = fired;
		for track in &mut system.tracks {
			match driver {
				Driver::Reactor => {
					while track.poll_next(&waiter).is_ready() {
						fired += 1;
					}
				}
				Driver::Hand => {
					while track.poll_step(&waiter).is_ready() {
						fired += 1;
					}
				}
			}
		}
		while system.peer.poll_next(&waiter).is_ready() {
			fired += 1;
		}
		if fired == before {
			return fired;
		}
	}
}

#[cfg(feature = "sim")]
crate::reactor!(TrackRun => TrackRunAction);
#[cfg(feature = "sim")]
crate::reactor!(Peer => PeerAction);

/// Explore one session in an order drawn from `seed` until it is quiet, returning the spec's
/// `v` after every poll that changed it.
#[cfg(feature = "sim")]
pub fn explore(config: Config, seed: u64) -> Result<Vec<String>, crate::explore::Failed> {
	use crate::explore::{Explorer, Reactor, Step};

	let mut system = System::new(config, seed);
	let subs = system.tracks.len();
	let labels: Vec<String> = (1..=subs)
		.map(|s| format!("TrackRun[{s}]"))
		.chain(["Peer".to_owned()])
		.collect();
	let reactors: Vec<(&str, &dyn Reactor)> = labels
		.iter()
		.map(String::as_str)
		.zip(
			system
				.tracks
				.iter()
				.map(|track| track as &dyn Reactor)
				.chain([&system.peer as &dyn Reactor]),
		)
		.collect();
	let mut explorer = Explorer::new(seed, &reactors);

	let awake = |explorer: &Explorer| (0..subs).map(|s| explorer.awake(s)).collect::<Vec<_>>();
	let mut states = vec![system.tla(&awake(&explorer))];
	loop {
		let mut reactors: Vec<&mut dyn Reactor> = system
			.tracks
			.iter_mut()
			.map(|track| track as &mut dyn Reactor)
			.chain([&mut system.peer as &mut dyn Reactor])
			.collect();
		if let Step::Quiet = explorer.step(&mut reactors)? {
			return Ok(states);
		}
		let state = system.tla(&awake(&explorer));
		if states.last() != Some(&state) {
			states.push(state);
		}
	}
}

/// A TLA+ module defining `Traces`, each a sequence of the spec's `v`, with `st` as a tuple
/// of rows.
pub fn traces_module(name: &str, traces: &[Vec<String>]) -> String {
	let traces: Vec<String> = traces
		.iter()
		.map(|trace| format!("  <<\n    {}\n  >>", trace.join(",\n    ")))
		.collect();
	format!(
		"---- MODULE {name} ----\nEXTENDS Integers\n\\* Generated by kio-reactor-lab's `traces` example.\nTraces == <<\n{}\n>>\n====\n",
		traces.join(",\n")
	)
}

/// The `CONSTANTS` block of `config`'s `.cfg`.
pub fn constants(config: &Config) -> String {
	let set = |pick: &dyn Fn(&Sub) -> bool| {
		let subs: Vec<String> = (1..=config.subs.len())
			.filter(|&s| pick(&config.subs[s - 1]))
			.map(|s| s.to_string())
			.collect();
		format!("{{{}}}", subs.join(", "))
	};
	let live_group = config.subs.iter().find_map(|sub| sub.live).unwrap_or(1);
	assert!(
		config.subs.iter().all(|sub| sub.live.is_none_or(|k| k == live_group)),
		"the spec has one LiveGroup"
	);
	format!(
		"CONSTANTS\n    Subs = {}\n    G = {}\n    High = {}\n    Asc = {}\n    MaxStreams = {}\n    LiveSubs = {}\n    LiveGroup = {}\n",
		set(&|_| true),
		config.groups,
		set(&|sub| sub.high),
		set(&|sub| sub.asc),
		config.max_streams,
		set(&|sub| sub.live.is_some()),
		live_group,
	)
}
