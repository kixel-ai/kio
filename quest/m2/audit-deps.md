# [M] Audit dependencies

## Goal

Run the `deps` audit: `quest audit deps` prints what it checks, what each finding becomes, and how to plan its next run.

## Audit

- deps

## Plan

- Baseline: the previous run covered releases through 2026-11-26.
- Scope: the Cargo manifests, `rust-toolchain.toml`, the flake's inputs, and the
  GitHub Actions pinned in `.github/`. The flake inputs are commit pins, so they
  get health checks only. kio has no forks, `[patch]` entries, or vendored code.
- Check first:
  - Cargo's built-in `cargo::unused_dependencies` lint (Rust 1.100) could
    replace `cargo shear` in `just check`. On 1.100 it caught an unused
    dependency only in lib-only builds and stayed silent under `--all-targets`,
    so it could not replace cargo-shear yet.
  - `Counts::join_consumers` (`src/lib.rs`) hand-rolls a `compare_exchange_weak`
    loop, because `fetch_update` is deprecated and its replacement, `try_update`,
    needs Rust 1.95. Switch once the workspace `rust-version` floor reaches 1.95.
  - smallvec 2.0 was in beta (2.0.0-beta.5); kio uses 1.x's `union` feature.
  - criterion, which only the benches use, is the last dependency on syn 2.
  - Toolchain bumps change rustc diagnostics, so re-bless
    `macros/tests/ui/*.stderr` with `TRYBUILD=overwrite`. The toolchain must stay
    at 1.98 or later for macOS 27 (see `rust-toolchain.toml`).

## Required

- [m1](/quest/m1/README.md) - the audit runs once per milestone

## Scheduled

- 2026-12-03 - a week after the previous run

## Related

- [Crossbeam pin fence](/quest/m2/crossbeam-pin-fence.md) - owns the crossbeam-epoch release wait
