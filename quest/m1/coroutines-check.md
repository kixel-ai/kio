# [XS] Check that kio without its async feature compiles no coroutines

## Goal

`just check` fails when `kio`, built without its `async` feature, compiles a coroutine: an `async fn` body, `async` block, or `async` closure. `AGENTS.md`, `Cargo.toml`, and `src/lib.rs` all promise this, and nothing here checks it.

## Plan

`moq-dev/moq` checked it with `sh/rs/coroutines.sh kio -- --no-default-features --features macros,runtime,manual,serde`, which emits the crate's own MIR (`cargo rustc --lib -- --emit mir`) and fails on any coroutine name rustc gives it, such as `{async fn body of ...}`.
[moq-dev/moq#9675](https://github.com/moq-dev/moq/pull/9675) dropped that line when kio moved out, and [kixel-ai/kio#1](https://github.com/kixel-ai/kio/pull/1)'s list of restored lanes missed it.
Port it as a `just` recipe that `check` runs, with the same features, and say in the PR how much time it adds to the gate.
