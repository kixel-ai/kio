# [XS] Fail a breaking PR that skips MIGRATE.md

## Goal

A pull request whose title marks a breaking change (`type!:` or `type(scope)!:`) fails a required check unless it changes `MIGRATE.md` or `macros/MIGRATE.md`.

## Plan

The 2026-11-26 docs audit found three breaking changes to the 0.6.4 API without a `MIGRATE.md` entry ([moq-dev/moq#7036](https://github.com/moq-dev/moq/pull/7036), [#7475](https://github.com/moq-dev/moq/pull/7475), [#8293](https://github.com/moq-dev/moq/pull/8293)), each landed under a `!` title.

A small job on `pull_request` (`opened`, `synchronize`, `reopened`, `edited`, so a retitle re-runs it) reads the title and the file names the PR changes. Keep it in its own workflow so editing a PR body doesn't rerun `just ci`. It runs on whatever runner `check.yml` uses and joins the required checks.
