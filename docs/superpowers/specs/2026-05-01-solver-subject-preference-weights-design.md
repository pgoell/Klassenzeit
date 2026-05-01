# Per-Subject preference weights

**Sprint:** Realer Schulalltag + better scheduler (drop tier, P2).

**Closes (in `docs/superpowers/OPEN_THINGS.md`):** sprint item 9.

**ADR:** [0025: Per-Subject preference weights](../../adr/0025-subject-preference-weights.md), added in this PR.

## Goal

Replace the three boolean preference flags on `Subject` (`prefer_early_periods`, `avoid_first_period`, `avoid_last_period`) with `u32` weights so a school can express relative strength ("Mathematik is more strongly early than Deutsch"), not just on/off. The frontend subject edit dialog swaps each checkbox for a small number input bounded `0-10`. The Rust per-placement score multiplies the per-Subject weight against the existing per-axis global weight on `ConstraintWeights`; behaviour at `subject.<axis> = 1` is byte-identical to today's `subject.<axis> = true`, so existing seeds and persisted schedules carry over without observable drift.

The rename `prefer_early_periods` (plural bool) becomes `prefer_early_period` (singular, u32) so the Subject-side and `ConstraintWeights`-side identifiers match per axis. The other two field names (`avoid_first_period`, `avoid_last_period`) keep their names and only flip type.

## Non-goals

- Per-class subject preference overrides. Filed under acknowledged deferrals as `school_class_subject_preferences`. The global per-Subject weight + the Jahrgang-level home-room mapping cover the prototype's pattern; revisit when a school surfaces a per-class deviation.
- A "preference flags" column on the subjects list page. Already filed under acknowledged deferrals from PR-9c; not preempted here.
- Promoting `ConstraintWeights` per-axis fields to user-editable settings. They stay operator-only, edited via Rust code or seed today; an admin-facing surface is out of scope.
- Custom scoring functions per Subject (e.g., quadratic in position). The shape stays linear-in-position for `prefer_early_period` and binary-at-edge for the two `avoid` axes; non-linear shapes need different telemetry and tuning, deferred until benches surface a real gap.
- LAHC move-set changes. The score formula is still `O(1)` per placement and stays additive across axes, so the existing Change move's delta path generalises automatically. No new move shape ships here.

## Architecture changes

### Database (Alembic migration)

Flip the three boolean columns on `subjects` to `INTEGER NOT NULL DEFAULT 0`, backfilling each row from its existing boolean:

```sql
ALTER TABLE subjects
    ALTER COLUMN prefer_early_periods
        TYPE INTEGER
        USING (CASE WHEN prefer_early_periods THEN 1 ELSE 0 END),
    ALTER COLUMN prefer_early_periods SET DEFAULT 0,
    ALTER COLUMN prefer_early_periods SET NOT NULL;
```

Repeat for `avoid_first_period` and `avoid_last_period`. The same migration also renames `prefer_early_periods` to `prefer_early_period` (singular) per Q2:

```sql
ALTER TABLE subjects RENAME COLUMN prefer_early_periods TO prefer_early_period;
```

The downgrade reverses both: drop default + NOT NULL + cast `INTEGER` back to `BOOLEAN` via `USING (<col> <> 0)` (lossy, any value `>= 1` rounds back to `TRUE`), then rename `prefer_early_period` back to `prefer_early_periods`.

Migration filename: `<rev>_subject_preference_weights.py`. Single migration touching all three columns plus the rename, modelled on the previous shape (`<prev_rev>_add_subject_avoid_last_period.py`).

### `klassenzeit_backend.db.models.subject.Subject`

Three columns flip type:

```python
prefer_early_period: Mapped[int] = mapped_column(
    Integer, nullable=False, server_default=text("0"), default=0
)
avoid_first_period: Mapped[int] = mapped_column(
    Integer, nullable=False, server_default=text("0"), default=0
)
avoid_last_period: Mapped[int] = mapped_column(
    Integer, nullable=False, server_default=text("0"), default=0
)
```

Each column is `Integer` (Postgres `INTEGER`, signed 32-bit; range covers our `0-10` cap with massive headroom). `nullable=False` and the default `0` mirror the existing pattern.

### `klassenzeit_backend.scheduling.schemas.subject`

Three sites per schema, all gaining a `Field(ge=0, le=10)` constraint:

```python
class SubjectCreate(BaseModel):
    prefer_early_period: int = Field(0, ge=0, le=10)
    avoid_first_period: int = Field(0, ge=0, le=10)
    avoid_last_period: int = Field(0, ge=0, le=10)
```

`SubjectUpdate` mirrors with `int | None = None`. `SubjectResponse` declares plain `int`. The `Field(ge=0, le=10)` raises a 422 with a typed `min`/`max` error so the frontend can map it to a form field error.

### `klassenzeit_backend.scheduling.routes.subjects`

Six sites in the CRUD module; rename and pass the integer through, line-for-line. Create accepts the integer, update applies it, response returns it, list returns it.

### `klassenzeit_backend.scheduling.solver_io.build_problem_json`

Emit each as an integer:

```python
"prefer_early_period": s.prefer_early_period,
"avoid_first_period": s.avoid_first_period,
"avoid_last_period": s.avoid_last_period,
```

The wire format is `{ ..., "prefer_early_period": <int>, "avoid_first_period": <int>, "avoid_last_period": <int> }` per Subject; missing fields default to `0` on the Rust side via `#[serde(default)]`, preserving forward-compatibility for callers that still emit only a subset.

### `klassenzeit_backend.seed.demo_grundschule`, `demo_grundschule_zweizuegig`, `demo_grundschule_dreizuegig`

The shared `_SubjectSpec` dataclass:

```python
@dataclasses.dataclass(frozen=True)
class _SubjectSpec:
    name: str
    short_code: str
    color: str
    prefer_early_period: int = 0
    avoid_first_period: int = 0
    avoid_last_period: int = 0
```

Existing literal flags rewrite mechanically:

- `_SubjectSpec("Deutsch", "D", "chart-1", prefer_early_periods=True, avoid_last_period=True)` becomes `_SubjectSpec("Deutsch", "D", "chart-1", prefer_early_period=1, avoid_last_period=1)`.
- `_SubjectSpec("Mathematik", "M", "chart-2", prefer_early_periods=True, avoid_last_period=True)` becomes `_SubjectSpec("Mathematik", "M", "chart-2", prefer_early_period=1, avoid_last_period=1)`.
- `_SubjectSpec("Sport", "SP", "chart-4", avoid_first_period=True)` becomes `_SubjectSpec("Sport", "SP", "chart-4", avoid_first_period=1)`.

The construct-Subject loops in each seed flip the field-name parameters from `prefer_early_periods=spec.prefer_early_periods` to `prefer_early_period=spec.prefer_early_period`.

### `solver-core::types::Subject`

```rust
pub struct Subject {
    pub id: SubjectId,
    /// Per-Subject weight applied to the early-period axis. Per-placement
    /// penalty contributes `tb.position * weights.prefer_early_period *
    /// subject.prefer_early_period` (saturating). Zero disables this axis for
    /// the subject; the active default for non-flagged subjects is zero.
    #[serde(default)]
    pub prefer_early_period: u32,
    /// Per-Subject weight applied at `tb.position == 0`. Zero disables.
    #[serde(default)]
    pub avoid_first_period: u32,
    /// Per-Subject weight applied at `tb.position == max_position_for_day`
    /// for the placement's day. Zero disables.
    #[serde(default)]
    pub avoid_last_period: u32,
}
```

`#[serde(default)]` keeps the serde shape forward-compatible with callers that omit a field (the field reads as `0`, identical to today's `false`). The rename `prefer_early_periods → prefer_early_period` is the only field-name churn; the other two stay.

### `solver-core::types::ConstraintWeights`

Unchanged. The `prefer_early_period`, `avoid_first_period`, `avoid_last_period` fields stay `u32` with active default `1` per ADR 0017 / 0024. The score formula's two-multiplier shape (per-Subject `*` per-axis global) is preserved.

### `solver-core::score::subject_preference_score`

Per-axis branches replace `if subject.<flag> { weights.<axis> * factor }` with `weights.<axis>.saturating_mul(subject.<axis>).saturating_mul(factor)`:

```rust
pub(crate) fn subject_preference_score(
    subject: &Subject,
    tb: &TimeBlock,
    max_position_for_day: u8,
    weights: &ConstraintWeights,
) -> u32 {
    let mut score: u32 = 0;
    if subject.prefer_early_period > 0 {
        score = score.saturating_add(
            weights
                .prefer_early_period
                .saturating_mul(subject.prefer_early_period)
                .saturating_mul(u32::from(tb.position)),
        );
    }
    if subject.avoid_first_period > 0 && tb.position == 0 {
        score = score.saturating_add(
            weights
                .avoid_first_period
                .saturating_mul(subject.avoid_first_period),
        );
    }
    if subject.avoid_last_period > 0 && tb.position == max_position_for_day {
        score = score.saturating_add(
            weights
                .avoid_last_period
                .saturating_mul(subject.avoid_last_period),
        );
    }
    score
}
```

Each branch's `subject.<axis> > 0` guard short-circuits when the per-Subject weight disables the axis. The per-axis global multiplier no longer needs its own guard inside the function: `subject.<axis> > 0 && weights.<axis> == 0` produces a zero `saturating_mul` (cheap and correct). Function signature matches the existing `(subject, tb, max_position_for_day, weights)` parameter order; only the per-axis branch bodies change.

### `solver-core::score::score_solution`

The whole-solution early-exit at the top of `score_solution` (`if weights.class_gap == 0 && weights.teacher_gap == 0 && weights.prefer_early_period == 0 && weights.avoid_first_period == 0 && weights.prefer_home_room == 0 && weights.avoid_last_period == 0 { return 0; }`) stays unchanged. It is still correct: when every per-axis global is zero, every per-axis term contributes zero regardless of per-Subject weight. The hoist points (subject-by-id, lesson-by-id, max-position-per-day, home-room-by-class) build once per call exactly as today.

### `solver-core` test surface

- Update existing `subject_preference_score_*` unit tests to construct Subjects with `u32` values rather than booleans (`prefer_early_periods: true` becomes `prefer_early_period: 1`). Behaviour assertions at `value = 1` are byte-identical.
- Add `subject_preference_score_scales_linearly_with_subject_weight`: assert that doubling `subject.prefer_early_period` doubles the per-placement contribution at the same `tb.position`. Mirror cases for `avoid_first_period` and `avoid_last_period`.
- Update `solver-core/tests/score_property.rs`: the proptest generator types flip from `bool` to `u32` in `[0, 10]`. The property "score is non-negative; doubling weights doubles output up to saturation" generalises naturally.
- Update bench fixtures in `solver-core/benches/solver_fixtures.rs`: literal `true / false` values in subject construction flip to `1 / 0`.

### Backend test surface

- Update `backend/tests/scheduling/test_subjects_routes.py` (or the existing route tests by whatever filename): existing assertions on `True / False` flip to `1 / 0`. Add a new `test_create_subject_rejects_out_of_range_weight` that asserts `prefer_early_period=11` returns 422 with the typed Pydantic error mapped onto the field; mirror cases for `avoid_first_period` and `avoid_last_period`.
- Demo seed solvability tests (`backend/tests/seed/test_demo_grundschule_solvability.py` and the zweizuegig + dreizuegig variants) keep their existing assertions; soft scores should be unchanged.

### Frontend Zod schema

`frontend/src/features/subjects/schema.ts`:

```ts
export const SubjectFormSchema = z.object({
  // ...
  prefer_early_period: z.number().int().min(0).max(10),
  avoid_first_period: z.number().int().min(0).max(10),
  avoid_last_period: z.number().int().min(0).max(10),
});
```

The form default value flips from `false` to `0`. The form field name `prefer_early_periods` renames to `prefer_early_period` consistent with the Pydantic / Rust rename.

### Frontend dialog

`frontend/src/features/subjects/subjects-dialogs.tsx`:

Each of the three checkbox rows becomes a small number-input row. The `Checkbox` component is replaced with `<Input type="number" min={0} max={10} step={1} />`; the `FormLabel` text adjusts to read "weight" rather than the on/off framing, and the `FormDescription` text updates per locale ("Higher values = stronger preference; 0 disables this axis for the subject"). The form submit handler maps `Number(field.value)` into the request body at the existing call site.

### Frontend i18n

`frontend/src/i18n/locales/{en,de}.json`:

- Rename key `subjects.fields.preferEarlyPeriods` to `subjects.fields.preferEarlyPeriod` (matches the field rename).
- Update each axis's `.help` text per locale to describe "weight 0-10, 0 disables" rather than "on / off".

EN snippets:

```json
"preferEarlyPeriod": {
  "label": "Prefer early periods (weight)",
  "help": "Higher values steer this subject to earlier periods more strongly. 0 disables this axis."
},
"avoidFirstPeriod": {
  "label": "Avoid first period (weight)",
  "help": "Higher values penalise placement in the first period more strongly. 0 disables this axis."
},
"avoidLastPeriod": {
  "label": "Avoid last period (weight)",
  "help": "Higher values penalise placement in the last period more strongly. 0 disables this axis."
}
```

DE mirrors in the same structure.

### Frontend test surface

`frontend/src/features/subjects/subjects-dialogs.test.tsx` (or whatever filename the existing tests use): swap the checkbox interaction for number-input interactions. New assertion: typing `3` in the prefer-early-period input and submitting fires `onCreate` / `onUpdate` with `prefer_early_period: 3` in the body.

Regenerate `frontend/src/lib/api-types.ts` via `mise run fe:types` after the backend Pydantic flip; the boolean fields become `number` fields.

### Demo seed Rust mirror (bench fixtures)

`solver/solver-core/benches/solver_fixtures.rs` mirrors the same field renames + type flips. Any literal `prefer_early_periods: true` becomes `prefer_early_period: 1`; same for the other two axes.

### `solver-py`

`grep -n "prefer_early_periods\|avoid_first_period\|avoid_last_period" solver/solver-py/` is run during the solver-core commit; if hits exist (PyO3 contract tests constructing literal Subject JSON), they flip in the same commit. Stub file `klassenzeit_solver/__init__.pyi` carries function signatures only and does not need a stub bump.

## Migration story

The Alembic migration backfills booleans into integers via `CASE WHEN <bool> THEN 1 ELSE 0 END`, preserving every existing schedule's soft score exactly. Down-migration rounds non-zero integers back to `TRUE`, lossy by design (a pre-down-migration value of `5` becomes `TRUE` after downgrade then `1` if the migration is re-applied). The lossy down-path is acceptable because the prototype runs production-and-staging on the latest migration and downgrade is a development concern only.

Existing demo seeds rewrite mechanically: `prefer_early_periods=True` becomes `prefer_early_period=1`, etc. No customer data exists outside the seed scripts, so the migration is purely a development concern; the staging deploy auto-applies it.

## Test plan

- `mise run lint && mise run test` green at every commit boundary.
- New solver-core unit test `subject_preference_score_scales_linearly_with_subject_weight` (and mirrors for `avoid_first_period`, `avoid_last_period`) passes.
- `solver-core/tests/score_property.rs` proptest passes with the broader `u32` generator.
- New backend test `test_create_subject_rejects_out_of_range_weight` (and mirrors) returns 422 on out-of-range and 200 on a valid non-binary value (e.g. `3`).
- Updated frontend Vitest spec types `3` in the prefer-early-period input and asserts the request body carries `prefer_early_period: 3`.
- Demo seed solvability tests stay green without modification.
- `mise run bench` shows soft score identical to baseline on all three fixtures (greedy and LAHC); p50 wall-clock within 3% of baseline; if outside, refresh `BASELINE.md` and cite the diff in the PR body.
- Playwright smoke spec `frontend/e2e/flows/grundschule-smoke.spec.ts` passes (no behaviour change visible to the smoke flow; this is regression coverage).

## Commit split

Four commits along the dependency graph (per Q6):

1. `feat(solver-core): per-subject preference weights u32` — covers `solver-core::types`, `solver-core::score`, unit tests, proptest, bench fixture, ADR 0025, plus any `solver-py` PyO3 test fixture updates that still construct boolean literals.
2. `feat(backend): subject preference weights u32 + Alembic migration` — covers ORM, Pydantic, routes, solver_io JSON emission, route tests.
3. `feat(frontend): subject preference weight inputs replace checkboxes` — covers Zod, dialog, i18n keys, regenerated `api-types.ts`, Vitest spec.
4. `chore(seed): rewrite demo seeds with int preference weights` — covers `_SubjectSpec` and the three demo seed modules.

Each commit ends with the relevant `mise run lint` + targeted test green; the pre-push runs the full suite. If any commit drags lint failures across crates due to the rename, fold the necessary cross-crate updates into that commit rather than splitting further; the goal is one logical step per commit, not strictly one crate per commit.

## Risks

- **Alembic type-flip on a non-empty column.** Postgres requires a `USING` clause to coerce; the spec includes it. If a future migration runs against a heavily-loaded table, the `ALTER COLUMN ... TYPE` rewrites the column; for the prototype the table is small and the rewrite cost is trivial.
- **Frontend regenerated types out of sync with manual edits.** `mise run fe:types` is the only safe way to regenerate `api-types.ts`; any hand edit would be undone. The Frontend commit runs the regeneration step explicitly.
- **The Subject-side rename leaks into PR titles or commit scopes.** Convention is `feat(solver-core)`, `feat(backend)`, `feat(frontend)`, `chore(seed)`; no scope rename is needed for this work. Verify the `cog verify` step on each commit.
- **Demo seed soft score drifts.** Possible if any seed accidentally converts a `True` to `0` rather than `1`. The bench passes at identical soft score is the canonical check; if drift appears, audit the seed rewrite first.
- **Sprint scope.** Item 9 is P2; item 10 (block-aware FFD eligibility) remains conditional on a placement-rate regression. If item 9 closes cleanly within this PR, the sprint has reached its drop-tier exit and the next user prompt is the sprint-close question.

## Documentation updates

- ADR 0025 in `docs/adr/0025-subject-preference-weights.md`. Index in `docs/adr/README.md`.
- `docs/superpowers/OPEN_THINGS.md`: mark item 9 ✅ shipped with the PR number; item 10 stays as the conditional drop-tier remainder.
- Auto-memory `project_roadmap_status.md` description and body refresh to "sprint item 9 closed; item 10 conditional", with the "How to apply" line shifting to the sprint-close prompt.
- No `docs/architecture/overview.md` update needed (the soft-constraint surface is described at the axis-list level, not the per-Subject knob level).
