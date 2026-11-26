//! Drive many `SendCredit` sessions to quiet in priority order, for counting instructions
//! under `perf stat`: `cost reactor|hand|setup <runs>`.
//!
//! A binary rather than an example, so it builds without the `sim` feature, as production
//! code would. `setup` only builds the sessions, for subtracting.

use std::hint::black_box;

use kio_lab::send_credit::{Config, Driver, System, drive};

fn main() {
	let mut args = std::env::args().skip(1);
	let (Some(mode), Some(runs), None) = (args.next(), args.next(), args.next()) else {
		panic!("usage: cost reactor|hand|setup <runs>");
	};
	let driver = match mode.as_str() {
		"reactor" => Some(Driver::Reactor),
		"hand" => Some(Driver::Hand),
		"setup" => None,
		other => panic!("unknown mode {other:?}"),
	};
	let runs: u64 = runs.parse().expect("runs is a count");

	let mut fired = 0;
	for run in 0..runs {
		for (_, config) in Config::all() {
			let mut system = System::new(config, run % 64);
			if let Some(driver) = driver {
				fired += drive(&mut system, driver);
			}
			black_box(&system);
		}
	}
	println!("{mode}: {fired} actions fired");
}
