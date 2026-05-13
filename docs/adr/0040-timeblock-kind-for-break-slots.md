# 0040 — TimeBlock.kind for break slots

- **Status:** Accepted
- **Date:** 2026-05-13

## Context

Hessen Grundschule days have two Hofpausen (after the 2nd and 4th period) and, in Ganztag schools, a Mittagspause. Today these breaks are not rows in the `time_blocks` table; they are implicit gaps between consecutive `(day_of_week, position)` entries. Two downstream pieces of OPEN_THINGS item 3 (Pausen / Aufsichtspflichten) are blocked by this shape: the future supervision-rota objective has nothing to attach to, and the admin-facing schedule grid presents a six-cell day to the user while the real Grundschule day has eight slots. The trigger for the supervision objective (a customer school tracks supervision) has not fired, but the schema foundation is on the critical path and is shippable on its own.

## Decision

Add a `TimeBlock.kind` Postgres native ENUM with values `lesson` and `break`, mirroring the `SchoolType` precedent on `Stundentafel`. All existing rows default to `lesson`; new break rows are first-class members of the same table at distinct `position` ordinals within the day. The solver IO boundary filters `kind == lesson` so `solver-core`, `solver-py`, and LAHC determinism are byte-equal on identical lesson-only fixtures. The frontend renders break cells as non-bookable variants and lets admins toggle a cell's kind in the time-blocks editor.

## Alternatives considered

- **Sibling `Break` table.** Keep `TimeBlock` unchanged; add a parallel `Break(week_scheme_id, day_of_week, start_time, end_time)` table. Rejected because the frontend already iterates a flat list of TimeBlocks keyed by `(day, position)`; a parallel table forks the rendering, complicates the `timeBlocksByDayPosition` map, and denies that a break is conceptually a slot in the period grid.
- **Richer initial enum** (`lesson | hofpause | fruehstueckspause | mittagspause` or `lesson | break_short | break_long`). Rejected on YAGNI: nothing consumes the subdivision today; supervision-rota work can derive duration from `start_time` / `end_time` when it lands. Subdividing later is a cheap additive migration.
- **Solver-core awareness of kind.** Pass `kind` to solver-core and skip non-lesson slots in placement, eligibility maps, and the LAHC inner loop. Rejected because the IO-boundary filter is strictly compatible with today's solver (zero RNG-budget shift, zero new test vectors) and the supervision objective is a separate PR whose IO contract widens then.

## Consequences

Easier: admins see and edit breaks in the WeekScheme grid; the schedule grid reflects the real Hessen day shape; the supervision-rota objective has a stable anchor when it lands. The `(week_scheme_id, day_of_week, position)` uniqueness still holds; positions strictly increase within a day, just now interleaving break rows.

Harder: every consumer of the `time_blocks` query that used to assume "all rows are bookable" now filters `kind == lesson` when bookable-count is the intent. Quality predicates that operate on raw `TimeBlock.position` (interior-gap, day-length) must project onto lesson ordinals before scoring; this PR threads that projection in `test_grundschule_schedule_quality.py`; a future admin-facing quality endpoint should fold the projection into `quality_checks.py`.

Revisit if a customer school surfaces a need to distinguish Hofpause from Mittagspause at the schema level (subdivide the enum), or if the supervision-rota objective needs solver-core awareness of `kind` (widen the IO boundary to pass the flag).
