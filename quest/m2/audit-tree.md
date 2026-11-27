# [M] Audit the quest tree

## Goal

Run the `tree` audit: `quest audit tree` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

Baseline from the 2026-11-26 run, the first since kio left `moq-dev/moq` (866ef06):

- Check first for `moq-dev/moq` leftovers the extraction carried over: repository-relative paths such as `rs/...` and `sh/...` that exist only in moq (link them by URL to `moq-dev/moq`), moq's milestone numbers, moq's Nightly workflow, and moq commit hashes. kio's CI is `.github/workflows/check.yml` plus whatever post-merge workflow has landed since; there is no Nightly.
- The cross-repository edge runs both ways. kio quests that measure or fix kio for the relay (`kio-waiter-spike`, `perf/kio-waiter-costs`, `kio-channel-contention/`) live in moq's tree and already say kio changes land here first; `moq-dev/moq`'s `quest/m2/kio-crates-io.md` waits on this tree's crates.io publish. Read moq's quests that name `kio` (grep its `quest/` tree on GitHub) for work that now belongs here.
- Quests built on moq-net logic (the `kio-state-machines` spikes' `SendCredit` and `WarmSettle` fixtures) must keep that logic out of `kio`'s `src/`, as `AGENTS.md` requires.

## Audit

- tree

## Required

- [m1](/quest/m1/README.md) - the next milestone's work, so this audit runs once per milestone

## Scheduled

- 2026-12-03 - a week after the last run, so a fast milestone does not rerun it at once
