# [XS] Release kio past 0.6.4 from the first release PR

## Goal

crates.io holds a `kio` past the `0.6.4` that `moq-dev/moq`'s `release` branch published, and `kio-macros` `0.1.1`, from the first release PR that release-plz opened in `kixel-ai/kio`, so `moq-dev/moq` can move its `kio` dependency from a git rev back to crates.io (moq's `quest/m2/kio-crates-io.md`) before its next release cut.

## Plan

`release.yml` now ends in a `pr` job that runs `release-plz release-pr` on every push to `main`, after it publishes and tags (see `CONTRIBUTING.md`'s Releasing section). It needs an App token that can open PRs, held by the `release-pr` environment (created, deployable from `main` only); the App itself and its two secrets are the maintainer's.

Once the token exists, a push to `main` opens the first release PR. It carries `0.7.0` for `kio`, from the `!` titles and the breaks cargo-semver-checks finds, and `0.1.1` for `kio-macros`. Merging it publishes both crates and tags `kio-v0.7.0` and `macros-v0.1.1`.

After that release publishes:
- Drop the "take kio from Git until the next release ships" notes in `README.md` (the install line and the docs line that says it covers 0.6.4).
- Rename `MIGRATE.md`'s `## Unreleased` heading to the released version (by hand this once; if the next release needs it again, make `release.yml` do it).

## Waiting

- Maintainer (PC): add the release-plz App token, since an agent cannot create an App or read its private key. Create a GitHub App (or reuse `moq-bot` or `kixel-quest`, both installed on every `kixel-ai` repository) with repository permissions Contents and Pull requests set to read and write, installed on `kixel-ai/kio`; then add its client ID as `APP_CLIENT_ID` and its private key as `APP_PRIVATE_KEY` to the `release-pr` environment of `kixel-ai/kio` (Settings, Environments), not as repository secrets. Add both to Bitwarden Secrets Manager too, so the next repository can reuse them. Check with `gh api repos/kixel-ai/kio/environments/release-pr/secrets --jq '.secrets[].name'`, which should list both, then run `gh workflow run release.yml -R kixel-ai/kio --ref main` and expect a `chore: release` PR.

## Required

- [Drop kio's MoQ names](/quest/m1/api-trims.md) - the breaking changes that should ride the first release, landed before it
