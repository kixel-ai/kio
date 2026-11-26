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
  1. [Pure `StateMachine` + BFS + Kani/Aeneas](/quest/m9/kio-state-machines/pure-machine.md);
  2. [`#[kio::reactor]` guarded-action macro](/quest/m9/kio-state-machines/reactor-macro.md);
  3. Adversarial `kio::clock::sim` + `kio::Tasks` scheduler + lost-wakeup detector (done; results below);
  4. `rust2-lean` MIR extraction in `kixel-ai/rust2` (`quest/m2/lean-spike.md` there).
- ✅ Benchmark targets: `SendCredit` (`rs/moq-net/verify/credit/SendCredit.tla`, 289 lines: stream-credit line, preemption, `give_way`, `stuck` cursor, `Line::wake_head`) across all three `kio` spikes, plus routing (`WarmSettle` from `rs/moq-net/verify/warm/WarmSettle.tla` / `rs/moq-net/src/route/tests/warm.rs`) for the Lean 4 (`rust2-lean` vs. Aeneas) comparison (rejected: `Reconnect` / `Handover` for the `kio` spikes, `WarmSettle` across every spike).

Keep every prototype behind an internal/experimental feature or test crate until the comparison report settles the public API.

Public API: none (lab spikes only). Wire: none.

Adversarial sim results ([kixel-ai/kio#10](https://github.com/kixel-ai/kio/pull/10); rerun with `cargo test --release -p kio --lib credit::tests::report -- --ignored --nocapture`):

- What landed: `KIO_SIM_SCHEDULE=fifo|random|pct[:depth[:steps]]` orders ready tasks and `Tasks` children (and may cut a pass short so other tasks interleave), and `KIO_SIM_PROBE=1` re-polls every parked task and child at each stall, panicking with the spawn or push location of one that progresses. Both are experimental and default off, and `sim`-only. The `SendCredit` fixture is `src/clock/sim/credit.rs`: a poll-based model of `SendCredit.tla` without live groups, with targeted `WaiterList` wakes and four injected bugs.
- Lost wakeups (no head wake, no free wake): the end-state oracle at `idle` (`Ends`) already catches 99.4 to 100% of 1000 seeds under every schedule. The probe catches the same seeds, plus a few masked ones the oracle misses (2 credits: 998 vs. 995 seeds under random, 999 vs. 996 for children under FIFO). It also names the culprit track at the first stall instead of reporting a stuck end state. Probing costs about a third more per seed on the fixture, and 2.1x on a timer-heavy 50-task workload.
- Ordering bug (a track the peer's free woke opens without re-checking it heads the line): for spawned tasks, FIFO catches it on 11.0% of seeds, random on 45.1%, and PCT on 43.4%. For `Tasks` children, FIFO already catches 49.6%, because the set's rotating slot order varies who runs first; random and PCT catch 47.3% and 44.4%.
- A third lost-wakeup injection, dropping `make_room`'s wake of the reset stream's track, is never observable at 1 or 2 credits, by either the oracle or the probe. That wake may be redundant in `SendCredit.tla` itself, which is worth a TLC check in moq.
- Explorer: bounded depth-first replay from choice prefixes finds the ordering bug on 8/20 (branching at the first 8 choices) or 14/20 (first 64) data seeds, with medians of 6 and 97 runs. Random and PCT find it on 18/20 with medians of 3 and 1 runs, in a quarter to a sixth of the time. On one data seed, random and PCT each observe 156 distinct states in 1 s, while the explorer observes 97 to 150 before it exhausts its bound or is cut off; no exhaustive check bounds how many are reachable. Only 13.0% of adjacent polls by different units touch disjoint state, so DPOR has little to prune at poll granularity. Recommendation: drop systematic replay, and keep random and PCT with the probe.
- Cost: the new FIFO run loop matches the `FuturesUnordered` one (201 to 205 vs. 207 µs on a 50-task timer workload). Against it, random costs 2.2x, PCT 2.6x, the probe 2.1x, and random plus the probe 3.3x. Builds without `sim` compile none of it. With `sim` on (bench builds), `Tasks` benches move 0.4 to 3%, and an idle pass 0.9 ns. Running kio's own suite under every mode with several seeds surfaced no kio bug.

## Related

- [Spike: Verus proves kio::log's commit protocol as written](/quest/m9/verus-kio-log.md) - deductive proofs on `kio`'s unsafe lock-free internals
- [Miri's GenMC mode explores every schedule of kio's lock-free code](/quest/m2/miri-genmc.md) - weak-memory schedule exploration for `kio`
- `moq-dev/moq` external TLA+ trace validation quests ([subscribe](https://github.com/moq-dev/moq/blob/main/quest/m4/subscribe-trace-validation.md), [resume](https://github.com/moq-dev/moq/blob/main/quest/m4/resume-trace-validation.md), [reconnect](https://github.com/moq-dev/moq/blob/main/quest/m4/reconnect-trace-validation.md)) and [codec Kani](https://github.com/moq-dev/moq/blob/main/quest/m4/codec-kani.md)
