# Spike provable state-machine designs for kio

## Goal

Prototype and compare three candidate state-machine designs inside `kixel-ai/kio` on a shared `SendCredit` fixture, alongside `kixel-ai/rust2`'s `rust2-lean` MIR-extraction spike on routing (`WarmSettle`), touching no published API, wire format, or CLI. Once all three `kio` spikes finish, this epic writes a single comparison report covering:

- ergonomics (lines of code, boilerplate, and readability compared to hand-written `poll_*` loops);
- verification and bug-finding power (interleavings covered, injected bugs caught, counterexample quality, and what each approach proves vs. bounded-checks vs. fuzzes);
- verification speed and scalability vs. TLC's `SendCredit` configs;
- build time and runtime instruction cost;
- how each approach composes with `kixel-ai/rust2`'s `rust2-lean` spike and with `moq`'s zero-copy fan-out data plane.

It ends by planning follow-up quests for the winning direction (or dropping the approaches that do not pay for their weight).

## Plan

Decided 2026-11-25 (maintainer, `/quest-plan` on designing async Rust for state-machine verification):

- ✅ Goal: prototype and compare candidate state-machine designs for `kio` on real `moq-net` protocol logic as internal lab spikes, leaving `kio::log` Verus proofs and `m4` TLA+ trace validation to their existing quests (rejected: narrowing to pure `StateMachine` only, narrowing to `kio`'s poll layer only, dropping the spikes).
- ✅ Repo home: move `rs/kio` to `kixel-ai/kio` in `m1` first, then run these three spikes in `kixel-ai/kio`'s quest tree; plan the fourth approach (`rust2-lean` MIR extraction) in `kixel-ai/rust2`'s quest tree (rejected: putting all spikes in `rust2`, a fresh throwaway repo, keeping the spikes in `moq`'s `m9` lab first, putting MIR extraction inside `kio`).
- ✅ Approaches: spike all four:
  1. Pure `StateMachine` + BFS + Kani/Aeneas: done in `lab/pure-machine/` ([kixel-ai/kio#9](https://github.com/kixel-ai/kio/pull/9), whose description holds the numbers for the report);
  2. `#[reactor]` guarded-action macro: done in `lab/reactor/` ([kixel-ai/kio#11](https://github.com/kixel-ai/kio/pull/11)), results below;
  3. [Adversarial `kio::clock::sim` + `kio::Tasks` scheduler + lost-wakeup detector](/quest/m9/kio-state-machines/adversarial-sim.md);
  4. `rust2-lean` MIR extraction in `kixel-ai/rust2` (`quest/m2/lean-spike.md` there).
- ✅ Benchmark targets: `SendCredit` ([`rs/moq-net/verify/credit/SendCredit.tla`](https://github.com/moq-dev/moq/blob/main/rs/moq-net/verify/credit/SendCredit.tla), 289 lines: stream-credit line, preemption, `give_way`, `stuck` cursor, `Line::wake_head`) across all three `kio` spikes, plus routing (`WarmSettle` from [`rs/moq-net/verify/warm/WarmSettle.tla`](https://github.com/moq-dev/moq/blob/main/rs/moq-net/verify/warm/WarmSettle.tla) / [`rs/moq-net/src/route/tests/warm.rs`](https://github.com/moq-dev/moq/blob/main/rs/moq-net/src/route/tests/warm.rs)) for the Lean 4 (`rust2-lean` vs. Aeneas) comparison (rejected: `Reconnect` / `Handover` for the `kio` spikes, `WarmSettle` across every spike).

Keep every prototype behind an internal/experimental feature or test crate until the comparison report settles the public API. The `SendCredit` and `WarmSettle` fixtures are moq-net logic, so they live in tests or an unpublished workspace crate such as `lab/pure-machine/`, never in `kio`'s `src/`, since `AGENTS.md` keeps `kio` free of MoQ assumptions; only generic machinery (scheduler policy, `#[kio::reactor]`) may go behind a `kio` feature.

Reactor spike results (`lab/reactor/`, `kio-macros`'s `unstable-reactor` feature), for the report:

- Ergonomics: the macro is 248 lines; `SendCredit` is two reactors, `TrackRun` (`open`, `hand_out`) and `Peer` (`finish`, `free`), about 200 lines of actions over a 215-line session model. A hand-written `poll_step` over the same actions is 6 lines, so the macro buys the explorer surface, not brevity.
- Runtime: `poll_next` costs what the same loop written by hand does: 28.8k instructions per session driven to quiet for both, within 0.01% (`perf stat`, 80k sessions, release, no `sim`).
- Build: `unstable-reactor` adds 0.03 s to `kio-macros`; a `kio-reactor-lab` rebuild is 0.22 s.
- Bugs: a missing `wake_head` is caught as a lost wakeup on `TrackRun[n].open` in 32-78% of seeds per config; a `give_way` that assumes `open` ran first panics in `hand_out` in 9-20% of seeds, and never under the priority order. Each seed takes 60-85 µs; 100k seeds reach 49-79% of the split spec's distinct states.
- Trace validation: 200 runs per config replay in TLC in about 3 s each. The glue is a 50-line projection to the spec's `v`, a 41-line trace spec, and a 45-line `SendCreditSplit.tla`: the reactor's actions are finer than the spec's atomic `Step(s)`, so traces validate against the split refinement, which TLC checks against every rule in 5 s to 4.5 min per config (59k-441k states vs. the original's 42k-296k).
- Contract: an action that changes state must return `Ready`, so `moq-net`'s `Open::poll`, which joins the line, asks for credit, and resets streams on a `Pending` return, splits into a `Ready` per change; the explorer only sees what an action reports.

Public API: none (lab spikes only). Wire: none.

## Related

- [Miri's GenMC mode explores every schedule of kio's lock-free code](/quest/m2/miri-genmc.md) - weak-memory schedule exploration for `kio`
- `moq-dev/moq` external TLA+ trace validation quests ([subscribe](https://github.com/moq-dev/moq/blob/main/quest/m4/subscribe-trace-validation.md), [resume](https://github.com/moq-dev/moq/blob/main/quest/m4/resume-trace-validation.md), [reconnect](https://github.com/moq-dev/moq/blob/main/quest/m4/reconnect-trace-validation.md)) and [codec Kani](https://github.com/moq-dev/moq/blob/main/quest/m4/codec-kani.md)
