//! The sender's wait for stream credit, as a pure machine: moq-net's `TrackRun` cursor
//! (`lite/publisher.rs`) and its session's line (`transport/open.rs`), as `SendCredit.tla`
//! models them.
//!
//! The peer grants `max_streams` concurrent streams, and one more each time it frees one.
//! Each subscription hands its groups out one at a time, and one group per subscription
//! waits for credit in the session's line, ranked as the transport sends streams. Only the
//! head of the line asks the transport for credit. A waiting open resets the lowest-ranked
//! stream it preempts. A group handed out while another waits replaces it if it outranks
//! it (`give_way`), and is shed otherwise. While the waiting group ranks below every stream
//! its subscription serves (`stuck`), the cursor hands out only groups that outrank it.
//!
//! The machine holds only what the control plane decides. The data plane stays outside: a
//! poll is told which groups the subscription's cache holds, as opaque keys, and the
//! machine answers with keys to take, shed, open, or reset.
//!
//! The code is written for Charon and Aeneas as much as for people: `while` loops over
//! indices rather than iterators, no closures, and no trait objects.

pub mod world;

use crate::{Disabled, StateMachine};

/// What stays fixed for a session's life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Shape<const S: usize> {
	/// Each subscription's send group priority, higher first.
	pub priority: [u8; S],
	/// Whether each subscription's groups go oldest first.
	pub ascending: [bool; S],
	/// The peer's initial stream credit.
	pub max_streams: u8,
	/// A regression to inject, to show the checker catches it.
	pub mutant: Option<Mutant>,
}

/// A regression the checker should catch, each a line of the real code left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Mutant {
	/// A waiting group never gives way to one that outranks it: the cursor hands out
	/// nothing while a group waits, stuck or not.
	NoGiveWay,
	/// A waiting open never resets a stream it preempts (`Lines::make_room` does nothing).
	NoPreemption,
	/// Leaving the line never wakes the next head (`Line::leave` skips `wake_head`).
	NoLeaveWake,
}

/// Where an open or a stream ranks, as `open::Rank` orders it: the higher priority, then
/// the send group that took credit longest ago, then the group's rank in its subscription.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Rank {
	/// The subscription's send group priority.
	pub priority: u8,
	/// When its send group last took credit: the earlier, the higher.
	pub turn: u8,
	/// The group's rank in its subscription (`priority::in_group`).
	pub in_group: i16,
}

/// Whether `x` is `Some(v)`; Aeneas has no model of `Option`'s `==`.
fn is<T: Copy + PartialEq>(x: Option<T>, v: T) -> bool {
	match x {
		Some(y) => y == v,
		None => false,
	}
}

/// Whether `a` orders above `b`.
pub fn above(a: Rank, b: Rank) -> bool {
	a.priority > b.priority
		|| (a.priority == b.priority && (a.turn < b.turn || (a.turn == b.turn && a.in_group > b.in_group)))
}

fn same(a: Rank, b: Rank) -> bool {
	a.priority == b.priority && a.turn == b.turn && a.in_group == b.in_group
}

/// An open's place in line: its rank as it joined, then its arrival.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Place {
	/// Its rank as it joined.
	pub rank: Rank,
	/// The line's arrival count when it joined.
	pub arrival: u8,
}

/// One subscription's part of the session: `TrackRun`'s waiting group and its streams.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Sub<const G: usize> {
	/// The group waiting for stream credit (`TrackRun::opening`), by key from 1 to `G`.
	pub opening: Option<u8>,
	/// The waiting open's place in line, once it joined.
	pub place: Option<Place>,
	/// The waiting open asked the transport for credit, so a freed stream wakes it.
	pub asked: bool,
	/// When its send group last took credit, 0 before it ever did.
	pub turn: u8,
	/// The groups with a stream the session lists: open, neither sent whole nor reset.
	pub serving: [bool; G],
}

/// One session's send credit: `S` subscriptions of `G` groups each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Session<const S: usize, const G: usize> {
	/// What stays fixed.
	pub shape: Shape<S>,
	/// Each subscription's part.
	pub subs: [Sub<G>; S],
	/// The line's arrival counter.
	pub arrivals: u8,
	/// Streams reset to make room whose credit has yet to reach an open (`Line::resets`).
	pub resets: u8,
	/// Streams opened, which also numbers the send groups' turns.
	pub opened: u8,
	/// Streams the peer freed.
	pub freed: u8,
}

/// What the environment tells a [`Session`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Event {
	/// One pass of subscription `sub`'s task (`TrackRun::poll_step`): its waiting open,
	/// then its cursor, whose cache holds the groups in the `cached` bit set (bit `k - 1`
	/// for group `k`) and hands out `pick` if the machine admits it.
	Poll {
		/// The subscription.
		sub: usize,
		/// The groups its cache holds.
		cached: u16,
		/// The group the cursor hands out, if the machine admits it.
		pick: Option<u8>,
	},
	/// `sub`'s stream of `group` was sent whole.
	Finished {
		/// The subscription.
		sub: usize,
		/// The group.
		group: u8,
	},
	/// The peer freed a stream, granting one more.
	Freed,
}

/// What a [`Session`] asks of its environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Output {
	/// Wake subscription `.0`'s task.
	Wake(usize),
	/// The pass changed nothing: park subscription `.0`'s task until it is woken.
	Park(usize),
	/// The cursor handed `group` out of the cache; it now waits for credit.
	Take {
		/// The subscription.
		sub: usize,
		/// The group.
		group: u8,
	},
	/// Give `group` up without a stream: handed out below the waiting group, or the
	/// waiting group giving way.
	Shed {
		/// The subscription.
		sub: usize,
		/// The group.
		group: u8,
	},
	/// Open `group`'s stream, taking a credit.
	Open {
		/// The subscription.
		sub: usize,
		/// The group.
		group: u8,
	},
	/// Reset `group`'s stream to make room.
	Reset {
		/// The subscription.
		sub: usize,
		/// The group.
		group: u8,
	},
}

impl<const S: usize, const G: usize> Session<S, G> {
	/// A session that has opened nothing yet.
	pub fn new(shape: Shape<S>) -> Self {
		let sub = Sub {
			opening: None,
			place: None,
			asked: false,
			turn: 0,
			serving: [false; G],
		};
		Self {
			shape,
			subs: [sub; S],
			arrivals: 0,
			resets: 0,
			opened: 0,
			freed: 0,
		}
	}

	/// Subscription `s`'s part, copied: Aeneas cannot match on an enum read in place
	/// through an array, nor join loop contexts that borrow one.
	fn sub(&self, s: usize) -> Sub<G> {
		self.subs[s]
	}

	/// Group `k`'s rank in subscription `s` (`priority::in_group`), the higher first.
	pub fn in_group(&self, s: usize, k: u8) -> i16 {
		if self.shape.ascending[s] { -(k as i16) } else { k as i16 }
	}

	/// The rank of subscription `s`'s open or stream of group `k`, as of now.
	pub fn rank(&self, s: usize, k: u8) -> Rank {
		Rank {
			priority: self.shape.priority[s],
			turn: self.sub(s).turn,
			in_group: self.in_group(s, k),
		}
	}

	/// Whether the peer has credit for another stream.
	pub fn credit(&self) -> bool {
		// Widened, so a session near the counters' end cannot overflow the sum (Kani).
		(self.opened as u16) < self.shape.max_streams as u16 + self.freed as u16
	}

	/// Whether `s` goes before `t` in line.
	fn before(&self, s: usize, t: usize) -> bool {
		match (self.sub(s).place, self.sub(t).place) {
			(Some(a), Some(b)) => above(a.rank, b.rank) || (same(a.rank, b.rank) && a.arrival < b.arrival),
			_ => false,
		}
	}

	/// How many opens wait in line.
	fn placed(&self) -> u8 {
		let mut n = 0;
		let mut s = 0;
		while s < S {
			if self.sub(s).place.is_some() {
				n += 1;
			}
			s += 1;
		}
		n
	}

	/// How many opens go before `s` in line.
	fn ahead(&self, s: usize) -> u8 {
		let mut n = 0;
		let mut t = 0;
		while t < S {
			if self.before(t, s) {
				n += 1;
			}
			t += 1;
		}
		n
	}

	/// Whether `s` is first in line.
	pub fn is_head(&self, s: usize) -> bool {
		self.sub(s).place.is_some() && self.ahead(s) == 0
	}

	/// The first in line, if anyone waits.
	fn head(&self) -> Option<usize> {
		let mut s = 0;
		while s < S {
			if self.is_head(s) {
				return Some(s);
			}
			s += 1;
		}
		None
	}

	/// Whether an open of `s`'s group `k` may reset `t`'s stream of group `j`
	/// (`Rank::preempts`): a lower-priority subscription's, or an older one of its own.
	pub fn preempts(&self, s: usize, k: u8, t: usize, j: u8) -> bool {
		self.shape.priority[s] > self.shape.priority[t] || (s == t && self.in_group(s, k) > self.in_group(t, j))
	}

	/// The lowest-ranked listed stream, of subscription `only` if given.
	fn lowest(&self, only: Option<usize>) -> Option<(usize, u8)> {
		let mut low: Option<(usize, u8)> = None;
		let mut t = 0;
		while t < S {
			if only.is_none() || is(only, t) {
				let mut j = 0;
				while j < G {
					let k = (j + 1) as u8;
					if self.sub(t).serving[j] {
						low = match low {
							Some((u, l)) if !above(self.rank(u, l), self.rank(t, k)) => Some((u, l)),
							_ => Some((t, k)),
						};
					}
					j += 1;
				}
			}
			t += 1;
		}
		low
	}

	/// The stream an open of `s`'s group `k` would reset to make room: the lowest-ranked
	/// one if it preempts that, or else its own lowest if it preempts that.
	#[expect(clippy::question_mark, reason = "Aeneas has no model of `?` on `Option`")]
	pub fn candidate(&self, s: usize, k: u8) -> Option<(usize, u8)> {
		let Some((t, j)) = self.lowest(None) else {
			return None;
		};
		if self.preempts(s, k, t, j) {
			return Some((t, j));
		}
		match self.lowest(Some(s)) {
			Some((_, j)) if self.preempts(s, k, s, j) => Some((s, j)),
			_ => None,
		}
	}

	/// `Lines::make_room`: each open in line not owed a reset resets the lowest-ranked
	/// stream it preempts. Whether anything changed.
	fn make_room(&mut self, out: &mut Vec<Output>) -> bool {
		// The opens past the first `resets` in line, fixed before any reset.
		let mut owed = [false; S];
		let mut s = 0;
		while s < S {
			owed[s] = self.sub(s).place.is_some() && self.ahead(s) >= self.resets;
			s += 1;
		}
		let mut changed = false;
		loop {
			// The first owed in line.
			let mut first: Option<usize> = None;
			let mut s = 0;
			while s < S {
				if owed[s] {
					first = match first {
						Some(f) if !self.before(s, f) => Some(f),
						_ => Some(s),
					};
				}
				s += 1;
			}
			let Some(s) = first else {
				return changed;
			};
			owed[s] = false;
			if matches!(self.shape.mutant, Some(Mutant::NoPreemption)) {
				continue;
			}
			let Some(k) = self.sub(s).opening else {
				continue;
			};
			if let Some((t, j)) = self.candidate(s, k) {
				self.subs[t].serving[(j - 1) as usize] = false;
				self.resets += 1;
				out.push(Output::Reset { sub: t, group: j });
				out.push(Output::Wake(t));
				changed = true;
			}
		}
	}

	/// `Line::join`: `s`'s waiting open takes the next place in line.
	fn join(&mut self, s: usize, k: u8) {
		self.subs[s].place = Some(Place {
			rank: self.rank(s, k),
			arrival: self.arrivals,
		});
		self.arrivals += 1;
	}

	/// `Line::leave`: `s` leaves the line, waking whoever is now first if `s` was, and the
	/// line owes no more resets than opens.
	fn leave(&mut self, s: usize, out: &mut Vec<Output>) {
		let was_head = self.is_head(s);
		self.subs[s].place = None;
		let placed = self.placed();
		if self.resets > placed {
			self.resets = placed;
		}
		if was_head
			&& !matches!(self.shape.mutant, Some(Mutant::NoLeaveWake))
			&& let Some(head) = self.head()
		{
			out.push(Output::Wake(head));
		}
	}

	/// `s`'s waiting group `k` takes credit: its stream opens and is listed, and its send
	/// group takes a turn.
	fn open(&mut self, s: usize, k: u8, out: &mut Vec<Output>) {
		self.opened += 1;
		let sub = &mut self.subs[s];
		sub.serving[(k - 1) as usize] = true;
		sub.opening = None;
		sub.turn = self.opened;
		out.push(Output::Open { sub: s, group: k });
	}

	/// `Open::poll` for `s`'s waiting group. Whether anything changed.
	fn poll_open(&mut self, s: usize, out: &mut Vec<Output>) -> bool {
		let Some(k) = self.sub(s).opening else {
			return false;
		};
		// Nobody waits: take credit if there is any, and join the line only without it.
		if self.sub(s).place.is_none() && self.placed() == 0 {
			if self.credit() {
				self.open(s, k, out);
			} else {
				self.join(s, k);
				self.subs[s].asked = true;
				self.make_room(out);
			}
			return true;
		}
		let mut changed = false;
		if self.sub(s).place.is_none() {
			self.join(s, k);
			changed = true;
		}
		if !self.is_head(s) {
			return self.make_room(out) || changed;
		}
		if self.credit() {
			self.open(s, k, out);
			// Whichever open the credit reaches, a reset's credit may be the one it took.
			if self.resets > 0 {
				self.resets -= 1;
			}
			self.leave(s, out);
			return true;
		}
		if !self.sub(s).asked {
			self.subs[s].asked = true;
			changed = true;
		}
		self.make_room(out) || changed
	}

	/// `TrackRun::stuck`: `s`'s waiting group ranks below every stream `s` serves, which it
	/// may then not reset.
	pub fn stuck(&self, s: usize) -> bool {
		let Some(w) = self.sub(s).opening else {
			return false;
		};
		let mut held = false;
		let mut j = 0;
		while j < G {
			if self.sub(s).serving[j] {
				if self.in_group(s, (j + 1) as u8) <= self.in_group(s, w) {
					return false;
				}
				held = true;
			}
			j += 1;
		}
		held
	}

	/// Whether `s`'s cursor may hand out group `k`: any while none waits; while one waits
	/// and is stuck, any when newest first, since that sheds the backlog, and only one that
	/// outranks it when oldest first; otherwise none.
	pub fn admits(&self, s: usize, k: u8) -> bool {
		let Some(w) = self.sub(s).opening else {
			return true;
		};
		!matches!(self.shape.mutant, Some(Mutant::NoGiveWay)) && self.stuck(s) && (!self.shape.ascending[s] || k < w)
	}

	/// The cursor hands out `s`'s group `k`: shed if it ranks below the waiting group, or
	/// replacing it (`give_way`) to wait itself.
	fn cursor(&mut self, s: usize, k: u8, out: &mut Vec<Output>) {
		if let Some(w) = self.sub(s).opening {
			if self.in_group(s, k) < self.in_group(s, w) {
				out.push(Output::Shed { sub: s, group: k });
				return;
			}
			out.push(Output::Shed { sub: s, group: w });
			self.subs[s].opening = None;
			if self.sub(s).place.is_some() {
				self.leave(s, out);
			}
		}
		out.push(Output::Take { sub: s, group: k });
		self.subs[s].opening = Some(k);
	}

	/// One pass of `s`'s task: the waiting open, then the cursor. A pass that changes
	/// nothing parks the task.
	fn poll(&mut self, s: usize, cached: u16, pick: Option<u8>, out: &mut Vec<Output>) -> Result<(), Disabled> {
		// A cache never holds a group this subscription already took (Kani found the
		// machine trusting it: a taken group handed out again would wait while streaming).
		if cached & self.taken(s) != 0 {
			return Err(Disabled);
		}
		// Outputs wait in `scratch` until the pass is known to be enabled.
		let mut next = *self;
		let mut scratch = Vec::new();
		let changed = next.poll_open(s, &mut scratch);
		let mut ready = false;
		let mut j = 0;
		while j < G {
			if cached & (1 << j) != 0 && next.admits(s, (j + 1) as u8) {
				ready = true;
			}
			j += 1;
		}
		if !ready {
			if !changed {
				scratch.push(Output::Park(s));
			}
		} else {
			let Some(k) = pick else {
				return Err(Disabled);
			};
			if k < 1 || k as usize > G || cached & (1 << (k - 1)) == 0 || !next.admits(s, k) {
				return Err(Disabled);
			}
			next.cursor(s, k, &mut scratch);
		}
		*self = next;
		// Not a push loop: Kani would need to unwind it as far as a pass has outputs.
		out.extend_from_slice(&scratch);
		Ok(())
	}

	/// The bit set of `s`'s groups waiting for credit or streaming.
	fn taken(&self, s: usize) -> u16 {
		let mut taken = 0;
		let mut j = 0;
		while j < G {
			if self.sub(s).serving[j] || is(self.sub(s).opening, (j + 1) as u8) {
				taken |= 1 << j;
			}
			j += 1;
		}
		taken
	}

	/// `s`'s stream of group `k` was sent whole: it leaves the list, and its task wakes.
	fn finished(&mut self, s: usize, k: u8, out: &mut Vec<Output>) -> Result<(), Disabled> {
		if k < 1 || k as usize > G || !self.sub(s).serving[(k - 1) as usize] {
			return Err(Disabled);
		}
		self.subs[s].serving[(k - 1) as usize] = false;
		out.push(Output::Wake(s));
		Ok(())
	}

	/// The peer freed a stream: one more credit, and the opens that asked for it wake.
	fn freed(&mut self, out: &mut Vec<Output>) -> Result<(), Disabled> {
		if self.freed >= self.opened {
			return Err(Disabled);
		}
		self.freed += 1;
		let mut s = 0;
		while s < S {
			if self.sub(s).asked {
				self.subs[s].asked = false;
				out.push(Output::Wake(s));
			}
			s += 1;
		}
		Ok(())
	}

	/// Apply one event; the body of [`StateMachine::step`], for Charon to start from.
	pub fn step(&mut self, event: Event, out: &mut Vec<Output>) -> Result<(), Disabled> {
		match event {
			Event::Poll { sub, cached, pick } if sub < S => self.poll(sub, cached, pick, out),
			Event::Finished { sub, group } if sub < S => self.finished(sub, group, out),
			Event::Freed => self.freed(out),
			_ => Err(Disabled),
		}
	}

	/// Never more streams open than the peer granted (`WithinCredit`).
	pub fn within_credit(&self) -> bool {
		(self.opened as u16) <= self.shape.max_streams as u16 + self.freed as u16
	}

	/// The structure every reachable session keeps: only a waiting group has a place, it
	/// has no stream yet, and the line owes no more resets than it has opens.
	pub fn well_formed(&self) -> bool {
		let mut s = 0;
		while s < S {
			let sub = self.sub(s);
			match sub.opening {
				None if sub.place.is_some() => return false,
				Some(k) if k < 1 || k as usize > G || sub.serving[(k - 1) as usize] => return false,
				_ => {}
			}
			s += 1;
		}
		self.resets <= self.placed() && self.freed <= self.opened
	}
}

impl<const S: usize, const G: usize> StateMachine for Session<S, G> {
	type Event = Event;
	type Output = Output;

	fn step(&mut self, event: Event, out: &mut Vec<Output>) -> Result<(), Disabled> {
		Session::step(self, event, out)
	}

	fn invariant(&self) -> Result<(), &'static str> {
		if !self.within_credit() {
			return Err("WithinCredit");
		}
		if !self.well_formed() {
			return Err("WellFormed");
		}
		Ok(())
	}
}

#[cfg(kani)]
mod proofs;
