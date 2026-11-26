# [M] Audit the docs

## Goal

Run the `docs` audit: `quest audit docs` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

Baseline: `866ef06`, the extraction from `moq-dev/moq`, read by the 2026-11-26 run. Start from `git diff 866ef06` and the pages it left untouched.

- The crate docs (`src/lib.rs` and each module's `//!`) are the type and feature tour; `README.md` only points at docs.rs, and its example runs as a doctest through `src/lib.rs`'s `Readme`. Keep lists out of the README.
- `MIGRATE.md` and `macros/MIGRATE.md`: diff the public API against the last crates.io release with `cargo semver-checks check-release -p kio --baseline-version <last>` and every feature on, and match each breaking change to an entry.
- `CHANGELOG.md` entries through 0.6.4 link to `moq-dev/moq` compare views and PRs on purpose; that history stays there.
- Run rustdoc with `-W rustdoc::all -A rustdoc::private_doc_tests -W missing_docs`, once with every feature and once with none; `private_doc_tests` flags `src/task.rs`'s module example on purpose.
- Out of scope: `AGENTS.md` and `CONTRIBUTING.md` (the instructions audit), and MoQ names in public API (`MOQ_SIM_SEED`, `Wall::MOQ_EPOCH`), which the code audit owns.

## Audit

- docs

## Required

- [m1](/quest/m1/README.md) - the milestone this run sat in, so the audit runs once per milestone

## Scheduled

- 2026-12-03 - a week after the last run, so a fast milestone does not rerun it at once
