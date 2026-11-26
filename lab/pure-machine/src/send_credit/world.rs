//! A [`Session`] closed by an abstract environment, so [`crate::check`] explores it as TLC
//! explores `SendCredit.tla`: the same steps, rules, witnesses, and configs.
//!
//! The environment is each subscription's cache, which holds every group from the start
//! and hands them out in any order; each task, awake or parked; and the peer, which
//! eventually frees every stream sent whole or reset. A live group's producer never
//! finishes, so its stream holds its credit until an open resets it.
//!
//! A world is the TLA+ state, split between the machine and its environment, so a check
//! visits exactly the states TLC does.

use std::fmt;

use super::{Event, Mutant, Output, Session, Shape};
use crate::check::Explore;
use crate::{Disabled, StateMachine};

/// Where a group is, as far as the environment can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
	/// In the cache, not yet handed out.
	Cached,
	/// Handed out: waiting for credit or streaming, as the session says.
	Taken,
	/// Its stream was sent whole and has yet to be freed.
	Fin,
	/// Its stream was reset and has yet to be freed.
	Reset,
	/// Its stream was freed.
	Closed,
	/// Given up without a stream.
	Given,
}

/// A session and its environment.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct World<const S: usize, const G: usize> {
	/// The machine.
	pub session: Session<S, G>,
	/// Each subscription's groups.
	pub groups: [[Group; G]; S],
	/// Whether each subscription's task has a wake pending.
	pub awake: [bool; S],
	/// Each subscription's group whose producer never finishes, if any.
	pub live: [Option<u8>; S],
}

/// A step of the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
	/// An awake task's pass, its cursor handing out `pick` if admitted.
	Poll {
		/// The subscription.
		sub: usize,
		/// The cached group the cursor would hand out, if any is cached.
		pick: Option<u8>,
	},
	/// A stream is sent whole.
	Finish {
		/// The subscription.
		sub: usize,
		/// The group.
		group: u8,
	},
	/// The peer frees a stream sent whole or reset.
	Free {
		/// The subscription.
		sub: usize,
		/// The group.
		group: u8,
	},
}

/// What a group is in `SendCredit.tla`'s terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum St {
	/// `"cached"`
	Cached,
	/// `"opening"`
	Opening,
	/// `"serving"`
	Serving,
	/// `"fin"`
	Fin,
	/// `"reset"`
	Reset,
	/// `"closed"`
	Closed,
	/// `"given"`
	Given,
}

impl<const S: usize, const G: usize> World<S, G> {
	/// Every group cached and every task awake.
	pub fn new(shape: Shape<S>, live: [Option<u8>; S]) -> Self {
		Self {
			session: Session::new(shape),
			groups: [[Group::Cached; G]; S],
			awake: [true; S],
			live,
		}
	}

	/// Group `k` of subscription `s`, as the TLA+ model names its state.
	pub fn st(&self, s: usize, k: u8) -> St {
		match self.groups[s][(k - 1) as usize] {
			Group::Cached => St::Cached,
			Group::Taken if self.session.subs[s].opening == Some(k) => St::Opening,
			Group::Taken => St::Serving,
			Group::Fin => St::Fin,
			Group::Reset => St::Reset,
			Group::Closed => St::Closed,
			Group::Given => St::Given,
		}
	}

	fn apply(&mut self, out: &[Output]) {
		for output in out {
			match *output {
				Output::Wake(s) => self.awake[s] = true,
				Output::Park(s) => self.awake[s] = false,
				Output::Take { sub, group } => self.groups[sub][(group - 1) as usize] = Group::Taken,
				Output::Shed { sub, group } => self.groups[sub][(group - 1) as usize] = Group::Given,
				Output::Open { .. } => {}
				Output::Reset { sub, group } => self.groups[sub][(group - 1) as usize] = Group::Reset,
			}
		}
	}

	/// The event `action` tells the session: a poll carries what the cache holds.
	pub fn event(&self, action: Action) -> Event {
		match action {
			Action::Poll { sub, pick } => {
				let mut cached = 0u16;
				for (j, group) in self.groups[sub].iter().enumerate() {
					if *group == Group::Cached {
						cached |= 1 << j;
					}
				}
				Event::Poll { sub, cached, pick }
			}
			Action::Finish { sub, group } => Event::Finished { sub, group },
			Action::Free { .. } => Event::Freed,
		}
	}

	/// Run to quiet, taking the first enabled action each step, and return the events the
	/// session saw: one schedule of many, to replay elsewhere.
	pub fn run(mut self) -> (Self, Vec<Event>) {
		let mut events = Vec::new();
		let mut actions = Vec::new();
		loop {
			actions.clear();
			self.events(&mut actions);
			let Some((action, next)) = actions.iter().find_map(|&action| {
				let mut next = self;
				next.step(action, &mut Vec::new()).ok().map(|()| (action, next))
			}) else {
				return (self, events);
			};
			events.push(self.event(action));
			self = next;
		}
	}

	/// `s`'s group `k` waits for credit that only streams it may not reset hold.
	fn blocked(&self, s: usize, k: u8) -> bool {
		!self.session.credit() && self.session.candidate(s, k).is_none()
	}

	/// The measure every step but the last raises (`Progress`): first the groups' states,
	/// twice, and the places taken; then the opens asking and the tasks parked.
	fn measure(&self) -> (u32, u32) {
		let mut counted = 0;
		let mut waits = 0;
		for s in 0..S {
			for k in 1..=G as u8 {
				counted += 2 * match self.st(s, k) {
					St::Cached => 0,
					St::Opening => 1,
					St::Serving => 2,
					St::Fin | St::Reset => 3,
					St::Closed | St::Given => 4,
				};
			}
			let sub = &self.session.subs[s];
			counted += u32::from(sub.place.is_some());
			waits += u32::from(sub.asked) + u32::from(!self.awake[s]);
		}
		(counted, waits)
	}

	/// `Ends`: once quiet, every group was sent and freed or given up, is a live group's
	/// stream, or waits, blocked, or stays cached behind its subscription's blocked open.
	pub fn ends(&self) -> bool {
		(0..S).all(|s| {
			(1..=G as u8).all(|k| match self.st(s, k) {
				St::Closed | St::Given => true,
				St::Serving => self.live[s] == Some(k),
				St::Opening => self.blocked(s, k),
				St::Cached => self.session.subs[s].opening.is_some_and(|w| self.blocked(s, w)),
				St::Fin | St::Reset => false,
			})
		})
	}

	/// `TopDelivered`: once quiet, each subscription's highest-ranked group reached the
	/// peer, was reset, or is blocked.
	pub fn top_delivered(&self) -> bool {
		(0..S).all(|s| {
			let top = (1..=G as u8)
				.max_by_key(|&k| self.session.in_group(s, k))
				.expect("a group");
			match self.st(s, top) {
				St::Serving | St::Fin | St::Reset | St::Closed => true,
				St::Cached | St::Opening => self.blocked(s, top),
				St::Given => false,
			}
		})
	}
}

impl<const S: usize, const G: usize> StateMachine for World<S, G> {
	type Event = Action;
	type Output = ();

	fn step(&mut self, action: Action, _: &mut Vec<()>) -> Result<(), Disabled> {
		let mut out = Vec::new();
		match action {
			Action::Poll { sub, .. } if !self.awake[sub] => return Err(Disabled),
			Action::Finish { sub, group } if self.live[sub] == Some(group) => return Err(Disabled),
			Action::Free { sub, group }
				if !matches!(self.groups[sub][(group - 1) as usize], Group::Fin | Group::Reset) =>
			{
				return Err(Disabled);
			}
			_ => {}
		}
		self.session.step(self.event(action), &mut out)?;
		match action {
			Action::Poll { .. } => {}
			Action::Finish { sub, group } => self.groups[sub][(group - 1) as usize] = Group::Fin,
			Action::Free { sub, group } => self.groups[sub][(group - 1) as usize] = Group::Closed,
		}
		self.apply(&out);
		Ok(())
	}

	fn invariant(&self) -> Result<(), &'static str> {
		self.session.invariant()
	}
}

impl<const S: usize, const G: usize> Explore for World<S, G> {
	fn events(&self, out: &mut Vec<Action>) {
		for sub in 0..S {
			if self.awake[sub] {
				let mut any = false;
				for k in 1..=G as u8 {
					if self.groups[sub][(k - 1) as usize] == Group::Cached {
						out.push(Action::Poll { sub, pick: Some(k) });
						any = true;
					}
				}
				if !any {
					out.push(Action::Poll { sub, pick: None });
				}
			}
			for group in 1..=G as u8 {
				match self.groups[sub][(group - 1) as usize] {
					Group::Taken => out.push(Action::Finish { sub, group }),
					Group::Fin | Group::Reset => out.push(Action::Free { sub, group }),
					_ => {}
				}
			}
		}
	}

	fn transition(&self, next: &Self) -> Result<(), &'static str> {
		let (counted, waits) = self.measure();
		let (counted_next, waits_next) = next.measure();
		if counted_next > counted || (counted_next == counted && waits_next > waits) {
			Ok(())
		} else {
			Err("Progress")
		}
	}

	fn quiet(&self) -> Result<(), &'static str> {
		if !self.ends() {
			return Err("Ends");
		}
		if !self.top_delivered() {
			return Err("TopDelivered");
		}
		Ok(())
	}
}

impl<const S: usize, const G: usize> fmt::Debug for World<S, G> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let session = &self.session;
		write!(
			f,
			"opened={} freed={} resets={} arrivals={}",
			session.opened, session.freed, session.resets, session.arrivals
		)?;
		for s in 0..S {
			let sub = &session.subs[s];
			write!(f, " | s{}:", s + 1)?;
			for k in 1..=G as u8 {
				write!(f, " {k}={:?}", self.st(s, k))?;
			}
			if !self.awake[s] {
				write!(f, " parked")?;
			}
			if sub.asked {
				write!(f, " asked")?;
			}
			if let Some(place) = sub.place {
				write!(f, " place={}", place.arrival)?;
			}
			write!(f, " turn={}", sub.turn)?;
		}
		Ok(())
	}
}

/// `SendCredit.tla`'s configs: two subscriptions of three groups.
pub mod configs {
	use super::*;

	/// The world each config checks.
	pub type Credit = World<2, 3>;

	fn shape(high: [bool; 2], ascending: [bool; 2], max_streams: u8, mutant: Option<Mutant>) -> Shape<2> {
		Shape {
			priority: [u8::from(high[0]), u8::from(high[1])],
			ascending,
			max_streams,
			mutant,
		}
	}

	/// `SendCredit.cfg`: one newest first and one oldest first, over one stream of credit.
	pub fn base(mutant: Option<Mutant>) -> Credit {
		World::new(shape([false, false], [false, true], 1, mutant), [None, None])
	}

	/// `SendCreditLive.cfg`: as `base`, each subscription's group 2 never finishing.
	pub fn live(mutant: Option<Mutant>) -> Credit {
		World::new(shape([false, false], [false, true], 1, mutant), [Some(2), Some(2)])
	}

	/// `SendCreditPriority.cfg`: as `base`, the newest-first subscription at a higher priority.
	pub fn priority(mutant: Option<Mutant>) -> Credit {
		World::new(shape([true, false], [false, true], 1, mutant), [None, None])
	}

	/// `SendCreditStreams.cfg`: as `base` over two streams of credit, the orders swapped.
	pub fn streams(mutant: Option<Mutant>) -> Credit {
		World::new(shape([false, false], [true, false], 2, mutant), [None, None])
	}
}

#[cfg(test)]
mod tests {
	use super::configs::*;
	use super::*;
	use crate::check::{check, find};

	/// Each config visits exactly the states TLC finds in `SendCredit.tla` (TLC 1.7.4's
	/// distinct states), so the machine and its environment are the model, step for step.
	#[test]
	fn matches_tlc() {
		for (name, world, tlc) in [
			("SendCredit", base(None), 89_676),
			("SendCreditLive", live(None), 41_622),
			("SendCreditPriority", priority(None), 99_058),
			("SendCreditStreams", streams(None), 296_373),
		] {
			let stats = check(world).unwrap_or_else(|violation| panic!("{name}: {violation:?}"));
			assert_eq!(stats.distinct, tlc, "{name}: {stats:?}");
		}
	}

	/// Each witness, expected to fail in TLC, is reached.
	#[test]
	fn witnesses() {
		let preempts = |w: &Credit, _| (0..2).any(|s| w.groups[s].contains(&Group::Reset));
		let waits = |w: &Credit, _| w.session.subs.iter().any(|sub| sub.place.is_some());
		let gives_way = |w: &Credit, _| (0..2).any(|s| w.groups[s].contains(&Group::Given));
		let blocked = |w: &Credit, quiet| quiet && w.session.subs.iter().any(|sub| sub.opening.is_some());
		assert!(find(priority(None), preempts).is_some(), "NeverPreempts");
		assert!(find(base(None), waits).is_some(), "NeverWaits");
		assert!(find(base(None), gives_way).is_some(), "NeverGivesWay");
		assert!(find(live(None), blocked).is_some(), "NeverBlocked");
	}

	/// Each injected regression breaks a rule, with a shortest trace.
	#[test]
	fn mutants() {
		for (mutant, world, rule) in [
			(Mutant::NoGiveWay, live(Some(Mutant::NoGiveWay)), "TopDelivered"),
			(Mutant::NoPreemption, live(Some(Mutant::NoPreemption)), "Ends"),
			(Mutant::NoLeaveWake, streams(Some(Mutant::NoLeaveWake)), "Ends"),
		] {
			let violation = check(world).expect_err(&format!("{mutant:?} passes"));
			assert_eq!(violation.rule, rule, "{mutant:?}: {violation:?}");
		}
	}
}
