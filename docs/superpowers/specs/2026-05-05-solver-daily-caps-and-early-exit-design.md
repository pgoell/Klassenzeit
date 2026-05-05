# Daily caps + solver optimum-aware deadline (active sprint, items 38 + 39)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Algorithm phase: items 38 (per-day caps) + 39 (early exit + raised deadline).
**Goal.** Stop the production solver from emitting schedules with three-of-a-subject in a row, missing placements, and short-budget local minima. Two structural changes: introduce two new hard constraints that the runtime currently does not check, and let the LAHC outer loop terminate as soon as it has found an objective-floor solution.

**Non-goal.** No tuning of existing soft-constraint weights. No new soft constraints (the long Monday gap reported alongside the caps work is deferred per the user). No frontend rendering of the new violation kinds beyond the form fields. No changes to bake-off fixtures' canonical inputs. No CP-SAT / Kempe parallel ensemble or polling/SSE incumbent streaming. Item 30 (memory and time-to-feasible bench columns) and item 31 (schedule-quality bench output) stay queued behind this PR.

## Context

Three quality issues surfaced on the production schedule for Grundschule class 1a: two unplaced lessons, three consecutive German lessons on Thursday, and a long Monday gap. Investigation showed:

1. **No max-hours-per-subject-per-day constraint exists.** `solver-core/src/types.rs:364-382` lists six `ViolationKind` variants (`NoQualifiedTeacher`, `TeacherOverCapacity`, `NoFreeTimeBlock`, `NoSuitableRoom`, `LessonGroupSplit`, `PinnedConflict`). None covers same-subject runs or daily-hour caps. The bake-off bench never measured this, so "Kempe handled it in bake-off" was a false premise. The solver places 3 German lessons in a row not because it failed to optimise, but because nothing tells it not to.
2. **No max-lessons-per-class-per-day constraint exists either.** Class daily volume is bounded only by the class's `time_blocks` rows. A class scheduled on periods 1–8 Mon–Fri must accept all 40 weekly slots, even if pedagogically it should top out at 5 lessons/day on certain days.
3. **`solve_deadline_ms` defaults to 200** (`backend/src/klassenzeit_backend/core/settings.py:55` and `backend/.env.example:20`). The bake-off bench runs at 5 s/cell; production runs ~25× shorter and frequently stops the LAHC loop in the FFD-induced local minimum. The two unplaced 1a lessons are most likely budget-bound.
4. **The LAHC loop runs to deadline even when it has already found an objective-floor solution** (`hard == 0 && soft == 0 && placements_total == expected`). The objective is `hard*BIG + missing*BIG + soft`; that triple is the floor. Continuing past it spends budget on no measurable improvement.

ADR 0031 + ADR 0032 picked `lahc_rr_kempe` as production default on a bench that does not exercise the missing constraints. The post-item-37 bench shows soft-score zero across all four canonical fixtures at 5 s budget; tightening the production budget plus terminating early at the floor lets production reach the same quality bar the bench validates.

Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run). Anchor commits: 9cd9acf (item 29 / ADR 0032), e9897fa (item 37), 66d0b7c (item 28).

## Scope

**In scope.**

- New ORM column `subjects.max_hours_per_day INTEGER NOT NULL DEFAULT 2` (`backend/src/klassenzeit_backend/db/models/subject.py:Subject`). Counts hours, not lessons: a 2-period block lesson contributes 2 to the daily count for that `(class_id, day, subject_id)` triple.
- New ORM column `school_classes.max_lessons_per_day INTEGER NULL` (`backend/src/klassenzeit_backend/db/models/school_class.py:SchoolClass`). `NULL` means "no cap beyond what `time_blocks` allow"; an integer caps total lessons (placements, regardless of period span) per class per day. Counts lessons, not periods, because the user's framing ("max lessons per day") is a count of distinct activities the class attends, not slot occupancy.
- One Alembic revision adding both columns with server defaults. Down-migration drops both. Style: `collections.abc.Sequence` + PEP 604 unions per backend/CLAUDE.md.
- Two new `ViolationKind` variants in `solver-core/src/types.rs`: `SubjectDailyHourCapExceeded { class_id, subject_id, day_of_week, count, cap }` and `ClassDailyLessonCapExceeded { class_id, day_of_week, count, cap }`. Cap is `u8`; count is `u8` (rationale: positions per day are already `u8` in the codebase, both caps fit).
- Hard-feasibility pruning in `solve.rs:try_place_block` (FFD greedy + LAHC change-move's recreate path) and `lahc.rs:rr_attempt`'s recreate / Kempe move's recreate paths. The four call sites already share a per-window legality gate (teacher busy, class busy, teacher over capacity); the cap checks land alongside those, with `continue 'outer` (or equivalent rejection) on violation. No solver-level violation construction at runtime: the cap is enforced by pruning, the new `ViolationKind` variants exist for diagnostic reporting and tests.
- Cap-check tracking state on `GreedyState` (`solve.rs`): two new fields, `subject_hours_by_class_day: HashMap<(SchoolClassId, u8, SubjectId), u8>` and `lessons_by_class_day: HashMap<(SchoolClassId, u8), u8>`. Incremented when a placement lands, decremented in the row-removal helper used by `rr_ruin_block` / Kempe rollback.
- Wire cap fields onto solver-side `Subject` and `SchoolClass` types (`solver-core/src/types.rs`) and into `solver-py`'s JSON shape (`solver-py/src/lib.rs`). `Class` cap stays `Option<u8>` mirroring the DB; `Subject` cap is `u8`.
- Pydantic schema additions in `backend/src/klassenzeit_backend/scheduling/schemas/`: `SubjectCreate.max_hours_per_day: int = 2` (Field with `ge=1, le=20`), `SubjectUpdate.max_hours_per_day: int | None = None`. `SchoolClassCreate.max_lessons_per_day: int | None = None` (Field with `ge=1, le=20`), `SchoolClassUpdate.max_lessons_per_day: int | None = None`. The `Update` shapes use the `model_fields_set` convention (per backend/CLAUDE.md PATCH-handler rule) to allow explicit-null-clears the class field. The `Subject` field is non-null so its `Update` semantics are "leave alone if absent, replace if present".
- Subject route handlers manually construct `SubjectResponse(...)` (per backend/CLAUDE.md "audit each manual construction site after alembic revision" rule); add the new field to each construction in `scheduling/routes/subjects.py`. Class routes follow whichever pattern they use today; verify and adjust in lockstep.
- Frontend form additions:
  - Subject edit dialog (`frontend/src/features/subjects/...`): a labeled number input bound to `max_hours_per_day`, default 2, validation `min=1 max=20`. i18n key in DE + EN locale catalogues.
  - Class edit dialog (`frontend/src/features/school-classes/...`): an optional labeled number input bound to `max_lessons_per_day`, empty cell ⇒ `null`, validation `min=1 max=20`. Hint text: "leave empty for no cap".
- `ViolationResponse.kind` Literal in `scheduling/schemas/schedule.py` widens to include the two new variants. `mise run fe:types` regenerates `frontend/src/lib/api-types.ts` in lockstep. Per backend/CLAUDE.md two tests in `tests/scheduling/test_solver_io.py` (`test_count_violations_by_kind_clean_solve_returns_zeros`, `test_count_violations_by_kind_aggregates_mixed_kinds`) update to the new closed-enum kind set.
- Early-exit gate in the LAHC main loop (`solver-core/src/lahc.rs:86`, the `while iter < max_iter && start.elapsed() < deadline` loop) and at the R&R outer-loop boundary (`rr_attempt` post-iteration). Predicate: after each accepted incumbent improvement, if `state.hard_violations == 0 && state.soft_score == 0 && placements.len() == placements_expected`, break out of the loop. `placements_expected` is the count from the `Problem`'s lesson list (sum of `hours_per_week`, already computed in the solve setup).
- Default `solve_deadline_ms` in `backend/src/klassenzeit_backend/core/settings.py` and `backend/.env.example` raised from 200 to 5000. `.env.test` keeps `KZ_SOLVE_DEADLINE_MS=0` (test-mode greedy-only path). The Settings test `test_solver_backend_default_is_production_choice` (or its sibling for the deadline) updates in the same commit.
- New regression test `solver/solver-core/tests/daily_caps.rs`: build a Problem where the cap default forces a non-trivial layout (one class, one subject with `hours_per_week=4 preferred_block_size=1` where the only feasible layout under cap=2 is 2+2 across two days), assert no `(class, day, subject)` triple exceeds 2 in the returned `Solution`. Red without the legality gate, green with it.
- New regression test `solver/solver-core/tests/early_exit.rs`: build a tiny problem the FFD greedy already solves to objective floor, run `lahc_rr_kempe` with `deadline=10s`, assert `Solution.solve_duration_ms < 1000` (ample margin; in practice exits in <50 ms).
- Update `solver-core/tests/lahc_property.rs::lahc_small_problem`: extend the lesson generator's `hours_per_week` range so the generator actually exercises the cap. Add a property assertion to both `lahc_rr_never_decreases_placement_count` and `lahc_rr_kempe_never_decreases_placement_count`: no `(class, day, subject)` triple exceeds the configured per-subject cap on any feasible solution.
- ADR `docs/adr/0033-solver-daily-caps-and-early-exit.md`. Sections: context, decision, consequences (including: existing fixtures verified to remain feasible at default cap=2; `lahc_rr_kempe` retains soft-score 0 on bench post-change). Index in `docs/adr/README.md`.
- `docs/superpowers/OPEN_THINGS.md`: append items 38 + 39 to the active sprint's algorithm phase, then delete both because this PR closes them. Promote the next pickup line to point at item 30.

**Out of scope.**

- Item 30 (memory and time-to-feasible bench columns), item 31 (schedule-quality bench output), item 32 (Python-side auto-assign solvability tests). Stay in their phase order.
- Long-Monday-gap symptom. Deferred per the user.
- Frontend slider for per-request solve budget. Deferred behind item 30's observability work; if `solve_deadline_ms` raised to 5 s + early exit suffices for production, the slider is unnecessary.
- SSE / polling streaming of incumbent solutions. Premature; rebuild only when evidence shows step 1 + raised deadline insufficient.
- CP-SAT + Kempe parallel ensemble. Same reasoning.
- Retroactive validation of persisted schedules. Existing `ScheduledLesson` rows that pre-date the cap stay valid until the next regeneration; the cap is enforced only at solve time.
- Bake-off `BENCH_RESULTS.md` refresh. Not required: the bench fixtures are already cap-compliant under the default, the change preserves Kempe's soft-score 0 outcomes, and the BENCH_RESULTS lifecycle is a separate item (29 closed, future refreshes will pick this up).

## Failure mode and fix

**Trigger 1 (caps).** The solver places lesson `L` for class `C` of subject `S` at time-block `T` whenever the per-window hard-feasibility checks pass: teacher free, class free, teacher under capacity, room available. There is no check on `(C, day_of(T), S)` daily hours or `(C, day_of(T))` daily lessons. Repeated invocations on the same day push the daily count above any pedagogically reasonable ceiling; the solver has no signal that this is a regression.

**Fix shape (caps).** Cap state is folded into `GreedyState` so that incremental placement / removal updates run in O(1):

```rust
pub(crate) struct GreedyState {
    // existing fields ...
    pub subject_hours_by_class_day: HashMap<(SchoolClassId, u8, SubjectId), u8>,
    pub lessons_by_class_day: HashMap<(SchoolClassId, u8), u8>,
}
```

Inside `try_place_block`, after the existing teacher / class / teacher-capacity checks but before the score-delta computation, add:

```rust
for class in class_ids {
    let key = (*class, first_tb.day_of_week, lesson.subject_id);
    let current = state.subject_hours_by_class_day.get(&key).copied().unwrap_or(0);
    if current.saturating_add(n) > subject.max_hours_per_day {
        #[cfg(feature = "solver-trace")]
        trace::ffd_trace(lesson.id, first_tb.day_of_week, first_tb.position, None, "subject_daily_cap");
        continue 'outer;
    }
    if let Some(cap) = class_max_lessons_per_day.get(class).copied() {
        let lessons_today = state.lessons_by_class_day.get(&(*class, first_tb.day_of_week)).copied().unwrap_or(0);
        if lessons_today.saturating_add(1) > cap {
            #[cfg(feature = "solver-trace")]
            trace::ffd_trace(lesson.id, first_tb.day_of_week, first_tb.position, None, "class_daily_lesson_cap");
            continue 'outer;
        }
    }
}
```

`class_max_lessons_per_day: &HashMap<SchoolClassId, u8>` is built at solve setup time from the optional cap field; classes with `None` simply do not appear in the map. The placement and row-decrement helpers update both bookkeeping fields in lockstep with the existing teacher / class / room counters.

LAHC's change-move and Kempe / R&R recreate paths reuse `try_place_block`'s legality checks today; pruning is therefore inherited automatically. The row-removal helper used by `rr_ruin_block` and `kempe_rollback` decrements the new bookkeeping fields by the row's `(class, day, subject)` and `(class, day)` keys.

**Trigger 2 (deadline / early exit).** `lahc.rs` runs the LAHC loop until `iter >= max_iter` or `deadline` elapses. There is no mid-loop check for "have we already reached the objective floor?" Production's 200 ms wall budget either runs out before LAHC escapes the FFD local minimum (yielding suboptimal placements / unplaced lessons), or finds the floor early and then spins for the remaining budget.

**Fix shape (early exit).** After each accepted candidate that updates the incumbent (`lahc.rs`'s LAHC main loop, after the `if candidate < incumbent` branch), check the floor predicate:

```rust
if state.hard_violations == 0
    && state.soft_score == 0
    && placements.len() == placements_expected as usize
{
    break;
}
```

Same predicate at the R&R outer-loop boundary (after `rr_attempt` returns a non-failed iteration). Raising the deadline default to 5000 ms gives hard problems room to find optimum; the early exit ensures easy problems pay only their own solve time.

## Determinism and bench impact

**Determinism.** Cap pruning rejects candidate windows the same way the existing teacher / class checks do; no new RNG draws, no acceptance-branch reordering. Early exit short-circuits a deterministic predicate; same-seed runs reach the same incumbent on the same iteration, so the early-exit decision is itself deterministic. The R&R determinism property test `lahc_rr_deterministic_under_seed_and_iter_cap` should pass byte-identically.

**Bench.** `BASELINE.md` covers FFD greedy + LAHC change-move criterion benches; the cap pruning adds two HashMap lookups per candidate window (one O(1), the optional class-cap lookup is also O(1)). Expected criterion regression: <2%, well inside the 20% perf budget. Bake-off `BENCH_RESULTS.md` is not regenerated in this PR; spot-check `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule` and paste the receipt into the PR body to confirm `lahc_rr_kempe` still hits `placements_med = expected`, `hard = 0`, `soft = 0`. If any cell regresses, root-cause before merge (most likely culprit: a fixture lesson with `hours_per_week >= 3 preferred_block_size = 1` where cap=2 forces an unsatisfiable layout, in which case raise that subject's `max_hours_per_day` in fixture data).

## Tests

1. **`solver-core/tests/daily_caps.rs` (new)** — targeted regression. One class, one subject with `hours_per_week=4 preferred_block_size=1`, default cap=2, 5 days × 5 positions, room and teacher always available. Run `lahc` (greedy-only) and assert no `(class, day, subject)` triple has count > 2 in the returned `Solution.placements`. Add a second case with `hours_per_week=4` and `max_hours_per_day=4` (raised cap on the subject), assert all 4 hours can land on a single day. Both seeds 1..=8.
2. **`solver-core/tests/daily_caps.rs` (class cap)** — one class with `max_lessons_per_day=4`, daily time-blocks of 6 positions, total weekly hours = 25 (forced spillover). Assert no day on the class has > 4 placements.
3. **`solver-core/tests/early_exit.rs` (new)** — tiny FFD-feasible problem, `lahc_rr_kempe` with `deadline=10s`, assert the wall-clock returned in `Solution.solve_duration_ms` is < 1000 ms (margin: in practice <50 ms). Indirect proof of early-exit firing.
4. **`solver-core/tests/lahc_property.rs` widening** — extend `lahc_small_problem`'s lesson generator's `hours_per_week` to cover 2..=4 (already widened in item 37; verify still in place). Add a new `assert!` to both existing property tests: no `(class, day, subject)` triple exceeds the per-subject cap on any returned `Solution`.
5. **`backend/tests/db/test_subject_max_hours_per_day_migration.py` (new)** — alembic up/down round-trip for the new revision, assert the column exists with `NOT NULL` + default 2 after upgrade and is gone after downgrade. Mirror for `school_classes.max_lessons_per_day` (nullable).
6. **`backend/tests/scheduling/test_subject_routes.py` and `test_school_class_routes.py`** — extend existing CRUD tests to round-trip the new fields; assert default 2 for subject create-without-cap, assert PATCH with explicit null clears class cap.
7. **`backend/tests/scheduling/test_solver_io.py`** — update the two violation-kind tests per backend/CLAUDE.md rule. Add a new test that posts a problem where 3 hours of one subject in one day is the only FFD-greedy choice, confirms the solver returns a `Solution` honoring the cap (no triple exceeded) and the unschedulable lesson(s) raise `NoFreeTimeBlock` (existing variant) rather than the new cap variants (which are diagnostic-only since pruning prevents construction).
8. **`backend/tests/core/test_settings.py`** — add `test_solve_deadline_ms_default_is_5000`, mirroring the solver-backend default test pattern.
9. **`frontend/src/features/subjects/__tests__/subject-edit.test.tsx`** — render the edit dialog, assert the new `max_hours_per_day` input renders with default 2, type 3 + submit, assert payload includes `max_hours_per_day: 3`.
10. **`frontend/src/features/school-classes/__tests__/school-class-edit.test.tsx`** — render the edit dialog, assert the new `max_lessons_per_day` input renders empty, type 5 + submit, assert payload includes `max_lessons_per_day: 5`. Second case: type 5, then clear, submit, assert payload includes `max_lessons_per_day: null`.
11. **`mise run lint`**, **`cargo nextest run --workspace`**, **`mise run test:py`**, **`mise run fe:test`** all green.

## Documentation

- `docs/adr/0033-solver-daily-caps-and-early-exit.md`: context, decision (two caps + early exit + raised deadline), consequences (production-default deadline raised; new violation kinds added; existing schedules grandfathered until regeneration; bake-off bench fixtures verified compatible). Index in `docs/adr/README.md`.
- `docs/superpowers/OPEN_THINGS.md`: append items 38 (per-day caps) + 39 (early exit + raised deadline) to the active-sprint algorithm phase, mark closed-by-this-PR, then delete both per OPEN_THINGS hygiene rule. Promote the next-pickup line to item 30.
- `solver/CLAUDE.md`: add a one-line bullet under hard-constraint rules: "Per-day caps (`Subject.max_hours_per_day`, `Class.max_lessons_per_day`) are enforced via legality pruning in `try_place_block` and the row-removal helper; `GreedyState` carries the per-`(class, day, subject)` and `(class, day)` counters."
- `backend/CLAUDE.md`: add a one-line bullet under data access: "Subject + SchoolClass have per-day caps; PATCH handlers must use `model_fields_set` for `max_lessons_per_day` (nullable)."
- `frontend/CLAUDE.md`: no new rule; the form pattern follows existing edit-dialog conventions.
- Auto-memory: refresh `roadmap_status.md` to note items 38 + 39 shipped and item 30 is now next pickup.

## Acceptance criteria

- New regression tests green (`daily_caps.rs`, `early_exit.rs`, widened property tests).
- Backend, frontend, and lint suites green.
- Migration up/down round-trips.
- `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule` shows `lahc_rr_kempe` at `placements_med == expected`, `hard_med == 0`, `soft_med == 0`. Receipt pasted into PR body.
- ADR 0033 written and indexed.
- OPEN_THINGS items 38 + 39 added and removed in lockstep with the close.
- PR body lists the bench receipt and the three closed symptoms (caps fix symptom 2; raised deadline + early exit fix symptom 1; symptom 3 explicitly deferred).
