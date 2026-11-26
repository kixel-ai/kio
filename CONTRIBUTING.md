# Commits

PRs into `main` land as squash commits, so the PR title becomes the commit subject.

- Use conventional-commit subjects (`feat: ...`, `fix: ...`, `perf: ...`, `docs: ...`, `chore: ...`).
- Never commit compiled binaries or build output.

# PRs

Keep the body short and structured, not narrated.
Have at least these sections:

- **Problem**: a summary of the problem and why this PR is needed.
- **Approach**: a summary of the approach taken to solve the problem.
- **Impact**: a bullet point for every public API change, marking breaking ones with `!`.
- **Alternatives**: any alternative approaches considered.
- **Follow-ups**: any issues encountered or quests created.

# Landing

A PR lands once CI's `check` job and its local `quest review` rounds pass; queue the reviewed head with `quest merge <pr>`.
The admin bypass of `main`'s rulesets is for emergencies.

# Verified code

`src/log.rs` carries a Verus proof of its append and read paths, written in Verus's attribute syntax so the file stays plain Rust. Only Verus sets the `verus_keep_ghost` cfg that turns the proof on, and only the `verify/` package, which builds kio's sources for `just verify`, depends on `vstd` and opts in to `cargo verus`. So plain builds, `just ci`, and downstream crates, Verus ones included, compile no Verus crate for kio, and editing the log needs no Verus. Keep `verify/Cargo.toml`'s dependencies in step with kio's non-optional ones. CI's `verify` job proves it; to run that locally, `nix develop .#verus --command just verify` (Linux only).

- Contracts sit in `#[cfg_attr(verus_keep_ghost, verus_spec(...))]`; proof steps in `proof!`, ghost `let`s in `proof_decl!`, and a call's ghost arguments in `proof_with!` just before it, so inside the `unsafe` block of a call to an `unsafe fn` (inside `open_atomic_invariant!`, a `verus_spec(with ...)` attribute on the statement instead). Ghost types, spec functions, and lemmas live in `verus!` blocks under `#[cfg(verus_keep_ghost)]`, and ghost fields under `#[cfg(verus_keep_ghost_body)]`. Plain builds expand all of these to nothing (`src/log/erased.rs`).
- Every atomic the proof tracks goes through `src/log/atomic.rs` and is accessed inside `open_atomic_invariant!`, one atomic operation per block.
- A change that moves a store, weakens a `Release`, reads a slot past `filled`, or skips a ghost update fails `verify`, not `ci`. Fix the code or extend the proof; never add an `assume`, and mark any new trusted (`external_body`) function `TRUSTED` with why.
