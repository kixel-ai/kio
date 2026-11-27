//! Check every `SendCredit.tla` config and mutant, printing what each covered and how fast.
//!
//! `cargo run --release -p kio-pure-machine --example check [config|mutant]`

use std::time::Instant;

use kio::model::check;
use kio_pure_machine::send_credit::Mutant;
use kio_pure_machine::send_credit::world::configs::{self, Credit};

/// A config, by name, with a regression to inject if any.
type Run = (&'static str, fn(Option<Mutant>) -> Credit, Option<Mutant>);

fn main() {
	let only = std::env::args().nth(1);
	let runs: [Run; 7] = [
		("SendCredit", configs::base, None),
		("SendCreditLive", configs::live, None),
		("SendCreditPriority", configs::priority, None),
		("SendCreditStreams", configs::streams, None),
		("NoGiveWay", configs::live, Some(Mutant::NoGiveWay)),
		("NoPreemption", configs::live, Some(Mutant::NoPreemption)),
		("NoLeaveWake", configs::streams, Some(Mutant::NoLeaveWake)),
	];
	if let Some(only) = &only
		&& !runs.iter().any(|(name, ..)| name == only)
	{
		let names: Vec<_> = runs.iter().map(|(name, ..)| *name).collect();
		eprintln!("unknown config or mutant {only}; one of {}", names.join(", "));
		std::process::exit(2);
	}
	for (name, config, mutant) in runs {
		if only.as_deref().is_some_and(|only| only != name) {
			continue;
		}
		#[expect(
			clippy::disallowed_methods,
			reason = "timing the checker on the host, not a kio clock"
		)]
		let start = Instant::now();
		let result = check(config(mutant));
		#[expect(
			clippy::disallowed_methods,
			reason = "timing the checker on the host, not a kio clock"
		)]
		let elapsed = start.elapsed();
		match result {
			Ok(stats) => println!(
				"{name}: {} distinct, {} generated, depth {}, peak queue {}, {:.3}s, {:.0} distinct/s",
				stats.distinct,
				stats.generated,
				stats.depth,
				stats.queue,
				elapsed.as_secs_f64(),
				stats.distinct as f64 / elapsed.as_secs_f64(),
			),
			Err(violation) => println!("{name}: {:.3}s, {violation:?}", elapsed.as_secs_f64()),
		}
	}
}
