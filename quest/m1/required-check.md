# [M] Land kio through GitHub's merge queue, built on a self-hosted runner

## Goal

GitHub's merge queue lands every PR into `main`. It builds each picked PR on top of `main` on a self-hosted kio runner and merges exactly the tree it tested, so landing one PR no longer makes every other pick re-merge `main` and rerun CI.
`main` requires the queue build's check, so nothing lands while it is pending or failed.
No job that a `pull_request` or `merge_group` run reaches uses GitHub's runners.
`CONTRIBUTING.md`'s Landing section drops "No ruleset requires `check` yet" and `--squash`, since the queue applies its own merge method.

## Plan

Decided 2026-11-26 (maintainer, /quest-run): ✅ keep, widened to also turn on GitHub's merge queue with a self-hosted `merge_group` run, so one quest covers the kio runner group, the required check, and the queue (rejected: drop; a separate merge-queue quest; no queue).
The reason: `quest merge` lands only heads that contain `main`. Each landing therefore forced every other picked PR to re-merge `main` and rerun CI, one at a time, as landing kixel-ai/kio#2, #3, and #13 did that day.

Today `main`'s only ruleset is the org's `Pull request` rule, so `quest merge` lands past a pending `check`.
[kixel-ai/dotfiles#369](https://github.com/kixel-ai/dotfiles/pull/369) bars a public repository's merge gates from GitHub's runners; its other jobs may stay there.

Recommended: follow kixel-ai/quest, the nearest kixel-ai repository on a merge queue.
- `queue.yml` runs on `merge_group` and calls `kixel-ai/kio/.github/workflows/queue-build.yml@main`.
  - That workflow runs `just ci` on `[self-hosted, kio-queue]` and posts the required `Queue` check run.
  - The `kio-queue` group admits only that workflow at `refs/heads/main`, so only a maintainer's pick reaches the runner. A PR's edit to `queue-build.yml` runs only after it lands.
- `queue-pr.yml` reports `Queue` as skipped on each PR, so GitHub admits the PR to the queue.
- `check.yml` drops `pull_request` and runs only on pushes to `main`, which may stay on GitHub's runners.
  - Rejected: keeping a PR-level `check` on a fork-safe self-hosted runner. Those runners don't exist yet (dotfiles' `quest/m0/no-github-runners.md`), and they would double each PR's CI for feedback that `just ci` before pushing already gives.
- Add a `main` ruleset like kixel-ai/quest's:
  - It requires `Queue` from GitHub Actions.
  - Its merge queue squashes, builds and merges one entry at a time, and times out after 90 minutes.
  - Create it only once the three queue workflows are on `main`; until then nothing reports `Queue`, and every PR would wait on it.
  - `REQUIRED_CHECK` in dotfiles' `bin/repo-settings` does not fit: it requires a check on the PR itself, and it sets no merge queue.
- Validate by landing one PR through the queue: a merge group run on the kio runner, `Queue` passing, and a squash commit on `main`.

## Waiting

- kixel-ai/dotfiles: a `kio-queue` runner group, holding a kio runner on nixos, admits only `kixel-ai/kio/.github/workflows/queue-build.yml@refs/heads/main`, as `rind-queue` does for rind. Advance it with a dotfiles PR adding a `kio-ci` entry beside `rind-ci` in `nixos/modules/ci-runner-list.nix`, plus a kio entry in `nixos/modules/quest-land.nix` so a dropped pick is fixed and re-enqueued. Check that `gh api orgs/kixel-ai/actions/runner-groups` lists `kio-queue`.
