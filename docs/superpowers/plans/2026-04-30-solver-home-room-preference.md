# Home-room preference Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `SchoolClass.home_room_id: UUID | None` plus a `prefer_home_room` soft-constraint axis that nudges placements toward each class's eponymous Klassenraum, fully wired through database, solver core, backend API, frontend dialog, demo seeds, and benches.

**Architecture:** Nullable FK on `school_classes` to `rooms` with `ON DELETE SET NULL`. `crate::types::SchoolClass` gains an `Option<RoomId>` field; `ConstraintWeights` gains `prefer_home_room: u32`. A new per-placement helper `score::home_room_penalty` mirrors `subject_preference_score`; `score_solution` builds a `HashMap<SchoolClassId, Option<RoomId>>` once per call and sums the helper across placements per member class. Greedy and LAHC pick it up automatically through `score_solution`. Frontend adds a dropdown "Klassenraum" in the school-class edit dialog.

**Tech Stack:** Rust (`solver-core`), Python (FastAPI, SQLAlchemy async, Pydantic, Alembic), TypeScript (React 19, TanStack Query, RHF, Zod, react-i18next, shadcn/ui).

---

## File structure

**Create:**
- `backend/alembic/versions/<rev>_add_school_class_home_room_id.py`
- `docs/adr/0023-home-room-preference.md`

**Modify:**
- `solver/solver-core/src/types.rs` (add `SchoolClass.home_room_id`, `ConstraintWeights.prefer_home_room`)
- `solver/solver-core/src/score.rs` (new `home_room_penalty` helper; widen `score_solution` short-circuit; build home-room lookup; integrate axis)
- `solver/solver-core/src/solve.rs` (add `prefer_home_room: 1` to `solve()` active default)
- `solver/solver-core/benches/solver_fixtures.rs` (set `home_room_id` per fixture class)
- `solver/solver-core/benches/BASELINE.md` (regenerate via `mise run bench:record`)
- `backend/src/klassenzeit_backend/db/models/school_class.py` (add column)
- `backend/src/klassenzeit_backend/scheduling/schemas/school_class.py` (Pydantic on Create / Update / Response)
- `backend/src/klassenzeit_backend/scheduling/solver_io.py` (emit `home_room_id` per SchoolClass)
- `backend/src/klassenzeit_backend/seed/demo_grundschule.py` (assign per-class home rooms)
- `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py`
- `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py`
- `frontend/src/lib/api-types.ts` (regenerated via `mise run fe:types`)
- `frontend/src/features/school-classes/schema.ts` (Zod)
- `frontend/src/features/school-classes/school-classes-dialogs.tsx` (dropdown)
- `frontend/src/i18n/locales/en.json` and `de.json`
- `docs/superpowers/OPEN_THINGS.md` (mark sprint item 7 done; file column follow-up)
- `docs/adr/README.md` (index entry for ADR 0023)

**Test:**
- `solver/solver-core/src/score.rs` (`#[cfg(test)]` block for `home_room_penalty` and integration through `score_solution`)
- `solver/solver-core/tests/grundschule_smoke.rs` (touches if needed; otherwise leave)
- `backend/tests/scheduling/test_solver_io.py` (add a JSON-shape assertion for `home_room_id`)
- `backend/tests/scheduling/test_school_class_routes.py` (round-trip the new field; create / read / update / null)
- `frontend/src/features/school-classes/school-classes-dialogs.test.tsx` (new spec for dropdown; render, select, save null)
- `frontend/tests/msw-handlers.ts` (extend SchoolClass fixtures with optional `home_room_id`)

---

## Task 1: Solver core types extension

**Files:**
- Modify: `solver/solver-core/src/types.rs`
- Test: same file (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Extend `SchoolClass` with `home_room_id: Option<RoomId>`**

In `types.rs`, change the `SchoolClass` struct:

```rust
/// A school class that receives lessons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchoolClass {
    /// Stable identifier for this school class.
    pub id: SchoolClassId,
    /// Optional home-room identifier; when set, the `prefer_home_room`
    /// soft-constraint axis penalises placements of this class outside the
    /// referenced room. `None` means the class has no preferred room and the
    /// axis no-ops for it. Wire format is additive: existing JSON callers
    /// without the field deserialise to `None`.
    #[serde(default)]
    pub home_room_id: Option<RoomId>,
}
```

- [ ] **Step 2: Extend `ConstraintWeights` with `prefer_home_room: u32`**

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConstraintWeights {
    pub class_gap: u32,
    pub teacher_gap: u32,
    pub prefer_early_period: u32,
    pub avoid_first_period: u32,
    /// Penalty per (class, placement) pair where the class has a non-null
    /// `home_room_id` that does not match the placement's `room_id`.
    /// Multi-class lessons accumulate the penalty per non-matching member
    /// class. Zero means the axis is disabled.
    pub prefer_home_room: u32,
}
```

- [ ] **Step 3: Add a serde round-trip test for `SchoolClass.home_room_id`**

Inside the existing `#[cfg(test)] mod tests` block in `types.rs`:

```rust
#[test]
fn school_class_round_trips_home_room_id_when_present() {
    let room_id = Uuid::from_bytes([8; 16]);
    let class_id = Uuid::from_bytes([9; 16]);
    let json = format!(
        r#"{{"id":"{class_id}","home_room_id":"{room_id}"}}"#
    );
    let sc: SchoolClass = serde_json::from_str(&json).unwrap();
    assert_eq!(sc.home_room_id, Some(RoomId(room_id)));
    let reserialised = serde_json::to_string(&sc).unwrap();
    let parsed_again: SchoolClass = serde_json::from_str(&reserialised).unwrap();
    assert_eq!(parsed_again, sc);
}

#[test]
fn school_class_defaults_home_room_id_to_none_when_field_omitted() {
    let class_id = Uuid::from_bytes([1; 16]);
    let json = format!(r#"{{"id":"{class_id}"}}"#);
    let sc: SchoolClass = serde_json::from_str(&json).unwrap();
    assert!(sc.home_room_id.is_none());
}
```

- [ ] **Step 4: Run tests to verify (red, then green)**

Run: `cargo nextest run -p solver-core types::`
Expected: tests pass once `home_room_id` is added; the existing `_school_class_*` tests still pass because the new field defaults via `#[serde(default)]`.

- [ ] **Step 5: Update existing fixture constructors that initialise `SchoolClass` literally**

Search for `SchoolClass {` across the workspace (excluding generated code). Each call site needs `home_room_id: None` to compile. Likely sites: tests in `solver-core/src/score.rs`, `solver-core/tests/*.rs`, `solver-core/benches/solver_fixtures.rs` (this is touched again in Task 5; for now just make it compile).

Run: `cargo build -p solver-core --tests --benches`
Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add solver/solver-core/src/types.rs solver/solver-core/src/score.rs solver/solver-core/tests/ solver/solver-core/benches/solver_fixtures.rs
git commit -m "feat(solver-core): add SchoolClass.home_room_id and prefer_home_room weight"
```

---

## Task 2: `home_room_penalty` helper (TDD)

**Files:**
- Modify: `solver/solver-core/src/score.rs`
- Test: same file (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write failing tests for `home_room_penalty`**

Add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn home_room_penalty_returns_zero_when_weight_is_zero() {
    let class_id = SchoolClassId(score_uuid(50));
    let lesson = Lesson {
        id: LessonId(score_uuid(60)),
        school_class_ids: vec![class_id],
        subject_id: SubjectId(score_uuid(40)),
        teacher_id: TeacherId(score_uuid(20)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    };
    let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
    lookup.insert(class_id, Some(RoomId(score_uuid(99))));
    let weights = ConstraintWeights {
        prefer_home_room: 0,
        ..ConstraintWeights::default()
    };
    let penalty = home_room_penalty(&lesson, &lookup, RoomId(score_uuid(30)), &weights);
    assert_eq!(penalty, 0);
}

#[test]
fn home_room_penalty_returns_zero_when_class_has_no_home_room() {
    let class_id = SchoolClassId(score_uuid(50));
    let lesson = Lesson {
        id: LessonId(score_uuid(60)),
        school_class_ids: vec![class_id],
        subject_id: SubjectId(score_uuid(40)),
        teacher_id: TeacherId(score_uuid(20)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    };
    let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
    lookup.insert(class_id, None);
    let weights = ConstraintWeights {
        prefer_home_room: 5,
        ..ConstraintWeights::default()
    };
    let penalty = home_room_penalty(&lesson, &lookup, RoomId(score_uuid(30)), &weights);
    assert_eq!(penalty, 0);
}

#[test]
fn home_room_penalty_returns_zero_when_room_matches_home_room() {
    let class_id = SchoolClassId(score_uuid(50));
    let home_room = RoomId(score_uuid(30));
    let lesson = Lesson {
        id: LessonId(score_uuid(60)),
        school_class_ids: vec![class_id],
        subject_id: SubjectId(score_uuid(40)),
        teacher_id: TeacherId(score_uuid(20)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    };
    let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
    lookup.insert(class_id, Some(home_room));
    let weights = ConstraintWeights {
        prefer_home_room: 5,
        ..ConstraintWeights::default()
    };
    let penalty = home_room_penalty(&lesson, &lookup, home_room, &weights);
    assert_eq!(penalty, 0);
}

#[test]
fn home_room_penalty_returns_weight_when_room_differs_from_home_room() {
    let class_id = SchoolClassId(score_uuid(50));
    let home_room = RoomId(score_uuid(30));
    let other_room = RoomId(score_uuid(31));
    let lesson = Lesson {
        id: LessonId(score_uuid(60)),
        school_class_ids: vec![class_id],
        subject_id: SubjectId(score_uuid(40)),
        teacher_id: TeacherId(score_uuid(20)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    };
    let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
    lookup.insert(class_id, Some(home_room));
    let weights = ConstraintWeights {
        prefer_home_room: 5,
        ..ConstraintWeights::default()
    };
    let penalty = home_room_penalty(&lesson, &lookup, other_room, &weights);
    assert_eq!(penalty, 5);
}

#[test]
fn home_room_penalty_sums_per_member_for_multi_class_lessons() {
    let c1 = SchoolClassId(score_uuid(50));
    let c2 = SchoolClassId(score_uuid(51));
    let c3 = SchoolClassId(score_uuid(52));
    let r1 = RoomId(score_uuid(30));
    let r2 = RoomId(score_uuid(31));
    let r3 = RoomId(score_uuid(32));
    let r_other = RoomId(score_uuid(33));
    let lesson = Lesson {
        id: LessonId(score_uuid(60)),
        school_class_ids: vec![c1, c2, c3],
        subject_id: SubjectId(score_uuid(40)),
        teacher_id: TeacherId(score_uuid(20)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    };
    let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
    lookup.insert(c1, Some(r1));
    lookup.insert(c2, Some(r2));
    lookup.insert(c3, Some(r3));
    let weights = ConstraintWeights {
        prefer_home_room: 4,
        ..ConstraintWeights::default()
    };
    // Placement in r_other: every class is mismatched, total = 3 * 4 = 12.
    assert_eq!(home_room_penalty(&lesson, &lookup, r_other, &weights), 12);
    // Placement in r1: only c2 and c3 are mismatched, total = 2 * 4 = 8.
    assert_eq!(home_room_penalty(&lesson, &lookup, r1, &weights), 8);
}
```

Run: `cargo nextest run -p solver-core score::tests::home_room_penalty`
Expected: FAIL with "cannot find function `home_room_penalty` in this scope".

- [ ] **Step 2: Implement `home_room_penalty`**

In `score.rs`, near `subject_preference_score`:

```rust
/// Per-placement home-room penalty. Returns `weights.prefer_home_room` once
/// per class in `lesson.school_class_ids` whose `home_room_id` is set and
/// does not match `placement_room_id`. Returns 0 when
/// `weights.prefer_home_room == 0`. Pure: depends only on the inputs;
/// allocation-free.
pub(crate) fn home_room_penalty(
    lesson: &Lesson,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    placement_room_id: RoomId,
    weights: &ConstraintWeights,
) -> u32 {
    if weights.prefer_home_room == 0 {
        return 0;
    }
    let mut score = 0u32;
    for class_id in &lesson.school_class_ids {
        if let Some(Some(home_id)) = home_room_lookup.get(class_id) {
            if *home_id != placement_room_id {
                score = score.saturating_add(weights.prefer_home_room);
            }
        }
    }
    score
}
```

Imports needed at top of file (if not already present): `use crate::ids::RoomId;`. The existing `use crate::ids::{LessonId, SchoolClassId, TeacherId, TimeBlockId};` widens to add `RoomId`.

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo nextest run -p solver-core score::tests::home_room_penalty`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add solver/solver-core/src/score.rs
git commit -m "feat(solver-core): add home_room_penalty per-placement helper"
```

---

## Task 3: Integrate home-room axis into `score_solution`

**Files:**
- Modify: `solver/solver-core/src/score.rs`
- Test: same file

- [ ] **Step 1: Write failing test exercising `score_solution` with the new axis**

Append to `score.rs` tests:

```rust
#[test]
fn score_solution_includes_home_room_penalty_per_class() {
    // Class 50 has a home room (uuid 30); two placements: one in room 30
    // (match, 0 penalty), one in room 31 (mismatch, +weight). Class 51 has
    // no home room (None), placement in room 31 contributes 0.
    let mut p = three_block_one_class_problem();
    // Add a second class without a home room.
    let class2 = SchoolClassId(score_uuid(51));
    p.school_classes.push(SchoolClass {
        id: class2,
        home_room_id: None,
    });
    // Set a home room on class 50.
    p.school_classes[0].home_room_id = Some(RoomId(score_uuid(30)));
    // Add a second room to score against.
    p.rooms.push(Room {
        id: RoomId(score_uuid(31)),
    });
    let weights = ConstraintWeights {
        prefer_home_room: 7,
        ..ConstraintWeights::default()
    };
    // Single placement, lesson 60, class 50, in non-home room 31. Penalty = 7.
    let placements = [Placement {
        lesson_id: LessonId(score_uuid(60)),
        time_block_id: TimeBlockId(score_uuid(10)),
        room_id: RoomId(score_uuid(31)),
    }];
    assert_eq!(score_solution(&p, &placements, &weights), 7);
}

#[test]
fn score_solution_zero_when_only_home_room_weight_set_and_no_home_rooms() {
    // No SchoolClass has a home room; even with a non-zero prefer_home_room
    // weight the score is 0.
    let p = three_block_one_class_problem();
    let weights = ConstraintWeights {
        prefer_home_room: 10,
        ..ConstraintWeights::default()
    };
    let placements = [place(60, 10), place(60, 12)];
    assert_eq!(score_solution(&p, &placements, &weights), 0);
}
```

Run: `cargo nextest run -p solver-core score::tests::score_solution_includes_home_room_penalty_per_class`
Expected: FAIL (axis not yet wired through `score_solution`; current short-circuit returns 0 because the four old weights are all 0; even after the short-circuit fix, `score_solution` doesn't yet sum the axis).

- [ ] **Step 2: Widen the short-circuit in `score_solution`**

Currently:

```rust
if weights.class_gap == 0
    && weights.teacher_gap == 0
    && weights.prefer_early_period == 0
    && weights.avoid_first_period == 0
{
    return 0;
}
```

Replace with:

```rust
if weights.class_gap == 0
    && weights.teacher_gap == 0
    && weights.prefer_early_period == 0
    && weights.avoid_first_period == 0
    && weights.prefer_home_room == 0
{
    return 0;
}
```

- [ ] **Step 3: Build the home-room lookup once and sum per placement**

Inside `score_solution`, after the existing `subject_lookup` line:

```rust
let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
    .school_classes
    .iter()
    .map(|c| (c.id, c.home_room_id))
    .collect();
```

Then add a `home_room_total` sum parallel to `subject_preference`:

```rust
let home_room_total: u32 = placements
    .iter()
    .map(|p| {
        let lesson = lesson_lookup[&p.lesson_id];
        home_room_penalty(lesson, &home_room_lookup, p.room_id, weights)
    })
    .sum();
```

Final return value adds `home_room_total`:

```rust
weights
    .class_gap
    .saturating_mul(class_gaps)
    .saturating_add(weights.teacher_gap.saturating_mul(teacher_gaps))
    .saturating_add(subject_preference)
    .saturating_add(home_room_total)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p solver-core score::`
Expected: every existing score test plus the two new ones pass.

- [ ] **Step 5: Commit**

```bash
git add solver/solver-core/src/score.rs
git commit -m "feat(solver-core): integrate home_room_penalty into score_solution"
```

---

## Task 4: Active default weight in `solve()`

**Files:**
- Modify: `solver/solver-core/src/solve.rs`
- Test: existing `solver-core/tests/grundschule_smoke.rs` (no new test needed; verify build + bench in Task 5)

- [ ] **Step 1: Add `prefer_home_room: 1` to the active default**

In `solve.rs`, replace the existing `solve()` body's `ConstraintWeights { ... }` literal:

```rust
pub fn solve(problem: &Problem) -> Result<Solution, Error> {
    let active_default = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 1,
            teacher_gap: 1,
            prefer_early_period: 1,
            avoid_first_period: 1,
            prefer_home_room: 1,
        },
        deadline: Some(Duration::from_millis(200)),
        ..SolveConfig::default()
    };
    solve_with_config(problem, &active_default)
}
```

- [ ] **Step 2: Update the doc comment on `solve()`**

Adjust the rustdoc to mention the new axis:

```rust
/// Solve the timetable problem using lowest-delta greedy placement followed
/// by a 200ms LAHC local-search pass. Active default soft-constraint weights
/// are `class_gap = teacher_gap = prefer_early_period = avoid_first_period
/// = prefer_home_room = 1`. Callers wanting greedy-only behaviour
/// (no LAHC pass) construct their own [`SolveConfig`] with `deadline: None`
/// and call [`solve_with_config`] directly.
pub fn solve(problem: &Problem) -> Result<Solution, Error> {
```

- [ ] **Step 3: Run all solver-core tests**

Run: `cargo nextest run -p solver-core`
Expected: every test passes including `grundschule_smoke`.

- [ ] **Step 4: Commit**

```bash
git add solver/solver-core/src/solve.rs
git commit -m "feat(solver-core): activate prefer_home_room default weight in solve()"
```

---

## Task 5: Bench fixtures + BASELINE.md refresh

**Files:**
- Modify: `solver/solver-core/benches/solver_fixtures.rs`
- Modify: `solver/solver-core/benches/BASELINE.md` (regenerated)

- [ ] **Step 1: Locate the bench fixture's class array**

Open `solver/solver-core/benches/solver_fixtures.rs`. Find each fixture's literal `SchoolClass { ... }` constructor (Task 1 already added `home_room_id: None` to satisfy the borrow checker). Each fixture also constructs rooms with stable `RoomId(...)` values; identify which `RoomId` corresponds to each class's eponymous Klassenraum.

- [ ] **Step 2: Set `home_room_id` per class in each fixture**

For the `grundschule` fixture: classes 1a/2a/3a/4a each map to "Klasse 1a"/"Klasse 2a"/"Klasse 3a"/"Klasse 4a" rooms. For `zweizuegig`: 1a/1b through 4a/4b map to their "Klasse Xy" rooms. For `dreizuegig`: 1a/1b/1c through 4a/4b/4c map to their "Klasse Xy" rooms.

Replace each `SchoolClass { id: <class_id>, home_room_id: None }` with `SchoolClass { id: <class_id>, home_room_id: Some(<eponymous_room_id>) }`. Keep the existing `home_room_id: None` for any non-class entity (none expected; verify).

If the fixture file uses a helper like `fn school_class(idx: u8) -> SchoolClass`, thread the home-room id through the helper signature.

- [ ] **Step 3: Verify bench compiles and runs**

Run: `cargo bench -p solver-core --bench solver_fixtures --no-run`
Expected: clean compile.

Run: `mise run bench`
Expected: criterion completes; soft-score column reflects the new axis. Confirm placements still pass (no hard-constraint regression); `dreizuegige` may show a small soft-score change because the Religion trio now scores against home rooms.

- [ ] **Step 4: Refresh BASELINE.md**

Run: `mise run bench:record`
Expected: `solver/solver-core/benches/BASELINE.md` updates with the new soft-score column for each fixture and a fresh footer (host CPU, kernel, rustc).

Verify the per-fixture wall-clock change is within the 20% budget. If a fixture exceeds the budget, investigate before continuing (almost always: home-room lookup being rebuilt per placement instead of once per `score_solution` call).

- [ ] **Step 5: Commit**

```bash
git add solver/solver-core/benches/solver_fixtures.rs solver/solver-core/benches/BASELINE.md
git commit -m "feat(solver-core): set home_room_id per fixture class; refresh BASELINE"
```

---

## Task 6: Backend ORM column

**Files:**
- Modify: `backend/src/klassenzeit_backend/db/models/school_class.py`

- [ ] **Step 1: Add the column to the SchoolClass ORM**

Replace the existing `SchoolClass` body with:

```python
class SchoolClass(Base):
    """A class/group of students (e.g. '5a', '10b')."""

    __tablename__ = "school_classes"

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    name: Mapped[str] = mapped_column(String(20), unique=True)
    grade_level: Mapped[int] = mapped_column(SmallInteger)
    stundentafel_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("stundentafeln.id"))
    week_scheme_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("week_schemes.id"))
    home_room_id: Mapped[uuid.UUID | None] = mapped_column(
        ForeignKey("rooms.id", ondelete="SET NULL"), nullable=True
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )
```

- [ ] **Step 2: Verify `ty` and ruff pass**

Run: `mise run lint:py`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add backend/src/klassenzeit_backend/db/models/school_class.py
git commit -m "feat(backend): add SchoolClass.home_room_id ORM column"
```

---

## Task 7: Alembic migration

**Files:**
- Create: `backend/alembic/versions/<rev>_add_school_class_home_room_id.py`

- [ ] **Step 1: Generate revision skeleton**

Run from `backend/`:

```bash
cd backend && uv run alembic revision -m "add school_class home_room_id"
```

Expected: a new file `alembic/versions/<random_rev>_add_school_class_home_room_id.py` with a placeholder body, `down_revision: str | Sequence[str] | None = "17f73c6e1a91"`.

- [ ] **Step 2: Replace generated body with the explicit add_column / drop_column pair**

```python
"""add school_class home_room_id

Revision ID: <rev>
Revises: 17f73c6e1a91
Create Date: <auto>

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "<rev>"
down_revision: str | Sequence[str] | None = "17f73c6e1a91"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "school_classes",
        sa.Column("home_room_id", sa.Uuid(), nullable=True),
    )
    op.create_foreign_key(
        "fk_school_classes_home_room_id_rooms",
        "school_classes",
        "rooms",
        ["home_room_id"],
        ["id"],
        ondelete="SET NULL",
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint(
        "fk_school_classes_home_room_id_rooms", "school_classes", type_="foreignkey"
    )
    op.drop_column("school_classes", "home_room_id")
```

Per `backend/CLAUDE.md`: use `collections.abc.Sequence` (not `typing.Sequence`). PEP 604 unions.

- [ ] **Step 3: Drop the test template + worker DBs (schema-changing migration)**

Per `backend/CLAUDE.md` ("Schema-changing PRs: drop the template + per-worker DBs before the first test run"):

```bash
psql -h localhost -p 5432 -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test_template;"
for n in 0 1 2 3 4 5 6 7; do
  psql -h localhost -p 5432 -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test_gw${n};"
done
```

(Adapt host / port / user to local Postgres setup; the conftest will recreate them on next test run.)

- [ ] **Step 4: Verify migration applies in dev**

```bash
mise run db:migrate
```

Expected: clean apply, head moves to the new revision.

Roundtrip-check: `cd backend && uv run alembic downgrade -1 && uv run alembic upgrade head` should both succeed without errors.

- [ ] **Step 5: Commit**

```bash
git add backend/alembic/versions/*_add_school_class_home_room_id.py
git commit -m "feat(backend): alembic migration for school_class.home_room_id"
```

---

## Task 8: Pydantic schemas + route round-trip (TDD)

**Files:**
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/school_class.py`
- Test: `backend/tests/scheduling/test_school_class_routes.py` (or wherever existing school-class route tests live; locate first)

- [ ] **Step 1: Locate existing school-class route test file**

Run: `ls backend/tests/scheduling/ | grep -i school`
Expected: at least one file (e.g. `test_school_class_routes.py`). Open it; identify a "create + read round-trip" pattern to mirror.

- [ ] **Step 2: Write failing tests for `home_room_id` round-trip**

Add three tests to the same file (give them distinct names per the unique-function-names rule). Adapt fixture names to match the existing patterns (likely `db_session`, `client`, `create_room`, etc.):

```python
async def test_school_class_create_round_trips_home_room_id(
    client: AsyncClient, create_room  # adapt to actual fixtures
) -> None:
    room = await create_room(name="Klasse 1a", short_name="1a")
    # ... existing pattern for stundentafel + week scheme setup ...
    body = {
        "name": "1a",
        "grade_level": 1,
        "stundentafel_id": str(stundentafel_id),
        "week_scheme_id": str(week_scheme_id),
        "home_room_id": str(room.id),
    }
    response = await client.post("/api/classes", json=body)
    assert response.status_code == 201
    payload = response.json()
    assert payload["home_room_id"] == str(room.id)


async def test_school_class_create_accepts_null_home_room_id(
    client: AsyncClient,
) -> None:
    # ... setup ...
    body = {
        "name": "1a",
        "grade_level": 1,
        "stundentafel_id": str(stundentafel_id),
        "week_scheme_id": str(week_scheme_id),
        "home_room_id": None,
    }
    response = await client.post("/api/classes", json=body)
    assert response.status_code == 201
    assert response.json()["home_room_id"] is None


async def test_school_class_update_clears_home_room_id_when_null(
    client: AsyncClient, create_room
) -> None:
    room = await create_room(name="Klasse 1a", short_name="1a")
    # ... create class with home_room_id=room.id ...
    response = await client.patch(
        f"/api/classes/{class_id}", json={"home_room_id": None}
    )
    assert response.status_code == 200
    assert response.json()["home_room_id"] is None
```

Run: `mise run test:py -- backend/tests/scheduling/test_school_class_routes.py -v`
Expected: FAIL with Pydantic validation rejecting `home_room_id` (extra field) or with `home_room_id` missing from the response.

- [ ] **Step 3: Add the field to all three Pydantic schemas**

Edit `backend/src/klassenzeit_backend/scheduling/schemas/school_class.py`:

```python
"""Pydantic schemas for school class routes."""

import uuid
from datetime import datetime

from pydantic import BaseModel


class SchoolClassCreate(BaseModel):
    """Request body for creating a school class."""

    name: str
    grade_level: int
    stundentafel_id: uuid.UUID
    week_scheme_id: uuid.UUID
    home_room_id: uuid.UUID | None = None


class SchoolClassUpdate(BaseModel):
    """Request body for patching a school class."""

    name: str | None = None
    grade_level: int | None = None
    stundentafel_id: uuid.UUID | None = None
    week_scheme_id: uuid.UUID | None = None
    home_room_id: uuid.UUID | None = None


class SchoolClassResponse(BaseModel):
    """Response body for a school class."""

    id: uuid.UUID
    name: str
    grade_level: int
    stundentafel_id: uuid.UUID
    week_scheme_id: uuid.UUID
    home_room_id: uuid.UUID | None = None
    created_at: datetime
    updated_at: datetime
```

- [ ] **Step 4: Verify the route handler propagates the field**

Open `backend/src/klassenzeit_backend/scheduling/routes/school_classes.py`. Locate the `create` and `update` handlers. They likely use `body.model_dump(exclude_unset=True)` and pass to `SchoolClass(...)` or `setattr` updates. The new field is plain; no special handling needed unless the handler whitelists fields explicitly. If it does, add `home_room_id` to the whitelist.

The PATCH handler must accept explicit `null` to clear the FK. Pydantic's `exclude_unset=True` already distinguishes "not sent" from "sent as null"; verify by reading the handler's update path.

- [ ] **Step 5: Run tests to verify pass**

Run: `mise run test:py -- backend/tests/scheduling/test_school_class_routes.py -v`
Expected: all three tests pass.

- [ ] **Step 6: Verify the existing test suite still passes**

Run: `mise run test:py -- backend/tests/scheduling/ -v`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add backend/src/klassenzeit_backend/scheduling/schemas/school_class.py backend/src/klassenzeit_backend/scheduling/routes/school_classes.py backend/tests/scheduling/test_school_class_routes.py
git commit -m "feat(backend): expose SchoolClass.home_room_id on Pydantic schemas"
```

---

## Task 9: `build_problem_json` emits `home_room_id`

**Files:**
- Modify: `backend/src/klassenzeit_backend/scheduling/solver_io.py`
- Test: `backend/tests/scheduling/test_solver_io.py`

- [ ] **Step 1: Write a failing test for the new field shape**

Add to `backend/tests/scheduling/test_solver_io.py`:

```python
async def test_build_problem_json_emits_home_room_id_per_school_class(
    db_session,  # adapt to actual fixtures
    create_room,
    create_school_class,
    # ... other prereq factories
) -> None:
    home_room = await create_room(name="Klasse 1a", short_name="1a")
    cls = await create_school_class(name="1a", grade_level=1, home_room_id=home_room.id)
    # build a minimal solvable scenario where cls is the requested class

    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    payload = json.loads(problem_json)
    sc_entry = next(c for c in payload["school_classes"] if c["id"] == str(cls.id))
    assert sc_entry["home_room_id"] == str(home_room.id)


async def test_build_problem_json_emits_null_home_room_id_when_not_set(
    db_session, create_school_class
) -> None:
    cls = await create_school_class(name="1a", grade_level=1)  # no home_room_id
    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    payload = json.loads(problem_json)
    sc_entry = next(c for c in payload["school_classes"] if c["id"] == str(cls.id))
    assert sc_entry["home_room_id"] is None
```

(If `create_school_class` doesn't yet accept `home_room_id`, add the kwarg in the conftest factory. Locate via `grep -n "def create_school_class" backend/tests/scheduling/conftest.py`.)

Run: `mise run test:py -- backend/tests/scheduling/test_solver_io.py -v -k home_room`
Expected: FAIL with `KeyError: 'home_room_id'` because `build_problem_json` doesn't emit the field yet.

- [ ] **Step 2: Emit the field in `build_problem_json`**

In `solver_io.py`, locate the `school_classes` list comprehension (currently `[{"id": str(c.id)} for c in involved_classes]`) and replace with:

```python
"school_classes": [
    {
        "id": str(c.id),
        "home_room_id": str(c.home_room_id) if c.home_room_id else None,
    }
    for c in involved_classes
],
```

- [ ] **Step 3: Run tests to verify pass**

Run: `mise run test:py -- backend/tests/scheduling/test_solver_io.py -v`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add backend/src/klassenzeit_backend/scheduling/solver_io.py backend/tests/scheduling/test_solver_io.py backend/tests/scheduling/conftest.py
git commit -m "feat(backend): emit home_room_id in build_problem_json"
```

---

## Task 10: Demo seeds populate `home_room_id`

**Files:**
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule.py`
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py`
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py`

- [ ] **Step 1: Update `demo_grundschule.py` to assign home rooms**

The current pattern (around line 261) creates rooms after classes. Two options:

**Option A (chosen):** add a second pass that updates each class's `home_room_id` after both classes and rooms are flushed.

After the existing `for room_spec in _ROOMS:` loop and its `flush`, before the function ends or before lessons are created, add:

```python
# Assign each class its eponymous Klassenraum as home_room.
rooms_by_short = {
    room.short_name: room
    for room in (
        (await session.execute(select(Room).where(Room.name.startswith("Klasse "))))
        .scalars()
        .all()
    )
}
classes = (await session.execute(select(SchoolClass))).scalars().all()
for cls in classes:
    home_room = rooms_by_short.get(cls.name)
    if home_room is not None:
        cls.home_room_id = home_room.id
await session.flush()
```

(Add `select` and `Room` to the existing imports at the top if missing. Verify by reading the seed file's import block.)

- [ ] **Step 2: Mirror in `demo_grundschule_zweizuegig.py`**

Same shape; the `_ROOMS` array there already includes 8 Klassenräume named "Klasse 1a", "Klasse 1b", ... "Klasse 4b" matching class names. Add the second pass after rooms are created.

- [ ] **Step 3: Mirror in `demo_grundschule_dreizuegig.py`**

Same shape; 12 Klassenräume.

- [ ] **Step 4: Verify the seed runs end-to-end**

Run:

```bash
mise run db:reset
uv run klassenzeit-backend seed-grundschule
```

Then verify in psql:

```sql
SELECT name, home_room_id FROM school_classes;
```

Expected: every row has a non-null `home_room_id` matching its eponymous Klassenraum.

Repeat with the dreizuegige variant to confirm.

- [ ] **Step 5: Run seed solvability tests**

Run: `mise run test:py -- backend/tests/seed/ -v`
Expected: every solvability test still passes (the new axis is a soft constraint and does not block placement).

- [ ] **Step 6: Commit**

```bash
git add backend/src/klassenzeit_backend/seed/demo_grundschule.py backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py
git commit -m "feat(backend): assign eponymous Klassenraum as home_room in demo seeds"
```

---

## Task 11: Frontend types regen + Zod

**Files:**
- Modify: `frontend/src/lib/api-types.ts` (regenerated)
- Modify: `frontend/src/features/school-classes/schema.ts`

- [ ] **Step 1: Regenerate OpenAPI types**

Run: `mise run fe:types`
Expected: `frontend/src/lib/api-types.ts` updates; `SchoolClassResponse`, `SchoolClassCreate`, `SchoolClassUpdate` now expose `home_room_id?: string | null`.

- [ ] **Step 2: Add `home_room_id` to the Zod form schema**

Edit `frontend/src/features/school-classes/schema.ts`:

```typescript
import { z } from "zod";

export const SchoolClassFormSchema = z.object({
  name: z.string().trim().min(1, "Name is required").max(100),
  grade_level: z.number().int().min(1, "Grade is required"),
  stundentafel_id: z.string().min(1, "Curriculum is required"),
  week_scheme_id: z.string().min(1, "Week scheme is required"),
  home_room_id: z.string().nullable(),
});

export type SchoolClassFormValues = z.infer<typeof SchoolClassFormSchema>;
```

Per `frontend/CLAUDE.md`: do NOT use `z.uuid()` for FK form fields (Zod v4 enforces RFC 4122 version bits and pattern UUIDs in tests fail). `z.string().nullable()` accepts both UUID strings and `null`. The empty-option in the dropdown will map to `null`.

- [ ] **Step 3: Run frontend lint + tests**

Run: `mise run fe:test`
Expected: existing school-classes specs still pass (they don't yet exercise the new field).

Run: `mise exec -- pnpm -C frontend lint`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/api-types.ts frontend/src/features/school-classes/schema.ts
git commit -m "feat(frontend): regen api-types and Zod schema for home_room_id"
```

---

## Task 12: Dropdown in school-class edit dialog (TDD)

**Files:**
- Modify: `frontend/src/features/school-classes/school-classes-dialogs.tsx`
- Modify: `frontend/src/features/school-classes/school-classes-dialogs.test.tsx` (locate or create)
- Modify: `frontend/tests/msw-handlers.ts`

- [ ] **Step 1: Locate or create the dialog test file**

Run: `ls frontend/src/features/school-classes/`
Expected: a `*-dialogs.test.tsx` may or may not exist. If absent, create one mirroring `frontend/src/features/rooms/rooms-dialogs.test.tsx`.

- [ ] **Step 2: Add a `useRooms()` hook**

Open `frontend/src/features/rooms/hooks.ts` and confirm a `useRooms()` hook exists. If not, add one mirroring `useSchoolClasses()`. (Likely it already exists; verify.)

- [ ] **Step 3: Extend MSW handlers with rooms data**

In `frontend/tests/msw-handlers.ts`, ensure the `/api/rooms` GET handler returns at least two rooms (e.g. "Klasse 1a" and "Turnhalle") so the dropdown has options. Also extend the SchoolClass fixtures with optional `home_room_id` so create/update round-trips can reflect the new field.

- [ ] **Step 4: Write failing tests**

Inside the test file, add three tests (use unique names per the unique-function-names rule):

```typescript
test("school-class create dialog renders home_room dropdown with rooms", async () => {
  // pin English locale per frontend/CLAUDE.md
  await i18n.changeLanguage("en");
  // open create dialog
  // assert getByRole("combobox", { name: /home room/i }) is rendered
  // open the dropdown, assert "Klasse 1a" option present
});

test("school-class edit dialog shows current home_room selection", async () => {
  // seed a school class with home_room_id = room1.id
  // open edit dialog
  // assert the combobox displays "Klasse 1a"
});

test("school-class edit dialog can clear home_room to null", async () => {
  // open edit dialog with home_room_id = room1.id
  // pick the "no home room" option
  // submit
  // assert PATCH body sent home_room_id: null
});
```

Run: `cd frontend && mise exec -- pnpm vitest run src/features/school-classes/school-classes-dialogs.test.tsx`
Expected: FAIL (dropdown not rendered yet).

- [ ] **Step 5: Add the dropdown to the dialog**

In `school-classes-dialogs.tsx`:

1. Import `useRooms` from `@/features/rooms/hooks`.
2. Inside `SchoolClassFormDialog`, after `const weekSchemes = useWeekSchemes();` add `const rooms = useRooms();` and `const roomOptions = rooms.data ?? [];`.
3. Update `defaultValues`: add `home_room_id: schoolClass?.home_room_id ?? null`.
4. Update `handleSchoolClassSubmit`'s `body`: add `home_room_id: values.home_room_id`.
5. Render a new `FormField` after the week-scheme dropdown:

```tsx
<FormField
  control={form.control}
  name="home_room_id"
  render={({ field }) => {
    const NULL_VALUE = "__none__";
    const value = field.value ?? NULL_VALUE;
    return (
      <FormItem>
        <FormLabel>{t("schoolClasses.fields.homeRoomLabel")}</FormLabel>
        <Select
          value={value}
          onValueChange={(next) =>
            field.onChange(next === NULL_VALUE ? null : next)
          }
        >
          <FormControl>
            <SelectTrigger>
              <SelectValue
                placeholder={t("schoolClasses.fields.homeRoomPlaceholder")}
              />
            </SelectTrigger>
          </FormControl>
          <SelectContent>
            <SelectItem value={NULL_VALUE}>
              {t("schoolClasses.fields.homeRoomPlaceholder")}
            </SelectItem>
            {roomOptions.map((option) => (
              <SelectItem key={option.id} value={option.id}>
                {option.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <FormMessage />
      </FormItem>
    );
  }}
/>
```

The `NULL_VALUE` sentinel is needed because Radix `Select` treats `value=""` as "uncontrolled". Sentinels are local-const, mapped at the boundary.

- [ ] **Step 6: Run tests to verify pass**

Run: `cd frontend && mise exec -- pnpm vitest run src/features/school-classes/school-classes-dialogs.test.tsx`
Expected: three new tests pass.

- [ ] **Step 7: Run full frontend test suite**

Run: `mise run fe:test`
Expected: clean.

Run: `cd frontend && mise exec -- pnpm exec tsc --noEmit`
Expected: clean.

- [ ] **Step 8: Browser verification (per `frontend/CLAUDE.md`'s "Browser verification" rule)**

Run: `mise run fe:dev` in one terminal; open `http://localhost:5173/school-classes` in a browser. Open the edit dialog for an existing class. Confirm:
- The "Home room" dropdown appears below "Week scheme".
- The current value is displayed (or "No home room" if null).
- Selecting a room and saving persists.
- Re-opening shows the new value.
- Selecting "No home room" and saving clears the field.

Note in the PR body that browser verification was performed.

- [ ] **Step 9: Commit**

```bash
git add frontend/src/features/school-classes/ frontend/tests/msw-handlers.ts
git commit -m "feat(frontend): add home_room dropdown to school-class edit dialog"
```

---

## Task 13: i18n strings (en + de)

**Files:**
- Modify: `frontend/src/i18n/locales/en.json`
- Modify: `frontend/src/i18n/locales/de.json`

- [ ] **Step 1: Add keys to `en.json`**

Locate the `schoolClasses.fields.*` block (already contains `gradeLevelLabel`, `stundentafelLabel`, `weekSchemeLabel`, etc.). Add:

```json
"homeRoomLabel": "Home room",
"homeRoomPlaceholder": "No home room"
```

- [ ] **Step 2: Add the same keys to `de.json`**

```json
"homeRoomLabel": "Klassenraum",
"homeRoomPlaceholder": "Kein Klassenraum"
```

- [ ] **Step 3: Verify type-checked i18n still passes**

Per `frontend/CLAUDE.md`: `t()` keys are typed against `en.json` via `src/i18n/types.d.ts`. Adding keys to en first, then de, keeps the type-check green.

Run: `cd frontend && mise exec -- pnpm exec tsc --noEmit`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/i18n/locales/en.json frontend/src/i18n/locales/de.json
git commit -m "feat(frontend): add home-room i18n keys (en + de)"
```

---

## Task 14: ADR 0023

**Files:**
- Create: `docs/adr/0023-home-room-preference.md`
- Modify: `docs/adr/README.md`

- [ ] **Step 1: Write ADR 0023 from the template**

Read `docs/adr/template.md` and `docs/adr/0022-lesson-group-coplacement.md` to mirror the project's ADR style. Per the root `.claude/CLAUDE.md`: title format is `# 0023: Home-room preference soft constraint` (colon, no em-dash) for new ADRs.

Body covers:
- Context: Hessen Grundschulen run lessons primarily in the class's eponymous Klassenraum; the demo lacked any signal toward this.
- Decision: nullable FK on `school_classes` to `rooms`; new `prefer_home_room` axis in `ConstraintWeights`; per-placement penalty per non-matching member class.
- Consequences: bench refresh, additive wire format, demo seeds map 1:1.
- Alternatives considered: M:N table (rejected: cardinality is 1), reverse FK (rejected: reading direction wrong).

Reference the spec: `docs/superpowers/specs/2026-04-30-solver-home-room-preference-design.md`.

- [ ] **Step 2: Append entry to `docs/adr/README.md` index**

Add the new row in the index table mirroring 0022's entry.

- [ ] **Step 3: Verify lint passes**

Run: `mise run lint`
Expected: clean (ADRs are markdown; no lint touches them aside from any prose rules).

- [ ] **Step 4: Commit**

```bash
git add docs/adr/0023-home-room-preference.md docs/adr/README.md
git commit -m "docs: add ADR 0023 home-room preference"
```

---

## Task 15: OPEN_THINGS update + roadmap memory refresh

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`

- [ ] **Step 1: Mark sprint item 7 done in OPEN_THINGS.md**

Locate the "Algorithm phase" subsection's item 7 ("Home-room preference"). Replace the body with the same shipped-format used by item 6 ("Lesson-group co-placement constraint"):

```markdown
7. **Home-room preference.** `[P1]` ✅ Shipped 2026-04-30 in PR `feat/solver-home-room-preference`. Adds nullable `SchoolClass.home_room_id: UUID | None` (`ON DELETE SET NULL`) and a `prefer_home_room` soft-constraint axis (active default weight 1) that penalises placements per non-matching member class. Demo seeds (Grundschule, zweizuegige, dreizuegige) map every class to its eponymous Klassenraum. Bench `BASELINE.md` refreshed; p50 wall-clock within 20 percent budget per fixture. ADR 0023 records the decision.
```

- [ ] **Step 2: File the list-page column follow-up under "Acknowledged deferrals"**

Add an entry mirroring the existing "Subjects table column for preference flags" entry:

```markdown
- **School-classes table column for home room.** A column showing each class's home-room name on the list view. Skipped in the home-room PR because the list table already carries Name, Grade, Curriculum, Week scheme, Actions; a sixth column clutters more than it helps for the prototype. Revisit when users say they cannot see at-a-glance which Klassenraum a class is pinned to. Surfaced during the home-room preference PR.
```

- [ ] **Step 3: Refresh the roadmap memory file**

Edit `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`:

- Update the `description:` frontmatter line: "Active focus moves to algorithm-phase P1 avoid-last-period (item 8)."
- Add a closed bullet for sprint item 7 in the "Algorithm phase (P1)" section.
- Update "How to apply" to point at item 8 (avoid-last-period) as the next candidate.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/OPEN_THINGS.md /home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md
git commit -m "docs: close sprint item 7 home-room preference"
```

(The memory file lives outside the repo; if `git add` rejects it, that's expected. Save it via `Write` instead and proceed without git.)

---

## Self-review

Walked the spec section-by-section against the plan:

- **Goal**: covered by Tasks 1-13.
- **Database (Alembic migration)**: Task 7.
- **`SchoolClass` ORM**: Task 6.
- **Pydantic Create/Update/Response**: Task 8.
- **`build_problem_json` emits `home_room_id`**: Task 9.
- **Demo seeds**: Task 10.
- **`solver-core::types` extensions**: Task 1.
- **`score::home_room_penalty`**: Task 2.
- **`score_solution` integration**: Task 3.
- **`solve()` active default**: Task 4.
- **LAHC: no special wiring**: covered implicitly (no task needed; the spec says LAHC picks it up through `score_solution`, which Tasks 3 and 4 already accomplish).
- **Frontend types regen + Zod**: Task 11.
- **Dropdown in dialog**: Task 12.
- **i18n keys**: Task 13.
- **Bench refresh**: Task 5.
- **ADR**: Task 14.
- **OPEN_THINGS update**: Task 15.

Placeholder scan: every task has actual file paths, real code blocks, exact commands. No "TBD" / "implement later" / "similar to Task N" placeholders.

Type consistency: `home_room_id` used uniformly across Rust (`Option<RoomId>`), Python (`uuid.UUID | None`), TypeScript (`string | null`). `prefer_home_room` weight name consistent across struct, JSON, helper signatures, doc comments. `home_room_penalty` helper signature `(lesson, lookup, placement_room_id, weights)` consistent across declaration, tests, call site in `score_solution`.

Plan is self-consistent and complete.
