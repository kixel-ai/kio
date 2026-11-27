# [M] Land kio through GitHub's merge queue, built on a self-hosted runner

## Goal

GitHub's merge queue lands every PR into `main`. It builds each picked PR on top of `main` on a self-hosted kio runner and merges exactly the tree it tested, so landing one PR no longer makes every other pick re-merge `main` and rerun CI.
`main` requires the queue build's check, so nothing lands while it is pending or failed.
No job that a `pull_request` or `merge_group` run reaches uses GitHub's runners.
`CONTRIBUTING.md`'s whole Landing section is rewritten for the queue. The maintainer queues the reviewed head once `just ci` and its local `quest review` rounds pass, and GitHub waits for `Queue` before merging. No step waits for a CI `check` on the PR, and no `--squash` remains, since the queue applies its own merge method.

## Plan

Decided 2026-11-26 (maintainer, /quest-run): ✅ keep, widened to also turn on GitHub's merge queue with a self-hosted `merge_group` run, so one quest covers the kio runner group, the required check, and the queue (rejected: drop; a separate merge-queue quest; no queue).
The reason: `quest merge` lands only heads that contain `main`. Each landing therefore forced every other picked PR to re-merge `main` and rerun CI, one at a time, as landing kixel-ai/kio#2, #3, and #13 did that day.

Decided 2026-11-26 (maintainer, /quest-run): ✅ the merge queue is the only gate, with no CI on PRs (rejected: also a CI check on PRs on a fork-safe self-hosted runner). Those runners don't exist yet (dotfiles' `quest/m0/no-github-runners.md`), and they would double each PR's CI for feedback that `just ci` before pushing already gives.

Decided 2026-11-26 (maintainer, /quest-run): ✅ fix the three findings from round 3 of kixel-ai/kio#6's review, as below (rejected: accept them): the `merge_group` guard on every `kio-queue` job, reporting `Queue` on PRs already open before the ruleset, and the full Landing rewrite in the Goal.

Today `main`'s only ruleset is the org's `Pull request` rule, so `quest merge` lands past a pending `check`.
[kixel-ai/dotfiles#369](https://github.com/kixel-ai/dotfiles/pull/369) bars a public repository's merge gates from GitHub's runners; its other jobs may stay there.

Follow kixel-ai/quest, the nearest kixel-ai repository on a merge queue:
- `queue.yml` runs on `merge_group` and calls `kixel-ai/kio/.github/workflows/queue-build.yml@main`.
  - That workflow runs what `check.yml`'s two jobs run today on `[self-hosted, kio-queue]`: `just ci`, and `just verify` in `nix develop .#verus`. It then posts the required `Queue` check run.
  - Before `just ci`, it runs `just migrate-check "$TITLE" "$BASE"`, so a breaking `!` title that changes neither `MIGRATE.md` nor `macros/MIGRATE.md` fails `Queue`. `BASE` is `github.event.merge_group.base_sha`, so the checkout needs that commit (`fetch-depth: 0`). `TITLE` is the PR's title, fetched with the read-only token by `gh pr view <n> --json title -q .title`, where `<n>` comes from the merge group's `head_ref` (`refs/heads/gh-readonly-queue/main/pr-<n>-<sha>`). Pass both through `env`, never `${{ }}` in the script, since the title is the PR author's. Then delete the interim `migrate.yml`, which only reports a miss after it lands.
  - The `kio-queue` group admits only that workflow at `refs/heads/main`, so a PR's edit to `queue-build.yml` runs only after it lands.
  - The group's allowlist names the workflow that defines a job, not the event that called it. A PR's workflow could still call `queue-build.yml@main` and reach the runner before anyone queues the PR.
  - So every job on `kio-queue` also needs `if: github.event_name == 'merge_group'`; for the report job, that is `always() && github.event_name == 'merge_group'`. Quest's `queue-build.yml` does the same.
- `queue-pr.yml` reports `Queue` as skipped on each PR, so GitHub admits the PR to the queue. Its `workflow_dispatch` input reports the same on a given head.
- `check.yml` drops `pull_request` and runs only on pushes to `main`, which may stay on GitHub's runners.
- Add a `main` ruleset like kixel-ai/quest's:
  - It requires `Queue` from GitHub Actions.
  - Its merge queue squashes, builds and merges one entry at a time, and times out after 90 minutes.
  - Create it only once the three queue workflows are on `main`; until then nothing reports `Queue`, and every PR would wait on it.
  - Before creating it, run `gh workflow run queue-pr.yml -f sha=<head>` for each open PR's head. A PR whose test merge commit predates `queue-pr.yml` gets no `pull_request` run from it, so without this it would wait on `Queue` forever.
  - `REQUIRED_CHECK` in dotfiles' `bin/repo-settings` does not fit: it requires a check on the PR itself, and it sets no merge queue.
- Validate the rollout by landing, through the queue, a PR that was already open before the ruleset: `Queue` reported skipped on its head, a merge group run on the kio runner, `Queue` passing, and a squash commit on `main`.

## Waiting

- kixel-ai/dotfiles: a `kio-queue` runner group, holding a kio runner on nixos, admits only `kixel-ai/kio/.github/workflows/queue-build.yml@refs/heads/main`, as `rind-queue` does for rind. Advance it with a dotfiles PR adding a `kio-ci` entry beside `rind-ci` in `nixos/modules/ci-runner-list.nix`, plus a kio entry in `nixos/modules/quest-land.nix` so a dropped pick is fixed and re-enqueued. Check that `gh api orgs/kixel-ai/actions/runner-groups` lists `kio-queue`.
