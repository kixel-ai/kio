# [XL] Audit agent instructions

## Goal

Run the `instructions` audit: `quest audit instructions` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

Baseline from the 2026-11-26 run, the first since kio left `moq-dev/moq`:

- Read through `main` at 866ef06 and the heads of kixel-ai/kio#1 to #8 on 2026-11-26; start the next window there.
- Messages: `quest messages` read on desktop (49 unnamed directories dropped), mini (1), fedora (5), and kixtop (6); only desktop had kio entries. nixos was unreachable and runs CI, not agent sessions.
- Harnesses seen: Claude Code (Opus 5.5, Haiku 5.5), Codex (gpt-6.2-sol, as `quest review`'s reviewer), and Antigravity (Gemini 4 Neon, kixel-ai/kio#1).
- Loaded sizes: root `AGENTS.md` 1,055 bytes after this run, which Claude Code reaches through `CLAUDE.md`'s `@AGENTS.md`, and Codex and Antigravity read directly; each harness's global file adds 4,157 bytes, and quest's hook about 9,000 characters. The eight skill descriptions total 961 characters. Every set is far under its limit (Codex 32 KiB, Antigravity 24,000 bytes per file, Claude Code 200 lines per file and 1,536 characters per description).
- Guidance read 2026-11-26: every page `quest guide prompting` links, plus the Opus 5.5 and Haiku 5.5 prompting pages, OpenAI's GPT-6.2 Codex prompting guide, and Antigravity's skills page.
- Out of scope here: the `quest-*` skill stubs and `AGENTS.md`'s `Quests:` line, which `quest init` plants from kixel-ai/quest; fix those there.
- Kept against the evidence:
  - `CONTRIBUTING.md`'s five PR sections, skipped by one of two PR bodies (a quest-only PR written in Antigravity, which never loads `CONTRIBUTING.md`): it is the maintainer's format across moq-dev/moq and kixel-ai/crabx.
  - `just ci` before pushing, skipped once on a quest-only change: about 17 s warm.
  - The poll-first and `unsafe` rules, which no session touched in a one-day window: they guard kio's core design.
- Check first:
  - `CONTRIBUTING.md`'s Landing section: [required-check](/quest/m1/required-check.md)'s merge queue retires its "No ruleset requires `check` yet" sentence and `--squash`.
  - `AGENTS.md`'s host-clock test rule, now that `just test` sets `KIO_DENY_HOST_TIMERS=1` (kixel-ai/kio#4): whether it can shrink to naming `kio::clock::host_time()`.
  - `AGENTS.md`'s "no coroutines" promise, which only [coroutines-check](/quest/m1/coroutines-check.md) enforces.

## Audit

- instructions

## Required

- [m1](/quest/m1/README.md) - the next milestone's work, so this audit runs once per milestone

## Scheduled

- 2026-12-03 - a week after the last run, so a fast milestone does not rerun it at once
