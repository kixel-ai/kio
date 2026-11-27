# [XS] Release kio past 0.6.4 from the first release PR

## Goal

crates.io holds a `kio` past the `0.6.4` that `moq-dev/moq`'s `release` branch published, and `kio-macros` `0.1.1`, from the first release PR that release-plz opened in `kixel-ai/kio`, so `moq-dev/moq` can move its `kio` dependency from a git rev back to crates.io (moq's `quest/m2/kio-crates-io.md`) before its next release cut.

## Plan

`release.yml` now ends in a `pr` job that runs `release-plz release-pr` on every push to `main`, after it publishes and tags (see `CONTRIBUTING.md`'s Releasing section). It needs an App token that can open PRs, held by the `release-pr` environment (created, deployable from `main` only); the `kixel-ai-release` GitHub App (Contents and Pull requests read/write, installed only on this repository) and its two secrets exist there.

Once this merges, the next push to `main` opens the first release PR. It carries `0.7.0` for `kio`, from the `!` titles and the breaks cargo-semver-checks finds, and `0.1.1` for `kio-macros`. Merging it publishes both crates and tags `kio-v0.7.0` and `macros-v0.1.1`.

After that release publishes:
- Drop the "take kio from Git until the next release ships" notes in `README.md` (the install line and the docs line that says it covers 0.6.4).
- Rename `MIGRATE.md`'s `## Unreleased` heading to the released version (by hand this once; if the next release needs it again, make `release.yml` do it).
