# [M] Audit test coverage

## Goal

Run the `tests` audit: `quest audit tests` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

Baseline: 2026-11-26, the first run in this repository ([kixel-ai/kio#7](https://github.com/kixel-ai/kio/pull/7)).
CI produces no coverage report, so it was measured once without committing the tooling: `cargo llvm-cov --workspace` (cargo-llvm-cov 0.9.2 from nixpkgs, with `LLVM_COV` and `LLVM_PROFDATA` pointed at nixpkgs' `llvmPackages_22`, matching rustc 1.98.1's LLVM 22.1.8).
Line coverage, test modules included: `kio` 94.2% (5951/6320), `kio-macros` 74.8% (552/738).
The earlier figure, `kio` 93.9% in `moq-dev/moq`'s 2026-11-23 audit, also counted moq's own tests exercising kio, so it does not compare.

What the measurement cannot see:

- Doc tests: cargo-llvm-cov runs them only on nightly.
- `kio-macros`' rejections: trybuild compiles `macros/tests/ui/` in a child rustc that is not instrumented, so every `syn::Error` branch reads uncovered. Compare the `error:` lines in `macros/tests/ui/*.stderr` against the messages in `macros/src/` instead.
- The `runtime.rs` guard tests (`a_wait_on_the_host_clock_aborts` and its siblings): their child process aborts before writing a profile.
- The default build (no `runtime` feature): the self dev-dependency in `Cargo.toml` turns on `async`, `manual`, `sim`, and `serde` for every test build, so the std fallback clock in `src/clock/runtime.rs` (`cfg(not(feature = "runtime"))`) compiles under `just check` but never runs.

### Check first

- `src/weak.rs` (76.9%), `src/lock.rs` (72.0%), and `src/queue.rs` (84.6%) are the lowest files; what is left there is forwarding accessors and `Debug` impls, judged not worth a test.
- Gaps judged out of unit-test reach: the re-checks after registration in `Channel::poll_closed` and `consumer::poll_wanted_changed`, and `log::Read::walk` running past an evicted head, are each a race window. [miri-genmc](/quest/m2/miri-genmc.md) is the route to them.
- Rejections with no UI fixture: `#[kio::wrapper]` on an `extern fn`; `#[kio::machine]` with two methods, with no `wait!`, with a nested `wait!`, with a state name it generates itself, or with a body that does not end in its `Poll::Ready` value; and every `#[kio::test]` rejection. Each one only misreports a compile error, so none was added.
- `a_paused_runtimes_wall_time_moves_with_it` (`src/clock/runtime.rs`) asserts that the host's wall and monotonic reads land within 1 ms of each other, which a preemption between them breaks. It has never been seen to fail; look for it in CI flakes.
- Sample `fix` commits since this run for missing regression tests. The kio history before 866ef06 is the subtree filtered out of `moq-dev/moq`, so moq PR numbers in subjects resolve on moq-dev/moq.

## Audit

- tests

## Scheduled

- 2026-12-03 - one week after the 2026-11-26 run

## Required

- [m1](/quest/m1/README.md) - the audit runs once per milestone
