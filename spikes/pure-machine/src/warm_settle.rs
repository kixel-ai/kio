//! Where a relay pulls a broadcast from, as a pure machine: `WarmSettle.tla`'s ranked rule
//! (`Rule = "order"`, `Summed = FALSE`), the one moq-net's warm routing simulator measures
//! (`route/tests/warm.rs`).
//!
//! A relay with demand pulls through one parent: its route's next hop, or a neighbour that
//! advertised a warm copy. Readers are ranked, and a relay takes a warm copy only from a
//! neighbour feeding a reader ranked below every reader it feeds itself, so warm pulls
//! never form a loop. A move to a new parent keeps the old one until the new one answers.
//!
//! The relay sees its neighbours only through what each last told it; the network that
//! delivers those words late, and the route layer, stay outside. Relays are numbered from
//! 1 to `N`; a path is the bit set of the relays on it.
//!
//! Written for Charon and Aeneas: `while` loops, no closures, a `Vec` built by `push`.

use crate::{Disabled, StateMachine};

/// No rank: above every rank a reader has.
pub const NO_RANK: u32 = u32::MAX;

/// What a parent's answer, or an advert, carries: the copy's cost from the publisher, the
/// relays its pull runs through, and the lowest rank of the readers it feeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sig {
	/// The copy's cost from the publisher.
	pub cost: u32,
	/// The relays its pull runs through, bit `r - 1` for relay `r`.
	pub path: u16,
	/// The lowest rank of the readers the sender feeds.
	pub rank: u32,
}

/// A parent's answer to a pull.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Answer {
	/// No answer yet.
	None,
	/// Waiting for its own parent.
	Wait,
	/// Fed, with this copy.
	Live(Sig),
	/// It has nothing to offer.
	Lost,
}

/// What a relay last heard from one neighbour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Heard {
	/// The rank the neighbour's pull through this relay carries, if it pulls through it.
	pub pull: Option<u32>,
	/// Its answer to this relay's pull through it.
	pub answer: Answer,
	/// Its advert of a warm copy, if any.
	pub advert: Option<Sig>,
}

impl Heard {
	/// Nothing heard.
	pub const EMPTY: Self = Self {
		pull: None,
		answer: Answer::None,
		advert: None,
	};
}

/// A link to a neighbour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Link {
	/// Whether its session is open.
	pub open: bool,
	/// Its price for a cold pull.
	pub cold: u32,
	/// Its price for a warm one.
	pub warm: u32,
}

/// How a relay's pull through a neighbour stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pull {
	/// Answered with a copy that does not run through this relay.
	Live,
	/// Answered with a copy that runs through this relay.
	Looped,
	/// Lost: answered lost, or silent after answering live.
	Lost,
	/// Not answered yet.
	Pending,
}

/// One place to pull from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
	/// The neighbour to pull through.
	pub via: u8,
	/// What the pull costs.
	pub cost: u32,
	/// Whether it is a warm copy rather than the route.
	pub copy: bool,
	/// The rank the copy's sender feeds, [`NO_RANK`] for the route.
	pub rank: u32,
}

/// Whether `a` is the better choice: the cheaper, a warm copy before the route, then the
/// higher relay, standing in for the simulator's rendezvous hash.
pub fn better(a: &Choice, b: &Choice) -> bool {
	a.cost < b.cost || (a.cost == b.cost && ((a.copy && !b.copy) || (a.copy == b.copy && a.via > b.via)))
}

/// The best of `choices`, if any.
pub fn best(choices: &[Choice]) -> Option<Choice> {
	let mut best: Option<Choice> = None;
	let mut i = 0;
	while i < choices.len() {
		let choice = choices[i];
		best = match best {
			Some(b) if !better(&choice, &b) => Some(b),
			_ => Some(choice),
		};
		i += 1;
	}
	best
}

/// One relay of `N`, other than the publisher's, and what it knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Relay<const N: usize> {
	/// Its number.
	pub id: u8,
	/// Its own reader's rank, if it has a reader.
	pub reader: Option<u32>,
	/// Its route toward the publisher: next hop and cost, if it has one.
	pub route: Option<(u8, u32)>,
	/// Its link to each relay, if they neighbour.
	pub links: [Option<Link>; N],
	/// What it last heard from each.
	pub heard: [Heard; N],
	/// The neighbour it pulls through.
	pub up: Option<u8>,
	/// A move under way: it replaces `up` once answered.
	pub next: Option<u8>,
	/// Its pulls answered live once, whose silence is a loss, bit `r - 1` for relay `r`.
	pub answered: u16,
}

/// What a relay hears or does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Event {
	/// Neighbour `from` told it `heard`.
	Hear {
		/// The neighbour.
		from: u8,
		/// What it said.
		heard: Heard,
	},
	/// It reacts to what it knows, if that changes its pull.
	React,
}

/// Whether `x` is `Some(v)`; Aeneas has no model of `Option`'s `==`.
fn is(x: Option<u8>, v: u8) -> bool {
	match x {
		Some(y) => y == v,
		None => false,
	}
}

fn bit(r: u8) -> u16 {
	1 << (r - 1)
}

impl<const N: usize> Relay<N> {
	fn link(&self, r: u8) -> Option<Link> {
		self.links[(r - 1) as usize]
	}

	fn heard(&self, r: u8) -> Heard {
		self.heard[(r - 1) as usize]
	}

	fn is_open(&self, r: u8) -> bool {
		matches!(self.link(r), Some(Link { open: true, .. }))
	}

	/// Whether neighbour `c` pulls through this relay.
	pub fn down(&self, c: u8) -> bool {
		self.is_open(c) && self.heard(c).pull.is_some()
	}

	/// Whether it has demand: a reader, or a neighbour pulling through it.
	pub fn demand(&self) -> bool {
		if self.reader.is_some() {
			return true;
		}
		let mut c = 1;
		while c as usize <= N {
			if self.down(c) {
				return true;
			}
			c += 1;
		}
		false
	}

	/// The lowest rank among the readers it feeds: its own, and those its pulling
	/// neighbours carry.
	pub fn mine(&self) -> u32 {
		let mut rank = match self.reader {
			Some(own) => own,
			None => NO_RANK,
		};
		let mut d = 1;
		while d as usize <= N {
			if self.down(d)
				&& let Some(pull) = self.heard(d).pull
				&& pull < rank
			{
				rank = pull;
			}
			d += 1;
		}
		rank
	}

	/// How its pull through `s` stands.
	pub fn pull(&self, s: u8) -> Pull {
		match self.heard(s).answer {
			Answer::Live(sig) if sig.path & bit(self.id) == 0 => Pull::Live,
			Answer::Live(_) => Pull::Looped,
			Answer::Lost => Pull::Lost,
			_ if self.answered & bit(s) != 0 => Pull::Lost,
			_ => Pull::Pending,
		}
	}

	/// Where it could pull from while pulling through `u`: its route's next hop, and the
	/// warm copies it heard of whose senders feed a reader ranked below its own, never
	/// through a neighbour pulling through it or a copy whose pull runs through it.
	pub fn options(&self, u: Option<u8>) -> Vec<Choice> {
		let mine = self.mine();
		let mut choices = Vec::new();
		if let Some((hop, cost)) = self.route
			&& !self.down(hop)
		{
			choices.push(Choice {
				via: hop,
				cost,
				copy: false,
				rank: NO_RANK,
			});
		}
		let mut s = 1;
		while s as usize <= N {
			// An advert, or the live parent's answer when it advertised nothing.
			let heard = self.heard(s);
			let sig = match heard.advert {
				Some(sig) if self.is_open(s) => Some(sig),
				None if is(u, s) && self.pull(s) == Pull::Live => match heard.answer {
					Answer::Live(sig) => Some(sig),
					_ => None,
				},
				_ => None,
			};
			if let (Some(sig), Some(link)) = (sig, self.link(s))
				&& !self.down(s)
				&& sig.path & bit(self.id) == 0
				&& sig.rank < mine
			{
				choices.push(Choice {
					via: s,
					cost: link.cold,
					copy: true,
					rank: sig.rank,
				});
			}
			s += 1;
		}
		choices
	}

	/// The pull and move its rule picks, from the pull `u` and move `x` it has once a move
	/// that was answered took effect.
	#[expect(clippy::manual_map, reason = "Aeneas translates no closures")]
	fn decide(&self, u: Option<u8>, x: Option<u8>) -> (Option<u8>, Option<u8>) {
		let choices = self.options(u);
		let best = match best(&choices) {
			Some(choice) => Some(choice.via),
			None => None,
		};
		let Some(u) = u else {
			return (best, None);
		};
		match self.pull(u) {
			Pull::Live if best.is_some() => toward(u, x, best),
			Pull::Pending if is(best, u) => (Some(u), x),
			_ => (best, None),
		}
	}

	/// Its pull and move once it has reacted to what it knows.
	pub fn next_pulls(&self) -> (Option<u8>, Option<u8>) {
		if !self.demand() {
			return (None, None);
		}
		let (u, x) = match self.next {
			Some(x) => match self.pull(x) {
				Pull::Live => (Some(x), None),
				Pull::Lost | Pull::Looped => (self.up, None),
				Pull::Pending => (self.up, Some(x)),
			},
			None => (self.up, None),
		};
		self.decide(u, x)
	}
}

/// Head for `t`, keeping `u` until the move is answered.
fn toward(u: u8, x: Option<u8>, t: Option<u8>) -> (Option<u8>, Option<u8>) {
	match t {
		Some(t) if t == u => (Some(u), None),
		Some(t) if is(x, t) => (Some(u), x),
		_ => (Some(u), t),
	}
}

impl<const N: usize> StateMachine for Relay<N> {
	type Event = Event;
	type Output = ();

	fn step(&mut self, event: Event, _: &mut Vec<()>) -> Result<(), Disabled> {
		match event {
			Event::Hear { from, heard } => {
				if from < 1 || from as usize > N || !self.is_open(from) || self.heard(from) == heard {
					return Err(Disabled);
				}
				self.heard[(from - 1) as usize] = heard;
				if (is(self.up, from) || is(self.next, from)) && matches!(heard.answer, Answer::Live(_)) {
					self.answered |= bit(from);
				}
				Ok(())
			}
			Event::React => {
				let (up, next) = self.next_pulls();
				if (up, next) == (self.up, self.next) {
					return Err(Disabled);
				}
				// A pull replaced outright by one through the same neighbour starts afresh.
				let mut answered = 0;
				let mut s = 1;
				while s as usize <= N {
					if self.answered & bit(s) != 0 && (is(up, s) || is(next, s)) && self.pull(s) != Pull::Lost {
						answered |= bit(s);
					}
					s += 1;
				}
				self.up = up;
				self.next = next;
				self.answered = answered;
				Ok(())
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The drones mesh, from relay 2's side: a costly uplink to the publisher's relay 1,
	/// and cheap links to drones 3 and 4.
	fn drone() -> Relay<4> {
		let link = |cold, warm| Some(Link { open: true, cold, warm });
		Relay {
			id: 2,
			reader: Some(10 * 5 + 2),
			route: Some((1, 10)),
			links: [link(10, 4), None, link(2, 1), link(2, 1)],
			heard: [Heard::EMPTY; 4],
			up: None,
			next: None,
			answered: 0,
		}
	}

	fn advert(rank: u32, path: u16) -> Heard {
		Heard {
			pull: None,
			answer: Answer::None,
			advert: Some(Sig { cost: 14, path, rank }),
		}
	}

	#[test]
	fn pulls_its_route_with_nothing_warm() {
		assert_eq!(drone().next_pulls(), (Some(1), None));
	}

	/// A neighbour feeding a lower-ranked reader is a warm copy to take; one feeding a
	/// higher-ranked reader is not, so two drones never pull through each other.
	#[test]
	fn takes_only_lower_ranked_copies() {
		let mut relay = drone();
		relay.heard[2] = advert(10 * 5 + 3, 0b0101);
		assert_eq!(relay.next_pulls(), (Some(1), None), "drone 3 feeds a higher rank");
		relay.heard[3] = advert(10 * 5 + 1, 0b1001);
		assert_eq!(relay.next_pulls(), (Some(4), None), "drone 4 feeds a lower rank");
	}

	/// A live pull moves to a better copy, keeping the old parent until the new one answers.
	#[test]
	fn moves_keeping_the_live_pull() {
		let mut relay = drone();
		relay.up = Some(1);
		relay.heard[0].answer = Answer::Live(Sig {
			cost: 0,
			path: 0b0001,
			rank: NO_RANK,
		});
		relay.heard[3] = advert(1, 0b1001);
		let mut out = Vec::new();
		relay
			.step(Event::React, &mut out)
			.expect("a better copy moves the pull");
		assert_eq!((relay.up, relay.next), (Some(1), Some(4)));
		relay
			.step(
				Event::Hear {
					from: 4,
					heard: Heard {
						answer: Answer::Live(Sig {
							cost: 14,
							path: 0b1001,
							rank: 1,
						}),
						..advert(1, 0b1001)
					},
				},
				&mut out,
			)
			.expect("drone 4 answers");
		relay
			.step(Event::React, &mut out)
			.expect("the answered move takes effect");
		assert_eq!((relay.up, relay.next), (Some(4), None));
	}
}
