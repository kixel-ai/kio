# [S] Write kio's changelog and version bump at release

## Goal

Each release from `kixel-ai/kio`, starting with the first, ships a `CHANGELOG.md` entry for `kio` and `kio-macros` and a version bump that matches its breaking changes, without anyone writing either by hand.

## Plan

`release.yml` publishes when `main` carries a version crates.io lacks, but nothing here bumps that version or writes `CHANGELOG.md`: in `moq-dev/moq`, release-plz did both, which is why the entries through 0.6.4 have its shape. `[Unreleased]` holds only an entry one PR wrote by hand, though `main` breaks the 0.6.4 API (see `MIGRATE.md`).

Decided 2026-11-26 (maintainer, /quest-run): ✅ keep this quest and use release-plz (rejected: git-cliff for the changelog alone, both by hand, dropping this quest).

Run release-plz in release-PR mode (`release-plz release-pr`), which opens a PR bumping both crates from the conventional squash titles, writing their changelogs, and running cargo-semver-checks, which catches the breaks it has lints for; the bump otherwise follows the squash titles, so a break it misses still bumps the major version through its `!` title, which [migrate-check](/quest/m1/migrate-check.md) ties to a `MIGRATE.md` entry; `release.yml` keeps publishing once that PR merges.
Its first run needs a starting point, since the `kio-v*` tags live in `moq-dev/moq`. Start after this repository's copy of the commit `kio-v0.6.2` was cut from: it is the last release on main's line, while 0.6.3 and 0.6.4 were backports on moq's `release` branch, so everything main changed since 0.6.2 is unreleased. Tagging the extraction commit instead would drop all of it from the first entry. That range also holds the main-side commits that 0.6.3 and 0.6.4 already shipped as backports, such as the contended benchmark; leave those out of the first entry, since `CHANGELOG.md` already lists them.
The release PR needs a token that can open PRs; keep it out of pull-request workflows.

## Related

- [crates.io publishing](/quest/m1/crates-io-publish.md) - the first release this writes the entry and bump for
