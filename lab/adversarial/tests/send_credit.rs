//! The fixture's checks, in one test so nothing else in this binary reads the environment
//! while it sets the schedule.

use kio_adversarial_lab::{Bug, Fixture, configure, run};

const PCT: &str = "pct:3:200";

fn caught(fixture: Fixture, seeds: u64) -> Vec<(u64, String)> {
	(0..seeds)
		.filter_map(|seed| run(fixture, seed, seed).map(|failure| (seed, failure)))
		.collect()
}

fn under<T>(schedule: &str, probe: bool, check: impl FnOnce() -> T) -> T {
	// SAFETY: this binary's only test runs on one thread.
	unsafe { configure(schedule, probe) };
	check()
}

#[test]
fn send_credit() {
	std::panic::set_hook(Box::new(|_| {}));

	// The model holds every rule under every schedule, and the probe finds no lost wakeup.
	for children in [false, true] {
		let fixture = Fixture {
			children,
			..Fixture::SMALL
		};
		for schedule in ["fifo", "random", PCT] {
			let failures = under(schedule, true, || caught(fixture, 40));
			assert_eq!(failures, [], "{schedule}, children {children}");
		}
	}

	// The probe names a lost wakeup where it happens, under the fixed order too.
	for bug in [Bug::NoHeadWake, Bug::NoFreeWake] {
		for children in [false, true] {
			let fixture = Fixture {
				bug: Some(bug),
				children,
				..Fixture::SMALL
			};
			let failures = under("fifo", true, || caught(fixture, 40));
			assert!(!failures.is_empty(), "{bug:?} slipped by, children {children}");
			for (seed, failure) in failures {
				assert!(failure.starts_with("lost wakeup"), "{bug:?} seed {seed}: {failure}");
				assert!(
					failure.contains("send_credit.rs"),
					"{bug:?} seed {seed} blames no track: {failure}"
				);
			}
		}
	}

	// The fixed order wakes the track that overtook first and so rarely lets the other
	// steal the credit; adversarial schedules catch it on more seeds.
	let fixture = Fixture {
		bug: Some(Bug::StealCredit),
		..Fixture::SMALL
	};
	let fifo = under("fifo", false, || caught(fixture, 40)).len();
	for schedule in ["random", PCT] {
		let failures = under(schedule, false, || caught(fixture, 40));
		assert!(
			failures.len() > fifo,
			"{schedule} caught {} to FIFO's {fifo}",
			failures.len()
		);
		for (seed, failure) in failures {
			assert!(failure.starts_with("line order"), "{schedule} seed {seed}: {failure}");
		}
		// The same seed replays the same schedule, so a failure's seed reproduces it.
		for seed in 0..10 {
			let once = under(schedule, false, || run(fixture, seed, seed));
			assert_eq!(once, run(fixture, seed, seed), "{schedule} seed {seed}");
		}
	}

	let _ = std::panic::take_hook();
}
