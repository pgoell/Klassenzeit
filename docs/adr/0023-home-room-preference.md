# 0023: Home-room preference soft constraint

- **Status:** Accepted
- **Date:** 2026-04-30

## Context

Sprint item 7 (algorithm phase, P1) of the "Realer Schulalltag" sprint. Hessen Grundschulen run almost every lesson in the class's eponymous Klassenraum: 1a's lessons in "Klasse 1a", 2b's in "Klasse 2b", and so on. The demo placed lessons by hard feasibility plus four soft axes (`class_gap`, `teacher_gap`, `prefer_early_period`, `avoid_first_period`); none signalled "this room belongs to this class". The dreizuegige seed surfaces the modelling gap most clearly: the Religion trio (RK / RE / ETH) for one Jahrgang must land in one shared room, and the demo had no way to express that the room should preferentially be the teacher's room rather than a random Werkraum.

## Decision

1. **Nullable FK on `school_classes`.** New column `home_room_id UUID NULL REFERENCES rooms(id) ON DELETE SET NULL`. Cardinality is 1 (a class has at most one home room); rooms with no matching class stay unmapped. Wire format additive end-to-end (Pydantic, Zod, OpenAPI, solver JSON).
2. **`prefer_home_room` axis on `ConstraintWeights`.** Per-placement penalty per non-matching member class: a placement of a multi-class lesson contributes `weights.prefer_home_room` once per class in `school_class_ids` whose `home_room_id` is set and does not match the placement's `room_id`. Single-class lessons fall out as a one-iteration loop. The active-default `solve()` weight is `1`, alongside the existing four axes.
3. **`score::home_room_penalty` helper.** Pure, allocation-free, mirrors the `subject_preference_score` template. `score_solution` builds a `HashMap<SchoolClassId, Option<RoomId>>` once per call and sums the helper across placements. Greedy lowest-delta and LAHC delta evaluator both pick up the axis through `score_solution`.

## Alternatives considered

- **Many-to-many `school_class_home_rooms` join.** Rejected because the real-world cardinality is 1; the join table forces solver and frontend to handle a near-empty set.
- **Reverse FK on `Room.home_class_id`.** Rejected because reading direction is wrong (the frontend's class edit dialog is the natural write surface) and it conflates room typing (capacity, suitability) with class ownership.
- **Skip multi-class lessons (score only single-class).** Rejected because the dreizuegige Religion trio loses a real signal; the per-member sum honestly captures "two classes are away from their home room" without a special case.

## Consequences

Easier: visible "1a's lessons mostly happen in 1a's room" in the demo without manual rules; future per-class room overrides plug into the same axis. Harder: the score's per-placement loop iterates `school_class_ids`; `score_solution` hosts a fourth `HashMap` per call. Bench `BASELINE.md` confirms p50 wall-clock within 1 percent of the previous baseline across all three fixtures (grundschule / zweizuegig / dreizuegig). Revisit when a school surfaces per-class subject-preference overrides ("Sport last period for 4c"), at which point a `school_class_subject_preferences` table joins this axis.
