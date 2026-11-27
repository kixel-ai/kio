# Commits

PRs into `main` land as squash commits, so the PR title becomes the commit subject.

- Use conventional-commit subjects (`feat: ...`, `fix: ...`, `perf: ...`, `docs: ...`, `chore: ...`).

# PRs

Keep the body short and structured, not narrated.
Have at least these sections:

- **Problem**: a summary of the problem and why this PR is needed.
- **Approach**: a summary of the approach taken to solve the problem.
- **Impact**: a bullet point for every public API change. A breaking one says how to migrate, and the PR title marks it with `!` (`feat!: ...`).
- **Alternatives**: any alternative approaches considered.
- **Follow-ups**: any issues encountered or quests created.

# Landing

A PR lands on the maintainer's pick once CI's `check` and `verify` jobs and its local `quest review` rounds pass; queue the reviewed head with `quest merge <pr> --land <sha> --squash`.
No ruleset requires either job yet, so a pending run does not hold the merge: queue only once both pass.
Ask before using the admin bypass of `main`'s rulesets.

# Releasing

Nobody edits a version or `CHANGELOG.md` by hand. Every push to `main` runs `release.yml`, whose last job has release-plz open or regenerate one release PR (`release-plz-*` branch, titled `chore: release`) from the squash titles since each crate's last tag: it bumps `kio` and `kio-macros`, writes `CHANGELOG.md` and `macros/CHANGELOG.md`, and takes a breaking bump from a `!` title or a break `cargo-semver-checks` finds. Merging that PR is the release: the same workflow then publishes to crates.io and tags `kio-v<version>` and `macros-v<version>` (not `kio-macros-v`, which release-plz would also read as a `kio` tag), which is where the next changelog starts. Write each squash title for the changelog reader, and mark a change `chore`, `ci`, `test`, `build`, `bench`, or `quest` only when users should not read it (`.release-plz.toml` leaves those out).

`just ci` fails a release PR whose commit does not sit on `main`'s tip, so it cannot merge before `release.yml` regenerates it; re-run `release.yml` on `main` if that never happens. `just release-pr` is the command CI runs, in `nix develop .#release`.

# Verified code

`src/log.rs` carries a Verus proof of its append and read paths, written in Verus's attribute syntax so the file stays plain Rust. Only Verus sets the `verus_keep_ghost` cfg that turns the proof on, and only the `verify/` package, which builds kio's sources for `just verify`, depends on `vstd` and opts in to `cargo verus`. So plain builds, `just ci`, and downstream crates, Verus ones included, compile no Verus crate for kio, and editing the log needs no Verus. Keep `verify/Cargo.toml`'s dependencies in step with kio's non-optional ones. CI's `verify` job proves it; to run that locally, `nix develop .#verus --command just verify` (Linux only).

- Contracts sit in `#[cfg_attr(verus_keep_ghost, verus_spec(...))]`; proof steps in `proof!`, ghost `let`s in `proof_decl!`, and a call's ghost arguments in `proof_with!` just before it, so inside the `unsafe` block of a call to an `unsafe fn` (inside `open_atomic_invariant!`, a `verus_spec(with ...)` attribute on the statement instead). Ghost types, spec functions, and lemmas live in `verus!` blocks under `#[cfg(verus_keep_ghost)]`, and ghost fields under `#[cfg(verus_keep_ghost_body)]`. Plain builds expand all of these to nothing (`src/log/erased.rs`).
- Every atomic the proof tracks goes through `src/log/atomic.rs` and is accessed inside `open_atomic_invariant!`, one atomic operation per block.
- A change that publishes `filled` before writing its slot, passes an order weaker than `Release` to a tracked store, reads a slot past `filled`, or skips a ghost update fails `verify`, not `ci`. Verus does not check the bodies of the `external_body` atomic wrappers in `src/log/atomic.rs`, so weakening an ordering inside one passes `verify`. The proof is safety-only: storing `end` before `filled`, which makes a reader report a committed entry `Evicted`, still passes. Fix the code or extend the proof; never add an `assume`, and mark any new trusted (`external_body`) function `TRUSTED` with why.
