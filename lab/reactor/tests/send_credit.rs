//! `SendCredit` as `#[reactor]`s: the priority order and the explorer agree on a correct
//! session, and the explorer names the action behind each injected bug.

use kio_reactor_lab::{
	explore::Failure,
	send_credit::{Bug, Config, Driver, System, drive, explore},
};

/// Seeds per config: enough for every injected bug to surface within the first few dozen.
const SEEDS: u64 = 300;

#[test]
fn every_order_stays_correct() {
	for (name, config) in Config::all() {
		for seed in 0..SEEDS {
			if let Err(failed) = explore(config.clone(), seed) {
				panic!("{name}: {failed}");
			}
		}
	}
}

#[test]
fn the_priority_order_reaches_quiet() {
	for (_, config) in Config::all() {
		for seed in 0..SEEDS {
			for driver in [Driver::Reactor, Driver::Hand] {
				let mut system = System::new(config.clone(), seed);
				drive(&mut system, driver);
				// Nothing is left mid-send: every stream ended, or is a live group's.
				for (sub, row) in config.subs.iter().zip(system.states()) {
					for (k, st) in (1..).zip(row) {
						let live = sub.live == Some(k);
						assert!(
							!matches!(st, "fin" | "reset") && (st != "serving" || live),
							"{driver:?} left group {k} {st}"
						);
					}
				}
			}
		}
	}
}

/// The first failure of `bug` across the configs and seeds, printed.
fn first_failure(bug: Bug) -> Failure {
	for (name, config) in Config::all() {
		for seed in 0..SEEDS {
			if let Err(failed) = explore(config.clone().with(bug), seed) {
				println!("{name}: {failed}");
				return failed.failure;
			}
		}
	}
	panic!("no seed caught {bug:?}");
}

#[test]
fn catches_a_missing_wake_head() {
	let failure = first_failure(Bug::SkipWakeHead);
	assert!(
		matches!(failure, Failure::LostWakeup { action: "open", .. }),
		"{failure:?}"
	);
}

#[test]
fn catches_give_way_relying_on_priority() {
	let failure = first_failure(Bug::GiveWayAssumesPlaced);
	assert!(
		matches!(failure, Failure::Panic { action: "hand_out", .. }),
		"{failure:?}"
	);

	// The priority order never reaches it: `open` always places the group first.
	for (_, config) in Config::all() {
		for seed in 0..SEEDS {
			let mut system = System::new(config.clone().with(Bug::GiveWayAssumesPlaced), seed);
			drive(&mut system, Driver::Reactor);
		}
	}
}
