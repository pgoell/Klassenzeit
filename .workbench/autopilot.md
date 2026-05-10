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
- When closing an item that takes a previously-unassigned ADR number, `grep -n "ADR <NNNN>" docs/OPEN_THINGS.md` and refresh every stale reference in the same commit. Items can pre-claim an ADR number in their bodies (e.g., item 47 said "open ADR 0035 (next-available number)" before item 55 actually claimed 0035), and stale references can sit in unrelated items' bodies, in section preambles, and in the active-sprint header. The plan's edit list often only enumerates references inside the closing item itself, missing the others.
- Same shape applies to OPEN_THINGS item numbers: when closing item N, `grep -n "item N" docs/OPEN_THINGS.md` and refresh forward-looking references in OTHER active items' bodies (lines that say "consumes item N's refresh", "blocked by item N", "informed by item N's data"). Past-tense narrative references inside another item's `**shipped**` ship-event paragraph are historical anchors and stay. Closing item 44 on 2026-05-08 required updating item 47's "consumes item 44's full refresh" wording in the same commit; item 22's "the directional read defers to item 44's full refresh" inside its ship narrative was historical and was not touched. The plan's edit list often enumerates only the closing item's own bullet, missing forward-looking references in adjacent items.
- When closing the last item under an `### <Phase> phase` header inside the active sprint program, DELETE the empty header alongside the bullet. Empty section scaffolding is doc noise, and an OPEN_THINGS reader scanning for the next pickup should not have to skip past content-free headers. Same shape applies to any `### Tier` / `### Phase` grouping under `## Active sprint program: ...` whose final entry closes. Closing item 43 on 2026-05-08 collapsed the empty `### Sprint-tidy phase` header alongside the bullet; the next pickup line in the active-sprint header is the canonical "what to work on next" surface, not the phase headers.
- P2 conditional items framed "promote / add / land X only if Y shows up in a future refresh" sometimes have an unfireable trigger by construction. Before re-checking the trigger on the latest data, ask whether Y CAN happen at all. Hard-constraint predicates enforced post-condition by `solver-core/src/validate.rs`'s validator trio surface as `Err(Error::Input)` (panic-cell rows in BENCH_RESULTS.md), not as `Violation` entries with a non-zero count, so a "non-zero count" trigger is structurally foreclosed and the right close is wontfix-by-construction with the closing commit citing the validator path. Item 43's `room_hop` axis was the exemplar; future `validate_*`-enforced predicates inherit the same shape.

**`DONE_WITH_CONCERNS` triage.** If a subagent reports `DONE_WITH_CONCERNS` and the concern is in-scope (perf-budget regression on a solver task, an unfixed lint, a missing test the spec required), fix it before committing, either in the main session or by re-dispatching with the gap added to the acceptance criteria. Do not carry the concern into the PR. For algorithm-phase work specifically, include the BASELINE.md 20% regression budget as an explicit acceptance criterion in the subagent prompt so the agent knows to optimise within its own task rather than surface the breach.

**Module-deletion commits sweep stale doc-comment references in production source.** When a commit deletes a Python module (`refactor: delete dead X module + tests`), the deletion subagent's verification step `rg -n 'X' backend/src/` typically returns matches in unrelated production source files (model docstrings, seed comments) where prior commits documented the deleted symbol's role. Fold those doc-comment edits into the same commit, since they describe a symbol that no longer exists; do NOT touch xfail `reason=` strings inside test files (those are historical anchors that the matching test-refit item will rewrite when the xfail comes off), alembic migration docstrings (immutable history), or `CLAUDE.md` (the improving-instructions step at autopilot step 6 owns that surface). Path: edit the in-scope production references, `git add` only those files, `git commit --amend --no-edit` so the structural-deletion commit ships clean. Surfaced 2026-05-10 closing OPEN_THINGS item 69 when the Task 1 subagent flagged stale references in `db/models/scheduled_lesson.py` and `seed/demo_grundschule_dreizuegig.py` via `DONE_WITH_CONCERNS`.

**Upstream-state check on tool-named P0 spikes.** When a P0 OPEN_THINGS item names a specific external tool (Timefold, Choco Solver, OptaPlanner successor, etc.), do a quick PyPI / GitHub status check before brainstorming the integration path. Upstream archive status, Python-version compatibility against `mise.toml`'s pin, and EOL signals can flip the spike's outcome from "integrate" to "document and reject with concrete reasons". Item 55 (ADR 0035) flipped to rejection because Timefold's Python binding was archived 2025-10-06, supports Python 3.10-3.12 only (we pin 3.14.2), and the Java path violated the item's "bounded spike, do not let this become a product rewrite" framing.
