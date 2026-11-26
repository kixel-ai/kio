# [S] kio's threaded tests run under many Miri schedules and ThreadSanitizer

## Goal

kio's post-merge CI runs the tests that spawn threads under Miri across many
scheduler seeds, and kio's whole library test suite under ThreadSanitizer. A
data race or an ordering bug in kio's locks, atomics, wakers, or lock-free log
fails the run, with no change to the code under test.

## Plan

Decided 2026-11-26 (maintainer, /quest-run): ✅ keep (rejected: drop).

kio's half of `moq-dev/moq`'s
[`quest/m4/miri-seeds-tsan.md`](https://github.com/moq-dev/moq/blob/main/quest/m4/miri-seeds-tsan.md),
which the maintainer decided 2026-11-10 for kio and moq-net together. kio's
tests left moq in the extraction (866ef06), so moq's Nightly can no longer run
them; moq's quest keeps moq-net. The decisions carry over: shim-free tools only
(rejected: Shuttle, and loom back for kio alone, since
[moq-dev/moq#7474](https://github.com/moq-dev/moq/pull/7474) removed loom for
needing a `cfg(loom)` shim around every primitive and never finding a race).
m2 matches moq's m4: later work, not current.

- Build on the post-merge Miri lane that [ci-coverage](/quest/m1/ci-coverage.md) restores, which
  runs each test once. Add a pass over every Miri-compatible library test
  that spawns threads (grep `src/` for `thread::spawn`: `log`, `channel`,
  `weak`, `task`, and `clock` modules today) with `-Zmiri-many-seeds`, keeping
  `log::` on Tree Borrows as that lane does. Choose the seed count from the
  measured time.
- ThreadSanitizer: a `just tsan` recipe builds std with the sanitizer
  (`-Zsanitizer=thread -Zbuild-std`) and runs kio's library tests at
  near-native speed, reaching tests Miri ignores for foreign calls or cost. A
  suppression needs a comment naming the false positive.
- Both are random, not exhaustive: a pass is evidence, not a proof. Say so in
  each recipe's doc line.

Public API: none. Wire: none.

## Required

- [Restore CI coverage](/quest/m1/ci-coverage.md) - the post-merge Miri lane this adds a pass to

## Related

- [Miri's GenMC mode](/quest/m2/miri-genmc.md) - exhaustive schedules once upstream supports `cargo miri test`
