# Solver Grundschule Constraints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the generated Grundschule schedule compact and home-room anchored by adding one hard constraint (same room per class/day/subject), one new soft cost (per-class daily-balance), one per-subject weight axis (`prefer_late_period`), and bumped default weights for `class_gap` / `teacher_gap` / `prefer_home_room`. Reduce the seed Wochenschema from 7 periods to 6. Replace the dialog-driven Wochenschema editor with a grid editor. Bake an automated quality-check layer so future regressions surface as red CI rather than as bad schedules in the UI.

**Architecture:** Solver-core gains a placement-time hard reject for same-day same-subject room mismatches and two new soft-score terms. Backend gains a migration adding `subjects.prefer_late_period`, plumbing through `solver_io`, plus a new `quality_checks` module of pure-function predicates that an integration test asserts against the freshly-generated demo Grundschule. Frontend swaps the time-blocks table for a grid that mirrors the schedule view.

**Tech Stack:** Rust 1.85 (`solver-core`), PyO3 0.28 (`solver-py`), FastAPI + SQLAlchemy async + Alembic + Pydantic v2 (backend), React 19 + TanStack Query/Router + shadcn/ui + Vitest (frontend).

---

## File Structure

### solver-core (Rust)

- **Modify** `solver/solver-core/src/types.rs`: add `ConstraintWeights.prefer_late_period`, `ConstraintWeights.class_day_balance`; add `Subject.prefer_late_period`; add `ViolationKind::RoomMismatchSameSubjectSameDay`.
- **Modify** `solver/solver-core/src/score.rs`: add `class_day_balance_cost`; extend `subject_preference_score` with `prefer_late_period`; update zero-weights short-circuit.
- **Modify** `solver/solver-core/src/solve.rs`: in the placement loop, track first-room-per-(class, day, subject) and reject candidates that would use a different room.
- **Modify** `solver/solver-core/src/lahc.rs`: same enforcement in the neighbour-feasibility check.
- **Modify** `solver/solver-core/src/validate.rs`: post-solve invariant assertion (defense in depth).
- **Modify** `solver/solver-core/benches/solver_fixtures.rs`: add the new `Subject` and `ConstraintWeights` fields to fixture literals.
- **Modify** `solver/solver-core/tests/common/mod.rs`, `tests/grundschule_smoke.rs`, `tests/lahc_property.rs`, `tests/properties.rs`, `tests/score_property.rs`, `tests/ffd_solver_outcome.rs`: cascade the field additions.
- **Modify** `solver/solver-core/benches/BASELINE.md`: re-record baseline at end of solver phase.
- **Create** `solver/solver-core/tests/same_room_property.rs`: property test for the new hard constraint.
- **Create** `solver/solver-core/tests/class_day_balance_property.rs`: property test for the new soft cost.
- **Create** `solver/solver-core/tests/prefer_late_period_unit.rs`: unit test that FÖ-shaped input lands late.

### solver-py

- **Modify** `solver/solver-py/python/klassenzeit_solver/types.pyi`: add `prefer_late_period` to `Subject`, `class_day_balance` and `prefer_late_period` to `ConstraintWeights`, `room_mismatch_same_subject_same_day` to `ViolationKind` literal.

### Backend (Python)

- **Create** `backend/alembic/versions/<rev>_add_prefer_late_period_to_subjects.py`.
- **Modify** `backend/src/klassenzeit_backend/db/models/subject.py`: add `prefer_late_period: Mapped[int]`.
- **Modify** `backend/src/klassenzeit_backend/subjects/schemas.py`: add the field to `SubjectBase`.
- **Modify** `backend/src/klassenzeit_backend/scheduling/solver_io.py`: forward `prefer_late_period` per subject; emit the new fields in `ConstraintWeights`.
- **Modify** `backend/src/klassenzeit_backend/scheduling/routes/schedule.py`: update default weights literal.
- **Create** `backend/src/klassenzeit_backend/scheduling/quality_checks.py`: pure-function predicates and `QualityIssue` dataclass.
- **Create** `backend/tests/scheduling/test_quality_checks.py`: predicate unit tests.
- **Create** `backend/tests/scheduling/test_grundschule_schedule_quality.py`: integration test running the predicates against a freshly-generated Grundschule schedule.

### Seed (Python)

- **Modify** `backend/src/klassenzeit_backend/seed/demo_grundschule.py`: drop `_PERIODS` row 7; set `prefer_late_period=5` on the FÖ subject row.

### Frontend (TS/TSX)

- **Replace** `frontend/src/features/week-schemes/time-blocks-table.tsx` with `frontend/src/features/week-schemes/time-blocks-grid.tsx`.
- **Modify** `frontend/src/features/week-schemes/hooks.ts`: add `useAddTimeBlockRow`, `useRemoveTimeBlockRow`.
- **Modify** `frontend/src/features/week-schemes/week-schemes-page.tsx`: render the new grid in place of the table.
- **Modify** `frontend/src/i18n/locales/{en,de}.json`: replace `weekSchemes.timeBlocks.*` with `weekSchemes.grid.*`.
- **Create** `frontend/src/features/week-schemes/time-blocks-grid.test.tsx`.
- **Modify** `frontend/tests/msw-handlers.ts` if the bulk hooks need adjusted handlers (likely not; existing per-block handlers fan out).

### Docs

- **Modify** `docs/superpowers/OPEN_THINGS.md`: mark the Grundschule schedule quality follow-up as resolved; surface deferred items (per-class overrides, multi-Wochenschema schools, weight tuning UI).

---

## Tasks

### Task 1: Add `Subject.prefer_late_period` field to solver-core types

**Files:**
- Modify: `solver/solver-core/src/types.rs:152-177`
- Modify: every fixture with a `Subject { ... }` literal (cascade list above).

- [ ] **Step 1.1: Add field to `Subject` struct**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    pub id: SubjectId,
    #[serde(default)]
    pub prefer_early_period: u32,
    #[serde(default)]
    pub avoid_first_period: u32,
    #[serde(default)]
    pub avoid_last_period: u32,
    /// Per-Subject weight applied to the late-period axis. Scoring adds
    /// `(max_position_for_day - tb.position) * weights.prefer_late_period
    /// * subject.prefer_late_period` per placement (saturating). Zero
    /// disables this axis for the subject. Wire format is additive:
    /// callers omitting the field deserialise to 0.
    #[serde(default)]
    pub prefer_late_period: u32,
}
```

- [ ] **Step 1.2: Cascade `prefer_late_period: 0` through every `Subject { ... }` literal**

`cargo build -p solver-core` lists each offender. Add the field at the end of every literal in `src/`, `benches/`, and `tests/`.

- [ ] **Step 1.3: Run solver-core tests**

```bash
mise run test:rust
```

All previously-green tests stay green; no new behaviour yet.

- [ ] **Step 1.4: Commit**

```bash
git add solver/solver-core
git commit -m "refactor(solver-core): add Subject.prefer_late_period field (no behaviour)"
```

---

### Task 2: Score `prefer_late_period` axis

**Files:**
- Modify: `solver/solver-core/src/score.rs:194-224` (extend `subject_preference_score`)
- Modify: `solver/solver-core/src/score.rs:20-28` (extend the zero-weights short-circuit)
- Modify: `solver/solver-core/src/types.rs:36-64` (add `ConstraintWeights.prefer_late_period`)

- [ ] **Step 2.1: Add failing unit test**

Append to `score.rs`'s `mod tests`:

```rust
#[test]
fn subject_preference_score_linear_in_distance_from_max_when_prefer_late_set() {
    let weights = ConstraintWeights {
        prefer_late_period: 4,
        ..ConstraintWeights::default()
    };
    let mk_subject = |w: u32| Subject {
        id: SubjectId(score_uuid(40)),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: w,
    };
    // max_position_for_day = 5; pos 0 contributes 5 * 4 * 1 = 20,
    // pos 5 contributes 0.
    for pos in 0u8..=5 {
        let tb = TimeBlock {
            id: TimeBlockId(score_uuid(10)),
            day_of_week: 0,
            position: pos,
        };
        assert_eq!(
            subject_preference_score(&mk_subject(1), &tb, 5, &weights),
            u32::from(5 - pos) * 4
        );
    }
}
```

- [ ] **Step 2.2: Run test, verify failure**

```bash
cargo nextest run -p solver-core subject_preference_score_linear_in_distance_from_max_when_prefer_late_set
```

Expected: FAIL (field does not exist on `ConstraintWeights`).

- [ ] **Step 2.3: Add `ConstraintWeights.prefer_late_period: u32` and `class_day_balance: u32`**

```rust
pub struct ConstraintWeights {
    pub class_gap: u32,
    pub teacher_gap: u32,
    pub prefer_early_period: u32,
    pub avoid_first_period: u32,
    pub prefer_home_room: u32,
    pub avoid_last_period: u32,
    /// Global multiplier on the late-period axis. Per-placement penalty
    /// is `(max_position_for_day - tb.position) * weights.prefer_late_period
    /// * subject.prefer_late_period` (saturating). Zero disables the axis
    /// globally; a non-zero global with `subject.prefer_late_period == 0`
    /// still contributes nothing.
    pub prefer_late_period: u32,
    /// Penalty applied per-class for daily-count imbalance. Cost is the
    /// sum of `|count(day) - mean|` over days for each class with at
    /// least one placement. Multiplied by this weight (saturating).
    /// Zero disables the axis.
    pub class_day_balance: u32,
}
```

- [ ] **Step 2.4: Extend `subject_preference_score`**

Add the late-period term inside the function:

```rust
if subject.prefer_late_period > 0 && weights.prefer_late_period > 0 {
    let distance = u32::from(max_position_for_day.saturating_sub(tb.position));
    score = score.saturating_add(
        weights
            .prefer_late_period
            .saturating_mul(subject.prefer_late_period)
            .saturating_mul(distance),
    );
}
```

- [ ] **Step 2.5: Update the early-exit guard at the top of `score_solution`**

Add `weights.prefer_late_period == 0 && weights.class_day_balance == 0` to the existing all-zero check.

- [ ] **Step 2.6: Run all tests**

```bash
mise run test:rust
```

Expected: all green, including the new test.

- [ ] **Step 2.7: Commit**

```bash
git add solver/solver-core
git commit -m "feat(solver-core): add prefer_late_period subject weight axis"
```

---

### Task 3: New soft cost `class_day_balance`

**Files:**
- Modify: `solver/solver-core/src/score.rs` (add helper + integration into `score_solution`)
- Create: `solver/solver-core/tests/class_day_balance_property.rs`

- [ ] **Step 3.1: Add failing unit test**

Append to `score.rs`'s `mod tests`:

```rust
#[test]
fn class_day_balance_zero_for_perfectly_even_spread() {
    // 4 placements over 4 days = 1/1/1/1, balance cost = 0.
    let mut p = three_block_one_class_problem();
    for day in 1..=3u8 {
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(score_uuid(20 + day)),
            day_of_week: day,
            position: 0,
        });
    }
    let weights = ConstraintWeights {
        class_day_balance: 5,
        ..ConstraintWeights::default()
    };
    let placements = [place(60, 10), place(60, 21), place(60, 22), place(60, 23)];
    assert_eq!(score_solution(&p, &placements, &weights), 0);
}

#[test]
fn class_day_balance_penalises_lopsided_spread() {
    // 4 placements over 4 days = 4/0/0/0; mean = 1; |4-1|+|0-1|*3 = 6.
    let mut p = three_block_one_class_problem();
    for day in 1..=3u8 {
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(score_uuid(20 + day)),
            day_of_week: day,
            position: 0,
        });
    }
    // Need extra positions on day 0 to fit four placements.
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(score_uuid(13)),
        day_of_week: 0,
        position: 3,
    });
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(score_uuid(14)),
        day_of_week: 0,
        position: 4,
    });
    let weights = ConstraintWeights {
        class_day_balance: 5,
        ..ConstraintWeights::default()
    };
    // All four placements on day 0.
    let placements = [
        place(60, 10),
        place(60, 11),
        place(60, 12),
        place(60, 13),
    ];
    // Balance = |4-1| + |0-1|*3 = 6, weighted = 30.
    assert_eq!(score_solution(&p, &placements, &weights), 30);
}
```

- [ ] **Step 3.2: Run, verify failure**

```bash
cargo nextest run -p solver-core class_day_balance
```

Expected: FAIL (helper not implemented yet).

- [ ] **Step 3.3: Implement `class_day_balance_cost` and integrate**

In `score.rs`:

```rust
pub(crate) fn class_day_balance_cost(
    by_class_day: &HashMap<(SchoolClassId, u8), Vec<u8>>,
    classes: &[SchoolClass],
    days: u8,
) -> u32 {
    if days == 0 {
        return 0;
    }
    let mut total: u32 = 0;
    for class in classes {
        let counts: Vec<u32> = (0..days)
            .map(|d| {
                by_class_day
                    .get(&(class.id, d))
                    .map(|v| v.len() as u32)
                    .unwrap_or(0)
            })
            .collect();
        let sum: u32 = counts.iter().sum();
        if sum == 0 {
            continue;
        }
        // Use times-days arithmetic so we stay in integers; scale by days at end.
        let mean_times_days = sum;
        for c in &counts {
            let scaled = (c.saturating_mul(u32::from(days))).abs_diff(mean_times_days);
            total = total.saturating_add(scaled);
        }
        // Total scaled by days; divide once.
        // (we accumulate and divide at the end of the per-class block to avoid losing precision when sum % days != 0)
        // Note: integer division here is acceptable; any rounding bias is sub-1 per class.
    }
    total / u32::from(days.max(1))
}
```

Wire into `score_solution` between subject_preference and home_room totals; pass `by_class_day`, `&problem.school_classes`, and the day count derived from `problem.time_blocks` (max `day_of_week` + 1).

Multiply by `weights.class_day_balance` and saturating_add into the final total.

- [ ] **Step 3.4: Add property test integration**

Create `solver/solver-core/tests/class_day_balance_property.rs`:

```rust
//! Property: a balanced problem (lessons divisible by day count) solved with
//! a non-zero class_day_balance weight produces a per-class daily count
//! whose max - min is at most 1.

use solver_core::{solve_with_config, ConstraintWeights, SolveConfig};

mod common;

#[test]
fn balanced_problem_lands_within_one_per_day() {
    let problem = common::balanced_grundschule_problem(); // helper to add
    let config = SolveConfig {
        weights: ConstraintWeights {
            class_day_balance: 10,
            ..ConstraintWeights::default()
        },
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve");
    let counts = common::per_class_day_counts(&problem, &solution);
    for (_class, daily) in counts {
        let (mn, mx) = (daily.iter().min().copied().unwrap_or(0), daily.iter().max().copied().unwrap_or(0));
        assert!(mx - mn <= 1, "daily counts {:?} differ by more than 1", daily);
    }
}
```

Add `balanced_grundschule_problem` and `per_class_day_counts` helpers to `tests/common/mod.rs`.

- [ ] **Step 3.5: Run all tests**

```bash
mise run test:rust
```

- [ ] **Step 3.6: Commit**

```bash
git add solver/solver-core
git commit -m "feat(solver-core): add class_day_balance soft cost"
```

---

### Task 4: Hard constraint `RoomMismatchSameSubjectSameDay`

**Files:**
- Modify: `solver/solver-core/src/types.rs:319` (add variant to `ViolationKind`)
- Modify: `solver/solver-core/src/solve.rs` (placement-time enforcement)
- Modify: `solver/solver-core/src/lahc.rs` (LAHC neighbour-feasibility enforcement)
- Modify: `solver/solver-core/src/validate.rs` (post-solve invariant)
- Create: `solver/solver-core/tests/same_room_property.rs`

- [ ] **Step 4.1: Add failing property test**

Create `solver/solver-core/tests/same_room_property.rs`:

```rust
//! Property: for every solved problem, no `(class, day, subject)` triple
//! has placements in two different rooms.

use std::collections::HashMap;
use solver_core::{solve_with_config, SolveConfig};

mod common;

#[test]
fn no_room_hopping_within_day_for_one_subject() {
    let problem = common::demo_grundschule_problem();
    let solution = solve_with_config(&problem, &SolveConfig::default()).expect("solve");
    let mut groups: HashMap<(_, u8, _), _> = HashMap::new();
    for placement in &solution.placements {
        let lesson = problem.lessons.iter().find(|l| l.id == placement.lesson_id).unwrap();
        let tb = problem.time_blocks.iter().find(|t| t.id == placement.time_block_id).unwrap();
        for class in &lesson.school_class_ids {
            let key = (*class, tb.day_of_week, lesson.subject_id);
            let entry = groups.entry(key).or_insert_with(|| placement.room_id);
            assert_eq!(*entry, placement.room_id, "room hop for {:?}", key);
        }
    }
}
```

- [ ] **Step 4.2: Add `RoomMismatchSameSubjectSameDay` variant**

In `types.rs:319`:

```rust
pub enum ViolationKind {
    NoQualifiedTeacher,
    TeacherOverCapacity,
    NoFreeTimeBlock,
    NoSuitableRoom,
    LessonGroupSplit,
    PinnedConflict,
    /// A candidate placement would put a (class, day, subject) triple in a
    /// second room within the same day. The solver rejects the placement;
    /// this variant signals that the rejection chain led to NoSuitableRoom.
    RoomMismatchSameSubjectSameDay,
}
```

- [ ] **Step 4.3: Enforce at placement time in `solve.rs`**

Track, per FFD step, a map `(class_id, day_of_week, subject_id) -> RoomId` populated as placements are made. In `try_place_block` (or its room-selection step), skip rooms that disagree with an entry already in the map. If no compatible room exists, fail the block with `NoSuitableRoom` and emit a `RoomMismatchSameSubjectSameDay` violation alongside.

Implementation sketch (annotate the existing room-loop):

```rust
let key = |class_id, day, subject_id| (class_id, day, subject_id);
let mut locked_room: HashMap<(SchoolClassId, u8, SubjectId), RoomId> = HashMap::new();
// ... seed from existing accepted placements ...
'rooms: for room_id in candidate_rooms {
    for class_id in &lesson.school_class_ids {
        let k = key(*class_id, day_of_week, lesson.subject_id);
        if let Some(locked) = locked_room.get(&k) {
            if *locked != room_id {
                continue 'rooms;
            }
        }
    }
    // ... rest of feasibility check ...
    // on accept: locked_room.entry(...).or_insert(room_id) for each class.
    break;
}
```

- [ ] **Step 4.4: Enforce in `lahc.rs` neighbour feasibility**

In the LAHC accept-or-reject path, before scoring a swap, recompute the post-swap `locked_room` map and reject if any class would see a room mismatch. RNG draw count must stay invariant across this branch (per `solver/CLAUDE.md`); do the check after the two `random_range` draws, not before.

- [ ] **Step 4.5: Add post-solve invariant in `validate.rs`**

```rust
pub fn validate_no_room_hopping(problem: &Problem, placements: &[Placement]) -> Result<(), Error> {
    let mut groups: HashMap<(SchoolClassId, u8, SubjectId), RoomId> = HashMap::new();
    for placement in placements {
        let lesson = problem.lessons.iter().find(|l| l.id == placement.lesson_id)
            .ok_or_else(|| Error::Input(format!("unknown lesson {:?}", placement.lesson_id)))?;
        let tb = problem.time_blocks.iter().find(|t| t.id == placement.time_block_id)
            .ok_or_else(|| Error::Input(format!("unknown time block {:?}", placement.time_block_id)))?;
        for class_id in &lesson.school_class_ids {
            let key = (*class_id, tb.day_of_week, lesson.subject_id);
            match groups.entry(key) {
                Entry::Vacant(v) => { v.insert(placement.room_id); }
                Entry::Occupied(o) => {
                    if *o.get() != placement.room_id {
                        return Err(Error::Input(format!(
                            "room hopping detected for class {:?} day {} subject {:?}",
                            class_id, tb.day_of_week, lesson.subject_id
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}
```

Call from `solve.rs` after the placement loop succeeds; failure here is a solver bug, surface as `Error::Input` with a clear message.

- [ ] **Step 4.6: Run all tests**

```bash
mise run test:rust
```

Expected: all green including the new property test.

- [ ] **Step 4.7: Commit**

```bash
git add solver/solver-core
git commit -m "feat(solver-core): hard constraint same-room per (class, day, subject)"
```

---

### Task 5: Bump default solver weights

**Files:**
- Modify: `backend/src/klassenzeit_backend/scheduling/routes/schedule.py:54-56` (default `Weights` literal)
- Audit: every test that pins exact weight values; convert exact-cost assertions to property-style.

- [ ] **Step 5.1: Find pinned-exact weight assertions**

```bash
rg -n "class_gap" backend/tests/ solver/solver-core/tests/ | rg -v "class_gap=1$" | head -20
rg -n "prefer_home_room" backend/tests/ solver/solver-core/tests/ | head -20
```

- [ ] **Step 5.2: Update default weights literal**

In `backend/src/klassenzeit_backend/scheduling/routes/schedule.py`:

```python
DEFAULT_WEIGHTS = Weights(
    class_gap=10,
    teacher_gap=10,
    prefer_early_period=1,
    avoid_first_period=1,
    avoid_last_period=1,
    prefer_home_room=5,
    prefer_late_period=1,
    class_day_balance=5,
)
```

- [ ] **Step 5.3: Update tests pinning exact weights**

For each test that asserts `weight == 1` or a numeric outcome derived from weight=1, either pass an explicit weight in the test setup or rewrite the assertion to be ordinal ("class_gap > teacher_gap > prefer_home_room > 0"). Keep the test's intent.

- [ ] **Step 5.4: Run backend + solver tests**

```bash
mise run test:py
mise run test:rust
```

- [ ] **Step 5.5: Commit**

```bash
git add backend solver
git commit -m "feat(backend): bump default class_gap, teacher_gap, prefer_home_room weights"
```

---

### Task 6: Backend migration + Subject schema

**Files:**
- Create: `backend/alembic/versions/<rev>_add_prefer_late_period_to_subjects.py`
- Modify: `backend/src/klassenzeit_backend/db/models/subject.py`
- Modify: `backend/src/klassenzeit_backend/subjects/schemas.py`

- [ ] **Step 6.1: Generate Alembic revision**

```bash
mise exec -- uv run --directory backend alembic revision -m "add prefer_late_period to subjects"
```

Edit the generated file's upgrade/downgrade:

```python
def upgrade() -> None:
    op.add_column(
        "subjects",
        sa.Column("prefer_late_period", sa.Integer(), nullable=False, server_default="0"),
    )

def downgrade() -> None:
    op.drop_column("subjects", "prefer_late_period")
```

- [ ] **Step 6.2: Add the column to the SQLAlchemy model**

In `subject.py`:

```python
prefer_late_period: Mapped[int] = mapped_column(Integer, nullable=False, server_default="0")
```

- [ ] **Step 6.3: Add the field to `SubjectBase` schema**

In `schemas.py`:

```python
prefer_late_period: int = Field(default=0, ge=0)
```

- [ ] **Step 6.4: Reset test DB and run backend tests**

```bash
mise run db:reset
mise run db:migrate
mise run test:py -- backend/tests/subjects -v
```

- [ ] **Step 6.5: Commit**

```bash
git add backend/alembic backend/src/klassenzeit_backend/db backend/src/klassenzeit_backend/subjects
git commit -m "feat(backend): add prefer_late_period column on subjects"
```

---

### Task 7: Plumb `prefer_late_period` and new weight defaults into solver_io

**Files:**
- Modify: `backend/src/klassenzeit_backend/scheduling/solver_io.py:114-150`
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/__init__.py` (the Pydantic `Subject` and `Weights` shapes that go into the `Problem` JSON)

- [ ] **Step 7.1: Add fields to the wire schemas**

The `Subject` Pydantic model used by `build_problem_json` gains `prefer_late_period: int = 0`. The `Weights` model gains `prefer_late_period: int = 0` and `class_day_balance: int = 0`.

- [ ] **Step 7.2: Forward the column in `build_problem_json`**

Read `subject.prefer_late_period` from the SQLAlchemy entity and emit it on the wire.

- [ ] **Step 7.3: Rebuild the binding**

```bash
mise run solver:rebuild
```

- [ ] **Step 7.4: Regenerate frontend types**

```bash
mise run fe:types
```

- [ ] **Step 7.5: Run backend tests**

```bash
mise run test:py
```

- [ ] **Step 7.6: Commit**

```bash
git add backend frontend/src/lib/api-types.ts solver/solver-py
git commit -m "feat(backend): forward prefer_late_period and new weights to solver"
```

---

### Task 8: Seed update — Wochenschema 6 periods + FÖ prefers late

**Files:**
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule.py`

- [ ] **Step 8.1: Drop period 7**

In `_PERIODS`, remove the trailing entry (13:20-14:05). The list now has 6 entries (positions 1-6).

- [ ] **Step 8.2: Set `prefer_late_period=5` on the FÖ subject row**

Find the FÖ subject creation in `_seed_subjects` and add `prefer_late_period=5`.

- [ ] **Step 8.3: Reset and re-seed local DB to verify**

```bash
mise run db:reset
mise run db:migrate
mise exec -- uv run --directory backend python -m klassenzeit_backend.seed.demo_grundschule
```

Inspect the resulting Wochenschema in the DB; expect 6 periods × 5 days = 30 time_blocks per scheme.

- [ ] **Step 8.4: Commit**

```bash
git add backend/src/klassenzeit_backend/seed
git commit -m "feat(seed): grundschule wochenschema 6 periods, FÖ prefers late"
```

---

### Task 9: Quality-checks module

**Files:**
- Create: `backend/src/klassenzeit_backend/scheduling/quality_checks.py`
- Create: `backend/tests/scheduling/test_quality_checks.py`

- [ ] **Step 9.1: Add failing predicate unit tests**

In `test_quality_checks.py`:

```python
def test_check_room_hop_returns_issue_for_two_rooms_one_subject_one_day():
    placements = [
        Placement(class_id=C1, day=0, subject_id=DEUTSCH, room_id=ROOM_A, lesson_id=L1, time_block_id=TB1),
        Placement(class_id=C1, day=0, subject_id=DEUTSCH, room_id=ROOM_B, lesson_id=L2, time_block_id=TB2),
    ]
    issues = list(check_room_hop(placements))
    assert len(issues) == 1
    assert issues[0].kind == "room_hop"
    assert issues[0].school_class_id == C1
    assert issues[0].day_of_week == 0
    assert issues[0].subject_id == DEUTSCH

def test_check_room_hop_returns_empty_for_single_room():
    placements = [
        Placement(class_id=C1, day=0, subject_id=DEUTSCH, room_id=ROOM_A, lesson_id=L1, time_block_id=TB1),
        Placement(class_id=C1, day=0, subject_id=DEUTSCH, room_id=ROOM_A, lesson_id=L2, time_block_id=TB2),
    ]
    assert list(check_room_hop(placements)) == []

def test_check_class_day_balance_flags_spread_over_max():
    counts = {C1: [6, 5, 5, 4, 3]}  # spread = 3
    issues = list(check_class_day_balance(counts, max_spread=2))
    assert len(issues) == 1
    assert issues[0].kind == "imbalance"

def test_check_home_room_ratio_flags_low_usage():
    placements = [
        Placement(class_id=C1, day=0, subject_id=DEUTSCH, room_id=ROOM_OTHER, lesson_id=L1, time_block_id=TB1),
        Placement(class_id=C1, day=0, subject_id=DEUTSCH, room_id=ROOM_OTHER, lesson_id=L2, time_block_id=TB2),
    ]
    home_rooms = {C1: ROOM_HOME}
    issues = list(check_home_room_ratio(placements, home_rooms, min_ratio=0.6, exempt_subjects=set()))
    assert len(issues) == 1
    assert issues[0].kind == "home_room_miss"

def test_check_interior_gaps_returns_issue_above_threshold():
    # class with positions [1, 2, 4, 5] on day 0 has 1 interior gap
    counts = {(C1, 0): [1, 2, 4, 5]}
    issues = list(check_interior_gaps(counts, max_gaps_per_class=0))
    assert len(issues) == 1
```

- [ ] **Step 9.2: Run, verify failure**

```bash
mise run test:py -- backend/tests/scheduling/test_quality_checks.py -v
```

Expected: import error or collection error.

- [ ] **Step 9.3: Implement `quality_checks.py`**

```python
from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import Literal
from uuid import UUID


@dataclass(frozen=True)
class Placement:
    class_id: UUID
    day: int
    subject_id: UUID
    room_id: UUID
    lesson_id: UUID
    time_block_id: UUID


@dataclass(frozen=True)
class QualityIssue:
    kind: Literal["room_hop", "imbalance", "home_room_miss", "day_too_long", "interior_gap"]
    school_class_id: UUID
    day_of_week: int | None = None
    subject_id: UUID | None = None
    detail: dict[str, object] = field(default_factory=dict)


def check_room_hop(placements: list[Placement]) -> Iterable[QualityIssue]:
    seen: dict[tuple[UUID, int, UUID], UUID] = {}
    yielded: set[tuple[UUID, int, UUID]] = set()
    for p in placements:
        key = (p.class_id, p.day, p.subject_id)
        first = seen.setdefault(key, p.room_id)
        if first != p.room_id and key not in yielded:
            yielded.add(key)
            yield QualityIssue(
                kind="room_hop",
                school_class_id=p.class_id,
                day_of_week=p.day,
                subject_id=p.subject_id,
                detail={"rooms": sorted({str(first), str(p.room_id)})},
            )


def check_class_day_balance(
    counts_per_class: dict[UUID, list[int]], max_spread: int = 2
) -> Iterable[QualityIssue]:
    for class_id, daily in counts_per_class.items():
        if not daily:
            continue
        spread = max(daily) - min(daily)
        if spread > max_spread:
            yield QualityIssue(
                kind="imbalance",
                school_class_id=class_id,
                detail={"daily": daily, "spread": spread, "max_spread": max_spread},
            )


def check_home_room_ratio(
    placements: list[Placement],
    home_rooms: dict[UUID, UUID],
    min_ratio: float,
    exempt_subjects: set[UUID],
) -> Iterable[QualityIssue]:
    by_class: dict[UUID, tuple[int, int]] = {}
    for p in placements:
        if p.subject_id in exempt_subjects:
            continue
        if home_rooms.get(p.class_id) is None:
            continue
        hits, total = by_class.get(p.class_id, (0, 0))
        total += 1
        if home_rooms[p.class_id] == p.room_id:
            hits += 1
        by_class[p.class_id] = (hits, total)
    for class_id, (hits, total) in by_class.items():
        if total == 0:
            continue
        ratio = hits / total
        if ratio < min_ratio:
            yield QualityIssue(
                kind="home_room_miss",
                school_class_id=class_id,
                detail={"ratio": ratio, "min_ratio": min_ratio, "hits": hits, "total": total},
            )


def check_interior_gaps(
    positions_per_class_day: dict[tuple[UUID, int], list[int]],
    max_gaps_per_class: int,
) -> Iterable[QualityIssue]:
    gaps_by_class: dict[UUID, int] = {}
    for (class_id, _day), positions in positions_per_class_day.items():
        if len(positions) < 2:
            continue
        positions_sorted = sorted(set(positions))
        span = positions_sorted[-1] - positions_sorted[0] + 1
        gaps = span - len(positions_sorted)
        gaps_by_class[class_id] = gaps_by_class.get(class_id, 0) + gaps
    for class_id, gaps in gaps_by_class.items():
        if gaps > max_gaps_per_class:
            yield QualityIssue(
                kind="interior_gap",
                school_class_id=class_id,
                detail={"interior_gaps_total": gaps, "max_allowed": max_gaps_per_class},
            )


def check_day_length(
    positions_per_class_day: dict[tuple[UUID, int], list[int]],
    max_position: int,
) -> Iterable[QualityIssue]:
    for (class_id, day), positions in positions_per_class_day.items():
        for pos in positions:
            if pos > max_position:
                yield QualityIssue(
                    kind="day_too_long",
                    school_class_id=class_id,
                    day_of_week=day,
                    detail={"position": pos, "max_position": max_position},
                )
                break
```

- [ ] **Step 9.4: Run tests, all green**

```bash
mise run test:py -- backend/tests/scheduling/test_quality_checks.py -v
```

- [ ] **Step 9.5: Commit**

```bash
git add backend/src/klassenzeit_backend/scheduling/quality_checks.py backend/tests/scheduling/test_quality_checks.py
git commit -m "feat(backend): schedule quality predicates module"
```

---

### Task 10: Integration test — Grundschule schedule meets quality bar

**Files:**
- Create: `backend/tests/scheduling/test_grundschule_schedule_quality.py`

- [ ] **Step 10.1: Write the integration test**

Use the existing seeding test fixtures. Pseudo-shape:

```python
import pytest

from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule
from klassenzeit_backend.scheduling.routes.schedule import generate_schedule_for_class
from klassenzeit_backend.scheduling.quality_checks import (
    Placement,
    check_room_hop,
    check_class_day_balance,
    check_home_room_ratio,
    check_interior_gaps,
    check_day_length,
)


@pytest.mark.asyncio
async def test_grundschule_schedule_meets_quality_bar(db_session, ...):
    await seed_demo_grundschule(db_session)
    classes = await load_classes(db_session)
    for klass in classes:
        await generate_schedule_for_class(db_session, klass.id)
    placements = await load_all_placements(db_session)
    home_rooms = {c.id: c.home_room_id for c in classes if c.home_room_id is not None}
    counts_per_class = group_counts_per_class(placements)
    positions_per_class_day = group_positions_per_class_day(placements)
    specialty_subjects = {s.id for s in await load_subjects(db_session) if s.short_name in {"SP", "KU", "MU"}}
    issues = []
    issues += list(check_room_hop(placements))
    issues += list(check_class_day_balance(counts_per_class, max_spread=2))
    issues += list(check_home_room_ratio(placements, home_rooms, min_ratio=0.6, exempt_subjects=specialty_subjects))
    issues += list(check_interior_gaps(positions_per_class_day, max_gaps_per_class=1))
    issues += list(check_day_length(positions_per_class_day, max_position=6))
    assert issues == [], f"quality issues: {issues}"
```

- [ ] **Step 10.2: Run**

```bash
mise run test:py -- backend/tests/scheduling/test_grundschule_schedule_quality.py -v
```

Expected: green. If red, log the issues, diagnose by triple-checking constraints landed and weights bumped (Task 5).

- [ ] **Step 10.3: Commit**

```bash
git add backend/tests/scheduling/test_grundschule_schedule_quality.py
git commit -m "test(backend): grundschule schedule quality integration test"
```

---

### Task 11: Wochenschema grid editor (frontend)

**First action:** invoke `frontend-design` via the `Skill` tool — frontend CLAUDE.md mandates it for new UI surfaces.

**Files:**
- Create: `frontend/src/features/week-schemes/time-blocks-grid.tsx`
- Modify: `frontend/src/features/week-schemes/hooks.ts` (add bulk hooks)
- Modify: `frontend/src/features/week-schemes/week-schemes-page.tsx` (swap component)
- Delete: `frontend/src/features/week-schemes/time-blocks-table.tsx`
- Modify: `frontend/src/i18n/locales/{en,de}.json` (`weekSchemes.grid.*` namespace)
- Create: `frontend/src/features/week-schemes/time-blocks-grid.test.tsx`

- [ ] **Step 11.1: Add bulk hooks**

In `hooks.ts`:

```typescript
export function useAddTimeBlockRow(schemeId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: { positions: Array<{ day_of_week: number; position: number; start_time: string; end_time: string }> }) => {
      await Promise.all(
        input.positions.map((slot) =>
          client.POST("/api/week-schemes/{scheme_id}/time-blocks", {
            params: { path: { scheme_id: schemeId } },
            body: slot,
          }),
        ),
      );
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: weekSchemeDetailQueryKey(schemeId) }),
  });
}

export function useRemoveTimeBlockRow(schemeId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: { block_ids: string[] }) => {
      await Promise.all(
        input.block_ids.map((id) =>
          client.DELETE("/api/week-schemes/{scheme_id}/time-blocks/{block_id}", {
            params: { path: { scheme_id: schemeId, block_id: id } },
          }),
        ),
      );
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: weekSchemeDetailQueryKey(schemeId) }),
  });
}
```

- [ ] **Step 11.2: Build the grid component**

`time-blocks-grid.tsx` (outer/inner draft-from-fetch):

```tsx
export function TimeBlocksGrid({ schemeId }: { schemeId: string }) {
  const detail = useWeekSchemeDetail(schemeId);
  if (detail.isLoading) return <p>{t("common.loading")}</p>;
  if (!detail.data) return null;
  return <TimeBlocksGridInner scheme={detail.data} schemeId={schemeId} />;
}

function TimeBlocksGridInner({ scheme, schemeId }: ...) {
  const blocks = scheme.time_blocks;
  const days = [...new Set(blocks.map(b => b.day_of_week))].sort();
  const maxPosition = Math.max(0, ...blocks.map(b => b.position));
  const rows = Array.from({ length: maxPosition + 1 }, (_, i) => i + 1);
  // Tailwind grid: columns = days.length, rows = positions + 1 header.
  // Each cell either renders a TimeBlockChip or an EmptyCell with "+" affordance.
  // Bottom row: "Add row" button. Left rail: per-row remove (X).
}
```

Visual idiom mirrors `schedule-page-class-view.tsx`. Header row shows day names. Left rail shows period numbers. Cells are clickable.

- [ ] **Step 11.3: Add i18n keys**

`weekSchemes.grid.title`, `weekSchemes.grid.addRow`, `weekSchemes.grid.removeRow`, `weekSchemes.grid.empty`, `weekSchemes.grid.addBlock`, `weekSchemes.grid.editBlock`, `weekSchemes.grid.deleteBlock`, plus error keys for FK conflicts. Both `en.json` and `de.json`.

- [ ] **Step 11.4: Wire into `week-schemes-page.tsx`**

Replace the `<TimeBlocksTable />` import and usage with `<TimeBlocksGrid />`.

- [ ] **Step 11.5: Delete the old table**

```bash
git rm frontend/src/features/week-schemes/time-blocks-table.tsx frontend/src/features/week-schemes/time-blocks-table.test.tsx
```

- [ ] **Step 11.6: Add Vitest tests**

Cover (a) bulk add: click "Add row" → expects `POST` to all five day-positions, (b) bulk remove: click row "X" → expects `DELETE` for all five, (c) per-cell add: click empty cell → opens inline form → submit creates one block, (d) per-cell delete: click X on chip → fires one `DELETE`. Use MSW handlers from existing setup.

- [ ] **Step 11.7: Run frontend tests + typecheck**

```bash
mise run fe:test
cd frontend && mise exec -- pnpm exec tsc --noEmit
```

- [ ] **Step 11.8: Manual browser verification**

Start dev server, navigate to /week-schemes, edit the Grundschule scheme, verify add row / remove row / per-cell ops behave as expected.

- [ ] **Step 11.9: Commit**

```bash
git add frontend
git commit -m "feat(frontend): wochenschema grid editor"
```

---

### Task 12: Bench refresh + OPEN_THINGS update

**Files:**
- Modify: `solver/solver-core/benches/BASELINE.md`
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 12.1: Run bench, compare**

```bash
mise run bench
```

If within the 20% budget, refresh the baseline:

```bash
mise run bench:record
```

If outside the budget, surface in the PR body and discuss before continuing.

- [ ] **Step 12.2: Update OPEN_THINGS**

Add resolved item: "Grundschule schedule quality (room hopping, day length, balance) constrained and CI-asserted."

Add deferred items:

- Per-class `regular_max_period` / `absolute_max_period` overrides (today the Wochenschema is the only ceiling).
- Multi-Wochenschema schools (one per Bildungsgang).
- Admin UI for tuning solver weights per school.
- Frontend surfacing of `QualityIssue`s ("your schedule has 3 home-room misses").

- [ ] **Step 12.3: Commit**

```bash
git add solver/solver-core/benches/BASELINE.md docs/superpowers/OPEN_THINGS.md
git commit -m "docs: refresh solver baseline and update OPEN_THINGS"
```

---

## Self-Review

- **Spec coverage.** Each spec section maps to a task: §3.1 → Tasks 1-5; §3.2 → Tasks 6-7, 9-10; §3.3 → Task 8; §3.4 → Task 11; §4 testing → Tasks 9, 10, 11.6, plus property tests inside Tasks 2-4. Bench refresh in Task 12.
- **Placeholder scan.** No "TBD" / "implement later". Code blocks ship complete signatures. Cascade lists for `Problem` field additions are explicit (Task 1.2).
- **Type consistency.** `QualityIssue.kind` literals match across module + tests. `ConstraintWeights` field names match across Rust + Pydantic + frontend OpenAPI types (auto-regen in Task 7.4). `RoomMismatchSameSubjectSameDay` literal is consistent.
- **Risks captured.** Bench budget (Task 12.1), pinned-exact weight assertions (Task 5.1), frontend diff overflow (Task 11), migration in CI (Task 6).

## Execution Handoff

Subagent-driven execution. Each task above runs in its own fresh `general-purpose` subagent per `superpowers:subagent-driven-development`. Tasks 1-5 share `solver-core` source files and run sequentially. Tasks 6-8 share backend schemas and run sequentially after the binding rebuild. Task 9 runs after Task 8. Task 10 runs after Task 9 and depends on Tasks 4-8. Task 11 (frontend) is independent of solver/backend and can run in parallel with Tasks 1-10 once the OpenAPI types regenerate (it can wait for Task 7's `fe:types` step). Task 12 closes the loop.
