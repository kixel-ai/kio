//! Fires a set of reactors' enabled actions in a seeded random order instead of their
//! priority order, checking every invariant after each action and probing for lost wakeups
//! once nothing is left to fire.
//!
//! Each action polls with a waiter of its own, so a wake reaches only the action that
//! registered for it, and an action that fires makes every action of its own reactor ready
//! again, as the reactor's poll loop would. When no action is ready, each parked one is
//! polled once more with [`Waiter::noop`]: one that fires was enabled without being woken.

use std::{
	fmt,
	panic::{AssertUnwindSafe, catch_unwind},
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
	task::{Poll, Wake, Waker},
};

use kio::Waiter;
use rand::{Rng as _, SeedableRng, rngs::StdRng};

/// A reactor's `sim` surface, as `#[reactor]` generates it.
pub trait Reactor {
	/// Every action's name, highest priority first.
	fn actions(&self) -> &'static [&'static str];
	/// Fire the action at `index` in [`Self::actions`] alone, if it is enabled.
	fn poll_action(&mut self, index: usize, waiter: &Waiter) -> Poll<()>;
	/// The first invariant that does not hold.
	fn violated(&self) -> Option<&'static str>;
}

/// Implement [`Reactor`] for a `#[reactor]` type from the items it generates.
#[macro_export]
macro_rules! reactor {
	($ty:ty => $action:ident) => {
		impl $crate::explore::Reactor for $ty {
			fn actions(&self) -> &'static [&'static str] {
				&$action::NAMES
			}
			fn poll_action(&mut self, index: usize, waiter: &::kio::Waiter) -> ::std::task::Poll<()> {
				<$ty>::poll_action(self, $action::ALL[index], waiter)
			}
			fn violated(&self) -> Option<&'static str> {
				<$ty>::violated(self)
			}
		}
	};
}

/// A wake recorded for one action's waiter.
#[derive(Default)]
struct Flag(AtomicBool);

impl Wake for Flag {
	fn wake(self: Arc<Self>) {
		self.0.store(true, Ordering::Relaxed);
	}
}

struct Slot {
	waiter: Waiter,
	woken: Arc<Flag>,
	parked: bool,
}

/// One poll of one action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
	/// The reactor, by its index in the explored set.
	pub reactor: usize,
	/// The action's name.
	pub action: &'static str,
	/// Whether it fired, or parked.
	pub fired: bool,
}

/// What a step of the explorer did.
#[derive(Debug)]
pub enum Step {
	/// One action was polled.
	Polled(Event),
	/// Every action is parked, and none of them is enabled.
	Quiet,
}

/// What went wrong.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
	/// An invariant failed after an action fired.
	Invariant {
		/// The reactor whose invariant failed.
		reactor: String,
		/// The invariant's name.
		invariant: &'static str,
	},
	/// A parked action was enabled, but nothing woke it.
	LostWakeup {
		/// The action's reactor.
		reactor: String,
		/// The action's name.
		action: &'static str,
	},
	/// An action panicked.
	Panic {
		/// The action's reactor.
		reactor: String,
		/// The action's name.
		action: &'static str,
		/// The panic's message.
		message: String,
	},
	/// Actions kept firing past the step bound.
	Endless,
}

/// A failure, with the actions that led to it.
#[derive(Debug)]
pub struct Failed {
	/// What went wrong.
	pub failure: Failure,
	/// The seed that replays it.
	pub seed: u64,
	/// Every poll before it, the action that failed last.
	pub trace: Vec<String>,
}

impl fmt::Display for Failed {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match &self.failure {
			Failure::Invariant { reactor, invariant } => write!(f, "invariant {reactor}.{invariant} broken")?,
			Failure::LostWakeup { reactor, action } => {
				write!(f, "lost wakeup: {reactor}.{action} is enabled, but nothing woke it")?
			}
			Failure::Panic {
				reactor,
				action,
				message,
			} => write!(f, "{reactor}.{action} panicked: {message}")?,
			Failure::Endless => write!(f, "no quiet state within the step bound")?,
		}
		writeln!(f, " (seed {})", self.seed)?;
		for (index, line) in self.trace.iter().enumerate() {
			writeln!(f, "{:4}. {line}", index + 1)?;
		}
		Ok(())
	}
}

/// Fires reactors' actions in a seeded random order.
pub struct Explorer {
	seed: u64,
	rng: StdRng,
	labels: Vec<String>,
	slots: Vec<Vec<Slot>>,
	trace: Vec<Event>,
	steps: usize,
	limit: usize,
}

impl Explorer {
	/// Explore `reactors`, each named by its label, in an order drawn from `seed`.
	pub fn new(seed: u64, reactors: &[(&str, &dyn Reactor)]) -> Self {
		let slots = reactors
			.iter()
			.map(|(_, reactor)| {
				(0..reactor.actions().len())
					.map(|_| {
						let woken = Arc::new(Flag::default());
						Slot {
							waiter: Waiter::new(Waker::from(woken.clone())),
							woken,
							parked: false,
						}
					})
					.collect()
			})
			.collect();
		Self {
			seed,
			rng: StdRng::seed_from_u64(seed),
			labels: reactors.iter().map(|(label, _)| (*label).to_owned()).collect(),
			slots,
			trace: Vec::new(),
			steps: 0,
			limit: 100_000,
		}
	}

	/// Whether any of `reactor`'s actions is ready to poll: never parked since it last fired,
	/// or woken since.
	pub fn awake(&self, reactor: usize) -> bool {
		self.slots[reactor].iter().any(Slot::ready)
	}

	/// Every poll so far.
	pub fn trace(&self) -> &[Event] {
		&self.trace
	}

	/// Poll one ready action, or once none is, probe the parked ones for a lost wakeup.
	///
	/// `reactors` are the ones [`Self::new`] named, in the same order.
	pub fn step(&mut self, reactors: &mut [&mut dyn Reactor]) -> Result<Step, Failed> {
		let ready: Vec<(usize, usize)> = self
			.slots
			.iter()
			.enumerate()
			.flat_map(|(reactor, slots)| {
				slots
					.iter()
					.enumerate()
					.filter(|(_, slot)| slot.ready())
					.map(move |(action, _)| (reactor, action))
			})
			.collect();
		if ready.is_empty() {
			return self.probe(reactors).map(|()| Step::Quiet);
		}
		self.steps += 1;
		if self.steps > self.limit {
			return Err(self.fail(Failure::Endless));
		}

		let pick = (self.rng.next_u64() % ready.len() as u64) as usize;
		let (reactor, action) = ready[pick];
		let name = reactors[reactor].actions()[action];
		let slot = &mut self.slots[reactor][action];
		slot.woken.0.store(false, Ordering::Relaxed);
		let polled = catch_unwind(AssertUnwindSafe(|| reactors[reactor].poll_action(action, &slot.waiter)));
		let fired = match polled {
			Ok(poll) => poll.is_ready(),
			Err(panic) => {
				self.trace.push(Event {
					reactor,
					action: name,
					fired: true,
				});
				return Err(self.fail(Failure::Panic {
					reactor: self.labels[reactor].clone(),
					action: name,
					message: message(&*panic),
				}));
			}
		};
		let event = Event {
			reactor,
			action: name,
			fired,
		};
		self.trace.push(event.clone());
		if !fired {
			slot.parked = true;
			return Ok(Step::Polled(event));
		}

		// The reactor's poll loop runs every action again after one fires.
		for slot in &mut self.slots[reactor] {
			slot.parked = false;
		}
		for (index, reactor) in reactors.iter().enumerate() {
			if let Some(invariant) = reactor.violated() {
				return Err(self.fail(Failure::Invariant {
					reactor: self.labels[index].clone(),
					invariant,
				}));
			}
		}
		Ok(Step::Polled(event))
	}

	/// Poll every parked action once with a waiter nothing wakes: one that fires lost its wakeup.
	fn probe(&mut self, reactors: &mut [&mut dyn Reactor]) -> Result<(), Failed> {
		let noop = Waiter::noop();
		for (reactor, slots) in self.slots.iter().enumerate() {
			for action in 0..slots.len() {
				let polled = catch_unwind(AssertUnwindSafe(|| reactors[reactor].poll_action(action, &noop)));
				if !matches!(polled, Ok(Poll::Pending)) {
					let action = reactors[reactor].actions()[action];
					self.trace.push(Event {
						reactor,
						action,
						fired: true,
					});
					return Err(self.fail(Failure::LostWakeup {
						reactor: self.labels[reactor].clone(),
						action,
					}));
				}
			}
		}
		Ok(())
	}

	fn fail(&self, failure: Failure) -> Failed {
		Failed {
			failure,
			seed: self.seed,
			trace: self
				.trace
				.iter()
				.map(|event| {
					let verb = if event.fired { "fired" } else { "parked" };
					format!("{}.{} {verb}", self.labels[event.reactor], event.action)
				})
				.collect(),
		}
	}
}

impl Slot {
	fn ready(&self) -> bool {
		!self.parked || self.woken.0.load(Ordering::Relaxed)
	}
}

fn message(panic: &(dyn std::any::Any + Send)) -> String {
	if let Some(message) = panic.downcast_ref::<&str>() {
		(*message).to_owned()
	} else if let Some(message) = panic.downcast_ref::<String>() {
		message.clone()
	} else {
		"a non-string panic".to_owned()
	}
}
