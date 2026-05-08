# 0036: Move teacher assignment to solver decision variable

- **Status:** Accepted
- **Date:** 2026-05-08

## Context

Today's auto-assign + LAHC pipeline is two greedy heuristics with no shared lookahead. `auto_assign_teachers_for_lessons` (in `backend/src/klassenzeit_backend/scheduling/teacher_assignment.py`) commits a teacher to each `Lesson` based on subject qualification and weekly capacity only, ignoring teacher availability windows and cross-class scheduling pressure. The LAHC solver then rediscovers infeasibility at solve time as `no_free_time_block`. The 2026-05-08 measurement against `test_grundschule_schedule_meets_quality_bar` at the production 5000 ms LAHC deadline (OPEN_THINGS item 14) was 20/20 RED with the failing class and `hour_index` drifting run-to-run, confirming a structural rather than tuning problem. A user clicking "Generate Schedule" on a Grundschule today sees an unplaceable lesson on roughly every fresh seed.

This is a search-space decision, not a heuristic-tuning one. Two greedy heuristics with no shared lookahead cannot converge on feasibility for the Grundschule fixtures the integration tests already cover; the solver already handles the cross-axis search for window + room and is the natural place for teacher choice as well.

## Decision

Move teacher choice into the solver as a decision variable. Six mechanism changes, landing across OPEN_THINGS items 62 to 71:

1. `Lesson.teacher_id` semantics flip to "pin or null." Null means "let the solver decide; not pinned." No DDL migration; the column is already nullable.
2. Solver wire format gains per-`Lesson` `teacher_candidates: Vec<TeacherId>` (precomputed at the backend boundary in `scheduling/solver_io.py:build_problem_json`) and `teacher_pin: Option<TeacherId>` (mirrors `Lesson.teacher_id` if non-null). Output `Placement` gains `teacher_id: TeacherId`.
3. New hard constraint: per-(class, subject) teacher uniformity. New `ViolationKind::ClassSubjectTeacherSplit`; placement-time pruning in `try_place_block`; post-condition validator in `solver-core/src/validate.rs`. CP-SAT `add_max_equality` over `t_chosen[lesson, teacher]` BoolVars.
4. New soft constraint: prefer Klassenlehrer for own-class lessons when qualified. New `ConstraintWeights.prefer_class_teacher: u32`. LAHC slice + CP-SAT objective both penalise unmet `(class, subject)` pairs.
5. New `SchoolClass.class_teacher_id: uuid.UUID | None` FK to `Teacher` (`ON DELETE SET NULL`).
6. `auto_assign_teachers_for_lessons` is deleted. `POST /api/classes/{id}/generate-lessons` instead validates that every newly-created Lesson has at least one qualified Teacher and raises 422 with a specific error code if a subject has none.

## Alternatives considered

- **Smarter feasibility-aware greedy auto-assign.** Rejected. Backtracking inside auto-assign duplicates the LAHC primary search path, and the greedy still cannot compose with the multi-class-lesson and lesson-group constraints already in the solver. Item 14's 20/20 RED is the empirical evidence that "two greedies feeding each other" is the wrong shape.
- **Soft constraint instead of hard for `(class, subject)` uniformity.** Rejected. The Grundschule mental model ("Frau Müller teaches all her class's German lessons") is structural; a soft cap admits split-teacher solutions whenever soft cost is outweighed. Sprint 4's differenziert (E-Kurs / G-Kurs) will need an extension via `differentiation_group_id` on `Lesson`; explicitly out of scope here.
- **Two-step migration of `Lesson.teacher_id` (separate "nullable" column or backfill).** Rejected. The column is already nullable; a migration would change zero rows. The semantic flip is route-handler-only.

## Consequences

Easier: `test_grundschule_schedule_meets_quality_bar` and `test_seeded_grundschule_solves_with_auto_assigned_teachers` go deterministically green (items 11 + 14 drop their xfails). The user-experience for "click Generate" becomes one-shot reliable on every Grundschule fixture. `QualityReport` gains a `prefer_class_teacher_misses` axis that gives admins a Klassenlehrer-coverage lever.

Harder: LAHC's `try_place_block` widens from a `(window, room)` picker to a `(window, room, teacher)` triple picker; CP-SAT gains per-`(lesson, candidate)` BoolVars plus an `add_exactly_one` per Lesson and per-(class, subject) `add_max_equality` for uniformity. Search space grows by a factor of |qualified teachers per class| per Lesson; CP-SAT wall-clock regression makes item 34's per-backend deadline more load-bearing.

Revisit triggers: (a) Sprint 4 differenziert lands; the per-(class, subject) uniformity constraint becomes wrong for E-Kurs / G-Kurs splits and needs a `differentiation_group_id` extension. (b) A school flow surfaces "different Klassenlehrer per term" or "co-teaching"; both require a separate ADR. (c) Item 73's bench refresh shows search-space widening costs more wall-clock than item 34's per-backend deadline can absorb; a deeper search-space pruning ADR follows.

The production-default revisit (item 47) claims ADR 0037 next; per-component vector reasoning per `solver/CLAUDE.md` continues there.
