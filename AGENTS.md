kio provides poll-native async shared-state primitives, a deterministic clock and simulator, and state-machine macros (`kio-macros`). See [README.md](README.md) and [quest/README.md](quest/README.md).

# Required

- Every wait is a `poll_*` method first. The `async` feature adds `async fn` wrappers (via `#[kio::wrapper]`); the default build compiles no coroutines and no host timer runtime.
- Keep `kio` free of domain-specific (MoQ) assumptions.
- Every `unsafe` block states its safety invariant inline and has tests exercising the ordering or lifetime rule it relies on.
- Unit tests use `kio::clock::sim` or `kio::clock::Manual` instead of sleeping on the host clock, except explicit host-clock tests gated for Miri.
- Every clippy suppression carries a `reason = "..."`.

# Development

PRs target `main`. Run `just ci` before pushing; outside the dev shell, run `nix develop --command just ...`.

Quests: before any work, run `quest context` and follow it, or read https://github.com/kixel-ai/quest/blob/main/docs/context.md where `quest` is not installed, unless this session already holds its rules.
