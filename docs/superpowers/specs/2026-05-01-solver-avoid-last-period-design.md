# Avoid-last-period soft constraint

**Sprint:** Realer Schulalltag + better scheduler (algorithm phase, P1).

**Closes (in `docs/superpowers/OPEN_THINGS.md`):** sprint item 8.

**ADR:** [0024: avoid-last-period soft constraint](../../adr/0024-avoid-last-period.md), added in this PR.

## Goal

Add a `Subject.avoid_last_period: bool` column and a matching `avoid_last` soft-constraint axis that penalises placements landing on the last period of the day. Symmetric to the existing `avoid_first_period` axis: a binary penalty per placement at `tb.position == max_position_for_day` for the placement's `day_of_week`. Demo seeds mark Mathematik and Deutsch as avoid-last so the demo schedule visibly steers academic Hauptfächer away from end-of-day fatigue slots. The bench `BASELINE.md` refreshes only if the new axis moves p50 wall-clock past noise (~3%).

## Non-goals

- Configurable per-subject weights for the existing or new soft axes. Sprint item 9 (P2) is the canonical home for that work; bundling P1 with P2 risks dragging P1 over a sprint boundary.
- Per-class avoid-last overrides ("Sport last period for 4c despite the global flag"). Filed under acknowledged deferrals: `school_class_subject_preferences` is the long-term shape.
- A "preference flags" column on the subjects list page. Already filed under acknowledged deferrals from PR-9c; not preempted here.
- Block-aware "last period" for `preferred_block_size > 1`. The existing avoid-first axis penalises only `tb.position == 0`; an n-block placement with `position == 0` already triggers it, and the avoid-last mirror behaves the same way (penalises only the placement on the actual last period, not the placement preceding it). Mixing block-shape semantics with the boolean axis is out of scope until users surface the gap.
- LAHC swap moves involving avoid-last placements. The Change move generalises naturally because avoid-last is a per-placement score (`subject_preference_score`), so its delta path needs no skip-guard. No new move shape ships here.

## Architecture changes

### Database (Alembic migration)

Add a non-nullable column to `subjects`, mirroring the existing pair `prefer_early_periods` and `avoid_first_period`:

```sql
ALTER TABLE subjects
    ADD COLUMN avoid_last_period BOOLEAN NOT NULL DEFAULT FALSE;
```

Server-side default `FALSE` so existing rows backfill without a data migration. Downgrade drops the column.

Migration filename: `<rev>_add_subject_avoid_last_period.py`. Single-column upgrade, single-column downgrade, modeled on `1064685e0d18_add_subject_preference_columns.py`.

### `klassenzeit_backend.db.models.subject.Subject`

Add the column next to the existing pair:

```python
avoid_last_period: Mapped[bool] = mapped_column(
    Boolean, nullable=False, server_default=text("false"), default=False
)
```

### `klassenzeit_backend.scheduling.schemas.subject`

Three sites:

- `SubjectCreate.avoid_last_period: bool = False`
- `SubjectUpdate.avoid_last_period: bool | None = None`
- `SubjectResponse.avoid_last_period: bool`

### `klassenzeit_backend.scheduling.routes.subjects`

Six sites in the existing CRUD module; mirror the existing pair line-for-line. Create accepts the flag, update applies it, response returns it, list returns it.

### `klassenzeit_backend.scheduling.solver_io.build_problem_json`

Emit `avoid_last_period` per `Subject` in the JSON payload alongside `prefer_early_periods` and `avoid_first_period`.

### `klassenzeit_backend.seed.demo_grundschule` and `demo_grundschule_dreizuegig`

`_SubjectSpec` gains `avoid_last_period: bool = False`. Mark Mathematik (`MA`) and Deutsch (`DE`) as `avoid_last_period=True`. The zweizuegige seed reuses the same `_SubjectSpec` table via the dreizuegige module's spec list, so all three demo seeds inherit the flag in one place.

The companion bench fixture `solver/solver-core/benches/solver_fixtures.rs` mirrors the flag: indices for Mathematik and Deutsch get `avoid_last_period: true` (literal Subject construction; uses the same indexing pattern as the existing `avoid_first_period: i == 7` line for Sport).

### `solver-core::types`

Extend `Subject`:

```rust
pub struct Subject {
    pub id: SubjectId,
    pub prefer_early_periods: bool,
    pub avoid_first_period: bool,
    /// When true, scoring adds `weights.avoid_last_period` per placement at
    /// the last `position` of the placement's `day_of_week`.
    #[serde(default)]
    pub avoid_last_period: bool,
}
```

`#[serde(default)]` keeps the wire format additive: existing JSON callers (and the bench fixtures' literal constructors that don't yet specify the field) stay valid through the staged commit.

Extend `ConstraintWeights`:

```rust
pub struct ConstraintWeights {
    pub class_gap: u32,
    pub teacher_gap: u32,
    pub prefer_early_period: u32,
    pub avoid_first_period: u32,
    pub prefer_home_room: u32,
    /// Constant penalty per placement of an `avoid_last_period` subject at
    /// the last `position` of its `day_of_week`. Active default: 1.
    pub avoid_last_period: u32,
}
```

### `solver-core::score`

`subject_preference_score` gains a `max_position_for_day: u8` parameter:

```rust
pub(crate) fn subject_preference_score(
    subject: &Subject,
    tb: &TimeBlock,
    max_position_for_day: u8,
    weights: &ConstraintWeights,
) -> u32 {
    let mut score = 0u32;
    if subject.prefer_early_periods {
        score = score
            .saturating_add(u32::from(tb.position).saturating_mul(weights.prefer_early_period));
    }
    if subject.avoid_first_period && tb.position == 0 {
        score = score.saturating_add(weights.avoid_first_period);
    }
    if subject.avoid_last_period && tb.position == max_position_for_day {
        score = score.saturating_add(weights.avoid_last_period);
    }
    score
}
```

Caller side (`score_solution` and the LAHC delta path) builds `max_position_per_day: HashMap<u8, u8>` once, then passes the per-placement value:

```rust
let max_position_per_day: HashMap<u8, u8> = problem
    .time_blocks
    .iter()
    .fold(HashMap::new(), |mut acc, tb| {
        acc.entry(tb.day_of_week)
            .and_modify(|m| *m = (*m).max(tb.position))
            .or_insert(tb.position);
        acc
    });
```

The early-out at the top of `score_solution` adds `&& weights.avoid_last_period == 0` so the all-zero shortcut still skips work when no axis is active.

### `solver-core::lahc` (Change-move delta path)

LAHC's Change move computes the soft-score delta by recomputing `subject_preference_score` for the old and new placement. Thread `max_position_per_day` into the LAHC outer scope (it's `&Problem`-derived and stable across iterations) and pass the per-placement value into both calls. No new RNG draws; the determinism property test's two-`random_range`-per-iteration invariant holds.

No skip-guard for avoid-last placements: the axis is per-placement scoring, not a placement-shape constraint like Doppelstunden or lesson-groups.

### `solver-core::solve` and `json` (active defaults)

`solve()`'s `SolveConfig::default()` and `solve_json`'s implicit defaults both carry the active-default `ConstraintWeights`. Add `avoid_last_period: 1` to both blocks. Five → six axes, all weight 1.

### `solver-py` test fixtures

Three fixtures across `solver/solver-py/tests/test_bindings.py` and `test_multi_class.py` build minimal Subject JSON dicts. Add `"avoid_last_period": False` to each. No new test cases at the binding layer; algorithm coverage lives in solver-core.

### Frontend (`frontend/src/...` subject schema + edit dialog)

Mirror the existing avoid_first_period plumbing:

- Zod schema: `avoid_last_period: z.boolean().default(false)` in the create + update schemas.
- Form: a third checkbox in the subject edit dialog beneath "Avoid first period".
- i18n keys: `subjects.form.avoidLastPeriod.label` and `subjects.form.avoidLastPeriod.description`, in both `en` and `de`. German copy: "Letzte Stunde meiden".
- OpenAPI types regenerate via `mise run fe:types` after the backend change ships; the regenerated file is committed.
- Vitest spec: extend the existing subject-form test to cover the new checkbox round-trip (label visible, default unchecked, toggling sends `avoid_last_period: true` in the mutation payload).

The subjects list table is unchanged; the deferred "preference flags column" remains deferred.

## Test plan

**solver-core unit tests** (in `score.rs`):

- `subject_preference_score_includes_avoid_last_at_max_position`: for a Subject with `avoid_last_period: true`, weight 7, max position 4, score is 7 at position 4 and 0 at positions 0..3.
- `subject_preference_score_excludes_avoid_last_when_flag_off`: same fixture with `avoid_last_period: false` returns 0 at position 4.
- `score_solution_includes_avoid_last_only_at_max_day_position`: a fixture with two days where day 0 maxes at position 3 and day 1 maxes at position 5, place the same subject at (day 0, pos 3), (day 0, pos 1), (day 1, pos 5), (day 1, pos 3); avoid-last fires twice.
- Add an `avoid_last_period: false` line to every existing literal `Subject` in `score.rs` tests, `validate.rs` tests, `ordering.rs` tests, and bench fixtures.

**solver-core integration tests** (in `solve.rs` or a sibling module):

- `greedy_avoids_max_position_for_avoid_last_subject_when_alternative_exists`: mirrors the existing `greedy_avoids_position_zero_for_avoid_first_subject_when_alternative_exists`. Two-day, four-position fixture; one subject flagged avoid-last; assert greedy lands the placement at a non-max position.

**LAHC property test:** unchanged. The determinism property test (`tests/lahc_property.rs`) does not need a new RNG-budget assertion because no new draws are added.

**Backend pytest:**

- `backend/tests/scheduling/test_subjects.py`: extend the existing avoid_first PATCH/POST test to assert avoid_last round-trips identically.
- `backend/tests/scheduling/test_solver_io.py`: extend the existing emission test to assert `avoid_last_period` appears in the JSON payload.
- `backend/tests/seed/test_demo_grundschule_*.py`: assert Mathematik and Deutsch carry `avoid_last_period=True` after the seed runs (mirrors existing avoid_first assertions if any; add if absent).

**Frontend Vitest:**

- `frontend/src/.../subject-form.test.tsx`: assert the new checkbox renders, defaults unchecked, and the form submit payload includes `avoid_last_period`.

**E2E:** no new Playwright spec. The existing schedule smoke spec exercises greedy placement; adding visual assertions against a soft-score axis is out of scope.

**Bench:** run `mise run bench` after the algorithm change. If p50 drift on any fixture exceeds ~3%, refresh `BASELINE.md` via `mise run bench:record` and cite the diff in the PR body. Otherwise, no refresh.

## Migration notes

- JSON wire format is additive (`#[serde(default)]` on the new Rust field, `avoid_last_period: bool = False` on the Pydantic side). Older clients sending JSON payloads without the field continue to work; the field defaults to false. This covers solver-py callers feeding pre-existing `Problem` JSON.
- Rust struct-literal call sites (bench fixtures in `solver_fixtures.rs`, in-crate test fixtures in `score.rs` / `solve.rs` / `validate.rs` / `ordering.rs` / `lahc.rs` and friends, integration tests under `solver-core/tests/`) all need the new field added in the same commit as the type extension; `#[serde(default)]` is for deserialization and does not unblock literal struct construction. Per the home-room PR's pattern, helper plus first caller land atomically.
- Existing `subjects` rows backfill via the server-side `DEFAULT FALSE`; no data migration needed.

## Commit split

1. **`feat(solver-core): avoid-last-period soft-constraint axis`.** Subject and ConstraintWeights fields, score.rs change (signature update, new tests, bench-fixture threading), active defaults in `json.rs` and `solve.rs`, LAHC delta path, validate.rs / ordering.rs test fixtures threaded.
2. **`feat(backend): avoid-last-period subject column + API`.** Alembic migration, ORM column, Pydantic schemas, route plumbing, solver_io emission, backend pytest. solver-py test fixtures updated.
3. **`feat(frontend): avoid-last-period checkbox in subject edit dialog`.** Zod schema, form field, i18n keys (en + de), regenerated OpenAPI types, Vitest spec.
4. **`feat: mark Mathe/Deutsch avoid-last in demo seeds + ADR 0024`.** `_SubjectSpec` updates in both demo seed files, ADR 0024 written, OPEN_THINGS sprint item 8 marked shipped, BASELINE.md refreshed if Q9 (in the brainstorm) says so.

Each commit must independently pass `mise run lint` and the relevant `mise run test:*` slice.
