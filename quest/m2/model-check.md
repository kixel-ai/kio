# [M] kio exports the BFS model checker for pure state machines

## Goal

A crate outside kio can model-check its own pure state machine with the breadth-first checker `lab/pure-machine` proved out on `SendCredit`: it visits every reachable state once, checks the machine's invariants and quiet-state rules, and prints the shortest trace to a broken rule. The checker lands behind an off-by-default feature, for dev-dependencies, as the `sim` feature is, and the published crate carries none of moq-net's logic.

## Plan

The state-machine spikes' comparison ([kixel-ai/kio#26](https://github.com/kixel-ai/kio/pull/26)) picked a pure `StateMachine` plus an exhaustive BFS as the way to check protocol logic. On `SendCredit` it visits exactly TLC's distinct states on every config, 250 to 2,900 times faster than single-worker TLC, so the Rust machine can be both the model and the code. Its consumer is `moq-dev/moq`'s port of send credit (`rs/moq-net/src/transport/open.rs`) to a pure machine.

- Move `StateMachine`, `Disabled`, and `check`'s `Explore`, `check`, `find`, `Stats`, `Trace`, and `Violation` (`lab/pure-machine/src/lib.rs`, `src/check.rs`, about 200 lines of code) into one kio module, named without repeating it (for example `model::Machine`, `model::check`).
- `lab/pure-machine` then uses kio's copy. Its `matches_tlc` test, where every config visits TLC's exact state count, stays as the checker's differential test. The fixtures stay in `lab/`, out of `src/`, as `AGENTS.md` requires.
- The checker needs only `Clone + Eq + Hash`, so a production state with `Vec`, `BTreeMap`, and `u64` checks as well as the spike's const-generic arrays and `u8`, which were for Kani and Aeneas. Say so in the docs.
- Keep the 64-bit fingerprints and their collision note (about 2^-29 at 300k states). Leave out symmetry reduction and parallel BFS until a consumer's state space needs them.

## Waiting

- Maintainer: where does the checker live? An agent cannot pick a crate's public surface.
  - A `kio` feature such as `model` (Recommended) - beside `sim` as kio's other deterministic test tool, and costs nothing when off
  - A separate `kio-model` crate - one more crate to publish and version, for about 200 lines
  - A copy in moq-net's dev code - no kio API, but every later consumer copies it again
- moq-dev/moq: a quest ports send credit to a pure machine checked by this checker. Check: moq's `quest/` tree names it. Until then the checker has no consumer outside kio's lab and stays private.
