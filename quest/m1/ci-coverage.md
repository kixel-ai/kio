# [S] Restore cross-platform, Miri, rustdoc, and bench CI coverage

## Goal

`kixel-ai/kio`'s GitHub Actions workflows cover the verification lanes that previously ran for `kio` inside `moq-dev/moq`:

- Windows and macOS test runs covering `kio`'s platform-specific clock backends.
- Nightly Miri runs covering `kio` under Stacked Borrows and `kio::log` under `-Zmiri-tree-borrows -Zmiri-ignore-leaks`.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --workspace --all-features` (matching `moq`'s `coroutines` doc check).
- Benchmark compile/run coverage (`cargo bench --locked --no-run` in PR checks, plus nightly Criterion / CodSpeed tracking if desired).

## Plan

When `kio` moved out of `moq-dev/moq`, `check.yml` brought over Linux clippy (default, `--no-default-features`, `--all-features`), `cargo fmt`, `cargo test --workspace` (including `kio-macros` trybuild UI tests), `cargo test --doc`, `actionlint`, `taplo`, `nixfmt`, and `quest check`, leaving the platform-specific and nightly lanes for this follow-up.

- Add Windows and macOS test jobs to `.github/workflows/check.yml` (or a dedicated workflow).
- Add a scheduled nightly workflow running Miri (strict pass + `kio::log` tree-borrows pass) and `cargo bench` compile/run checks.
- Add `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --workspace --all-features` to `just check` / `check.yml`.

Public API: none.
