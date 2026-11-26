//! The thin `kio` shell a [`Session`] runs in: one lock around the machine and its
//! [`Transport`], a waiter list per subscription task, and the outputs turned into wakes and
//! stream effects.
//!
//! Each subscription's task calls [`Driver::poll_step`] when woken, as moq-net's
//! `TrackRun::poll_step` does, and the transport reports streams sent whole and freed.
//! Everything the machine decides runs under the lock, as moq-net's `Line` does under its
//! mutex, and so do its stream effects: a pass's `Reset` of another subscription's stream
//! must not reach the transport before that subscription's earlier `Open`. Wakes fire after
//! the lock is released.

use std::sync::Mutex;
use std::task::Poll;

use crate::send_credit::{Event, Output, Session};

/// Where the machine's stream effects go: take, shed, open, or reset.
pub trait Transport {
	/// Carry out `effect`, called under the session's lock in the order the machine decided.
	///
	/// It must not call back into the driver, such as reporting a stream already sent whole
	/// through [`Driver::finished`], which would deadlock on that lock: defer such reports
	/// until `apply` returns.
	fn apply(&mut self, effect: Output);
}

/// A session's send credit, shared by its subscriptions' tasks and its transport.
pub struct Driver<T, const S: usize, const G: usize> {
	state: Mutex<State<T, S, G>>,
}

struct State<T, const S: usize, const G: usize> {
	session: Session<S, G>,
	transport: T,
	waiters: [kio::WaiterList; S],
	out: Vec<Output>,
}

impl<T: Transport, const S: usize, const G: usize> Driver<T, S, G> {
	/// Drive `session`, its effects carried out by `transport`.
	pub fn new(session: Session<S, G>, transport: T) -> Self {
		Self {
			state: Mutex::new(State {
				session,
				transport,
				waiters: std::array::from_fn(|_| kio::WaiterList::new()),
				out: Vec::new(),
			}),
		}
	}

	/// One pass of `sub`'s task, its cursor's cache holding `cached` and handing out `pick`
	/// if admitted: ready once its effects are applied, or pending, `waiter` registered, if
	/// the pass changed nothing.
	///
	/// `pick` is the cached group the cursor's order puts first, whatever the session admits:
	/// admission is decided here, under the lock, so an event between the caller's read and
	/// this pass cannot make the pick stale. In either order, that group is admitted whenever
	/// any cached group is.
	///
	/// # Panics
	///
	/// If `pick` is not cached, or is `None` while the cache holds a group the session admits,
	/// which is the caller's bug.
	pub fn poll_step(&self, sub: usize, cached: u16, pick: Option<u8>, waiter: &kio::Waiter) -> Poll<()> {
		let mut wake = [const { None }; S];
		let parked = {
			let mut state = self.lock();
			let state = &mut *state;
			state.out.clear();
			state
				.session
				.step(Event::Poll { sub, cached, pick }, &mut state.out)
				.expect("the cursor hands out a cached group the session admits");
			let parked = route(state, &mut wake, Some(sub));
			if parked {
				waiter.register(&mut state.waiters[sub]);
			}
			parked
		};
		wake_all(wake);
		if parked { Poll::Pending } else { Poll::Ready(()) }
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
		let mut wake = [const { None }; S];
		{
			let mut state = self.lock();
			let state = &mut *state;
			state.out.clear();
			state
				.session
				.step(event, &mut state.out)
				.expect("the transport reports what happened");
			route(state, &mut wake, None);
		}
		wake_all(wake);
	}

	/// The machine as of now.
	pub fn session(&self) -> Session<S, G> {
		self.lock().session
	}

	/// The state, refused once a pass panicked under the lock: the session may then be ahead
	/// of what its transport applied, by an unknown number of effects.
	fn lock(&self) -> std::sync::MutexGuard<'_, State<T, S, G>> {
		self.state.lock().expect("an earlier pass panicked mid-way")
	}
}

/// Carry out the machine's outputs: stream effects now, under the lock, and waiters to wake
/// once it is released into `wake`. Whether `polled`'s pass parked.
fn route<T: Transport, const S: usize, const G: usize>(
	state: &mut State<T, S, G>,
	wake: &mut [Option<kio::WaiterList>; S],
	polled: Option<usize>,
) -> bool {
	let mut parked = false;
	for output in state.out.drain(..) {
		match output {
			// A second wake of `s` in one pass finds its list taken already: keep the first.
			Output::Wake(s) if Some(s) != polled => {
				if wake[s].is_none() {
					wake[s] = Some(state.waiters[s].take());
				}
			}
			// The polled task is running: it polls again rather than waking itself.
			Output::Wake(_) => {}
			Output::Park(_) => parked = true,
			effect => state.transport.apply(effect),
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

	impl Transport for Vec<Output> {
		fn apply(&mut self, effect: Output) {
			self.push(effect);
		}
	}

	impl<const S: usize, const G: usize> Driver<Vec<Output>, S, G> {
		/// The effects applied since the last call.
		fn applied(&self) -> Vec<Output> {
			std::mem::take(&mut self.lock().transport)
		}
	}

	/// A pass that wakes a task twice still wakes it: with three subscriptions, two opens'
	/// `make_room` can each reset a stream of the third. The two-subscription configs never
	/// emit it, so this routes the outputs by hand.
	#[test]
	fn wakes_twice_once() {
		let driver = Driver::new(configs::base(None).session, Vec::new());
		let count = Arc::new(Count::default());
		let waiter = kio::Waiter::new(count.clone().into());
		let mut wake = [const { None }; 2];
		{
			let mut state = driver.lock();
			let state = &mut *state;
			waiter.register(&mut state.waiters[1]);
			state.out.extend([Output::Wake(1), Output::Wake(1)]);
			route(state, &mut wake, Some(0));
		}
		wake_all(wake);
		assert_eq!(count.0.load(Ordering::Relaxed), 1);
	}

	/// A parked task is woken when the stream holding the credit is freed, and then opens.
	#[test]
	fn wakes_the_parked_open() {
		let driver = Driver::new(configs::base(None).session, Vec::new());
		let counts: [Arc<Count>; 2] = Default::default();
		let waiters = counts.clone().map(|count| kio::Waiter::new(count.into()));

		// Subscription 1 takes group 3 and opens it with the only credit.
		assert!(driver.poll_step(0, 0b111, Some(3), &waiters[0]).is_ready());
		assert_eq!(driver.applied(), vec![Output::Take { sub: 0, group: 3 }]);
		assert!(driver.poll_step(0, 0b011, Some(2), &waiters[0]).is_ready());
		assert_eq!(driver.applied()[0], Output::Open { sub: 0, group: 3 });

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
		driver.applied();
		assert!(driver.poll_step(1, 0b110, Some(2), &waiters[1]).is_ready());
		assert_eq!(driver.applied()[0], Output::Open { sub: 1, group: 1 });
	}

	/// Replay a schedule through the driver, landing where the machine alone does.
	fn replay<T: Transport, const S: usize, const G: usize>(driver: &Driver<T, S, G>, events: &[Event]) {
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

	/// The transport sees every effect, in the machine's order.
	#[test]
	fn replays_like_the_machine() {
		for world in [
			configs::base(None),
			configs::live(None),
			configs::priority(None),
			configs::streams(None),
		] {
			let (end, events) = world.run();
			let driver = Driver::new(world.session, Vec::new());
			replay(&driver, &events);
			assert_eq!(driver.session(), end.session);

			let mut session = world.session;
			let mut effects = Vec::new();
			for &event in &events {
				let mut out = Vec::new();
				session.step(event, &mut out).expect("a recorded step replays");
				effects.extend(
					out.into_iter()
						.filter(|o| !matches!(o, Output::Wake(_) | Output::Park(_))),
				);
			}
			assert_eq!(driver.applied(), effects);
		}
	}
}
