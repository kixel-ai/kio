# [S] Run kio's check on a self-hosted runner and require it

## Goal

CI's `check` job runs on a self-hosted runner, and GitHub holds every PR into `main` until it passes on the head, so `quest merge` cannot land a head whose `check` is still running. `CONTRIBUTING.md`'s Landing section drops its "No ruleset requires `check` yet" sentence.

## Plan

`main`'s only ruleset is the org's `Pull request` rule. `quest merge` refuses a failed check but lands past a pending one that no ruleset requires, so today a PR can merge before `check` finishes.
The maintainer decided on 2026-11-26 that a public repository runs no merge gate on GitHub's runners ([kixel-ai/dotfiles#369](https://github.com/kixel-ai/dotfiles/pull/369), whose `quest/m0/no-github-runners.md` lists kio). Move `check.yml`'s job to the fork-safe runner group that quest provides first, then add `kixel-ai/kio` to `REQUIRED_CHECK` in kixel-ai/dotfiles' `bin/repo-settings`, as [kixel-ai/dotfiles#365](https://github.com/kixel-ai/dotfiles/pull/365) does for swole, and apply it.
`release.yml` and post-merge jobs may stay on GitHub's runners.

## Waiting

- kixel-ai/dotfiles: a fork-safe self-hosted runner group admits kixel-ai/kio (`quest/m0/no-github-runners.md` there: a runner with fresh state per job, in an org group per repository); check that the org's runner groups list one for kio.
