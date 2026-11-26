//! Kani harnesses: one step from any well-formed session, not only a reachable one.
//!
//! Run with `lab/pure-machine/kani.sh`, which installs Kani on first use.

use super::*;

/// Two subscriptions of three groups, as `SendCredit.tla`'s configs have.
type Credit = Session<2, 3>;

/// Any well-formed session within its credit.
fn any_session() -> Credit {
	let session: Credit = kani::any();
	kani::assume(session.well_formed() && session.within_credit());
	// The counters only grow, by one a step: u8 suits the model, and production's u64 never
	// runs out.
	kani::assume(session.arrivals < u8::MAX && session.opened < u8::MAX);
	session
}

/// `event` either is refused or leaves `session` well formed and within its credit,
/// without a panic.
fn keeps_invariant(mut session: Credit, event: Event) {
	let mut out = Vec::new();
	if session.step(event, &mut out).is_ok() {
		assert!(session.within_credit(), "WithinCredit");
		assert!(session.well_formed(), "WellFormed");
	}
}

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(kissat)]
fn poll_keeps_invariant() {
	let sub: usize = kani::any();
	kani::assume(sub < 2);
	keeps_invariant(
		any_session(),
		Event::Poll {
			sub,
			cached: kani::any(),
			pick: kani::any(),
		},
	);
}

#[kani::proof]
#[kani::unwind(4)]
fn finished_keeps_invariant() {
	keeps_invariant(
		any_session(),
		Event::Finished {
			sub: kani::any(),
			group: kani::any(),
		},
	);
}

#[kani::proof]
#[kani::unwind(4)]
fn freed_keeps_invariant() {
	keeps_invariant(any_session(), Event::Freed);
}
