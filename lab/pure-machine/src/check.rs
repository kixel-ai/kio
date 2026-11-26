//! A breadth-first, explicit-state model checker for a [`StateMachine`] and its environment.
//!
//! Like TLC, it keeps a 64-bit fingerprint of every state it has visited rather than the
//! state itself, so a state costs a few bytes once expanded, and it expands states in
//! breadth-first order, so the first trace it finds to a broken rule is a shortest one.
//! Each step is replayed from the initial state to print a trace, as TLC re-explores.

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::hash::{BuildHasherDefault, Hash, Hasher};

use crate::StateMachine;

/// A machine composed with its environment, closed so the checker can enumerate its steps.
pub trait Explore: StateMachine {
	/// Push every event that may be enabled here; [`StateMachine::step`] refuses the rest.
	fn events(&self, out: &mut Vec<Self::Event>);

	/// Check a rule about one step from `self` to `next`, as a TLA+ action property does.
	fn transition(&self, next: &Self) -> Result<(), &'static str> {
		let _ = next;
		Ok(())
	}

	/// Check a rule about a state where no event is enabled, such as what it may leave undone.
	fn quiet(&self) -> Result<(), &'static str> {
		Ok(())
	}
}

/// What a complete search covered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
	/// States visited, each once.
	pub distinct: usize,
	/// Steps taken, counting each step into an already visited state.
	pub generated: usize,
	/// The longest of the shortest paths to a state, in steps.
	pub depth: usize,
	/// The most states waiting to be expanded at once.
	pub queue: usize,
}

/// A path from the initial state, each step with the state it reached.
pub struct Trace<M: StateMachine> {
	/// Where the path starts.
	pub init: M,
	/// Each event and the state after it.
	pub steps: Vec<(M::Event, M)>,
}

impl<M: StateMachine + fmt::Debug> fmt::Display for Trace<M> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		writeln!(f, "0: {:?}", self.init)?;
		for (i, (event, state)) in self.steps.iter().enumerate() {
			writeln!(f, "{}: {event:?}\n   {state:?}", i + 1)?;
		}
		Ok(())
	}
}

/// A broken rule, and a shortest path that breaks it.
pub struct Violation<M: StateMachine> {
	/// The rule's name.
	pub rule: &'static str,
	/// The path; for a step rule its last step is the one that breaks it.
	pub trace: Trace<M>,
}

impl<M: StateMachine + fmt::Debug> fmt::Debug for Violation<M> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"{} broken after {} steps:\n{}",
			self.rule,
			self.trace.steps.len(),
			self.trace
		)
	}
}

/// Visit every state reachable from `init`, checking every rule, and stop at the first
/// broken one.
pub fn check<M: Explore>(init: M) -> Result<Stats, Violation<M>> {
	match search(init, true, |_, _| false) {
		Search::Done(stats) => Ok(stats),
		Search::Broken(violation) => Err(violation),
		Search::Found(_) => unreachable!("nothing to find"),
	}
}

/// A shortest path from `init` to a state `goal` holds in, told whether no event is enabled
/// there; `None` once every reachable state is visited without one. A witness: TLC's
/// expected-to-fail invariant. Rules broken on the way are ignored.
pub fn find<M: Explore>(init: M, goal: impl Fn(&M, bool) -> bool) -> Option<Trace<M>> {
	match search(init, false, goal) {
		Search::Found(trace) => Some(trace),
		Search::Done(_) | Search::Broken(_) => None,
	}
}

enum Search<M: StateMachine> {
	Done(Stats),
	Broken(Violation<M>),
	Found(Trace<M>),
}

/// Each visited state's way in: its parent's index and the event from there.
struct Node<E> {
	parent: u32,
	event: Option<E>,
}

/// Visit every state reachable from `init` until `goal` holds in one, checking the rules if
/// `rules`.
fn search<M: Explore>(init: M, rules: bool, goal: impl Fn(&M, bool) -> bool) -> Search<M> {
	let mut seen: HashSet<u64, BuildHasherDefault<Fingerprint>> = HashSet::default();
	let mut nodes: Vec<Node<M::Event>> = Vec::new();
	let mut queue: VecDeque<(M, u32)> = VecDeque::new();
	let mut stats = Stats {
		distinct: 1,
		generated: 0,
		depth: 0,
		queue: 1,
	};

	if rules && let Err(rule) = init.invariant() {
		return broken(&init, &[], 0, None, rule);
	}
	seen.insert(fingerprint(&init));
	nodes.push(Node { parent: 0, event: None });
	queue.push_back((init.clone(), 0));

	let mut events = Vec::new();
	let mut out = Vec::new();
	// The index of the first node a level deeper than the one being expanded.
	let mut level_end = 1;
	// A rule broken by a step out of this level, reported once the level's quiet states,
	// whose traces are a step shorter, are checked.
	let mut deeper = None;
	while let Some((state, index)) = queue.pop_front() {
		if index as usize >= level_end {
			if let Some(broken) = deeper {
				return broken;
			}
			stats.depth += 1;
			level_end = nodes.len();
		}
		events.clear();
		state.events(&mut events);
		let mut quiet = true;
		for event in events.drain(..) {
			let mut next = state.clone();
			out.clear();
			if next.step(event.clone(), &mut out).is_err() {
				continue;
			}
			quiet = false;
			stats.generated += 1;
			if deeper.is_some() {
				continue;
			}
			if rules && let Err(rule) = state.transition(&next) {
				deeper = Some(broken(&init, &nodes, index, Some(event), rule));
				continue;
			}
			if !seen.insert(fingerprint(&next)) {
				continue;
			}
			let Ok(child) = u32::try_from(nodes.len()) else {
				panic!("more than 2^32 states");
			};
			nodes.push(Node {
				parent: index,
				event: Some(event),
			});
			stats.distinct += 1;
			if rules && let Err(rule) = next.invariant() {
				deeper = Some(broken(&init, &nodes, child, None, rule));
				continue;
			}
			queue.push_back((next, child));
		}
		stats.queue = stats.queue.max(queue.len());
		if goal(&state, quiet) {
			return Search::Found(trace(&init, &nodes, index, None));
		}
		if rules
			&& quiet && let Err(rule) = state.quiet()
		{
			return broken(&init, &nodes, index, None, rule);
		}
	}
	deeper.unwrap_or(Search::Done(stats))
}

fn broken<M: Explore>(
	init: &M,
	nodes: &[Node<M::Event>],
	index: u32,
	last: Option<M::Event>,
	rule: &'static str,
) -> Search<M> {
	Search::Broken(Violation {
		rule,
		trace: trace(init, nodes, index, last),
	})
}

/// Replay the path to node `index` from `init`, then `last` if any.
fn trace<M: StateMachine>(init: &M, nodes: &[Node<M::Event>], index: u32, last: Option<M::Event>) -> Trace<M> {
	let mut events: Vec<M::Event> = last.into_iter().collect();
	let mut at = index as usize;
	while let Some(node) = nodes.get(at)
		&& let Some(event) = &node.event
	{
		events.push(event.clone());
		at = node.parent as usize;
	}
	events.reverse();

	let mut state = init.clone();
	let mut out = Vec::new();
	let steps = events
		.into_iter()
		.map(|event| {
			state.step(event.clone(), &mut out).expect("a recorded step replays");
			(event, state.clone())
		})
		.collect();
	Trace {
		init: init.clone(),
		steps,
	}
}

fn fingerprint<M: Hash>(state: &M) -> u64 {
	// SipHash with fixed keys, so a run is reproducible.
	let mut hasher = std::hash::DefaultHasher::new();
	state.hash(&mut hasher);
	hasher.finish()
}

/// The visited set's hasher: a fingerprint is already a hash.
#[derive(Default)]
struct Fingerprint(u64);

impl Hasher for Fingerprint {
	fn finish(&self) -> u64 {
		self.0
	}

	fn write(&mut self, _: &[u8]) {
		unreachable!("only fingerprints are hashed");
	}

	fn write_u64(&mut self, fingerprint: u64) {
		self.0 = fingerprint;
	}
}
