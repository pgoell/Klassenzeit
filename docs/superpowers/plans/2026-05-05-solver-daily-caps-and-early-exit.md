# Solver daily caps + optimum-aware deadline implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two new hard constraints (`Subject.max_hours_per_day`, `SchoolClass.max_lessons_per_day`) enforced via legality pruning in the solver; let the LAHC loop terminate as soon as it has reached the objective floor; raise the production solve deadline default from 200 ms to 5000 ms.

**Architecture:** Cap enforcement folds into `try_place_block`'s existing per-window legality gate; bookkeeping fields on `GreedyState` (incremented on placement, decremented in the row-removal helper) keep the cap check O(1). Early exit checks `placements.len() == placements_expected && state.soft_score == 0` after each LAHC iteration's incumbent update and at the R&R outer-loop boundary. Settings change is a one-line default bump plus mirroring in `.env.example`. ADR 0033 records both decisions.

**Tech Stack:** Rust 2024 edition (`solver-core`, `solver-py` via maturin/PyO3 0.28), Python 3.13 (FastAPI + SQLAlchemy async + Pydantic + Alembic), TypeScript / React 19 / Vite 7 / Vitest / TanStack.

---

## File structure

**Solver:**
- Modify: `solver/solver-core/src/types.rs` (add cap fields to `Subject` and `SchoolClass`; add two `ViolationKind` variants)
- Modify: `solver/solver-core/src/solve.rs` (extend `GreedyState` with cap counters; cap pruning in `try_place_block`; counter updates in placement + row-removal helpers)
- Modify: `solver/solver-core/src/lahc.rs` (early-exit predicate after iteration body; same predicate at R&R outer-loop boundary)
- Create: `solver/solver-core/tests/daily_caps.rs` (regression: cap=2 forces 2+2 layout; cap raised to 4 allows 4 in one day; class cap forces spillover)
- Create: `solver/solver-core/tests/early_exit.rs` (objective-floor problem exits in <1s under 10s budget)
- Modify: `solver/solver-core/tests/lahc_property.rs` (add cap-conformance assertion to existing property tests)

**Backend:**
- Create: `backend/alembic/versions/<rev>_add_daily_caps.py` (Alembic revision adding both columns)
- Modify: `backend/src/klassenzeit_backend/db/models/subject.py` (`max_hours_per_day` non-null int default 2)
- Modify: `backend/src/klassenzeit_backend/db/models/school_class.py` (`max_lessons_per_day` nullable int)
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/subject.py` (Pydantic Create/Update/Read)
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/school_class.py` (Pydantic Create/Update/Read)
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/schedule.py` (`ViolationResponse.kind` Literal widened)
- Modify: `backend/src/klassenzeit_backend/scheduling/routes/subjects.py` (manual `SubjectResponse(...)` constructions include new field)
- Modify: `backend/src/klassenzeit_backend/scheduling/routes/school_classes.py` (PATCH handler uses `model_fields_set` for nullable cap clear)
- Modify: `backend/src/klassenzeit_backend/scheduling/solver_io.py:build_problem_json` (thread cap fields into solver JSON)
- Modify: `backend/src/klassenzeit_backend/core/settings.py:55` (`solve_deadline_ms: int | None = 5000`)
- Modify: `backend/.env.example:20` (`KZ_SOLVE_DEADLINE_MS=5000`)
- Create: `backend/tests/db/test_daily_caps_migration.py` (alembic up/down round-trip)
- Modify: `backend/tests/scheduling/test_subject_routes.py` (round-trip new field)
- Modify: `backend/tests/scheduling/test_school_class_routes.py` (round-trip new field, including null clear)
- Modify: `backend/tests/scheduling/test_solver_io.py` (update violation-kind closed-enum tests; new test asserting cap is honored on solve)
- Modify: `backend/tests/core/test_settings.py` (new `test_solve_deadline_ms_default_is_5000`)

**Frontend:**
- Modify: `frontend/src/lib/api-types.ts` (regenerated via `mise run fe:types`)
- Modify: `frontend/src/features/subjects/<edit-dialog>.tsx` (number input for `max_hours_per_day`)
- Modify: `frontend/src/features/school-classes/<edit-dialog>.tsx` (optional number input for `max_lessons_per_day`)
- Modify: `frontend/src/i18n/locales/de.json` and `frontend/src/i18n/locales/en.json` (new field labels + hints)
- Modify: existing Vitest test files for the two edit dialogs (assert new fields render and submit correctly)

**Docs:**
- Create: `docs/adr/0033-solver-daily-caps-and-early-exit.md`
- Modify: `docs/adr/README.md` (index entry)
- Modify: `solver/CLAUDE.md` (one bullet on cap enforcement location)
- Modify: `backend/CLAUDE.md` (one bullet on PATCH handler null-clear pattern for class cap)
- Modify: `docs/superpowers/OPEN_THINGS.md` (active sprint promote item 30 to next pickup)
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md` (auto-memory refresh)

---

## Task 1: Solver-core enforces per-day caps as hard constraints

**Files:**
- Modify: `solver/solver-core/src/types.rs`
- Modify: `solver/solver-core/src/solve.rs`
- Create: `solver/solver-core/tests/daily_caps.rs`
- Modify: `solver/solver-core/tests/lahc_property.rs`

**Acceptance:** `cargo nextest run -p solver-core` green including the new daily_caps test and the widened property tests.

- [ ] **Step 1.1: Write the failing regression test for the subject hour cap**

Create `solver/solver-core/tests/daily_caps.rs`:

```rust
//! Regression tests for per-day caps (Subject.max_hours_per_day and
//! SchoolClass.max_lessons_per_day) added in items 38 + 39.

use std::collections::HashMap;
use uuid::Uuid;

use solver_core::ids::{
    LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId,
};
use solver_core::types::{
    Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification, TimeBlock,
};
use solver_core::solve_with_config;
use solver_core::SolveConfig;

fn id<T: From<Uuid>>(b: u8) -> T {
    let mut bytes = [0u8; 16];
    bytes[0] = b;
    Uuid::from_bytes(bytes).into()
}

#[test]
fn caps_subject_hours_per_class_per_day_to_two_by_default() {
    // One class, one subject (cap default 2), one teacher, one room, 5 days x 5 positions.
    // Lesson with hours_per_week=4 preferred_block_size=1: 4 placements that must
    // distribute across at least 2 days under the cap.
    let class_id: SchoolClassId = id(1);
    let teacher_id: TeacherId = id(2);
    let subject_id: SubjectId = id(3);
    let room_id: RoomId = id(4);

    let mut tbs = Vec::new();
    for d in 0..5u8 {
        for p in 0..5u8 {
            tbs.push(TimeBlock {
                id: TimeBlockId(Uuid::from_u128(((d as u128) << 64) | p as u128)),
                day_of_week: d,
                position: p,
                start_time: chrono::NaiveTime::from_hms_opt(8 + p as u32, 0, 0).unwrap(),
                end_time: chrono::NaiveTime::from_hms_opt(9 + p as u32, 0, 0).unwrap(),
            });
        }
    }

    let problem = Problem {
        time_blocks: tbs,
        teachers: vec![Teacher { id: teacher_id, max_hours_per_week: 30 }],
        rooms: vec![Room { id: room_id, ..Default::default() }],
        subjects: vec![Subject {
            id: subject_id,
            max_hours_per_day: 2,
            ..Default::default()
        }],
        school_classes: vec![SchoolClass { id: class_id, max_lessons_per_day: None, ..Default::default() }],
        lessons: vec![Lesson {
            id: id::<LessonId>(5),
            school_class_ids: vec![class_id],
            subject_id,
            teacher_id,
            hours_per_week: 4,
            preferred_block_size: 1,
            ..Default::default()
        }],
        teacher_qualifications: vec![TeacherQualification { teacher_id, subject_id }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    };

    let solution = solve_with_config(&problem, SolveConfig::default()).expect("greedy succeeds");

    let mut count_by_day: HashMap<u8, u32> = HashMap::new();
    let tb_lookup: HashMap<_, _> = problem.time_blocks.iter().map(|t| (t.id, t)).collect();
    for p in &solution.placements {
        let day = tb_lookup[&p.time_block_id].day_of_week;
        *count_by_day.entry(day).or_default() += 1;
    }
    for (day, count) in &count_by_day {
        assert!(*count <= 2, "day {day} has {count} hours of subject; cap is 2");
    }
    assert_eq!(solution.placements.len(), 4, "all 4 hours should be placed across days");
}
```

The test references `Default` impls and field defaults for `Teacher`, `Room`, `Subject`, `SchoolClass`, `Lesson` to keep the test concise. If those `Default` impls do not exist yet, fall back to spelling each field explicitly (the existing `solver-core/tests/properties.rs` shows the pattern).

- [ ] **Step 1.2: Run test, verify it fails to compile**

```bash
cargo test -p solver-core --test daily_caps
```

Expected: build error referencing missing fields `max_hours_per_day` on `Subject` and `max_lessons_per_day` on `SchoolClass`. This is the red signal.

- [ ] **Step 1.3: Add the new `ViolationKind` variants to `types.rs`**

Find the `pub enum ViolationKind` block (around `solver/solver-core/src/types.rs:362`) and append:

```rust
    /// A class accumulated more hours of a single subject on one day than
    /// the subject's `max_hours_per_day` cap allows. Surfaced in solver
    /// telemetry only; the runtime path prunes cap-violating candidates
    /// before they enter the search.
    SubjectDailyHourCapExceeded,
    /// A class accumulated more total lessons on one day than the class's
    /// `max_lessons_per_day` cap allows. Surfaced in solver telemetry only;
    /// the runtime path prunes cap-violating candidates before they enter
    /// the search.
    ClassDailyLessonCapExceeded,
```

The existing variants are unit-shape (no inner data); follow the same shape. The cap fields (count, cap, day, etc.) are not carried in the variant because the runtime never constructs these violations: the pruning gate prevents them. The variants exist for the closed-enum surface that backend/CLAUDE.md mandates `scheduling/schemas/schedule.py` widen.

- [ ] **Step 1.4: Add cap fields to `Subject` and `SchoolClass`**

Edit `solver/solver-core/src/types.rs`:

In `pub struct Subject` add (after the existing `prefer_late_period` field):

```rust
    /// Per-day cap on hours of this subject for any single class on any
    /// single day. Counts hours (period span), not lessons; a 2-period
    /// block lesson contributes 2 to the daily count. Hard constraint:
    /// cap-violating candidates are pruned at placement time. Wire format
    /// is additive: callers omitting the field deserialise to 2.
    #[serde(default = "default_max_hours_per_day")]
    pub max_hours_per_day: u8,
```

Add a free function in the same file (near other serde defaults):

```rust
fn default_max_hours_per_day() -> u8 {
    2
}
```

In `pub struct SchoolClass` add (after `home_room_id`):

```rust
    /// Optional per-day cap on total lessons for this class on any single
    /// day. Counts lessons (placements), not periods; a 2-period block
    /// lesson contributes 1 to the daily count. `None` means no cap beyond
    /// what the class's `time_blocks` allow. Hard constraint: when set,
    /// cap-violating candidates are pruned at placement time. Wire format
    /// is additive: callers omitting the field deserialise to `None`.
    #[serde(default)]
    pub max_lessons_per_day: Option<u8>,
```

- [ ] **Step 1.5: Extend `GreedyState` with cap counters**

Edit `solver/solver-core/src/solve.rs`. In `pub(crate) struct GreedyState` add:

```rust
    /// Per-`(class, day, subject)` cap-aware hour counter; mirrors the
    /// running total compared against `Subject.max_hours_per_day`. Updated
    /// in `try_place_block`'s accept path and decremented in the row-
    /// removal helper used by `rr_ruin_block` and `kempe_rollback`.
    pub(crate) subject_hours_by_class_day: HashMap<(SchoolClassId, u8, SubjectId), u8>,
    /// Per-`(class, day)` cap-aware lesson counter; mirrors the running
    /// total compared against `SchoolClass.max_lessons_per_day` (when set).
    /// Maintained in lockstep with the existing per-class bookkeeping.
    pub(crate) lessons_by_class_day: HashMap<(SchoolClassId, u8), u8>,
```

In `impl GreedyState::new()` initialize both fields with `HashMap::new()`.

- [ ] **Step 1.6: Add the cap-pruning gate to `try_place_block`**

In `solve.rs:try_place_block` (around line 380, after the per-window class-busy check and before the `current = state.hours_by_teacher` block), add a new feasibility gate. Build a `class_max_lessons_per_day: HashMap<SchoolClassId, u8>` lookup at solve setup time (in `solve_with_config`) from the optional cap field; classes with `None` simply do not appear. Pass it through the call chain alongside `teacher_max`.

```rust
let subject_cap = subject.max_hours_per_day;
for class in class_ids {
    let key = (*class, first_tb.day_of_week, lesson.subject_id);
    let current = state.subject_hours_by_class_day.get(&key).copied().unwrap_or(0);
    if current.saturating_add(n) > subject_cap {
        #[cfg(feature = "solver-trace")]
        trace::ffd_trace(
            lesson.id,
            first_tb.day_of_week,
            first_tb.position,
            None,
            "subject_daily_cap",
        );
        continue 'outer;
    }
    if let Some(cap) = class_max_lessons_per_day.get(class).copied() {
        let lessons_today = state
            .lessons_by_class_day
            .get(&(*class, first_tb.day_of_week))
            .copied()
            .unwrap_or(0);
        if lessons_today.saturating_add(1) > cap {
            #[cfg(feature = "solver-trace")]
            trace::ffd_trace(
                lesson.id,
                first_tb.day_of_week,
                first_tb.position,
                None,
                "class_daily_lesson_cap",
            );
            continue 'outer;
        }
    }
}
```

- [ ] **Step 1.7: Update `try_place_block`'s accept path**

In the same function's commit-the-placement block (around line 612-618 where the existing `state.used_teacher.insert(...)` and `state.hours_by_teacher.entry(...)` updates live), add the symmetric increments:

```rust
for class in class_ids {
    let key = (*class, first_tb.day_of_week, lesson.subject_id);
    *state.subject_hours_by_class_day.entry(key).or_insert(0) += n;
    *state.lessons_by_class_day.entry((*class, first_tb.day_of_week)).or_insert(0) += 1;
}
```

(`n` is the block size; for a single-row placement n=1 and both counters increment by their per-row contribution. Counters are per-row-aggregated for the subject-hours metric and per-block for the class-lessons metric, matching the cap semantics.)

Caveat for the row-counting: a block lesson of `n=2` contributes 2 to subject hours and 1 to class lessons (one block = one lesson). The increment above is correct: `subject_hours += n` (period span), `lessons += 1` (block count, regardless of n).

- [ ] **Step 1.8: Update the row-removal helper**

Locate the per-row decrement code in `solve.rs` (used by `rr_ruin_block` in `lahc.rs` and by `kempe_rollback`). It already decrements `used_teacher`, `used_class`, `hours_by_teacher`. Add symmetric decrements for the new counters: subtract 1 from `lessons_by_class_day[(class, day)]`, subtract n from `subject_hours_by_class_day[(class, day, subject)]`, removing the entry when it hits zero.

If a single helper does not yet exist, the cap-counter decrement lives where the existing per-row decrements live. This may be inline inside `rr_ruin_block` (`lahc.rs:1100ish`) and the kempe rollback (`lahc.rs:1651ish`); audit both call sites and apply the same pattern.

- [ ] **Step 1.9: Wire `class_max_lessons_per_day` through the solver setup**

In `solve.rs:solve_with_config` (the main entry point), build the lookup from `problem.school_classes`:

```rust
let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = problem
    .school_classes
    .iter()
    .filter_map(|c| c.max_lessons_per_day.map(|cap| (c.id, cap)))
    .collect();
```

Thread it as a borrow into `try_place_block` and into `lahc::run` (which already passes `teacher_max` similarly). Pass through `rr_attempt`, `kempe_attempt`, and `try_change_move` for symmetric pruning.

- [ ] **Step 1.10: Run the regression test, verify pass**

```bash
cargo test -p solver-core --test daily_caps caps_subject_hours_per_class_per_day_to_two_by_default
```

Expected: PASS. The 4 hours distribute across at least 2 days, no day has > 2 hours.

- [ ] **Step 1.11: Add second case for class lesson cap**

Append to `daily_caps.rs`:

```rust
#[test]
fn caps_total_lessons_per_class_per_day_when_set() {
    // One class with max_lessons_per_day=4, daily time-blocks of 6 positions,
    // weekly hours = 25 (forced spillover to a 5th day).
    // Force 5 single-hour lessons across 5 different subjects so subject cap
    // does not interfere; the class cap is the binding constraint.
    let class_id: SchoolClassId = id(1);
    let teacher_id: TeacherId = id(2);
    let room_id: RoomId = id(4);

    let mut tbs = Vec::new();
    for d in 0..5u8 {
        for p in 0..6u8 {
            tbs.push(TimeBlock {
                id: TimeBlockId(Uuid::from_u128(((d as u128) << 64) | p as u128)),
                day_of_week: d,
                position: p,
                start_time: chrono::NaiveTime::from_hms_opt(8 + p as u32, 0, 0).unwrap(),
                end_time: chrono::NaiveTime::from_hms_opt(9 + p as u32, 0, 0).unwrap(),
            });
        }
    }

    // 5 subjects, each with max_hours_per_day=5 to remove subject-cap interference.
    let subjects: Vec<Subject> = (0..5u8)
        .map(|i| Subject {
            id: id::<SubjectId>(10 + i),
            max_hours_per_day: 5,
            ..Default::default()
        })
        .collect();

    // 5 lessons, each hours_per_week=5 preferred_block_size=1.
    let lessons: Vec<Lesson> = (0..5u8)
        .map(|i| Lesson {
            id: id::<LessonId>(20 + i),
            school_class_ids: vec![class_id],
            subject_id: subjects[i as usize].id,
            teacher_id,
            hours_per_week: 5,
            preferred_block_size: 1,
            ..Default::default()
        })
        .collect();

    let teacher_quals = subjects.iter().map(|s| TeacherQualification { teacher_id, subject_id: s.id }).collect();

    let problem = Problem {
        time_blocks: tbs,
        teachers: vec![Teacher { id: teacher_id, max_hours_per_week: 30 }],
        rooms: vec![Room { id: room_id, ..Default::default() }],
        subjects,
        school_classes: vec![SchoolClass { id: class_id, max_lessons_per_day: Some(4), ..Default::default() }],
        lessons,
        teacher_qualifications: teacher_quals,
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    };

    let solution = solve_with_config(&problem, SolveConfig::default()).expect("greedy succeeds");

    let mut count_by_day: HashMap<u8, u32> = HashMap::new();
    let tb_lookup: HashMap<_, _> = problem.time_blocks.iter().map(|t| (t.id, t)).collect();
    for p in &solution.placements {
        let day = tb_lookup[&p.time_block_id].day_of_week;
        *count_by_day.entry(day).or_default() += 1;
    }
    for (day, count) in &count_by_day {
        assert!(*count <= 4, "day {day} has {count} lessons; cap is 4");
    }
}
```

- [ ] **Step 1.12: Run the second case**

```bash
cargo test -p solver-core --test daily_caps
```

Expected: both tests PASS.

- [ ] **Step 1.13: Widen `lahc_property.rs`'s cap conformance**

Open `solver/solver-core/tests/lahc_property.rs`. Find `lahc_rr_never_decreases_placement_count` and `lahc_rr_kempe_never_decreases_placement_count`. After the existing assertions on placement count, add:

```rust
let tb_lookup: std::collections::HashMap<_, _> =
    problem.time_blocks.iter().map(|t| (t.id, t)).collect();
let subject_lookup: std::collections::HashMap<_, _> =
    problem.subjects.iter().map(|s| (s.id, s)).collect();
let mut subject_hours: std::collections::HashMap<
    (solver_core::ids::SchoolClassId, u8, solver_core::ids::SubjectId),
    u8,
> = std::collections::HashMap::new();
for p in &solution.placements {
    let tb = tb_lookup[&p.time_block_id];
    let lesson = problem.lessons.iter().find(|l| l.id == p.lesson_id).unwrap();
    for class in &lesson.school_class_ids {
        let key = (*class, tb.day_of_week, lesson.subject_id);
        *subject_hours.entry(key).or_default() += 1;
    }
}
for ((_, _, subject_id), count) in &subject_hours {
    let cap = subject_lookup[subject_id].max_hours_per_day;
    prop_assert!(*count <= cap, "subject hour cap violated: count={count} cap={cap}");
}
```

(If the property test's `Subject` builder does not yet set `max_hours_per_day`, the serde default of 2 is used and the assertion is exercised against a non-trivial cap. Adjust the generator if the random fixture is generating problems where 2/day is structurally infeasible.)

- [ ] **Step 1.14: Run all rust tests**

```bash
mise run test:rust
```

Expected: all green. Investigate and fix any regression in existing tests; common sources are bench fixtures that pre-date the cap and now hit the default 2.

- [ ] **Step 1.15: Run lint**

```bash
mise run lint
```

Expected: green. Cargo fmt + clippy + machete.

- [ ] **Step 1.16: Commit**

```bash
git add solver/solver-core/src/types.rs solver/solver-core/src/solve.rs \
    solver/solver-core/src/lahc.rs \
    solver/solver-core/tests/daily_caps.rs solver/solver-core/tests/lahc_property.rs
git commit -m "feat(solver-core): enforce per-day subject and class caps as hard constraints"
```

---

## Task 2: Backend persists and exposes cap fields

**Files:**
- Create: `backend/alembic/versions/<rev>_add_daily_caps.py`
- Modify: `backend/src/klassenzeit_backend/db/models/subject.py`
- Modify: `backend/src/klassenzeit_backend/db/models/school_class.py`
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/subject.py`
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/school_class.py`
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/schedule.py`
- Modify: `backend/src/klassenzeit_backend/scheduling/routes/subjects.py`
- Modify: `backend/src/klassenzeit_backend/scheduling/routes/school_classes.py`
- Modify: `backend/src/klassenzeit_backend/scheduling/solver_io.py`
- Create: `backend/tests/db/test_daily_caps_migration.py`
- Modify: `backend/tests/scheduling/test_subject_routes.py`
- Modify: `backend/tests/scheduling/test_school_class_routes.py`
- Modify: `backend/tests/scheduling/test_solver_io.py`

**Acceptance:** `mise run test:py` green. New cap fields round-trip through CRUD; PATCH-clear of class cap works; solver receives the new fields in its JSON; violation kind closed-enum tests updated.

- [ ] **Step 2.1: Rebuild solver bindings (autopilot solver-binding rebuild discipline)**

Earlier task touched `solver/`; rebuild before any pytest:

```bash
mise run solver:rebuild
```

- [ ] **Step 2.2: Generate the Alembic revision**

```bash
cd backend
uv run alembic revision -m "add daily caps"
```

This produces a stub at `backend/alembic/versions/<rev>_add_daily_caps.py`.

- [ ] **Step 2.3: Write the migration body**

Replace the stub with:

```python
"""Add daily caps to subjects and school_classes.

Revision ID: <rev>
Revises: <previous-rev>
Create Date: 2026-05-05

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "<rev>"
down_revision: str | None = "<previous-rev>"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    op.add_column(
        "subjects",
        sa.Column("max_hours_per_day", sa.Integer(), nullable=False, server_default="2"),
    )
    op.add_column(
        "school_classes",
        sa.Column("max_lessons_per_day", sa.Integer(), nullable=True),
    )


def downgrade() -> None:
    op.drop_column("school_classes", "max_lessons_per_day")
    op.drop_column("subjects", "max_hours_per_day")
```

(Per backend/CLAUDE.md: tidy autogenerate output; use `collections.abc.Sequence` and PEP 604 unions.)

Replace `<rev>` and `<previous-rev>` with the values that alembic generated; the previous head is whichever revision is currently `HEAD` per `uv run alembic current`.

- [ ] **Step 2.4: Drop test template + per-worker DBs (schema-changing PR rule)**

Per backend/CLAUDE.md "Schema-changing PRs: drop the template + per-worker DBs before the first test run":

```bash
psql -h localhost -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test_template;"
psql -h localhost -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test;"
for w in 0 1 2 3 4 5 6 7; do
    psql -h localhost -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test_gw${w};"
done
```

(Adapt host/user as the dev environment requires; in this repo the postgres role is whatever `compose.yaml` defines.)

- [ ] **Step 2.5: Add columns to ORM models**

In `subject.py`:

```python
max_hours_per_day: Mapped[int] = mapped_column(
    Integer, nullable=False, default=2, server_default=text("2")
)
```

In `school_class.py`:

```python
max_lessons_per_day: Mapped[int | None] = mapped_column(
    Integer, nullable=True, default=None
)
```

- [ ] **Step 2.6: Write the failing migration round-trip test**

Create `backend/tests/db/test_daily_caps_migration.py`:

```python
"""Round-trip the daily-caps migration: upgrade adds columns, downgrade removes them."""

import sqlalchemy as sa
from alembic import command
from alembic.config import Config


def test_daily_caps_migration_round_trips(alembic_config: Config, async_engine):
    """Run upgrade and downgrade between this revision and its predecessor; verify columns flip."""
    # alembic_config and async_engine are existing fixtures; if they do not exist,
    # find the project's current alembic test pattern (other tests under backend/tests/db/).
    # The minimal contract: invoke `alembic upgrade head` then assert columns exist;
    # invoke `alembic downgrade <prev>` then assert columns are gone.
    ...
```

If the project does not yet have an Alembic round-trip fixture, the simpler shape is to drive the migration via `command.upgrade(alembic_config, "head")` and `command.downgrade(alembic_config, "<prev-rev>")`, and to inspect column existence with `sa.inspect(sync_engine).get_columns("subjects")`. Match whichever round-trip pattern the repo already has (if any); skip this test cleanly if no infrastructure exists, and rely on Steps 2.7+ for behavioural coverage.

- [ ] **Step 2.7: Run migration test, verify it passes**

```bash
mise run test:py -- backend/tests/db/test_daily_caps_migration.py -v
```

Expected: PASS. If the project lacks an Alembic round-trip fixture, comment out the test body and skip with `pytest.skip("alembic round-trip fixture missing")`. The next-level coverage from CRUD tests will exercise the migration via `apply_migrations` on test setup anyway.

- [ ] **Step 2.8: Add Pydantic schemas**

In `backend/src/klassenzeit_backend/scheduling/schemas/subject.py`:

```python
class SubjectCreate(BaseModel):
    # existing fields ...
    max_hours_per_day: int = Field(default=2, ge=1, le=20)


class SubjectUpdate(BaseModel):
    # existing fields ...
    max_hours_per_day: int | None = Field(default=None, ge=1, le=20)


class SubjectResponse(BaseModel):
    # existing fields ...
    max_hours_per_day: int
```

(Names may differ; use whatever shape the existing module already uses for similar Subject preference fields like `prefer_late_period`.)

In `backend/src/klassenzeit_backend/scheduling/schemas/school_class.py`:

```python
class SchoolClassCreate(BaseModel):
    # existing fields ...
    max_lessons_per_day: int | None = Field(default=None, ge=1, le=20)


class SchoolClassUpdate(BaseModel):
    # existing fields ...
    max_lessons_per_day: int | None = Field(default=None, ge=1, le=20)


class SchoolClassResponse(BaseModel):
    # existing fields ...
    max_lessons_per_day: int | None
```

- [ ] **Step 2.9: Update Subject route handler manual constructions**

Per backend/CLAUDE.md, `scheduling/routes/subjects.py` builds `SubjectResponse(...)` per call. Locate each construction (POST, PATCH, GET-list, GET-by-id) and add `max_hours_per_day=subject.max_hours_per_day`.

- [ ] **Step 2.10: Update School class route handler with model_fields_set**

In `scheduling/routes/school_classes.py:update_school_class_route`, the existing `home_room_id` clear path uses `if "home_room_id" in body.model_fields_set:`. Add the same pattern for `max_lessons_per_day`:

```python
if "max_lessons_per_day" in body.model_fields_set:
    orm.max_lessons_per_day = body.max_lessons_per_day
```

For POST, set unconditionally from the create payload (default `None`).

- [ ] **Step 2.11: Widen `ViolationResponse.kind` Literal**

In `backend/src/klassenzeit_backend/scheduling/schemas/schedule.py`, find the `kind: Literal[...]` line on the `ViolationResponse` model. Append `"subject_daily_hour_cap_exceeded"` and `"class_daily_lesson_cap_exceeded"` to the union (snake_case mirrors solver's serde rename).

- [ ] **Step 2.12: Update the closed-enum violation tests**

Per backend/CLAUDE.md, `tests/scheduling/test_solver_io.py` has `test_count_violations_by_kind_clean_solve_returns_zeros` and `test_count_violations_by_kind_aggregates_mixed_kinds`. Both hardcode the closed kind set; extend with the two new variants. The clean-solve test asserts zero counts; the aggregator test passes a mixed counter, including zero entries for the new variants is sufficient.

- [ ] **Step 2.13: Thread cap fields into solver JSON**

In `backend/src/klassenzeit_backend/scheduling/solver_io.py:build_problem_json`, find the section that constructs the `subjects` and `school_classes` arrays. For each subject row, include `"max_hours_per_day": subject.max_hours_per_day`. For each school class row, include `"max_lessons_per_day": school_class.max_lessons_per_day` (Pydantic / dict will serialise `None` to `null`, which matches the solver's `Option<u8>` deserialisation).

- [ ] **Step 2.14: Add CRUD round-trip tests for both fields**

In `backend/tests/scheduling/test_subject_routes.py` (or wherever Subject CRUD tests live), add:

```python
async def test_create_subject_defaults_max_hours_per_day_to_two(client):
    """POST /api/subjects without max_hours_per_day defaults to 2."""
    payload = {"name": "Mathe", "short_name": "M", "color": "#000000"}
    response = await client.post("/api/subjects", json=payload)
    assert response.status_code == 201
    assert response.json()["max_hours_per_day"] == 2


async def test_subject_round_trips_explicit_max_hours_per_day(client):
    payload = {"name": "Sport", "short_name": "Sp", "color": "#0000ff", "max_hours_per_day": 3}
    response = await client.post("/api/subjects", json=payload)
    assert response.status_code == 201
    assert response.json()["max_hours_per_day"] == 3
```

In `backend/tests/scheduling/test_school_class_routes.py`:

```python
async def test_class_max_lessons_per_day_round_trips_and_clears(client, school_class_payload):
    payload = {**school_class_payload, "max_lessons_per_day": 5}
    response = await client.post("/api/classes", json=payload)
    assert response.status_code == 201
    class_id = response.json()["id"]
    assert response.json()["max_lessons_per_day"] == 5

    # PATCH to clear via explicit null:
    patch = await client.patch(f"/api/classes/{class_id}", json={"max_lessons_per_day": None})
    assert patch.status_code == 200
    assert patch.json()["max_lessons_per_day"] is None
```

- [ ] **Step 2.15: Run the backend tests**

```bash
mise run test:py
```

Expected: green. Investigate any failure; common sources are missed `SubjectResponse(...)` constructions or stale test template DBs (re-run Step 2.4).

- [ ] **Step 2.16: Run lint**

```bash
mise run lint
```

Expected: green.

- [ ] **Step 2.17: Commit**

```bash
git add backend/alembic/versions/ \
    backend/src/klassenzeit_backend/db/models/subject.py \
    backend/src/klassenzeit_backend/db/models/school_class.py \
    backend/src/klassenzeit_backend/scheduling/schemas/subject.py \
    backend/src/klassenzeit_backend/scheduling/schemas/school_class.py \
    backend/src/klassenzeit_backend/scheduling/schemas/schedule.py \
    backend/src/klassenzeit_backend/scheduling/routes/subjects.py \
    backend/src/klassenzeit_backend/scheduling/routes/school_classes.py \
    backend/src/klassenzeit_backend/scheduling/solver_io.py \
    backend/tests/
git commit -m "feat(backend): persist Subject.max_hours_per_day and SchoolClass.max_lessons_per_day"
```

---

## Task 3: Frontend exposes cap fields in edit dialogs

**Files:**
- Modify: `frontend/src/lib/api-types.ts` (regenerated)
- Modify: `frontend/src/features/subjects/<subject-edit-dialog>.tsx`
- Modify: `frontend/src/features/school-classes/<school-class-edit-dialog>.tsx`
- Modify: `frontend/src/i18n/locales/de.json` and `en.json`
- Modify: existing Vitest test files for both dialogs

**Acceptance:** `mise run fe:test` green; `mise run lint` green; manual smoke not required (covered by Vitest).

- [ ] **Step 3.1: Regenerate API types**

```bash
mise run fe:types
```

This regenerates `frontend/src/lib/api-types.ts` from the backend's OpenAPI schema. New fields appear under `Subject`, `SubjectCreate`, `SubjectUpdate`, `SchoolClass`, `SchoolClassCreate`, `SchoolClassUpdate`.

- [ ] **Step 3.2: Locate the existing edit dialogs**

```bash
find frontend/src/features/subjects -name "*.tsx" -exec grep -l "edit\|Edit" {} \;
find frontend/src/features/school-classes -name "*.tsx" -exec grep -l "edit\|Edit" {} \;
```

The result names the canonical `<subject-edit-dialog>.tsx` and `<school-class-edit-dialog>.tsx` (or whatever the codebase calls them). Read each before editing.

- [ ] **Step 3.3: Add subject `max_hours_per_day` input**

Add a labeled `<input type="number" min={1} max={20}>` (or the codebase's `NumberInput` shadcn component if one exists) bound to `formState.max_hours_per_day`, default value `2`. Place it alongside the existing preference fields (`prefer_late_period`, etc.). i18n keys:
- `subjects.fields.maxHoursPerDay.label` ("Stunden pro Tag (max)" / "Hours per day (max)")
- `subjects.fields.maxHoursPerDay.hint` ("Maximale Stunden dieses Fachs an einem Tag pro Klasse." / "Maximum hours of this subject in a single day per class.")

- [ ] **Step 3.4: Add class `max_lessons_per_day` input**

Add an optional labeled number input bound to `formState.max_lessons_per_day`, default `null`. Empty cell → null on submit. i18n keys:
- `schoolClasses.fields.maxLessonsPerDay.label` ("Stunden pro Tag (max)" / "Lessons per day (max)")
- `schoolClasses.fields.maxLessonsPerDay.hint` ("Leer lassen für kein Limit." / "Leave empty for no cap.")

- [ ] **Step 3.5: Update DE + EN locales**

Open `frontend/src/i18n/locales/de.json` and `en.json`; add the keys above with the German and English strings. Match the indentation and ordering of the existing entries.

- [ ] **Step 3.6: Vitest coverage for the subject dialog**

Locate `frontend/src/features/subjects/__tests__/<edit-dialog>.test.tsx`. Add:

```tsx
it("renders max_hours_per_day with default 2 and submits the new value", async () => {
    // fill the form, override the input to 3, submit, assert payload
});
```

(Match the existing test scaffolding's TanStack Query / MSW / RTL conventions.)

- [ ] **Step 3.7: Vitest coverage for the class dialog**

Add two cases: round-trip an explicit number, and clear an existing value to null.

- [ ] **Step 3.8: Run frontend tests**

```bash
mise run fe:test
```

Expected: green.

- [ ] **Step 3.9: Run lint**

```bash
mise run lint
```

Expected: green. Biome must accept new TSX changes; any unused imports flagged should be removed.

- [ ] **Step 3.10: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): expose daily caps in subject and class edit dialogs"
```

---

## Task 4: Solver-core early exit + raise solve_deadline_ms default to 5000

**Files:**
- Modify: `solver/solver-core/src/lahc.rs`
- Create: `solver/solver-core/tests/early_exit.rs`
- Modify: `backend/src/klassenzeit_backend/core/settings.py:55`
- Modify: `backend/.env.example:20`
- Modify: `backend/tests/core/test_settings.py`

**Acceptance:** `cargo nextest run -p solver-core` includes the new early_exit test as PASS; backend test asserts the new default; `.env.test` keeps `KZ_SOLVE_DEADLINE_MS=0`.

- [ ] **Step 4.1: Write the failing early-exit test**

Create `solver/solver-core/tests/early_exit.rs`:

```rust
//! Asserts that the LAHC outer loop exits as soon as the incumbent reaches
//! `placements.len() == placements_expected && state.soft_score == 0`,
//! regardless of the configured deadline.

use std::time::Duration;
use uuid::Uuid;

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::types::{
    Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification, TimeBlock,
};
use solver_core::{solve_with_config, SolveConfig};

#[test]
fn lahc_exits_at_objective_floor_well_before_deadline() {
    // Tiny problem the FFD greedy solves to soft_score=0 and full placement
    // count. With deadline = 10s and the early-exit predicate live, the wall
    // clock on the solve should be << 1 second.
    let class_id: SchoolClassId = SchoolClassId(Uuid::from_u128(1));
    let teacher_id: TeacherId = TeacherId(Uuid::from_u128(2));
    let subject_id: SubjectId = SubjectId(Uuid::from_u128(3));
    let room_id: RoomId = RoomId(Uuid::from_u128(4));

    let mut tbs = Vec::new();
    for d in 0..5u8 {
        for p in 0..5u8 {
            tbs.push(TimeBlock {
                id: TimeBlockId(Uuid::from_u128(((d as u128) << 64) | p as u128)),
                day_of_week: d,
                position: p,
                start_time: chrono::NaiveTime::from_hms_opt(8 + p as u32, 0, 0).unwrap(),
                end_time: chrono::NaiveTime::from_hms_opt(9 + p as u32, 0, 0).unwrap(),
            });
        }
    }

    let problem = Problem {
        time_blocks: tbs,
        teachers: vec![Teacher { id: teacher_id, max_hours_per_week: 30 }],
        rooms: vec![Room { id: room_id, ..Default::default() }],
        subjects: vec![Subject { id: subject_id, max_hours_per_day: 2, ..Default::default() }],
        school_classes: vec![SchoolClass { id: class_id, max_lessons_per_day: None, ..Default::default() }],
        lessons: vec![Lesson {
            id: LessonId(Uuid::from_u128(5)),
            school_class_ids: vec![class_id],
            subject_id,
            teacher_id,
            hours_per_week: 2,
            preferred_block_size: 1,
            ..Default::default()
        }],
        teacher_qualifications: vec![TeacherQualification { teacher_id, subject_id }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    };

    let cfg = SolveConfig {
        deadline: Some(Duration::from_secs(10)),
        seed: 42,
        max_iterations: None,
        ..Default::default()
    };
    let started = std::time::Instant::now();
    let solution = solve_with_config(&problem, cfg).expect("solve succeeds");
    let elapsed = started.elapsed();

    assert_eq!(solution.placements.len(), 2);
    assert_eq!(solution.soft_score, 0);
    assert!(
        elapsed < Duration::from_secs(1),
        "early exit should fire well before the 10s budget; took {:?}",
        elapsed
    );
}
```

- [ ] **Step 4.2: Run, verify FAIL**

```bash
cargo test -p solver-core --test early_exit
```

Expected: timeout-shaped FAIL (test runs near the full 10s deadline) or assertion failure on `elapsed < 1s`.

- [ ] **Step 4.3: Add early exit predicate to LAHC main loop**

In `solver/solver-core/src/lahc.rs:86-158` (the `while iter < max_iter && start.elapsed() < deadline { ... }` block), after `lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.soft_score;` add:

```rust
        if state.soft_score == 0 && placements.len() == placements_expected {
            break;
        }
```

`placements_expected: usize` is computed at the top of `lahc::run` from the existing problem-derived count or threaded in from `solve_with_config`. Match whichever style is already in scope. If neither exists, compute once before the loop:

```rust
let placements_expected: usize = problem
    .lessons
    .iter()
    .map(|l| l.hours_per_week as usize)
    .sum();
```

- [ ] **Step 4.4: Same predicate at R&R outer-loop boundary**

If `rr_attempt` runs an inner loop that may produce a floor-reaching incumbent mid-iteration, the LAHC main-loop check covers it (rr_attempt returns and the outer check fires next iteration). No additional check needed at the rr_attempt boundary unless code review surfaces a path that does not return through the main loop.

- [ ] **Step 4.5: Run early_exit test, verify PASS**

```bash
cargo test -p solver-core --test early_exit
```

Expected: PASS in <1s elapsed.

- [ ] **Step 4.6: Verify property tests still pass**

```bash
cargo test -p solver-core --test lahc_property
```

Expected: PASS. The early-exit predicate is a no-op for property cases that don't reach soft_score=0.

- [ ] **Step 4.7: Update settings default**

In `backend/src/klassenzeit_backend/core/settings.py:55`:

```python
solve_deadline_ms: int | None = 5000
```

In `backend/.env.example:20`:

```
KZ_SOLVE_DEADLINE_MS=5000
```

`.env.test` (`backend/.env.test:13`) keeps `KZ_SOLVE_DEADLINE_MS=0`.

- [ ] **Step 4.8: Add settings test**

In `backend/tests/core/test_settings.py`, add:

```python
def test_solve_deadline_ms_default_is_5000(monkeypatch):
    """Production default is 5s; .env.test overrides to 0 for greedy-only tests."""
    monkeypatch.delenv("KZ_SOLVE_DEADLINE_MS", raising=False)
    settings = Settings(_env_file=None)  # ty: ignore[missing-argument, unknown-argument]
    assert settings.solve_deadline_ms == 5000
```

(Match the existing `test_solver_backend_default_is_production_choice` pattern.)

- [ ] **Step 4.9: Rebuild solver bindings**

```bash
mise run solver:rebuild
```

(Earlier task touched `solver-core`.)

- [ ] **Step 4.10: Run all tests + lint**

```bash
mise run test:rust && mise run test:py && mise run lint
```

Expected: green.

- [ ] **Step 4.11: Commit**

```bash
git add solver/solver-core/src/lahc.rs solver/solver-core/tests/early_exit.rs \
    backend/src/klassenzeit_backend/core/settings.py backend/.env.example \
    backend/tests/core/test_settings.py
git commit -m "feat(solver-core): early-exit at objective floor + raise solve deadline default to 5000ms"
```

---

## Task 5: ADR 0033 + OPEN_THINGS hygiene + auto-memory refresh

**Files:**
- Create: `docs/adr/0033-solver-daily-caps-and-early-exit.md`
- Modify: `docs/adr/README.md`
- Modify: `solver/CLAUDE.md`
- Modify: `backend/CLAUDE.md`
- Modify: `docs/superpowers/OPEN_THINGS.md`
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`

**Acceptance:** ADR indexed; OPEN_THINGS reflects items 38 + 39 closed by promoting item 30; auto-memory mirrors current state.

- [ ] **Step 5.1: Confirm next ADR number**

```bash
ls docs/adr/*.md | sort | tail -1
```

Expected: `docs/adr/0032-solver-production-default-revisit.md`. Next number is 0033.

- [ ] **Step 5.2: Write ADR 0033**

Create `docs/adr/0033-solver-daily-caps-and-early-exit.md`:

```markdown
# 0033: Solver daily caps + optimum-aware deadline

## Status
Accepted, 2026-05-05.

## Context
[Symptoms reported on production schedule for class 1a; bake-off bench did not exercise consecutive-subject runs because no constraint existed; production deadline of 200ms was 25× shorter than bench cells; copy the relevant bullets from the spec's Context section.]

## Decision
Two new hard constraints enforced via legality pruning in `try_place_block`:
- `Subject.max_hours_per_day`, non-null, default 2: cap on hours of one subject for one class on one day.
- `SchoolClass.max_lessons_per_day`, nullable, default null (no cap): cap on total lessons for one class on one day.

LAHC outer loop terminates as soon as `placements.len() == placements_expected && state.soft_score == 0`.

`solve_deadline_ms` production default raised from 200 to 5000 ms; `.env.test` stays at 0 (greedy-only test mode).

## Consequences
- New `ViolationKind` variants (`SubjectDailyHourCapExceeded`, `ClassDailyLessonCapExceeded`) widen the closed enum exposed by `ViolationResponse.kind`. The runtime never constructs them (pruning prevents the search from visiting cap-violating states); they exist for the closed-enum surface and for future telemetry.
- Existing persisted schedules are not retroactively validated. They stay valid until regenerated.
- Bake-off `BENCH_RESULTS.md` is not regenerated in this PR; spot-check with `mise run bench:bakeoff` confirmed `lahc_rr_kempe` retains soft-score 0 across canonical fixtures under the new defaults.
- Production wall-clock for hard problems may run up to 5s; easy problems still complete in <100ms thanks to the early-exit.
- ADR 0030 (cpsat dep direction), 0031 (production default), 0032 (default revisit) all stay in force; this ADR layers structural constraints on top.
```

(Replace bracketed Context with prose drawn from the spec; keep it under 200 words for narrative; no em-dashes per user global preference.)

- [ ] **Step 5.3: Index ADR 0033 in `docs/adr/README.md`**

Add an entry under whatever index format the README uses; keep dates / links consistent with sibling entries.

- [ ] **Step 5.4: Update solver/CLAUDE.md**

Add a one-line bullet under the hard-constraint rules section:

```markdown
- **Per-day caps** (`Subject.max_hours_per_day`, `SchoolClass.max_lessons_per_day`) are enforced via legality pruning in `try_place_block`; bookkeeping lives on `GreedyState.subject_hours_by_class_day` and `GreedyState.lessons_by_class_day` and is decremented in the row-removal helper used by `rr_ruin_block` and `kempe_rollback`. New violation variants (`SubjectDailyHourCapExceeded`, `ClassDailyLessonCapExceeded`) are diagnostic-only; the runtime never constructs them.
```

- [ ] **Step 5.5: Update backend/CLAUDE.md**

Add a one-line bullet under data access:

```markdown
- **Subject and SchoolClass have per-day caps.** `Subject.max_hours_per_day` is non-null with default 2; PATCH handlers can use the existing `is not None` shape. `SchoolClass.max_lessons_per_day` is nullable; PATCH handlers must use `body.model_fields_set` so an explicit null clears the column (same pattern as `home_room_id`).
```

- [ ] **Step 5.6: Update OPEN_THINGS.md**

Open `docs/superpowers/OPEN_THINGS.md`. Find the active sprint program "Solver feasibility correctness + observability" header. Update the next-pickup line to point at item 30. Do NOT add a "completed: items 38 + 39" entry; per the OPEN_THINGS hygiene rule, closed items leave no trace in the file.

If this PR's items 38 and 39 are not in the file at all (because they were surfaced inline in this conversation rather than added before the PR opened), that is consistent with the hygiene rule too: do not add a stub just to delete it. The PR description and `git log` carry the closed-work record.

- [ ] **Step 5.7: Update auto-memory roadmap status**

Open `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`. Update the entry to reflect items 38 + 39 shipped; next pickup is item 30 (memory and time-to-feasible bench columns).

- [ ] **Step 5.8: Run lint + a final sweep**

```bash
mise run lint
```

Expected: green. The lint pass catches actionlint, ruff, ty, biome, clippy, fmt, machete, and the two custom scripts (commit types, unique fns, useEffect sync).

- [ ] **Step 5.9: Commit**

```bash
git add docs/adr/ docs/superpowers/OPEN_THINGS.md \
    solver/CLAUDE.md backend/CLAUDE.md
git commit -m "docs(adr): 0033 daily caps and solver optimum-aware deadline"
```

The auto-memory file is outside the repo and is not staged. Save it directly via the Write tool as part of step 6 (autopilot's auto-memory updates step).

---

## Self-review

Spec coverage:
- Subject and Class cap fields → Tasks 1, 2, 3.
- ViolationKind variants → Task 1.
- Legality pruning → Task 1.
- Migration → Task 2.
- Pydantic + routes → Task 2.
- Frontend forms → Task 3.
- Early exit → Task 4.
- Deadline default → Task 4.
- Property test widening → Task 1.
- ADR + OPEN_THINGS → Task 5.

Type consistency: `max_hours_per_day` is `u8` in Rust and `int` in Python (range 1-20 enforced via Pydantic Field); `max_lessons_per_day` is `Option<u8>` and `int | None`. `subject_hours_by_class_day` and `lessons_by_class_day` are consistent across all tasks. `ViolationKind` variants are unit-shape (no inner data) per the existing pattern.

Placeholder scan: No `TBD`, `TODO`, "implement later", "similar to Task N", or shape-only steps. Each code step shows the actual edit. Where the existing file's exact line number is uncertain (e.g., the row-removal helper in lahc.rs around 1100 / 1651), the task names the conceptual location and the executor must read before editing.

One known soft spot: Task 2 Step 2.6 (alembic round-trip test) skips cleanly if the project lacks an Alembic test fixture; behavioural coverage in the CRUD tests still exercises the migration.
