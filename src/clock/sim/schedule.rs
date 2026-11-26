//! Which ready task the simulator runs next, and which woken [`Tasks`](crate::Tasks)
//! children a pass runs and in what order: the interleavings a run explores.
//!
//! Experimental: chosen per run from `KIO_SIM_SCHEDULE`, until the state-machine spikes
//! settle whether this stays.
//!
//! - `fifo` (the default): wake order, and a set's children in its own slot order.
//! - `random`: a uniform pick among the ready tasks; a pass shuffles its children and may
//!   leave the rest of them for the owner's next poll, so other tasks run in between.
//! - `pct[:<depth>[:<steps>]]`: probabilistic concurrency testing (Burckhardt et al.,
//!   ASPLOS 2010). Every task and child draws a random priority and the highest ready
//!   one runs; at `depth - 1` steps drawn from the first `steps`, the one running drops
//!   below every initial priority. Defaults to depth 3 over 1000 steps.
//!
//! A unit that wakes itself while it runs yields: under `pct` it drops below everything,
//! so a spin loop cannot starve the rest the way a fixed priority would let it. That
//! demotion voids the paper's bound (a bug needing `depth` ordering constraints among `n`
//! tasks and `steps` steps found with probability at least `1 / (n * steps^(depth - 1))`):
//! an order that needs a self-waking unit to run twice in a row is never drawn.
//!
//! Draws come from their own generator, seeded from the run's seed, so a schedule
//! replays bit-exact from its seed and leaves [`rng`](crate::clock::rng) draws unchanged.

use std::{cell::RefCell, rc::Rc};

use rand::{RngExt, SeedableRng, rngs::StdRng};

/// The order a run explores.
#[derive(Clone, Debug)]
pub(crate) enum Policy {
	Fifo,
	Random,
	Pct {
		depth: u32,
		steps: u64,
	},
	/// Takes each choice from `prefix`, the first option once it runs out, and records
	/// every choice with more than one option, for an explorer to branch from.
	#[cfg_attr(not(test), expect(dead_code, reason = "only the explorer replays"))]
	Replay {
		prefix: Vec<u32>,
		trace: Rc<RefCell<Vec<Choice>>>,
	},
}

/// One choice a replayed run made: the option it took, of how many.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
	pub taken: u32,
	pub options: u32,
}

/// How a run schedules, and whether it hunts lost wakeups.
#[derive(Clone, Debug)]
pub(crate) struct Config {
	pub policy: Policy,
	/// At every stall, re-poll each parked task and child before time moves, and panic
	/// if one makes progress, since nothing woke it.
	pub probe: bool,
}

/// Wake order, without probing.
impl Default for Config {
	fn default() -> Self {
		Self {
			policy: Policy::Fifo,
			probe: false,
		}
	}
}

impl Config {
	/// The config `KIO_SIM_SCHEDULE` and `KIO_SIM_PROBE` name: FIFO without probing when unset.
	pub fn from_env() -> Self {
		let policy = match std::env::var("KIO_SIM_SCHEDULE") {
			Ok(policy) => Policy::parse(&policy).unwrap_or_else(|| {
				panic!("KIO_SIM_SCHEDULE must be fifo, random, or pct[:<depth>[:<steps>]], not {policy:?}")
			}),
			Err(std::env::VarError::NotPresent) => Policy::Fifo,
			Err(err) => panic!("KIO_SIM_SCHEDULE: {err}"),
		};
		let probe = match std::env::var("KIO_SIM_PROBE").as_deref() {
			Ok("1") => true,
			Ok("0") | Err(std::env::VarError::NotPresent) => false,
			Ok(other) => panic!("KIO_SIM_PROBE must be 0 or 1, not {other:?}"),
			Err(err) => panic!("KIO_SIM_PROBE: {err}"),
		};
		Self { policy, probe }
	}
}

/// The environment settings beyond the seed that replay a run, each with a leading space.
impl std::fmt::Display for Config {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match &self.policy {
			Policy::Fifo => {}
			Policy::Random => f.write_str(" KIO_SIM_SCHEDULE=random")?,
			Policy::Pct { depth, steps } => write!(f, " KIO_SIM_SCHEDULE=pct:{depth}:{steps}")?,
			Policy::Replay { .. } => f.write_str(" (a replayed schedule)")?,
		}
		if self.probe {
			f.write_str(" KIO_SIM_PROBE=1")?;
		}
		Ok(())
	}
}

impl Policy {
	fn parse(policy: &str) -> Option<Self> {
		let mut parts = policy.split(':');
		let policy = match parts.next()? {
			"fifo" => Self::Fifo,
			"random" => Self::Random,
			"pct" => {
				let depth = parts.next().map_or(Some(3), |depth| depth.parse().ok())?;
				let steps = parts.next().map_or(Some(1000), |steps| steps.parse().ok())?;
				if depth == 0 || steps == 0 {
					return None;
				}
				Self::Pct { depth, steps }
			}
			_ => return None,
		};
		parts.next().is_none().then_some(policy)
	}
}

/// A schedulable unit's rank under `pct`; higher runs first.
pub(crate) type Priority = i64;

/// Draws a run's scheduling choices.
pub(crate) struct Scheduler {
	policy: Policy,
	rng: StdRng,
	/// Units run so far, counting each task poll and each child poll.
	steps: u64,
	/// The `pct` change points still ahead, latest first, with the priority each sets.
	changes: Vec<(u64, Priority)>,
	/// The lowest priority given out, so a yield goes below it.
	lowest: Priority,
	cursor: usize,
}

/// Mixed into the run's seed, so the schedule's draws are not the run's own.
const SALT: u64 = 0x9e37_79b9_7f4a_7c15;

impl Scheduler {
	pub fn new(policy: Policy, seed: u64) -> Self {
		let mut rng = StdRng::seed_from_u64(seed ^ SALT);
		let mut changes = Vec::new();
		if let Policy::Pct { depth, steps } = policy {
			// Change point `i` (from 1) sets priority `depth - i`, below every initial one.
			changes = (1..depth)
				.map(|i| (rng.random_range(0..steps), Priority::from(depth - i)))
				.collect();
			changes.sort_unstable_by(|a, b| b.cmp(a));
		}
		Self {
			policy,
			rng,
			steps: 0,
			changes,
			lowest: 0,
			cursor: 0,
		}
	}

	/// Whether the run keeps the fixed wake and slot order.
	pub fn is_fifo(&self) -> bool {
		matches!(self.policy, Policy::Fifo)
	}

	/// A new unit's priority, under `pct`, the one policy that ranks units.
	pub fn priority(&mut self) -> Option<Priority> {
		match self.policy {
			Policy::Pct { depth, .. } => Some(Priority::from(depth) + Priority::from(self.rng.random::<u32>())),
			_ => None,
		}
	}

	/// Pick one of the ready units, given in wake order with their priorities.
	pub fn pick(&mut self, ready: &[Priority]) -> usize {
		debug_assert!(!ready.is_empty());
		match self.policy {
			Policy::Fifo => 0,
			Policy::Random => self.rng.random_range(0..ready.len()),
			// The first of the highest, so ties go in wake order.
			Policy::Pct { .. } => {
				let best = ready.iter().copied().max().expect("a ready unit");
				ready
					.iter()
					.position(|&priority| priority == best)
					.expect("the best is ready")
			}
			Policy::Replay { .. } => self.replay(ready.len()),
		}
	}

	/// Pick the next woken child of a pass, or `None` to leave the rest for the owner's
	/// next poll so another ready task may run first. `other` is the best priority among
	/// the other ready tasks; a pass that ran nothing yet always runs one.
	pub fn pick_child(&mut self, woken: &[Priority], ran: bool, other: Option<Priority>) -> Option<usize> {
		match self.policy {
			Policy::Random if ran && self.rng.random_bool(0.25) => None,
			Policy::Pct { .. } => {
				let index = self.pick(woken);
				match other {
					Some(other) if ran && other > woken[index] => None,
					_ => Some(index),
				}
			}
			_ => Some(self.pick(woken)),
		}
	}

	/// Count a step by the unit at `priority`, which woke itself while it ran when `yielded`.
	pub fn ran(&mut self, priority: &mut Priority, yielded: bool) {
		self.steps += 1;
		if !matches!(self.policy, Policy::Pct { .. }) {
			return;
		}
		if yielded {
			self.lowest -= 1;
			*priority = self.lowest;
		}
		while self.changes.last().is_some_and(|&(step, _)| step < self.steps) {
			let (_, changed) = self.changes.pop().expect("a change point");
			*priority = (*priority).min(changed);
		}
	}

	fn replay(&mut self, options: usize) -> usize {
		let Policy::Replay { prefix, trace } = &self.policy else {
			unreachable!("replay outside a replayed run")
		};
		if options < 2 {
			return 0;
		}
		let options = u32::try_from(options).expect("fewer than 2^32 ready units");
		let taken = prefix.get(self.cursor).copied().unwrap_or(0).min(options - 1);
		self.cursor += 1;
		trace.borrow_mut().push(Choice { taken, options });
		taken as usize
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn policies_parse_strictly() {
		assert!(matches!(Policy::parse("fifo"), Some(Policy::Fifo)));
		assert!(matches!(Policy::parse("random"), Some(Policy::Random)));
		assert!(matches!(
			Policy::parse("pct"),
			Some(Policy::Pct { depth: 3, steps: 1000 })
		));
		assert!(matches!(
			Policy::parse("pct:2:50"),
			Some(Policy::Pct { depth: 2, steps: 50 })
		));
		for bad in ["", "FIFO", "pct:0", "pct:2:0", "pct:x", "random:1", "pct:1:2:3"] {
			assert!(Policy::parse(bad).is_none(), "{bad:?} parsed");
		}
	}

	/// A change point drops the running unit below every initial priority, and a yield
	/// drops it below everything given out so far.
	#[test]
	fn pct_change_points_and_yields_demote() {
		let mut scheduler = Scheduler::new(Policy::Pct { depth: 2, steps: 1 }, 7);
		let mut first = scheduler.priority().unwrap();
		let second = scheduler.priority().unwrap();
		assert!(first >= 2 && second >= 2);
		// The one change point is at step 0, so the first step lands on it.
		scheduler.ran(&mut first, false);
		assert_eq!(first, 1);
		let mut third = scheduler.priority().unwrap();
		scheduler.ran(&mut third, true);
		assert!(third < first, "a yield goes below the change point");
	}

	#[test]
	fn replay_follows_its_prefix_then_the_first_option() {
		let trace = Rc::new(RefCell::new(Vec::new()));
		let policy = Policy::Replay {
			prefix: vec![2, 5],
			trace: trace.clone(),
		};
		let mut scheduler = Scheduler::new(policy, 0);
		assert_eq!(scheduler.pick(&[0, 0, 0]), 2);
		assert_eq!(scheduler.pick(&[0]), 0, "one option is no choice");
		assert_eq!(scheduler.pick(&[0, 0]), 1, "clamped to the options");
		assert_eq!(scheduler.pick(&[0, 0]), 0);
		assert_eq!(
			*trace.borrow(),
			[
				Choice { taken: 2, options: 3 },
				Choice { taken: 1, options: 2 },
				Choice { taken: 0, options: 2 }
			]
		);
	}
}
