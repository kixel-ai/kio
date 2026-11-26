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

`src/log.rs` holds its append and read paths in Verus's `verus!` dialect. Plain `cargo build` and `just ci` erase the proof, so editing it needs no Verus. CI's `verify` job proves it; to run that locally, `nix develop .#verus --command just verify` (Linux only).

- Code inside `verus!` is still Rust: `requires`/`ensures` clauses, `proof { ... }` blocks, and `Tracked`/`Ghost` values vanish from plain builds.
- Every atomic the proof tracks goes through `src/log/atomic.rs` and is accessed inside `open_atomic_invariant!`, one atomic operation per block.
- A change that moves a store, weakens a `Release`, reads a slot past `filled`, or skips a ghost update fails `verify`, not `ci`. Fix the code or extend the proof; never add an `assume`, and mark any new trusted (`external_body`) function `TRUSTED` with why.
