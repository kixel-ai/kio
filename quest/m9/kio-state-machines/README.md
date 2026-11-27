# [M] Spike provable state-machine designs for kio

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
  3. Adversarial `kio::clock::sim` scheduler + lost-wakeup probe: done in `kio::clock::sim` and `lab/adversarial/` ([kixel-ai/kio#10](https://github.com/kixel-ai/kio/pull/10)), results below;
  4. `rust2-lean` MIR extraction in `kixel-ai/rust2` (`quest/m2/lean-spike.md` there).
- ✅ Benchmark targets: `SendCredit` ([`rs/moq-net/verify/credit/SendCredit.tla`](https://github.com/moq-dev/moq/blob/main/rs/moq-net/verify/credit/SendCredit.tla), 289 lines: stream-credit line, preemption, `give_way`, `stuck` cursor, `Line::wake_head`) across all three `kio` spikes, plus routing (`WarmSettle` from [`rs/moq-net/verify/warm/WarmSettle.tla`](https://github.com/moq-dev/moq/blob/main/rs/moq-net/verify/warm/WarmSettle.tla) / [`rs/moq-net/src/route/tests/warm.rs`](https://github.com/moq-dev/moq/blob/main/rs/moq-net/src/route/tests/warm.rs)) for the Lean 4 (`rust2-lean` vs. Aeneas) comparison (rejected: `Reconnect` / `Handover` for the `kio` spikes, `WarmSettle` across every spike).

Keep every prototype behind an internal/experimental feature or test crate until the comparison report settles the public API. The `SendCredit` and `WarmSettle` fixtures are moq-net logic, so they live in tests or an unpublished workspace crate such as `lab/pure-machine/`, never in `kio`'s `src/`, since `AGENTS.md` keeps `kio` free of MoQ assumptions; only generic machinery (scheduler policy, `#[kio::reactor]`) may go behind a `kio` feature.

Reactor spike results (`lab/reactor/`, `kio-macros`'s `unstable-reactor` feature), for the report:

- Ergonomics: the macro is 248 lines; `SendCredit` is two reactors, `TrackRun` (`open`, `hand_out`) and `Peer` (`finish`, `free`), about 200 lines of actions over a 215-line session model. A hand-written `poll_step` over the same actions is 6 lines, so the macro buys the explorer surface, not brevity.
- Runtime: `poll_next` costs what the same loop written by hand does: 28.8k instructions per session driven to quiet for both, within 0.01% (`perf stat`, 80k sessions, release, no `sim`).
- Build: `unstable-reactor` adds 0.03 s to `kio-macros`; a `kio-reactor-lab` rebuild is 0.22 s.
- Bugs: a missing `wake_head` is caught as a lost wakeup on `TrackRun[n].open` in 32-78% of seeds per config; a `give_way` that assumes `open` ran first panics in `hand_out` in 9-20% of seeds, and never under the priority order. Each seed takes 60-85 µs; 100k seeds reach 49-79% of the split spec's distinct states.
- Trace validation: 200 runs per config replay in TLC in about 3 s each. The glue is a 50-line projection to the spec's `v`, a 41-line trace spec, and a 45-line `SendCreditSplit.tla`: the reactor's actions are finer than the spec's atomic `Step(s)`, so traces validate against the split refinement, which TLC checks against every rule in 5 s to 4.5 min per config (59k-441k states vs. the original's 42k-296k).
- Contract: an action that changes state must return `Ready`, so `moq-net`'s `Open::poll`, which joins the line, asks for credit, and resets streams yet returns `Pending`, has to return `Ready` for that firing instead. Everything one `Ready` firing changes is one step: invariants run only after it, so its intermediate states go unchecked.

Public API: none (lab spikes only). Wire: none.

Adversarial sim results ([kixel-ai/kio#10](https://github.com/kixel-ai/kio/pull/10); rerun with `cargo run --release -p kio-adversarial-lab --example report`):

- What landed: in `kio::clock::sim`, `KIO_SIM_SCHEDULE=fifo|random|pct[:depth[:steps]]` orders ready tasks, and `KIO_SIM_PROBE=1` re-polls every parked task and every `Tasks` child at each stall, panicking with the spawn or push location of one that progresses. Both are experimental, default off, and `sim`-only. The probe fails a run woken from another thread, since such a wake lands at a moment no seed replays. A `Tasks` set keeps its own rotating slot order under every schedule. The `SendCredit` fixture is `lab/adversarial/`: a poll-based model of `SendCredit.tla` without live groups, with targeted `WaiterList` wakes and four injected bugs, driven through public `sim::run_seeded` and the env vars.
- Lost wakeups (no head wake, no free wake): the end-state oracle at `idle` (`Ends`) already catches 99.4 to 100% of 1000 seeds under every schedule. The probe catches the same seeds, plus a few masked ones the oracle misses (2 credits: 998 vs. 995 seeds under random, 999 vs. 996 for children under FIFO). It also names the culprit track at the first stall instead of reporting a stuck end state. Probing costs about a third more per seed on the fixture, and 2.1x on a timer-heavy 50-task workload.
- Ordering bug (a track the peer's free woke opens without re-checking it heads the line): for spawned tasks, FIFO catches it on 11.0% of seeds, random on 45.1%, and PCT on 43.4%. For `Tasks` children, FIFO already catches 49.6%, because the set's rotating slot order varies who runs first; random and PCT, which now order only the tasks around the set, catch 53.3% and 52.6%.
- A third lost-wakeup injection, dropping `make_room`'s wake of the reset stream's track, is never observable at 1 or 2 credits, by either the oracle or the probe. That wake may be redundant in `SendCredit.tla` itself, which is worth a TLC check in moq.
- Cost: the new FIFO run loop matches the `FuturesUnordered` one (201 to 205 vs. 207 µs on a 50-task timer workload). Against it, random costs 2.2x, PCT 2.6x, the probe 2.1x, and random plus the probe 3.3x. Builds without `sim` compile none of it. Running kio's own suite under every mode with several seeds surfaced no kio bug.
- Cut before landing, with the rows they produced: scheduling `Tasks` children by the policy (children under random and PCT caught the ordering bug on 47.3% and 44.4%, below FIFO's 49.6%, so it bought nothing); a depth-first explorer replaying choice prefixes (8/20 or 14/20 data seeds caught, medians of 6 and 97 runs, against 18/20 with medians of 3 and 1 for random and PCT; 97 to 150 distinct states in 1 s against 156); and per-poll footprints for a DPOR estimate (13.0% of successive state-changing polls by different units commute). Recommendation: keep random and PCT with the probe; drop systematic replay.

## Related

- [Miri's GenMC mode explores every schedule of kio's lock-free code](/quest/m2/miri-genmc.md) - weak-memory schedule exploration for `kio`
- `moq-dev/moq` external TLA+ trace validation quests ([subscribe](https://github.com/moq-dev/moq/blob/main/quest/m4/subscribe-trace-validation.md), [resume](https://github.com/moq-dev/moq/blob/main/quest/m4/resume-trace-validation.md), [reconnect](https://github.com/moq-dev/moq/blob/main/quest/m4/reconnect-trace-validation.md)) and [codec Kani](https://github.com/moq-dev/moq/blob/main/quest/m4/codec-kani.md)
