# Workbench Autopilot Profile

## Project name
kz

## Documentation paths
Specs: don't commit
Plans: don't commit
Open things: docs/OPEN_THINGS.md
ADRs: docs/adr

## PR behavior
Mode: automerge
Squash: yes

Hooks:
- post_pr: python3 .claude/commands/post_brainstorm_comments.py {{pr}}

## Project-specific rules

**Solver-binding rebuild discipline.** When a subagent runs Python or backend integration tests that consume `klassenzeit_solver` (the maturin-built PyO3 binding) AFTER an earlier task in the same autopilot run touched `solver/solver-core/` or `solver/solver-py/`, the agent's prompt MUST include `mise run solver:rebuild` as an explicit step before pytest. Otherwise the binding is stale and the agent observes phantom wire-format bugs (a Sprint A subagent misdiagnosed a non-existent `kind`-as-tagged-dict bug from a stale binding). Same shape applies to `mise run fe:types` after backend Pydantic schema changes if the next task touches frontend types.

**OPEN_THINGS.md hygiene at step 6.** `docs/OPEN_THINGS.md` is for OPEN items only. Specifically:

- When an item ships, DELETE it from OPEN_THINGS entirely. Do not leave a `✅ Shipped <date> in PR #<N>` line behind. The PR description and `git log` are the canonical record of what shipped; OPEN_THINGS is the canonical record of what is still open.
- When a PR closes the last open work in the active sprint program, promote the next `## Queued sprint: ...` to `## Active sprint program: ...` and DELETE the closed sprint section. Open quality / tuning items the closed sprint deferred move to a `## Open <topic> follow-ups` section between the new active sprint and `## Planned future sprints`, NOT to a "Completed" section (no such section exists in this file).
- When an item under `## Acknowledged deferrals` closes (intentionally or by side effect), DELETE the entry. Do not leave "Closed YYYY-MM-DD" annotations.
- When the item that ships was promoted from a `## Open <topic> follow-ups` stub (the stub's body says `[Promoted to P0, see active-sprint item N]` or similar), DELETE the stub too. The promoted-and-shipped chain leaves no trace.
- PR-number references that name PRs from the lifetime of an open item (e.g., "the regression PR #171 left this xfail behind") are fine: those name a still-relevant historical anchor for an open item. The rule is about closed items, not historical references.
- `## Reference data` (e.g., the Hessen Grundschule reference table) is research that future sprints consume and stays in the file.

**`DONE_WITH_CONCERNS` triage.** If a subagent reports `DONE_WITH_CONCERNS` and the concern is in-scope (perf-budget regression on a solver task, an unfixed lint, a missing test the spec required), fix it before committing, either in the main session or by re-dispatching with the gap added to the acceptance criteria. Do not carry the concern into the PR. For algorithm-phase work specifically, include the BASELINE.md 20% regression budget as an explicit acceptance criterion in the subagent prompt so the agent knows to optimise within its own task rather than surface the breach.
