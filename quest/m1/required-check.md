# [XS] Require kio's check job before a PR merges

## Goal

GitHub holds every PR into `main` until CI's `check` job passes on its head, so `quest merge` cannot land a head whose `check` is still running, and `CONTRIBUTING.md`'s Landing section drops its "No ruleset requires `check` yet" sentence.

## Plan

`main`'s only ruleset is the org's `Pull request` rule. `quest merge` refuses a failed check but lands past a pending one that no ruleset requires, so today a PR can merge before `check` finishes.
Add `kixel-ai/kio` to `REQUIRED_CHECK` in kixel-ai/dotfiles' `bin/repo-settings`, as [kixel-ai/dotfiles#365](https://github.com/kixel-ai/dotfiles/pull/365) does for swole, and apply it.
`check.yml` stays on GitHub's runners, which [kixel-ai/dotfiles#368](https://github.com/kixel-ai/dotfiles/pull/368) allows for public repositories; if that PR does not land, move it to a self-hosted runner first. Its `pull_request` trigger already reports on every PR.
