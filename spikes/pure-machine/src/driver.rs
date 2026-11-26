//! The thin `kio` shell a [`Session`] runs in: one lock around the machine, a waiter list
//! per subscription task, and the outputs turned into wakes and returned effects.
//!
//! Each subscription's task calls [`Driver::poll_step`] when woken, as moq-net's
//! `TrackRun::poll_step` does, and the transport reports streams sent whole and freed.
//! Everything the machine decides runs under the lock, as moq-net's `Line` does under its
//! mutex; wakes fire after it is released.

use std::sync::Mutex;
use std::task::Poll;

use crate::send_credit::{Event, Output, Session};

/// A session's send credit, shared by its subscriptions' tasks and its transport.
pub struct Driver<const S: usize, const G: usize> {
	state: Mutex<State<S, G>>,
}

struct State<const S: usize, const G: usize> {
	session: Session<S, G>,
	waiters: [kio::WaiterList; S],
	out: Vec<Output>,
}

/// What a pass asks its task to do: take, shed, open, or reset streams.
pub type Effects = Vec<Output>;

impl<const S: usize, const G: usize> Driver<S, G> {
	/// Drive `session`.
	pub fn new(session: Session<S, G>) -> Self {
		Self {
			state: Mutex::new(State {
				session,
				waiters: std::array::from_fn(|_| kio::WaiterList::new()),
				out: Vec::new(),
			}),
		}
	}

	/// One pass of `sub`'s task, its cursor's cache holding `cached` and handing out `pick`
	/// if admitted: the effects to carry out, or pending, `waiter` registered, if the pass
	/// changed nothing.
	///
	/// # Panics
	///
	/// If `pick` is not the cursor's to hand out, which is the caller's bug.
	pub fn poll_step(&self, sub: usize, cached: u16, pick: Option<u8>, waiter: &kio::Waiter) -> Poll<Effects> {
		let mut effects = Vec::new();
		let mut wake = [const { None }; S];
		let parked = {
			let mut state = self.lock();
			let state = &mut *state;
			state.out.clear();
			state
				.session
				.step(Event::Poll { sub, cached, pick }, &mut state.out)
				.expect("the cursor hands out a cached group the session admits");
			let parked = route(state, &mut effects, &mut wake, Some(sub));
			if parked {
				waiter.register(&mut state.waiters[sub]);
			}
			parked
		};
		wake_all(wake);
		if parked { Poll::Pending } else { Poll::Ready(effects) }
	}

	/// The transport sent `sub`'s stream of `group` whole.
	pub fn finished(&self, sub: usize, group: u8) {
		self.transport(Event::Finished { sub, group });
	}

	/// The peer freed a stream.
	pub fn freed(&self) {
		self.transport(Event::Freed);
	}

	fn transport(&self, event: Event) {
		let mut effects = Vec::new();
		let mut wake = [const { None }; S];
		{
			let mut state = self.lock();
			let state = &mut *state;
			state.out.clear();
			state
				.session
				.step(event, &mut state.out)
				.expect("the transport reports what happened");
			route(state, &mut effects, &mut wake, None);
		}
		debug_assert!(effects.is_empty(), "the transport's events only wake");
		wake_all(wake);
	}

	/// The machine as of now.
	pub fn session(&self) -> Session<S, G> {
		self.lock().session
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, State<S, G>> {
		self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}
}

/// Sort the machine's outputs: the waiters to wake once the lock is released move to
/// `wake`, other effects to `effects`. Whether `polled`'s pass parked.
fn route<const S: usize, const G: usize>(
	state: &mut State<S, G>,
	effects: &mut Effects,
	wake: &mut [Option<kio::WaiterList>; S],
	polled: Option<usize>,
) -> bool {
	let mut parked = false;
	for output in state.out.drain(..) {
		match output {
			Output::Wake(s) if Some(s) != polled => wake[s] = Some(state.waiters[s].take()),
			// The polled task is running: it polls again rather than waking itself.
			Output::Wake(_) => {}
			Output::Park(_) => parked = true,
			other => effects.push(other),
		}
	}
	parked
}

fn wake_all<const S: usize>(wake: [Option<kio::WaiterList>; S]) {
	for mut list in wake.into_iter().flatten() {
		list.wake();
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use std::sync::atomic::{AtomicUsize, Ordering};
	use std::task::Wake;

	use super::*;
	use crate::send_credit::world::configs;

	#[derive(Default)]
	struct Count(AtomicUsize);

	impl Wake for Count {
		fn wake(self: Arc<Self>) {
			self.0.fetch_add(1, Ordering::Relaxed);
		}
	}

	/// A parked task is woken when the stream holding the credit is freed, and then opens.
	#[test]
	fn wakes_the_parked_open() {
		let driver = Driver::new(configs::base(None).session);
		let counts: [Arc<Count>; 2] = Default::default();
		let waiters = counts.clone().map(|count| kio::Waiter::new(count.into()));

		// Subscription 1 takes group 3 and opens it with the only credit.
		let Poll::Ready(took) = driver.poll_step(0, 0b111, Some(3), &waiters[0]) else {
			panic!("the cursor hands out a group");
		};
		assert_eq!(took, vec![Output::Take { sub: 0, group: 3 }]);
		let Poll::Ready(opened) = driver.poll_step(0, 0b011, Some(2), &waiters[0]) else {
			panic!("the open takes the credit");
		};
		assert_eq!(opened[0], Output::Open { sub: 0, group: 3 });

		// Subscription 2 takes group 1, waits for credit, and parks.
		assert!(driver.poll_step(1, 0b111, Some(1), &waiters[1]).is_ready());
		assert!(
			driver.poll_step(1, 0b110, Some(2), &waiters[1]).is_ready(),
			"joins the line"
		);
		assert!(driver.poll_step(1, 0b110, Some(2), &waiters[1]).is_pending());
		assert_eq!(counts[1].0.load(Ordering::Relaxed), 0);

		driver.finished(0, 3);
		driver.freed();
		assert_eq!(
			counts[1].0.load(Ordering::Relaxed),
			1,
			"the freed stream wakes the open that asked"
		);
		let Poll::Ready(effects) = driver.poll_step(1, 0b110, Some(2), &waiters[1]) else {
			panic!("the woken open takes the credit");
		};
		assert_eq!(effects[0], Output::Open { sub: 1, group: 1 });
	}

	/// Replay a schedule through the driver, landing where the machine alone does.
	pub(crate) fn replay<const S: usize, const G: usize>(driver: &Driver<S, G>, events: &[Event]) {
		let waiter = kio::Waiter::noop();
		for &event in events {
			match event {
				Event::Poll { sub, cached, pick } => {
					let _ = driver.poll_step(sub, cached, pick, &waiter);
				}
				Event::Finished { sub, group } => driver.finished(sub, group),
				Event::Freed => driver.freed(),
			}
		}
	}

	#[test]
	fn replays_like_the_machine() {
		for world in [
			configs::base(None),
			configs::live(None),
			configs::priority(None),
			configs::streams(None),
		] {
			let (end, events) = world.run();
			let driver = Driver::new(world.session);
			replay(&driver, &events);
			assert_eq!(driver.session(), end.session);
		}
	}
}
