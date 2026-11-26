# [L] Audit build and CI cost

## Goal

Run the `cost` audit: `quest audit cost` prints what it checks, what each finding becomes, and how to plan its next run.

## Audit

- cost

## Required

- [m1](/quest/m1/README.md) - the cost audit runs once per milestone, after the last one's work lands

## Scheduled

- 2026-12-03 - a week after the m1 run, so a milestone that closes fast does not rerun it at once

## Plan

Baseline from the m1 run (2026-11-26), on GitHub-hosted `ubuntu-24.04`, from each job's own step timings:

- `check` job on a PR with a warm cargo cache, before the m1 fixes (`bbc4ebc`, run 45783132984): 102 s, of which `nix develop` building the dev shell took 71 s and `just ci` 14 s.
- The same job after them (`1cf5f6d`, run 45790618814, cold cargo cache because the toolchain file is in the key): 76 s, `nix develop` 23 s, cargo 30 s cold (clippy 12.4 s, test build 18.1 s), 213 lib tests 0.23 s.
- Cold cargo on `main` (`866ef06`, run 45766188406): clippy 15.7 + 0.7 + 9.5 s, test build 22.1 s, separate doctest build 6.6 s, trybuild 4.4 s.
- Published `.crate` sizes from `cargo package --workspace --no-verify`: `kio` 0.6.2 134,842 B (43 files), `kio-macros` 0.1.0 24,294 B (61 files).
- `Release` runs on every push to `main`: `plan` 17 s, plus `package` 64 s whenever a version is unpublished.

Check first:

- Whether the `check` job moved to a self-hosted runner with a persistent Nix store. On GitHub-hosted runners the dev shell is still the largest step, mostly unpacking and patching rust-overlay's `rustc` (about 9 to 30 s), which no binary cache serves.
- Whether `rust-toolchain.toml` still sets `profile = "minimal"`. The default profile pulls in rust-docs (677 MB, 67k files).
- The `kio` package list (`cargo package --list -p kio`). The `include` allowlist keeps repository tooling out.

Measured and left alone:

- Criterion's dependency tree is 36 of the workspace's 91 packages and about 39% of cold build unit time. A warm PR run pays nothing for it, because the `main` cache holds it. Moving the benches into an unpublished crate would help only cold builds. Revisit if cold builds become common, such as cacheless post-merge Windows or macOS lanes.
- A `restore-keys` fallback for the cargo cache: a toolchain bump invalidates the whole target directory, and only lockfile-changing PRs would gain.
- Skipping cargo on quest-only PRs: the dev shell is needed for `quest check` anyway, and warm cargo is about 14 s.
- Measurements on the agent host: its load average was over 5,000, so its wall times only rank work. They are not a baseline.
