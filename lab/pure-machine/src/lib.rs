//! Spike: control-plane logic as a pure, synchronous state machine, checked three ways.
//!
//! A [`StateMachine`] is a value: `Clone + Eq + Hash`, advanced one event at a time by
//! [`StateMachine::step`], which appends what the machine asks of its environment to an
//! output list and never blocks, sleeps, or reads a clock. That makes the same code:
//!
//! - drivable from `kio` tasks ([`driver`]), where outputs become wakes and stream opens;
//! - explorable by [`check`]'s breadth-first model checker, composed with an abstract
//!   environment ([`send_credit::world`]), which visits every reachable state once and
//!   prints the shortest trace to a broken rule;
//! - provable by Kani's bounded model checker (`cfg(kani)` harnesses, `kani.sh`) and by
//!   Lean 4 after Charon and Aeneas translate it (`proofs/`, `prove.sh`).
//!
//! [`send_credit`] is moq-net's wait for stream credit, as `SendCredit.tla` models it, and
//! [`warm_settle`] is the relay's choice of where to pull a broadcast from, as
//! `WarmSettle.tla` models it, kept small for the Lean comparison. The `check` example
//! times each config, and `cost` counts what the driver adds over a direct step.

pub mod check;
pub mod driver;
pub mod send_credit;
pub mod warm_settle;

/// A pure state machine: a value advanced one event at a time.
pub trait StateMachine: Clone + Eq + std::hash::Hash {
	/// What the environment tells the machine.
	type Event: Clone + std::fmt::Debug;
	/// What the machine asks of the environment.
	type Output: std::fmt::Debug;

	/// Apply `event`, appending the machine's requests to `out`, or refuse it, leaving the
	/// machine and `out` as they were, where it cannot happen.
	fn step(&mut self, event: Self::Event, out: &mut Vec<Self::Output>) -> Result<(), Disabled>;

	/// Check the rules every reachable state keeps, naming the first one broken.
	fn invariant(&self) -> Result<(), &'static str> {
		Ok(())
	}
}

/// An event that cannot happen in the state it was offered to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Disabled;
