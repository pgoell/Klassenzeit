# 0027: Pinned placements wire format

- **Status:** Accepted
- **Date:** 2026-05-03

## Context

The per-class solve at `POST /api/classes/{id}/schedule` re-solves the requested class against a problem that includes every other class's lessons but ignores the sibling classes' already-persisted placements. A class persisted yesterday can have its (time_block, room) slot silently overwritten by today's per-class re-solve of a sibling. Tracked in `OPEN_THINGS.md` "Acknowledged deferrals" as "Whole-school cross-class consistency".

Sprint C will introduce user-pinned manual edits (drag-drop a placement and lock it). Both surfaces (auto-pinned siblings, user-pinned manual edits) need the solver to honour the pins as hard constraints.

## Decision

The solver wire format gains a single additive field `Problem.pinned_placements: Vec<PinnedPlacement>`, where each entry fixes a specific (lesson_id, time_block_id, room_id) triple. FFD seeding skips lessons whose ids appear here (writing them into the initial Solution directly from the pin entries). LAHC's `try_change_move` skips placements whose lesson is pinned, mirroring the existing lesson-group skip-guard and preserving the determinism rule (two `random_range` calls per iteration regardless of branch).

Both Sprint A's auto-pinning and Sprint C's user-pinning feed into the same field. The solver cannot legitimately distinguish the two sources, the ORM stores them identically, and a Sprint C migration that splits them would gain nothing.

A new violation variant `ViolationKind::PinnedConflict { lesson_id, reason }` reports malformed pins (unknown ids, double-booked slots, block-size mismatches). Bad pins are dropped from the active set so the rest of the solve proceeds; the variant is a report, not an abort signal.

## Consequences

Easier: per-class re-solve respects sibling persisted placements, closing the OPEN_THINGS.md "Whole-school cross-class consistency" deferral. Sprint C reuses this primitive without schema or wire-format change beyond a `pinned: bool` column on `ScheduledLesson`. The wire format stays backwards-compatible: callers omitting `pinned_placements` deserialise to an empty Vec.

Harder: existing persisted schedules that drifted under the old per-class flow may have sibling overlaps. After this PR ships, the first per-class re-solve surfaces those overlaps as `PinnedConflict` violations rather than silently overwriting. One-time recovery: run "Generate all" once. This will be documented in the PR body.

Revisit when Sprint C's user-pinning lands and we need to distinguish auto-pinned siblings from user-pinned manual edits in UI surfacing (e.g., "your manual pin conflicts with X"); the distinction lives in the ORM (`pinned: bool` on `ScheduledLesson`) and the API response shape, not in the solver wire format.

## Alternatives considered

- **Soft-penalty siblings.** Treat sibling placements as a soft constraint scored against. Rejected because soft pins do not actually prevent drift on a tight schedule; the user-visible failure mode (sibling placement gets overwritten) is the exact behaviour that motivates this ADR.
- **Drop per-class re-solve.** Route everything through whole-school. Rejected on UX grounds: a teacher fixing one class's schedule should not have to wait for a school-wide solve.
- **Two separate wire fields (`auto_pinned`, `user_pinned`).** Rejected because the solver cannot legitimately distinguish them at solve time; duplication forces a Sprint C migration that does not need to exist.
