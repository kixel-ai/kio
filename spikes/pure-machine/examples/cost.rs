//! What the `kio` driver shell costs over stepping the machine directly: replay one
//! schedule of each `SendCredit.tla` config `ITERS` times, either way.
//!
//! `cargo build --release -p kio-pure-machine --example cost`, then count instructions with
//! `perf stat -e instructions:u target/release/examples/cost <none|step|driver> <iters>`;
//! `none` builds the schedules and stops, the baseline to subtract.

use std::hint::black_box;
use std::task::Poll;

use kio_pure_machine::driver::Driver;
use kio_pure_machine::send_credit::world::configs;
use kio_pure_machine::send_credit::{Event, Session};

fn main() {
	let mut args = std::env::args().skip(1);
	let mode = args.next().expect("a mode: none, step, or driver");
	let iters: usize = args.next().map_or(100_000, |n| n.parse().expect("an iteration count"));

	let runs: Vec<_> = [configs::base, configs::live, configs::priority, configs::streams]
		.into_iter()
		.map(|config| {
			let world = config(None);
			(world.session, world.run().1)
		})
		.collect();
	let events: usize = runs.iter().map(|(_, events)| events.len()).sum();

	let waiter = kio::Waiter::noop();
	let mut out = Vec::with_capacity(16);
	for _ in 0..iters {
		for (init, events) in &runs {
			match mode.as_str() {
				"none" => {}
				"step" => {
					let mut session: Session<2, 3> = *init;
					for &event in events {
						out.clear();
						session
							.step(black_box(event), &mut out)
							.expect("a recorded step replays");
					}
					black_box(&session);
				}
				"driver" => {
					let driver = Driver::new(*init);
					for &event in events {
						match black_box(event) {
							Event::Poll { sub, cached, pick } => {
								if let Poll::Ready(effects) = driver.poll_step(sub, cached, pick, &waiter) {
									black_box(effects);
								}
							}
							Event::Finished { sub, group } => driver.finished(sub, group),
							Event::Freed => driver.freed(),
						}
					}
					black_box(&driver);
				}
				_ => panic!("unknown mode {mode}"),
			}
		}
	}
	eprintln!("{mode}: {iters} x {events} events");
}
