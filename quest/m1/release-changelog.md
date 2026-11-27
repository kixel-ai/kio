# [S] Write kio's changelog and version bump at release

## Goal

Each release from `kixel-ai/kio`, starting with the first, ships a `CHANGELOG.md` entry for `kio` and `kio-macros` and a version bump that matches its breaking changes, without anyone writing either by hand. The first release publishes `kio` past the `0.6.4` that `moq-dev/moq`'s `release` branch put on crates.io, so moq can move its `kio` dependency from a git rev back to crates.io (moq's `quest/m2/kio-crates-io.md`) before its next release cut.

## Plan

`release.yml` publishes when `main` carries a version crates.io lacks, but nothing here bumps that version or writes `CHANGELOG.md`: in `moq-dev/moq`, release-plz did both, which is why the entries through 0.6.4 have its shape. `[Unreleased]` holds only an entry one PR wrote by hand, though `main` breaks the 0.6.4 API (see `MIGRATE.md`).

Decided 2026-11-26 (maintainer, /quest-run): ✅ keep this quest and use release-plz (rejected: git-cliff for the changelog alone, both by hand, dropping this quest).

Run release-plz in release-PR mode (`release-plz release-pr`), which opens a PR bumping both crates from the conventional squash titles, writing their changelogs, and running cargo-semver-checks, which catches the breaks it has lints for; the bump otherwise follows the squash titles, so a break it misses still gets a breaking-change bump (`0.7.0` from a `0.6.x` release) through its `!` title, which the queue's `just migrate-check` ([required-check](/quest/m1/required-check.md)) ties to a `MIGRATE.md` entry; `release.yml` keeps publishing once that PR merges. Run `release-pr` on every push to `main`, so the open release PR is regenerated after each merge and never ships a breaking change under a bump computed before it. A required merge check blocks a release PR not generated from the current `main` commit, so it cannot merge before that regeneration finishes (decided 2026-11-26, maintainer: ✅ require the check; rejected: accept the gap, revert the regeneration sentence).
Its first run needs a starting point, since the `kio-v*` tags live in `moq-dev/moq`. Start after this repository's copy of the commit `kio-v0.6.2` was cut from: it is the last release on main's line, while 0.6.3 and 0.6.4 were backports on moq's `release` branch, so everything main changed since 0.6.2 is unreleased. Tagging the extraction commit instead would drop all of it from the first entry. That range also holds the main-side commits that 0.6.3 and 0.6.4 already shipped as backports, such as the contended benchmark; leave those out of the first entry, since `CHANGELOG.md` already lists them.
The release PR needs a token that can open PRs; keep it out of pull-request workflows.

Publishing already works: crates.io trusts `kixel-ai/kio`'s `release.yml` in the `crates-io` environment for both `kio` and `kio-macros` (the `moq-dev/moq` publisher on `kio` was removed), and `release.yml` published `kio-macros` `0.1.0` on 2026-11-26 after a `0.0.0` placeholder reserved the name. `kio` on main is still `0.6.2`, which crates.io already holds, so `release.yml` skips it until this quest's first release PR bumps it.
