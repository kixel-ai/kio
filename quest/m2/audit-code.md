# [L] Audit the code

## Goal

Run the `code` audit: `quest audit code` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

Baseline from the 2026-11-26 run, the first since kio left `moq-dev/moq`:

- Lint suppressions: the workspace denies `clippy::allow_attributes`, so every suppression is an `#[expect]` that fails once stale. Two hold only for some feature sets and sit behind `cfg_attr`: `Timer`'s `large_enum_variant` in `src/clock/timer.rs` and `Clock`'s `unreachable_patterns` arm in `src/clock/captured.rs`. Recheck both across the feature matrix (none, each feature alone, all), since `just check` lints only three builds. The `#[allow]`s inside `kio-macros`' `quote!` output and in `macros/tests/ui/*.rs` trybuild fixtures are generated or uncompiled by clippy, so the lint does not reach them; keep them.
- MoQ leftovers: grep for `moq`, `relay`, `broadcast`, and `group` in code and comments first. `AGENTS.md` keeps kio free of MoQ assumptions.
- Public items unused in this repository have their users downstream: grep `moq-dev/moq`'s `rs/` (outside any vendored `rs/kio`) before calling one dead.
- Kept on purpose, skip unless the code around them changed:
  - `macros/tests/ui.rs` re-runs itself without `RUSTC_WRAPPER` and with `MBX_DISABLE=1`, because the maintainer's mbx rustc wrapper changes trybuild's diagnostics.
  - `unsafe impl Send/Sync` for `Channel` and `Lock` restate the derived bounds on purpose, so recursive channel graphs don't expand the proof at every level.
  - `Producer::poll` and `Shared::poll` share a body except the closed check; merging adds a flag parameter for 12 lines.
  - The ambient clock lookups (`clock::now`, `wall`, `origin`, `Clock::current`) each pick their source in a slightly different order, and `now` is hot, so they stay separate rather than resolving through one `Clock`.
  - `serde::UnixSeconds` saturates out-of-range seconds, as documented.
  - `Instant` and `Wall` repeat their checked and saturating arithmetic rather than share a macro.

## Audit

- code

## Required

- [m1](/quest/m1/README.md) - the next milestone's work, so this audit runs once per milestone

## Scheduled

- 2026-12-03 - a week after the last run, so a fast milestone does not rerun it at once
