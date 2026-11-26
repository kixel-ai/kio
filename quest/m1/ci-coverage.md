# [S] Restore cross-platform, Miri, rustdoc, and bench CI coverage

## Goal

`kixel-ai/kio`'s GitHub Actions cover the verification lanes that ran for `kio` inside `moq-dev/moq` before the move:

- The PR gate (`just check`, so `check.yml`) fails on rustdoc warnings: `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --workspace --all-features`.
- A post-merge workflow on `main` runs `kio`'s tests on Windows and macOS (the platform-specific clock backends), both Miri passes, and each Criterion bench once.
- A failed post-merge run files work without anyone watching the Actions tab.

Out of scope: performance tracking (Criterion history, CodSpeed) and Miri's GenMC mode, which [its own quest](/quest/m2/miri-genmc.md) owns.

## Plan

`check.yml` already runs Linux clippy over three feature sets (`--all-targets`, so bench targets type-check on every PR), `cargo fmt`, `cargo test --workspace` (including `kio-macros` trybuild UI tests), `cargo test --doc`, `actionlint`, `taplo`, `nixfmt`, and `quest check`.

Decided 2026-11-26 (/quest-iterate, user afk; the agent's recommendations):

- ✅ Windows, macOS, Miri, and the bench smoke run post-merge, not in the PR gate (rejected: adding them to `check.yml`). Context's shipping rule keeps cross-platform and slow suites off the gate, and moq dropped its Windows and macOS PR gates for cost in moq-dev/moq@0c139f57fa (#2600); moq ran these lanes only in its nightly.
- ✅ Trigger the post-merge workflow on `push` to `main` plus `workflow_dispatch`, coalesced with `concurrency: { group: post-merge, cancel-in-progress: true }` (rejected: a daily cron, which re-runs unchanged code and reports a day late).
- ✅ The bench lane runs `cargo test --locked -p kio --benches`, Criterion's test mode that runs each bench once, on Linux (rejected: `cargo bench --no-run` in PR checks, which adds only the link step to what `clippy --all-targets` already checks; rejected: Criterion or CodSpeed tracking, since nobody consumes the numbers and hosted runners are too noisy to gate on). This answers the CodeRabbit finding on [kixel-ai/kio#1](https://github.com/kixel-ai/kio/pull/1#discussion_r4704738717).
- ✅ rustdoc `-D warnings` joins `just check`, so it gates PRs and runs locally (rejected: post-merge only; it costs seconds).
- ✅ A failed post-merge run opens a GitHub issue, or comments on the open one with the same title, from a final `if: failure()` job holding `issues: write` (rejected: relying on GitHub's failure email, which reaches only the pusher). `main` is protected, so the write token stays off PR workflows.
- ✅ Miri runs as a `just miri` recipe, two passes ported from moq's `sh/rs/miri.sh` without nextest: `cargo miri test --locked -p kio --lib -- --skip log::` (Stacked Borrows, leak check), then `MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-ignore-leaks" cargo miri test --locked -p kio --lib log::` (crossbeam-epoch's collector breaks Stacked Borrows and never frees at exit). Nightly Rust with `miri` and `rust-src` comes from `dtolnay/rust-toolchain`, as moq's nightly did. Fail if either pass runs zero tests, so a renamed module cannot empty a pass.
- ✅ Windows and macOS run `cargo test --locked --workspace --all-features` with rustup reading `rust-toolchain.toml` (rejected: Nix, which does not run on Windows).
- ✅ Linux post-merge jobs use whatever runner `check.yml` uses; this quest does not pick runner labels.

Public API: none. Wire: none.

## Related

- [Miri's GenMC mode explores every schedule of kio's lock-free code](/quest/m2/miri-genmc.md) - adds a GenMC pass to the Miri lane this quest restores
