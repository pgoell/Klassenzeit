# Home-room preference soft constraint

**Sprint:** Realer Schulalltag + better scheduler (algorithm phase, P1).

**Closes (in `docs/superpowers/OPEN_THINGS.md`):** sprint item 7.

**ADR:** [0023: home-room preference soft constraint](../../adr/0023-home-room-preference.md), added in this PR.

## Goal

Add a `home_room_id: UUID | None` column on `SchoolClass` and a matching `prefer_home_room` soft-constraint axis that penalises placements where a class lands in a room other than its eponymous Klassenraum. Demo seeds populate every class with its own Klassenraum so the demo flow visibly demonstrates "1a's lessons mostly happen in 1a's room" without manual rules. The bench `BASELINE.md` is refreshed in the same PR.

## Non-goals

- Per-class subject-preference overrides ("Sport last period for 4c despite the global avoid-first flag"). Filed under acknowledged deferrals.
- Configurable per-class `prefer_home_room` weight. The active default of 1 ships first; tuning is downstream of empirical data.
- Cross-class home-room exceptions for cross-Jahrgang Religion groups. The per-Jahrgang Religion trios in the dreizuegige seed are already covered by the existing lesson-group co-placement constraint; cross-Jahrgang grouping is its own deferral.
- LAHC room-swap moves. LAHC's Change move only repicks `time_block`, not room; rooms are reassigned greedy-by-id during the move (existing behaviour). The new axis still scores correctly because the greedy room pick now factors home-room into its tiebreak.
- A "home room" column on the school-classes list page. Filed as an OPEN_THINGS follow-up, mirroring the deferred subject-flags column from PR-9c.

## Architecture changes

### Database (Alembic migration)

Add a nullable column to `school_classes`:

```sql
ALTER TABLE school_classes
    ADD COLUMN home_room_id UUID NULL
    REFERENCES rooms(id) ON DELETE SET NULL;
```

`ON DELETE SET NULL` so deleting a Room nulls out the FK without cascading the SchoolClass. Postgres handles nullable-FK migrations on a populated table fine; no backfill needed (every existing row defaults to NULL).

### `klassenzeit_backend.db.models.school_class.SchoolClass`

Add the column:

```python
home_room_id: Mapped[uuid.UUID | None] = mapped_column(
    ForeignKey("rooms.id", ondelete="SET NULL"), nullable=True
)
```

### `klassenzeit_backend.api.schemas` (or wherever `SchoolClassRead/Create/Update` live)

Add `home_room_id: UUID | None = None` to all three schemas.

### `klassenzeit_backend.scheduling.solver_io.build_problem_json`

Emit `home_room_id` per `SchoolClass` in the JSON payload. The Rust side already accepts the field via `#[serde(default)]`.

### `klassenzeit_backend.seed.demo_grundschule` (and `_zweizuegig`, `_dreizuegig`)

Populate `home_room_id` for every class with its eponymous Klassenraum (`1a → "Klasse 1a"`, `1b → "Klasse 1b"`, etc.). The seeds already create one Klassenraum per class.

### `solver-core::types`

Extend `SchoolClass`:

```rust
pub struct SchoolClass {
    pub id: SchoolClassId,
    #[serde(default)]
    pub home_room_id: Option<RoomId>,
}
```

`#[serde(default)]` keeps the wire format additive: existing JSON callers (and the bench fixtures' literal constructors) stay valid.

Extend `ConstraintWeights`:

```rust
pub struct ConstraintWeights {
    pub class_gap: u32,
    pub teacher_gap: u32,
    pub prefer_early_period: u32,
    pub avoid_first_period: u32,
    pub prefer_home_room: u32,
}
```

### `solver-core::score`

Add a per-placement helper following the `subject_preference_score` template:

```rust
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
        let home = home_room_lookup.get(class_id).copied().flatten();
        match home {
            Some(home_id) if home_id != placement_room_id => {
                score = score.saturating_add(weights.prefer_home_room);
            }
            _ => {}
        }
    }
    score
}
```

Pure, allocation-free, takes the minimum dependencies. `score_solution` builds the `home_room_lookup` map once per call (parallel to the existing `tb_lookup` and `lesson_lookup`) and sums the helper across placements.

The `score_solution` short-circuit at the top widens to include the new axis: only return zero early when *all* weights are zero.

### `solver-core::solve`

The greedy lowest-delta picker already integrates against `score_solution` via the running `state.soft_score`. No change needed beyond making the new axis flow through `score_solution`. The `solve()` active default adds `prefer_home_room: 1` alongside the other axes.

### LAHC

LAHC's delta evaluator in `lahc.rs` evaluates `delta = score_after - score_before` by re-running the relevant per-placement axes. Because LAHC's Change move only repicks the `time_block` (not the room), the home-room contribution is invariant for the moved placement; delta on this axis is always zero today. No special LAHC wiring needed; the helper exists so a future room-aware move type can plug in without rework.

### Frontend

- Regenerate OpenAPI types via `mise run fe:types`.
- Zod schema on the school-class form: `home_room_id: z.uuid().nullable().optional()`.
- School-class edit dialog: add a "Klassenraum" / "Home room" dropdown sourced from the rooms list. Sort rooms by name. Include an empty / "Kein Klassenraum" option that maps to null.
- i18n keys (en + de):
    - `schoolClasses.fields.homeRoom`: "Home room" / "Klassenraum"
    - `schoolClasses.placeholders.homeRoom`: "No home room" / "Kein Klassenraum"
- Vitest specs: dropdown renders rooms; dropdown shows current value; saving null clears the field.

## Data flow

1. User edits a school class, picks "Klasse 1a" from the home-room dropdown, saves.
2. `PATCH /api/school-classes/{id}` writes `home_room_id` to the database.
3. User triggers `POST /api/classes/{id}/schedule`.
4. `build_problem_json` includes `home_room_id` for every SchoolClass in the JSON payload.
5. `solver-py` parses the JSON; `solver-core::solve()` applies active default weights including `prefer_home_room = 1`.
6. `score_solution` builds the home-room lookup, scores each placement, sums the soft score; greedy and LAHC both pick rooms preferring the home room.
7. `Solution.placements` returns; the schedule view renders the placements; users see Klasse 1a's lessons mostly happen in "Klasse 1a".

## Error handling

- Null `home_room_id` is the no-op case: the helper returns 0 for that class, the rest of the lesson scores normally.
- A `home_room_id` pointing at a deleted Room cannot exist (the FK with `ON DELETE SET NULL` keeps the column null); no defensive code needed.
- `score_solution`'s short-circuit (zero when all weights are zero) guards against degenerate `ConstraintWeights::default()` calls.
- The new axis is a *soft* preference. Hard constraints (`RoomSubjectSuitability`, blocked times, double-booking) take precedence; greedy filters infeasible rooms before scoring.

## Testing

- **Rust unit tests** (`solver-core/src/score.rs`):
    - `home_room_penalty` returns zero when `weights.prefer_home_room == 0`.
    - Returns zero when class's `home_room_id` is None.
    - Returns zero when `placement_room_id == home_room_id`.
    - Returns `weights.prefer_home_room` when room differs.
    - Multi-class lesson with N classes scores `N * weights.prefer_home_room` when no class's home room matches.
    - Multi-class lesson where one of three classes' home room matches scores `2 * weights.prefer_home_room`.
- **Rust integration test**: a small `Problem` where a class with a home room and a class without it both place; the soft score reflects only the class with the home room set.
- **Backend unit tests**: `SchoolClassRead/Create/Update` round-trip `home_room_id`.
- **Seed solvability tests**: existing `test_demo_*_solvability.py` continue to pass; the assertion that the schedule is fully placed is unaffected by a soft constraint.
- **Frontend Vitest specs**: dropdown rendering, current-value display, null-save.
- **Bench**: `mise run bench:record` refreshes `BASELINE.md`. Expected: `prefer_home_room = 1` adds at most a few µs per fixture (one HashMap-lookup per member class per placement). Within the 20% budget per fixture; document any drift.

## Migration considerations

- Existing rows: every existing `school_classes` row stays NULL after the migration; no backfill.
- Existing seeds in dev databases: re-run `mise run db:reset && uv run klassenzeit-backend seed-grundschule` (or the dreizuegige variant) to repopulate `home_room_id` from the updated seed.
- Test database template cache: per `backend/CLAUDE.md`, schema-changing PRs must drop the template database and per-worker DBs before the first test run.

## Risks

- **Bench regression beyond 20%**: cheap to violate if `score_solution` accidentally clones the home-room lookup per placement. Mitigation: build the lookup once in `score_solution`; pass `&HashMap` to the helper. Confirmed by reading the existing per-placement subject-preference scan.
- **Multi-class lesson scoring overweights "no class matches" outcomes**: e.g. a Religion trio where none of three classes has its home room available scores `3` for that placement. This is desired (the placement *is* worse than a single-class miss), but a future calibration may revisit.
- **Frontend dropdown population**: the `useRooms()` query may not be loaded when the dialog opens. Mitigation: existing patterns for dependent dropdowns (WeekScheme dropdown on the school-class dialog) handle the loading state; reuse.

## Out of scope

Filed in `OPEN_THINGS.md` post-PR (or carried forward):

- List-page column for home-room (mirrors the deferred subject-flags column).
- Per-class subject-preference overrides (existing P2 deferral).
- Configurable `prefer_home_room` weight per class.
- LAHC room-aware move type.
