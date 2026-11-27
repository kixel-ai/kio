# [S] Land kio through Kixel Quest's merge queue

## Goal

Kixel Quest's hosted merge queue lands every PR into `main`. It builds each picked PR on top of `main` and merges exactly the tree it tested, so landing one PR no longer makes every other pick re-merge `main` and rerun its checks, and `main` accepts nothing that skipped it.
Its gate runs `just ci`, `just verify` in `nix develop .#verus`, and `just migrate-check` on the PR's title against the queue's base, so a breaking `!` title that changes neither `MIGRATE.md` nor `macros/MIGRATE.md` fails before it lands. The interim `migrate.yml`, which reports a miss only after it lands, is deleted.
kio adds no PR check and no self-hosted runner of its own.
`CONTRIBUTING.md`'s Landing section describes the queue, with no `--squash`, since the queue applies its own merge method.

## Plan

Decided 2026-11-26 (maintainer, /quest-run): ✅ the merge queue is the only gate, with no CI on PRs, since a PR check would double each PR's CI for feedback that `just ci` before pushing already gives.
The reason for a queue: `quest merge` lands only heads that contain `main`, so each landing forces every other picked PR to re-merge `main`, as landing kixel-ai/kio#2, #3, and #13 did that day.

Decided 2026-11-27 (maintainer): ✅ kio's gate is Kixel Quest's hosted merge queue, taken when kio migrates to it, with no interim runner (rejected: GitHub's merge queue on a self-hosted `kio-queue` runner group, kixel-ai/dotfiles#394). Until then, `quest merge` lands a head once the local checks in `CONTRIBUTING.md`'s Landing section and its review pass, and `check.yml` and `post-merge.yml` file an issue for breakage on `main` after it lands.

Declare the gate's commands in whatever form Kixel Quest's onboarding asks for. The PR title `migrate-check` reads is the PR author's, so it reaches the command through the environment, never interpolated into a script.

## Waiting

- Kixel Quest: its hosted merge queue accepts kio. Check kixel-ai/quest's docs for how a repository declares its merge gate, then migrate.
