# [S] Write kio's changelog and version bump at release

## Goal

Each release from `kixel-ai/kio`, starting with the first, ships a `CHANGELOG.md` entry for `kio` and `kio-macros` and a version bump that matches its breaking changes, without anyone writing either by hand.

## Plan

`release.yml` publishes when `main` carries a version crates.io lacks, but nothing here bumps that version or writes `CHANGELOG.md`: in `moq-dev/moq`, release-plz did both, which is why the entries through 0.6.4 have its shape. `[Unreleased]` is empty, though `main` breaks the 0.6.4 API (see `MIGRATE.md`).

Recommended: release-plz in release-PR mode (`release-plz release-pr`), which opens a PR bumping both crates from the conventional squash titles, writing their changelogs, and running cargo-semver-checks so a breaking change cannot ship as a minor bump; `release.yml` keeps publishing once that PR merges.
Its first run needs a starting point, since the `kio-v*` tags live in `moq-dev/moq`: tag `kio-v0.6.4` here at the extraction commit, or set the changelog's starting commit, so the first entry covers only the changes after 0.6.4.
The release PR needs a token that can open PRs; keep it out of pull-request workflows.

## Waiting

- Maintainer: generate releases with release-plz release PRs (recommended: it already wrote this changelog and adds a semver check), use git-cliff for the changelog alone while versions stay hand-bumped, or keep both by hand with an `[Unreleased]` entry per PR.

## Related

- [crates.io publishing](/quest/m1/crates-io-publish.md) - the first release this writes the entry and bump for
