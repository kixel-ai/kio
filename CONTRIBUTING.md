# Commits

PRs into `main` land as squash commits, so the PR title becomes the commit subject.

- Use conventional-commit subjects (`feat: ...`, `fix: ...`, `perf: ...`, `docs: ...`, `chore: ...`).

# PRs

Keep the body short and structured, not narrated.
Have at least these sections:

- **Problem**: a summary of the problem and why this PR is needed.
- **Approach**: a summary of the approach taken to solve the problem.
- **Impact**: a bullet point for every public API change. A breaking one says how to migrate, and the PR title marks it with `!` (`feat!: ...`).
  A `!` title also needs an entry in `MIGRATE.md` (or `macros/MIGRATE.md` for `kio-macros`); `just migrate-check "<title>"` checks a committed branch against `origin/main`.
- **Alternatives**: any alternative approaches considered.
- **Follow-ups**: any issues encountered or quests created.

# Landing

CI runs no checks on pull requests: run `just ci` before pushing, and `just verify` too for a change to `src/`, `verify/`, `Cargo.toml`, `Cargo.lock`, or the toolchain (`rust-toolchain.toml`, `nix/verus.nix`), since the proof builds the whole crate with Verus's pinned rustc (see Verified code).
A PR lands on the maintainer's pick once those and its local `quest review` rounds pass; queue the reviewed head with `quest merge <pr> --land <sha> --squash`.
`check.yml` and `post-merge.yml` run after each push to `main`, and a failure in either files an issue.
Ask before using the admin bypass of `main`'s rulesets.

# Releasing

Nobody edits a version or `CHANGELOG.md` by hand. Every push to `main` runs `release.yml`, whose last job has release-plz open or regenerate one release PR (`release-plz-*` branch, titled `chore: release`) from the squash titles since each crate's last tag: it bumps `kio` and `kio-macros`, writes `CHANGELOG.md` and `macros/CHANGELOG.md`, and takes a breaking bump from a `!` title or a break `cargo-semver-checks` finds. Merging that PR is the release: the same workflow then publishes to crates.io from the commit that set the version (so a later push or a rerun never ships unversioned changes) and tags `kio-v<version>` and `macros-v<version>` (not `kio-macros-v`, which release-plz would also read as a `kio` tag), which is where the next changelog starts. Write each squash title for the changelog reader, and mark a change `chore`, `ci`, `test`, `build`, `bench`, or `quest` only when users should not read it (`.release-plz.toml` leaves those out).

Until the merge queue lands, nothing re-checks a release PR at merge time, so a release PR that went stale (`main` moved after it was opened) would otherwise land. Before queueing one, check out its `release-plz-*` branch and run `just ci`: its `release-fresh` step fails a branch that does not contain `origin/main`'s tip. If it fails, wait for the latest `Release` run on `main` to regenerate the PR, or re-run `release.yml` on `main`. `just release-pr` is the command CI runs, in `nix develop .#release`.

The `pr` job authenticates as the `kixel-ai-release` GitHub App (Contents and Pull requests read/write, installed only on this repository); its `APP_CLIENT_ID` and `APP_PRIVATE_KEY` live in the `release-pr` environment, which only `main` can deploy to.

# Verified code

`src/log.rs` carries a Verus proof of its append and read paths, written in Verus's attribute syntax so the file stays plain Rust. Only Verus sets the `verus_keep_ghost` cfg that turns the proof on, and only the `verify/` package, which builds kio's sources for `just verify`, depends on `vstd` and opts in to `cargo verus`. So plain builds, `just ci`, and downstream crates, Verus ones included, compile no Verus crate for kio, and editing the log needs no Verus. Keep `verify/Cargo.toml`'s dependencies in step with kio's non-optional ones. CI's `verify` job proves it on `main`; to run it locally, `nix develop .#verus --command just verify` (Linux only).

- Contracts sit in `#[cfg_attr(verus_keep_ghost, verus_spec(...))]`; proof steps in `proof!`, ghost `let`s in `proof_decl!`, and a call's ghost arguments in `proof_with!` just before it, so inside the `unsafe` block of a call to an `unsafe fn` (inside `open_atomic_invariant!`, a `verus_spec(with ...)` attribute on the statement instead). Ghost types, spec functions, and lemmas live in `verus!` blocks under `#[cfg(verus_keep_ghost)]`, and ghost fields under `#[cfg(verus_keep_ghost_body)]`. Plain builds expand all of these to nothing (`src/log/erased.rs`).
- Every atomic the proof tracks goes through `src/log/atomic.rs` and is accessed inside `open_atomic_invariant!`, one atomic operation per block.
- A change that publishes `filled` before writing its slot, passes an order weaker than `Release` to a tracked store, reads a slot past `filled`, or skips a ghost update fails `verify`, not `ci`. Verus does not check the bodies of the `external_body` atomic wrappers in `src/log/atomic.rs`, so weakening an ordering inside one passes `verify`. The proof is safety-only: storing `end` before `filled`, which makes a reader report a committed entry `Evicted`, still passes. Fix the code or extend the proof; never add an `assume`, and mark any new trusted (`external_body`) function `TRUSTED` with why.
