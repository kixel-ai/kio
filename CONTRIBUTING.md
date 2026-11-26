# Commits

PRs into `main` land as squash commits, so the PR title becomes the commit subject.

- Use conventional-commit subjects (`feat: ...`, `fix: ...`, `perf: ...`, `docs: ...`, `chore: ...`).

# PRs

Keep the body short and structured, not narrated.
Have at least these sections:

- **Problem**: a summary of the problem and why this PR is needed.
- **Approach**: a summary of the approach taken to solve the problem.
- **Impact**: a bullet point for every public API change. A breaking one says how to migrate, and the PR title marks it with `!` (`feat!: ...`).
- **Alternatives**: any alternative approaches considered.
- **Follow-ups**: any issues encountered or quests created.

# Landing

A PR lands on the maintainer's pick once CI's `check` job and its local `quest review` rounds pass; queue the reviewed head with `quest merge <pr> --land <sha> --squash`.
No ruleset requires `check` yet, so a pending run does not hold the merge: queue only once it passes.
Ask before using the admin bypass of `main`'s rulesets.
