# Solver constraints for compact, home-room anchored Grundschule schedules

**Status:** design (2026-05-03)
**Owner:** /autopilot run on `feat/solver-grundschule-constraints`
**Brainstorm:** `/tmp/kz-brainstorm/brainstorm.md` (Q&A also posted as PR comments)
**Roadmap:** Quality pass on the scheduling UX program. Sprint A/B/C closed in `OPEN_THINGS.md`; this PR fixes the schedules those sprints expose.

## 1. Problem

The whole-school generator (Sprint A) places lessons that satisfy hard structural constraints (one lesson per slot, room/teacher availability, room-subject suitability) but the resulting Grundschule schedule fails on five quality axes that nobody is asserting against. Concrete evidence from the live demo DB, class 1a (grade 1):

| Day | Periods occupied | Rooms used | Notes |
|---|---|---|---|
| Mon | 1, 2, 3, 5, 6, 7 | Klasse 3a, 3a, Turnhalle, 2a, 4a, 4a | 6 lessons, ends 14:05, gap at P4 |
| Tue | 2, 3, 4, 5, 6 | 2a, 2a, 3a, 2a, Kunstraum | clean shape |
| Wed | 1, 2, 3, 4, 6 | 1a, 2a, 2a, 2a, 1a | gap at P5; D + M each in two rooms |
| Thu | 1, 3, 4, 5 | 2a, 4a, 2a, Turnhalle | gap at P2 |
| Fri | 2, 3, 5 | Turnhalle, Musikraum, Kunstraum | gaps at P1 and P4 |

Five distinct failure axes, with no automated check that any of them stays bounded:

1. **Day length.** Period 7 (13:20-14:05) is past the Hessen "4 Zeitstunden" guideline for Klasse 1.
2. **Mid-day gaps.** Five gaps across the week. The existing `class_gap` soft cost runs at weight 1, equal to every other axis.
3. **Same-subject room hopping.** Wednesday: D in Klasse 1a (P1) and Klasse 2a (P4); M in Klasse 2a (P2/P3) and Klasse 1a (P6). No constraint forbids this.
4. **Home-room underuse.** Of 23 placements, only 2 use Klasse 1a's home room despite `home_room_id` being set on every grade 1-4 class.
5. **Daily count imbalance.** 6 / 5 / 5 / 4 / 3 placements. Even spread for 23 lessons over 5 days is 5+5+5+4+4.

The ask: ship constraints that demonstrably move all five axes, plus an automated test layer so future regressions surface as red CI rather than as bad schedules in the UI. A second ask: rework the Wochenschema editor (currently a dialog-driven flat table) into a grid editor that admins can use to adjust the period count without database surgery.

## 2. Goals and non-goals

**Goals.**

- Hard constraint in `solver-core`: for every `(school_class, day_of_week, subject)`, all placements share one room.
- New per-class soft cost in `solver-core`: L1 distance from the per-class daily-mean placement count, summed across classes.
- New per-subject weight axis `prefer_late_period` (mirror of existing `prefer_early_period`). Subject FÖ seeded to `5`.
- Default solver weight bumps: `class_gap` 1→10, `teacher_gap` 1→10, `prefer_home_room` 1→5. New `class_day_balance` weight default `5`.
- Alembic migration adding `subjects.prefer_late_period int not null default 0`. SQLAlchemy + Pydantic + `solver_io` plumbing.
- Demo Grundschule seed: Wochenschema drops period 7 (now 6 periods, day ends 13:05). FÖ subject seeded with `prefer_late_period=5`.
- New backend module `scheduling/quality_checks.py`: pure-function predicates returning structured `QualityIssue` records. Reusable in tests, in a future endpoint, and as a CI gate.
- Backend integration test that seeds the demo Grundschule, runs the solver, and asserts every predicate returns no issues.
- Solver-core unit tests covering each new constraint and weight axis.
- Frontend Wochenschema editor: replace `time-blocks-table.tsx` with a grid layout (columns = days, rows = period numbers) supporting bulk add/remove of a row across all days plus per-cell add/edit/delete.

**Non-goals (deferred).**

- Per-class `regular_max_period` / `absolute_max_period` overrides. The Wochenschema is the single source of truth.
- A `Subject.is_extension` flag. Replaced by the `prefer_late_period` soft axis.
- Per-school admin tuning of solver weights. Defaults are global; tunable later.
- Surfacing `QualityIssue`s in the frontend ("your schedule has 3 home-room misses").
- Multi-Wochenschema schools. Each school still has one scheme.
- Rust solver heuristic changes (LAHC, simulated annealing tuning). Constraint shape only.

## 3. Architecture

### 3.1 Solver-core (`solver/solver-core/src/`)

Three changes inside `score.rs` and the `Problem` schema:

**Hard constraint `SameRoomPerSubjectPerDay`.** Added to the existing `validate.rs` post-placement validator. Implementation: walk the placement vector, group by `(class_id, day, subject_id)`, on each group walk and assert all `room_id` values equal. Returns a typed `HardViolation::SameRoomPerSubjectPerDay { class_id, day, subject_id, rooms }` on mismatch. Same predicate runs as a hard reject inside the placement loop so the search prunes early.

**Soft cost `class_day_balance`.** New field on `Weights`:

```rust
pub struct Weights {
    pub class_gap: i32,
    pub teacher_gap: i32,
    pub prefer_early_period: i32,
    pub avoid_first_period: i32,
    pub avoid_last_period: i32,
    pub prefer_home_room: i32,
    pub prefer_late_period: i32,    // new
    pub class_day_balance: i32,     // new
}
```

Default values change: `class_gap` 1→10, `teacher_gap` 1→10, `prefer_home_room` 1→5, `class_day_balance` 5, `prefer_late_period` 1 (per-axis weight; per-subject weight is on `Subject` itself and zero by default).

The score function gains:

```rust
fn class_day_balance_cost(placements: &[Placement], classes: &[SchoolClass], days: u8) -> i64 {
    let mut total: i64 = 0;
    for class in classes {
        let counts = day_counts(class.id, placements, days); // [u32; days]
        let sum: u32 = counts.iter().sum();
        if sum == 0 { continue; }
        let mean = sum as f64 / days as f64;
        for c in counts {
            total += ((c as f64) - mean).abs() as i64;
        }
    }
    total
}
```

**Per-subject `prefer_late_period`.** New field on `Subject`:

```rust
pub struct Subject {
    pub id: SubjectId,
    pub name: String,
    pub prefer_early_period: i32,
    pub avoid_first_period: i32,
    pub avoid_last_period: i32,
    pub prefer_late_period: i32,    // new
}
```

Score contribution mirrors `prefer_early_period`: cost grows linearly in the distance between the placement's position and the last position of the day. With FÖ at 5 and other subjects at 0, the search prefers FÖ in periods 5/6.

### 3.2 Backend (`backend/src/klassenzeit_backend/`)

**Migration.** `alembic/versions/<rev>_add_prefer_late_period_to_subjects.py`. One column, server default 0, not null, reversible.

**Model + schema.** `db/models/subject.py` adds the column. `subjects/schemas.py` adds the field to `SubjectBase` (and therefore to `SubjectRead`, `SubjectCreate`, `SubjectUpdate`). Default 0, validated `>= 0`.

**Solver IO.** `scheduling/solver_io.py:build_problem_json` reads `subject.prefer_late_period` and forwards it inside the per-subject record. `routes/schedule.py` updates the default `Weights` literal to the new numbers.

**Quality predicates.** New module `scheduling/quality_checks.py` exporting:

```python
@dataclass(frozen=True)
class QualityIssue:
    kind: Literal["room_hop", "imbalance", "home_room_miss", "day_too_long", "interior_gap"]
    school_class_id: UUID
    day_of_week: int | None
    subject_id: UUID | None
    detail: dict[str, object]

def check_room_hop(schedule: list[Placement]) -> Iterable[QualityIssue]: ...
def check_class_day_balance(schedule, classes, max_spread: int = 2) -> Iterable[QualityIssue]: ...
def check_home_room_ratio(schedule, classes, min_ratio: float = 0.6) -> Iterable[QualityIssue]: ...
def check_day_length(schedule, week_scheme, max_position: int) -> Iterable[QualityIssue]: ...
def check_interior_gaps(schedule, week_scheme, max_gaps_per_class: int = 2) -> Iterable[QualityIssue]: ...

def all_quality_issues(...) -> list[QualityIssue]: ...
```

Each predicate is a pure function on already-loaded entities. The integration test loads the freshly-generated schedule, calls `all_quality_issues(...)`, asserts the result is empty, and prints any issue's `detail` field on failure for fast debugging.

The `room_hop` predicate is the same shape as the solver's hard constraint, kept independent on purpose: defence in depth against a solver bug that lets a violation through, plus a sanity check on hand-pinned data.

### 3.3 Seed (`backend/src/klassenzeit_backend/seed/demo_grundschule.py`)

Three edits:

1. `_PERIODS` shrinks from 7 entries to 6. Period 7 (13:20-14:05) is removed. The 6-period day ends at 13:05.
2. `_seed_subjects` sets `prefer_late_period=5` on the FÖ row.
3. The post-seed solver call is unchanged; it now produces a schedule constrained by all of the above.

The two-track variants (`demo_grundschule_zweizuegig.py`, `demo_grundschule_dreizuegig.py`) inherit `_PERIODS` so they update for free. Sprint roadmap demos live elsewhere and are not Grundschule.

### 3.4 Frontend Wochenschema editor (`frontend/src/features/week-schemes/`)

`time-blocks-table.tsx` (335 LOC, dialog-driven) becomes `time-blocks-grid.tsx`: a Tailwind grid where columns are days and rows are period numbers `1..N`. Each cell is either a filled chip showing `start-end` time or an empty "+" affordance.

Interactions:

- **Add row** (button at bottom): bulk-creates one `time_block` per day at the next position. Default times derived from the previous row's end + 5 min break + 45 min slot.
- **Remove row** (X icon at the row's left): bulk-deletes that position across all days. Backend FK constraint already protects against orphaning a `scheduled_lesson`; the UI surfaces the resulting 409 with a clear translation.
- **Per-cell add** (click empty cell): inline form for start/end time, single time_block created.
- **Per-cell edit** (click filled cell): inline edit.
- **Per-cell delete** (X on filled chip): same FK protection.

State pattern follows `frontend/CLAUDE.md`: outer/inner draft-from-fetch. Outer fetches `useWeekSchemeDetail`, inner takes the loaded entity as a prop and seeds local edit state. Two new hooks (`useAddTimeBlockRow`, `useRemoveTimeBlockRow`) wrap the existing `useCreateTimeBlock` / `useDeleteTimeBlock` mutations with `Promise.all` over the day count.

The grid does not extract a primitive shared with the schedule view: the visual idiom is shared (Tailwind grid, same border/spacing tokens) but the cell content shape differs. Composition over abstraction; revisit if a third grid lands.

i18n keys: `weekSchemes.grid.*` namespace replaces the `weekSchemes.timeBlocks.*` keys used by the table. The legacy keys are deleted in the same commit.

## 4. Testing strategy

The user explicitly asked for "better testing instead of having to find out". Three layers:

**Layer 1 — solver-core unit tests (Rust).** New tests under `solver/solver-core/tests/`:

- `same_room_per_subject_per_day_property.rs`: property test, generate random valid problems with a fixed seed, solve, assert no `(class, day, subject)` group has more than one distinct room.
- `class_day_balance_property.rs`: solve a balanced problem (e.g. 25 lessons over 5 days) and assert max-min daily count for any class is `<= 2`.
- `prefer_late_period_unit.rs`: synthetic 2-class 1-FÖ-lesson fixture, assert FÖ's chosen position is in the latter half of the day.

**Layer 2 — backend integration test (Python).** `backend/tests/scheduling/test_grundschule_schedule_quality.py`. Seeds the demo Grundschule, calls the per-class generate endpoint for all four classes, then `all_quality_issues(...)` against the persisted schedule, asserts the result is empty. Runs against the standard test database, no special harness.

**Layer 3 — predicate unit tests.** `backend/tests/scheduling/test_quality_checks.py`. Each predicate gets a small synthetic input fixture (one class, one room hop) and a clean fixture (no issues). Covers the predicates themselves so a regression in the predicate logic is caught even if Layer 2 is green by coincidence.

The frontend grid editor gets a Vitest test (`time-blocks-grid.test.tsx`) covering the bulk add and bulk remove flows plus a per-cell edit. MSW handler updates in `tests/msw-handlers.ts` if the existing `useCreateTimeBlock` / `useDeleteTimeBlock` handlers need extension.

## 5. Commit plan

Ten commits on `feat/solver-grundschule-constraints`. Each independently builds and passes its own scoped test runner. Solver-core changes precede backend changes that consume them; the binding is rebuilt once via `mise run solver:rebuild` between solver-core and backend phases.

1. `feat(solver-core): hard constraint same-room per (class, day, subject)`
2. `feat(solver-core): add class_day_balance soft cost`
3. `feat(solver-core): add prefer_late_period subject weight axis`
4. `feat(solver-core): bump default class_gap, teacher_gap, prefer_home_room weights`
5. `feat(backend): add prefer_late_period column on subjects`
6. `feat(seed): grundschule wochenschema 6 periods, FÖ prefers late`
7. `feat(backend): schedule quality predicates module`
8. `test(backend): grundschule schedule quality integration test`
9. `feat(frontend): wochenschema grid editor`
10. `test(frontend): wochenschema grid editor interactions`

## 6. Risks and mitigations

- **Existing solver tests pin the current weight numbers.** Sweep `solver/solver-core/tests/` and `backend/tests/scheduling/` in commit 4 and convert exact-cost assertions to property-style ("class_gap > teacher_gap > prefer_home_room > 0") before the bump lands.
- **Existing pinned placements may already violate the same-room rule.** Sprint C added pinning. Run the new validator over the current fixtures during commit 1; if any violate, rewrite the fixture rather than weakening the rule.
- **Solver perf regression.** Same-room hard adds one O(placements) hash-group pass per evaluation. Daily-balance adds O(classes * days). Both should be in the noise. The `BASELINE.md` 20% regression budget governs; subagent reports the delta.
- **Wochenschema editor diff overflow.** Estimated ~500 frontend LOC + ~50 test LOC. If the actual diff exceeds ~700 LOC after tests, surface a split rather than push past the budget.
- **Migration runs against existing staging.** The new column is `not null` with a server default, so existing rows pick up `0` automatically. Reversible. No data migration step.

## 7. Success criteria

- All ten commits land on a green CI run.
- The integration test `test_grundschule_schedule_quality` runs against the demo Grundschule and reports zero `QualityIssue`s.
- Manual inspection of class 1a's regenerated schedule shows: at most 5 lessons per day, no room hopping within a day for one subject, daily counts within `[4, 5]`, home-room usage above 70% (excluding specialty subjects).
- The Wochenschema editor renders as a grid, supports add/remove row in one click, and persists changes through existing endpoints.
- Solver baseline performance regression is within the 20% budget per `BASELINE.md`.
