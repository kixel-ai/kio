//! Write explored `SendCredit` runs as TLC models: per config, a directory holding the
//! specs, `SendCreditTraces.tla` with each seed's run, `Trace.cfg` to validate the runs
//! against `SendCreditSplit.tla`, and `Split.cfg` to check that spec itself.
//!
//! `cargo run -p kio-reactor-lab --example traces -- <dir> <seeds>`; `just lab-tlc` runs TLC on them.

use std::{fs, path::PathBuf};

use kio_reactor_lab::send_credit::{Config, constants, explore, traces_module};

const SPECS: [(&str, &str); 3] = [
	("SendCredit.tla", include_str!("../verify/SendCredit.tla")),
	("SendCreditSplit.tla", include_str!("../verify/SendCreditSplit.tla")),
	("SendCreditTrace.tla", include_str!("../verify/SendCreditTrace.tla")),
];

fn main() {
	let mut args = std::env::args().skip(1);
	let (Some(dir), Some(seeds), None) = (args.next(), args.next(), args.next()) else {
		panic!("usage: traces <dir> <seeds>");
	};
	let dir = PathBuf::from(dir);
	let seeds: u64 = seeds.parse().expect("seeds is a count");
	// TLC accepts an empty trace suite without checking anything.
	assert!(seeds > 0, "seeds must be at least 1");

	for (name, config) in Config::all() {
		let traces: Vec<Vec<String>> = (0..seeds)
			.map(|seed| explore(config.clone(), seed).unwrap_or_else(|failed| panic!("{name}: {failed}")))
			.collect();
		let states: usize = traces.iter().map(Vec::len).sum();
		let out = dir.join(name);
		fs::create_dir_all(&out).expect("create the model's directory");
		for (file, text) in SPECS {
			fs::write(out.join(file), text).expect("write a spec");
		}
		fs::write(
			out.join("SendCreditTraces.tla"),
			traces_module("SendCreditTraces", &traces),
		)
		.expect("write the traces");
		let constants = constants(&config);
		fs::write(
			out.join("Trace.cfg"),
			format!(
				"{constants}\nSPECIFICATION TraceSpec\n\nINVARIANTS\n    StartsInit\n    Accepted\n    EndsQuiet\n    WithinCredit\n    Ends\n    TopDelivered\n\nCHECK_DEADLOCK FALSE\n"
			),
		)
		.expect("write Trace.cfg");
		fs::write(
			out.join("Split.cfg"),
			format!(
				"{constants}\nSPECIFICATION SplitSpec\n\nINVARIANTS\n    WithinCredit\n    Ends\n    TopDelivered\n    SplitGuarded\n\nPROPERTIES\n    Progress\n"
			),
		)
		.expect("write Split.cfg");
		println!("{name}: {seeds} runs, {states} states");
	}
}
