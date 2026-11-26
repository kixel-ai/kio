//! What the `kio` driver shell costs over stepping the machine directly: replay one
//! schedule of each `SendCredit.tla` config `ITERS` times, either way.
//!
//! `cargo build --release -p kio-pure-machine --example cost`, then count instructions with
//! `perf stat -e instructions:u target/release/examples/cost <none|step|driver> <iters>`;
//! `none` builds the schedules and stops, the baseline to subtract.

use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Wake;

use kio_pure_machine::driver::{Driver, Transport};
use kio_pure_machine::send_credit::world::configs;
use kio_pure_machine::send_credit::{Event, Output, Session};

/// A task waker that only counts, so parks register and wakes call a real waker.
#[derive(Default)]
struct Count(AtomicUsize);

impl Wake for Count {
	fn wake(self: Arc<Self>) {
		self.0.fetch_add(1, Ordering::Relaxed);
	}
}

/// A transport that only looks at each effect.
struct Sink;

impl Transport for Sink {
	fn apply(&mut self, effect: Output) {
		black_box(effect);
	}
}

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

	let counts: [Arc<Count>; 2] = Default::default();
	let waiters = counts.clone().map(|count| kio::Waiter::new(count.into()));
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
					let driver = Driver::new(*init, Sink);
					for &event in events {
						match black_box(event) {
							Event::Poll { sub, cached, pick } => {
								let _ = black_box(driver.poll_step(sub, cached, pick, &waiters[sub]));
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
	let woken: usize = counts.iter().map(|count| count.0.load(Ordering::Relaxed)).sum();
	eprintln!("{mode}: {iters} x {events} events, {woken} wakes");
}
