//! The spike's numbers as a Markdown table: each bug's catch rate, first catching seed, and
//! cost per seed under each schedule, with and without the probe.
//!
//! `cargo run --release -p kio-adversarial-lab --example report`

use std::collections::BTreeMap;

use kio_adversarial_lab::{Bug, Fixture, configure, run};

#[expect(clippy::disallowed_methods, reason = "measures the host time each seed costs")]
fn main() {
	std::panic::set_hook(Box::new(|_| {}));
	const SEEDS: u64 = 1000;
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
			for schedule in ["fifo", "random", "pct:3:200"] {
				for probe in [false, true] {
					// SAFETY: this binary is single-threaded.
					unsafe { configure(schedule, probe) };
					let fixture = Fixture { bug, children, ..base };
					let mut kinds = BTreeMap::<String, usize>::new();
					let mut first = None;
					let start = std::time::Instant::now();
					for seed in 0..SEEDS {
						if let Some(failure) = run(fixture, seed, seed) {
							// A clean session must pass under every schedule.
							assert!(bug.is_some(), "a clean session failed under {schedule}: {failure}");
							first.get_or_insert(seed);
							// What caught it: the message up to the first colon.
							let kind = failure.split(':').next().unwrap_or(&failure).to_string();
							*kinds.entry(kind).or_default() += 1;
						}
					}
					let micros = start.elapsed().as_micros() / u128::from(SEEDS);
					let caught: usize = kinds.values().sum();
					let first = first.map_or("-".to_string(), |seed| seed.to_string());
					let kinds: Vec<String> = kinds.iter().map(|(kind, n)| format!("{kind} {n}")).collect();
					let drive = if children { "children" } else { "tasks" };
					println!(
						"| {} | {bug:?} | {drive} | {schedule} | {probe} | {caught} | {first} | {micros} | {} |",
						base.max_streams,
						kinds.join(", ")
					);
				}
			}
		}
	}
	let _ = std::panic::take_hook();
}
