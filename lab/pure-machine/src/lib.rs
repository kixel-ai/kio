//! Spike: control-plane logic as a pure, synchronous state machine, checked three ways.
//!
//! Each machine is a [`kio::model::Machine`]: a `Clone + Eq + Hash` value, advanced one
//! event at a time by [`Machine::step`](kio::model::Machine::step), which appends what the
//! machine asks of its environment to an output list and never blocks, sleeps, or reads a
//! clock. That makes the same code:
//!
//! - drivable from `kio` tasks ([`driver`]), where outputs become wakes and stream opens;
//! - explorable by [`kio::model::check`]'s breadth-first model checker, composed with an
//!   abstract environment ([`send_credit::world`]), which visits every reachable state once
//!   and prints the shortest trace to a broken rule;
//! - provable by Kani's bounded model checker (`cfg(kani)` harnesses, `kani.sh`) and by
//!   Lean 4 after Charon and Aeneas translate it (`proofs/`, `prove.sh`).
//!
//! [`send_credit`] is moq-net's wait for stream credit, as `SendCredit.tla` models it, and
//! [`warm_settle`] is the relay's choice of where to pull a broadcast from, as
//! `WarmSettle.tla` models it, kept small for the Lean comparison. Their checks are
//! `kio::model`'s differential test: every `SendCredit.tla` config visits exactly TLC's
//! distinct states. The `check` example times each config, and `cost` counts what the
//! driver adds over a direct step.

pub mod driver;
pub mod send_credit;
pub mod warm_settle;
