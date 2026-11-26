# Commits

PRs into `main` land as squash commits, so the PR title becomes the commit subject.

- Use conventional-commit subjects (`feat: ...`, `fix: ...`, `perf: ...`, `docs: ...`, `chore: ...`).
- Never commit compiled binaries or build output.

# PRs

Keep the body short and structured, not narrated.
Have at least these sections:

- **Problem**: a summary of the problem and why this PR is needed.
- **Approach**: a summary of the approach taken to solve the problem.
- **Impact**: a bullet point for every public API change, marking breaking ones with `!`.
- **Alternatives**: any alternative approaches considered.
- **Follow-ups**: any issues encountered or quests created.

# Landing

A PR lands once CI's `check` job and its local `quest review` rounds pass; queue the reviewed head with `quest merge <pr>`.
The admin bypass of `main`'s rulesets is for emergencies.
