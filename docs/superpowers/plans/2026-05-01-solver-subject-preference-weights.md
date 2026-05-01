# Per-Subject preference weights Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `Subject.prefer_early_periods: bool`, `Subject.avoid_first_period: bool`, `Subject.avoid_last_period: bool` with `u32` weights so a school can express relative strength ("Mathematik more strongly early than Deutsch"); cap inputs `0-10` at the schema layer; rename `prefer_early_periods → prefer_early_period`; preserve existing soft-score behaviour at value `1`.

**Architecture:** Per-placement penalty becomes `subject.<axis> (u32) * weights.<axis> (u32) * factor`, where `factor` is `tb.position` for `prefer_early_period` and `1` (gated by position) for `avoid_first_period` and `avoid_last_period`. The per-axis global multiplier in `ConstraintWeights` stays as-is. The Alembic migration backfills `False → 0, True → 1` (lossy on downgrade). Frontend swaps three checkboxes for three small number inputs. Demo seeds rewrite literal flags from `True` to `1`. ADR 0025 records the decision.

**Tech Stack:** Rust (`solver-core`, `solver-py`, criterion bench, proptest), Python (FastAPI, SQLAlchemy async, Alembic, Pydantic v2, pytest), TypeScript (React 19, react-hook-form, Zod, Vitest, react-i18next).

---

## File Structure

**Solver-core (Rust):**

- Modify: `solver/solver-core/src/types.rs:122-144` — `Subject` struct: rename `prefer_early_periods → prefer_early_period`; flip all three from `bool` to `u32`; keep `#[serde(default)]` on each.
- Modify: `solver/solver-core/src/score.rs:185-210` — `subject_preference_score`: replace boolean branches with `subject.<axis> > 0` guards and `saturating_mul` against the per-Subject weight.
- Modify: `solver/solver-core/src/score.rs:430+` — existing unit tests: `Subject` literals flip from booleans to integers (`true → 1, false → 0`).
- Add (inside the existing `#[cfg(test)] mod tests` block in `score.rs`): three unit tests asserting linearity in the per-Subject weight (one per axis).
- Modify: `solver/solver-core/tests/score_property.rs` — proptest generator types flip from `bool` to `u32` in `[0, 10]`; `Subject` literals across helpers update.
- Modify: `solver/solver-core/benches/solver_fixtures.rs:80-100, 195-205, 360-370` — three fixtures (grundschule, zweizuegig, dreizuegig); bool literals flip to integer expressions.

**Solver-py (Rust + Python):**

- Modify: `solver/solver-py/tests/test_bindings.py` — fixture JSON: bools to integers.
- Modify: `solver/solver-py/tests/test_multi_class.py` — fixture JSON: bools to integers.

**Backend (Python):**

- Add: `backend/migrations/versions/<rev>_subject_preference_weights.py` — Alembic migration (rename `prefer_early_periods → prefer_early_period`; cast all three columns `BOOLEAN → INTEGER NOT NULL DEFAULT 0`).
- Modify: `backend/src/klassenzeit_backend/db/models/subject.py:21-29` — three columns flip from `Mapped[bool]` to `Mapped[int]`.
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/subject.py:15-42` — three Pydantic schemas (`SubjectCreate`, `SubjectUpdate`, `SubjectResponse`): rename + retype + add `Field(ge=0, le=10)` constraint.
- Modify: `backend/src/klassenzeit_backend/scheduling/routes/subjects.py:60-210` — six round-trip sites (create accept, response build × 4, update apply).
- Modify: `backend/src/klassenzeit_backend/scheduling/solver_io.py:259-261` — JSON emission per Subject: rename + integer pass-through.
- Modify: `backend/tests/scheduling/test_subjects.py` — existing assertions adapt; add `test_create_subject_rejects_out_of_range_weight` (and `_avoid_first`, `_avoid_last`).

**Frontend (TypeScript):**

- Regenerate: `frontend/src/lib/api-types.ts` via `mise run fe:types` after backend Pydantic flip.
- Modify: `frontend/src/features/subjects/schema.ts:5-15` — three Zod fields flip from `z.boolean()` to `z.number().int().min(0).max(10)`; rename `prefer_early_periods → prefer_early_period`.
- Modify: `frontend/src/features/subjects/subjects-dialogs.tsx:50-200` — three checkbox rows become three small number-input rows; default values flip from `false` to `0`; field name rename.
- Modify: `frontend/src/i18n/locales/en.json:140-160` and `frontend/src/i18n/locales/de.json:140-160` — rename one key (`preferEarlyPeriods → preferEarlyPeriod`); update `.help` text per axis.
- Add: `frontend/src/features/subjects/subjects-dialogs.test.tsx` — Vitest spec covering "type 3 in the prefer-early-period input → request body carries `prefer_early_period: 3`".

**Seeds (Python):**

- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule.py:53-73, 198-205` — `_SubjectSpec` flips bool fields to int; literal `True` flags rewrite to `1`; subject construction loop updates field-name parameter.
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py:248-253` — subject construction loop field-name parameter update (the `_SubjectSpec` source is the shared import).
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py:384-389` — same shape.

**Docs:**

- Add: `docs/adr/0025-subject-preference-weights.md` — ADR describing the bool-to-u32 flip, the cap, the rename, the migration mapping.
- Modify: `docs/adr/README.md` — index entry for ADR 0025.
- Modify: `docs/superpowers/OPEN_THINGS.md:24-27` — mark sprint item 9 ✅ shipped with PR number; close the description.

---

## Task 1: solver-core types + score + property test + bench fixtures + ADR 0025 + solver-py fixtures

**Files:**

- Modify: `solver/solver-core/src/types.rs`
- Modify: `solver/solver-core/src/score.rs`
- Modify: `solver/solver-core/tests/score_property.rs`
- Modify: `solver/solver-core/benches/solver_fixtures.rs`
- Modify: `solver/solver-py/tests/test_bindings.py`
- Modify: `solver/solver-py/tests/test_multi_class.py`
- Add: `docs/adr/0025-subject-preference-weights.md`
- Modify: `docs/adr/README.md`

- [ ] **Step 1.1: Write failing unit test for linearity (prefer-early)**

Append inside the existing `#[cfg(test)] mod tests` block in `solver/solver-core/src/score.rs` (after the existing `subject_preference_score_*` unit tests):

```rust
#[test]
fn subject_preference_score_scales_linearly_with_prefer_early_subject_weight() {
    let weights = ConstraintWeights {
        prefer_early_period: 2,
        ..ConstraintWeights::default()
    };
    let tb = TimeBlock {
        id: TimeBlockId(uuid::Uuid::nil()),
        day_of_week: 0,
        position: 3,
    };
    let mk = |w: u32| Subject {
        id: SubjectId(uuid::Uuid::nil()),
        prefer_early_period: w,
        avoid_first_period: 0,
        avoid_last_period: 0,
    };
    // single-weight = 2 * 3 * 1 = 6, double-weight = 2 * 3 * 2 = 12
    assert_eq!(subject_preference_score(&mk(1), &tb, 6, &weights), 6);
    assert_eq!(subject_preference_score(&mk(2), &tb, 6, &weights), 12);
    assert_eq!(subject_preference_score(&mk(0), &tb, 6, &weights), 0);
}

#[test]
fn subject_preference_score_scales_linearly_with_avoid_first_subject_weight() {
    let weights = ConstraintWeights {
        avoid_first_period: 5,
        ..ConstraintWeights::default()
    };
    let tb = TimeBlock {
        id: TimeBlockId(uuid::Uuid::nil()),
        day_of_week: 0,
        position: 0,
    };
    let mk = |w: u32| Subject {
        id: SubjectId(uuid::Uuid::nil()),
        prefer_early_period: 0,
        avoid_first_period: w,
        avoid_last_period: 0,
    };
    assert_eq!(subject_preference_score(&mk(1), &tb, 6, &weights), 5);
    assert_eq!(subject_preference_score(&mk(3), &tb, 6, &weights), 15);
    assert_eq!(subject_preference_score(&mk(0), &tb, 6, &weights), 0);
}

#[test]
fn subject_preference_score_scales_linearly_with_avoid_last_subject_weight() {
    let weights = ConstraintWeights {
        avoid_last_period: 4,
        ..ConstraintWeights::default()
    };
    let tb = TimeBlock {
        id: TimeBlockId(uuid::Uuid::nil()),
        day_of_week: 0,
        position: 6,
    };
    let mk = |w: u32| Subject {
        id: SubjectId(uuid::Uuid::nil()),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: w,
    };
    assert_eq!(subject_preference_score(&mk(1), &tb, 6, &weights), 4);
    assert_eq!(subject_preference_score(&mk(2), &tb, 6, &weights), 8);
    assert_eq!(subject_preference_score(&mk(0), &tb, 6, &weights), 0);
}
```

- [ ] **Step 1.2: Run new tests to verify they fail to compile**

Run: `cargo nextest run -p solver-core subject_preference_score_scales_linearly`

Expected: compilation error on field name `prefer_early_period` (does not exist on `Subject` yet) or `avoid_first_period` having type `bool` not `u32`. (The exact error text depends on which field rustc reports first; "no field `prefer_early_period`" is the most common.)

- [ ] **Step 1.3: Flip `Subject` in `solver/solver-core/src/types.rs`**

Replace lines 122-144 with:

```rust
/// A subject (the thing being taught in a lesson).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    /// Stable identifier for this subject.
    pub id: SubjectId,
    /// Per-Subject weight applied to the early-period axis. Scoring adds
    /// `tb.position * weights.prefer_early_period * subject.prefer_early_period`
    /// per placement (saturating). Zero disables this axis for the subject.
    /// Wire format is additive: callers omitting the field deserialise to 0.
    #[serde(default)]
    pub prefer_early_period: u32,
    /// Per-Subject weight applied at `tb.position == 0`. Scoring adds
    /// `weights.avoid_first_period * subject.avoid_first_period` per placement
    /// at the first period of any day (saturating). Zero disables this axis.
    /// Wire format is additive: callers omitting the field deserialise to 0.
    #[serde(default)]
    pub avoid_first_period: u32,
    /// Per-Subject weight applied at `tb.position == max_position_for_day`.
    /// Scoring adds `weights.avoid_last_period * subject.avoid_last_period`
    /// per placement at the last period of any day (saturating). Zero
    /// disables this axis. Wire format is additive: callers omitting the
    /// field deserialise to 0.
    #[serde(default)]
    pub avoid_last_period: u32,
}
```

- [ ] **Step 1.4: Flip `subject_preference_score` in `solver/solver-core/src/score.rs`**

Replace the body of `subject_preference_score` (currently lines ~192-210) with:

```rust
pub(crate) fn subject_preference_score(
    subject: &crate::types::Subject,
    tb: &TimeBlock,
    max_position_for_day: u8,
    weights: &ConstraintWeights,
) -> u32 {
    let mut score = 0u32;
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

Also update the function's rustdoc comment (the `///` block immediately above) to describe the multiplication semantics: replace "(linear) when the subject's `prefer_early_periods` flag is set" with "(linear, weighted by `subject.prefer_early_period`)" and remove every mention of "flag is set".

- [ ] **Step 1.5: Update existing `subject_preference_score_*` unit tests in `solver/solver-core/src/score.rs`**

Find every existing `Subject { ... prefer_early_periods: bool, avoid_first_period: bool, avoid_last_period: bool ... }` literal in the test module (around lines 270-650) and rewrite each:

- `prefer_early_periods: false` → `prefer_early_period: 0`
- `prefer_early_periods: true` → `prefer_early_period: 1`
- `avoid_first_period: false` → `avoid_first_period: 0`
- `avoid_first_period: true` → `avoid_first_period: 1`
- `avoid_last_period: false` → `avoid_last_period: 0`
- `avoid_last_period: true` → `avoid_last_period: 1`

Also rename test names that mention the flag identifier from plural to singular: `subject_preference_score_linear_in_position_when_prefer_early_set` stays (it's a behavioural name, not a field-name reference); leave assertions intact, the math-at-value-1 is byte-identical to math-at-bool-true.

For the proptest helper struct around line 552 (`fn subject_pref_test_case`), update the helper signature from `(prefer_early: bool, avoid_first: bool, avoid_last: bool)` to `(prefer_early: u32, avoid_first: u32, avoid_last: u32)` and pass through to the Subject literal.

- [ ] **Step 1.6: Update `solver/solver-core/tests/score_property.rs`**

Find every `prefer_early_periods: bool`, `avoid_first_period: bool`, `avoid_last_period: bool` Subject field and flip to `u32`. The proptest strategy that picks `prop_oneof![Just(false), Just(true)]` (or similar) is replaced with `0u32..=10u32`. Concretely:

- Around line 46, `Subject { prefer_early_periods: false, ... }` → `Subject { prefer_early_period: 0, ... }` (and rename field, two more lines for avoid_first/avoid_last).
- Around line 145, `prefer_early_periods: true` → `prefer_early_period: 1`.
- Around lines 166 and 215 (`weight` parameter passed in): update Subject literal field names to match (the property tests already pass a `weight: u32` to the per-Subject side; keep that, just rename the field).
- Around line 195, `avoid_first_period: true` → `avoid_first_period: 1` (and adjacent renames).

If a strategy looks like `proptest::collection::vec(any::<bool>(), ...)`, replace with `proptest::collection::vec(0u32..=10u32, ...)` for the per-Subject weight strategy. (Property tests today only randomise the global `ConstraintWeights` per-axis weight; the per-Subject side is fixed-shape, so most edits are straight field-name + literal-value swaps.)

- [ ] **Step 1.7: Update `solver/solver-core/benches/solver_fixtures.rs`**

Three fixture builders, identical edits in each:

- `prefer_early_periods: matches!(i, 0 | 1)` → `prefer_early_period: u32::from(matches!(i, 0 | 1))`. (Casts the bool back to 0 / 1.)
- `avoid_first_period: i == 7` (or `i == 9` in the dreizuegige fixture) → `avoid_first_period: u32::from(i == 7)` (or `i == 9`).
- `avoid_last_period: matches!(i, 0 | 1)` → `avoid_last_period: u32::from(matches!(i, 0 | 1))`.

Three locations: lines 89-91 (grundschule), 197-199 (zweizuegig), 365-367 (dreizuegig). Field-name rename (`prefer_early_periods → prefer_early_period`) is the only structural change; the bool-to-int cast preserves the existing on/off pattern.

- [ ] **Step 1.8: Update `solver/solver-py/tests/test_bindings.py` and `test_multi_class.py`**

In each file, replace the JSON fixture lines that emit booleans for the three axes:

- `"prefer_early_periods": False` → `"prefer_early_period": 0` (note the key rename).
- `"avoid_first_period": False` → `"avoid_first_period": 0`.
- `"avoid_last_period": False` → `"avoid_last_period": 0`.

Two literal sites in each file (`test_bindings.py` lines 42-44; `test_multi_class.py` lines 36-38). If any other test in `solver-py` constructs a Subject JSON dict with the old keys, fix it too: `grep -n "prefer_early_periods\|avoid_first_period\|avoid_last_period" solver/solver-py/tests/` lists every site.

- [ ] **Step 1.9: Run solver-core + solver-py tests**

Run: `cargo nextest run -p solver-core && uv run pytest solver/solver-py/tests`

Expected: all green. New linearity tests pass; existing `subject_preference_score_*` and `score_property.rs` tests pass at value 1 with byte-identical assertions; PyO3 contract tests pass with the renamed JSON field.

- [ ] **Step 1.10: Run criterion bench**

Run: `mise run bench`

Expected: criterion completes; soft-score columns in the output match the previous `BASELINE.md` exactly (greedy + LAHC, all three fixtures); p50 wall-clock within 3% of baseline. If the wall-clock drifts more than 3%, run `mise run bench:record` and inspect the diff before continuing; cite the diff in the PR body. If soft scores diverge, audit the bench fixture conversion in step 1.7 first.

- [ ] **Step 1.11: Add ADR 0025**

Create `docs/adr/0025-subject-preference-weights.md`:

```markdown
# 0025: Per-Subject preference weights

Date: 2026-05-01

## Status

Accepted

## Context

Sprint item 9 (`docs/superpowers/OPEN_THINGS.md`) replaces the three boolean preference flags on `Subject` (`prefer_early_periods`, `avoid_first_period`, `avoid_last_period`) with `u32` weights so a school can express relative strength ("Mathematik more strongly early than Deutsch"), not just on / off. ADR 0017 established the boolean preference axes; ADR 0024 added avoid-last-period as the last boolean axis. Both ADRs anticipated this follow-up.

## Decision

Each per-Subject preference field flips from `bool` to `u32` with `#[serde(default)]` (so callers omitting the field deserialise to `0`). The field `prefer_early_periods` (plural) renames to `prefer_early_period` (singular) to match the matching `ConstraintWeights.prefer_early_period` identifier; `avoid_first_period` and `avoid_last_period` keep their names and only flip type.

Per-placement penalty becomes `subject.<axis> * weights.<axis> * factor` (saturating throughout), where `factor` is `tb.position` for `prefer_early_period` and `1` (gated by position equality) for the two `avoid` axes. The per-axis global multiplier in `ConstraintWeights` stays as the operator-only on / off + global-strength dial; the per-Subject weight is the relative-strength dial exposed in the subject edit dialog.

Pydantic + Zod cap user input to `[0, 10]`. Rust stays free `u32` so a power-user override via env-driven seed is not blocked. Alembic migration backfills existing rows via `CASE WHEN <bool> THEN 1 ELSE 0 END`; downgrade is lossy (any value `>= 1` rounds back to `TRUE`).

## Consequences

Existing schedules' soft scores are byte-identical post-migration because `bool true` becomes `u32 1` and `1 * weights.<axis> = weights.<axis>`. Bench fixtures should not need a `BASELINE.md` refresh; if p50 drifts past 3%, refresh and explain. The frontend dialog gains three small number inputs in place of three checkboxes; one i18n key renames (`preferEarlyPeriods → preferEarlyPeriod`).

Per-class subject preference overrides remain a separate deferral (`school_class_subject_preferences`); per-Subject weights are a stepping stone, not a replacement.
```

- [ ] **Step 1.12: Update ADR index**

In `docs/adr/README.md`, append a row under the existing list:

```markdown
- [0025: Per-Subject preference weights](0025-subject-preference-weights.md) — adds u32 weights on Subject for relative-strength tuning.
```

(Match the existing entry shape; if the index uses a table or table-of-contents pattern, mirror it.)

- [ ] **Step 1.13: Run lint, then commit**

Run: `mise run lint`

Expected: green.

```bash
git add solver/solver-core/src/types.rs solver/solver-core/src/score.rs \
        solver/solver-core/tests/score_property.rs \
        solver/solver-core/benches/solver_fixtures.rs \
        solver/solver-py/tests/test_bindings.py solver/solver-py/tests/test_multi_class.py \
        docs/adr/0025-subject-preference-weights.md docs/adr/README.md
git commit -m "feat(solver-core): per-subject preference weights u32"
```

The commit body should briefly note "ADR 0025; rename prefer_early_periods → prefer_early_period; bool → u32 across types, score, proptest, bench fixtures, and PyO3 test fixtures".

---

## Task 2: Backend Alembic migration + ORM + Pydantic + routes + solver_io + route tests

**Files:**

- Add: `backend/migrations/versions/<rev>_subject_preference_weights.py`
- Modify: `backend/src/klassenzeit_backend/db/models/subject.py`
- Modify: `backend/src/klassenzeit_backend/scheduling/schemas/subject.py`
- Modify: `backend/src/klassenzeit_backend/scheduling/routes/subjects.py`
- Modify: `backend/src/klassenzeit_backend/scheduling/solver_io.py`
- Modify: `backend/tests/scheduling/test_subjects.py`

- [ ] **Step 2.1: Write failing route test for out-of-range weight**

Append to `backend/tests/scheduling/test_subjects.py`:

```python
@pytest.mark.asyncio
async def test_create_subject_rejects_out_of_range_prefer_early_period(
    client: AsyncClient,
) -> None:
    payload = {
        "name": "Mathematik",
        "short_code": "MA",
        "color": "chart-1",
        "prefer_early_period": 11,
        "avoid_first_period": 0,
        "avoid_last_period": 0,
    }
    response = await client.post("/api/subjects", json=payload)
    assert response.status_code == 422
    detail = response.json()["detail"]
    assert any(
        loc[-1] == "prefer_early_period" for loc in (e.get("loc", []) for e in detail)
    )


@pytest.mark.asyncio
async def test_create_subject_rejects_out_of_range_avoid_first_period(
    client: AsyncClient,
) -> None:
    payload = {
        "name": "Sport",
        "short_code": "SP",
        "color": "chart-1",
        "prefer_early_period": 0,
        "avoid_first_period": 11,
        "avoid_last_period": 0,
    }
    response = await client.post("/api/subjects", json=payload)
    assert response.status_code == 422


@pytest.mark.asyncio
async def test_create_subject_rejects_out_of_range_avoid_last_period(
    client: AsyncClient,
) -> None:
    payload = {
        "name": "Deutsch",
        "short_code": "DE",
        "color": "chart-1",
        "prefer_early_period": 0,
        "avoid_first_period": 0,
        "avoid_last_period": 11,
    }
    response = await client.post("/api/subjects", json=payload)
    assert response.status_code == 422


@pytest.mark.asyncio
async def test_create_subject_accepts_non_binary_weight(
    client: AsyncClient,
) -> None:
    payload = {
        "name": "Mathematik",
        "short_code": "MA",
        "color": "chart-1",
        "prefer_early_period": 3,
        "avoid_first_period": 0,
        "avoid_last_period": 2,
    }
    response = await client.post("/api/subjects", json=payload)
    assert response.status_code == 201
    body = response.json()
    assert body["prefer_early_period"] == 3
    assert body["avoid_last_period"] == 2
```

- [ ] **Step 2.2: Run new tests to verify they fail**

Run: `mise run test:py -- backend/tests/scheduling/test_subjects.py::test_create_subject_rejects_out_of_range_prefer_early_period -v`

Expected: FAIL with 422 expected vs the current bool-typed schema accepting `11` as truthy (or producing a 422 with a different field name). This anchors the red phase.

- [ ] **Step 2.3: Generate Alembic migration**

Run: `uv run alembic --config backend/alembic.ini revision -m "subject preference weights"`

This creates `backend/migrations/versions/<rev>_subject_preference_weights.py`. Edit the generated file's `upgrade()` and `downgrade()` to:

```python
"""subject preference weights

Revision ID: <generated>
Revises: <prev_revision>
Create Date: 2026-05-01 ...

"""

from collections.abc import Sequence

from alembic import op

# revision identifiers, used by Alembic.
revision: str = "<generated>"
down_revision: str | None = "<prev_revision>"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    op.alter_column(
        "subjects",
        "prefer_early_periods",
        new_column_name="prefer_early_period",
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN prefer_early_period DROP DEFAULT, "
        "ALTER COLUMN prefer_early_period TYPE INTEGER "
        "USING (CASE WHEN prefer_early_period THEN 1 ELSE 0 END), "
        "ALTER COLUMN prefer_early_period SET DEFAULT 0, "
        "ALTER COLUMN prefer_early_period SET NOT NULL"
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN avoid_first_period DROP DEFAULT, "
        "ALTER COLUMN avoid_first_period TYPE INTEGER "
        "USING (CASE WHEN avoid_first_period THEN 1 ELSE 0 END), "
        "ALTER COLUMN avoid_first_period SET DEFAULT 0, "
        "ALTER COLUMN avoid_first_period SET NOT NULL"
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN avoid_last_period DROP DEFAULT, "
        "ALTER COLUMN avoid_last_period TYPE INTEGER "
        "USING (CASE WHEN avoid_last_period THEN 1 ELSE 0 END), "
        "ALTER COLUMN avoid_last_period SET DEFAULT 0, "
        "ALTER COLUMN avoid_last_period SET NOT NULL"
    )


def downgrade() -> None:
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN avoid_last_period DROP DEFAULT, "
        "ALTER COLUMN avoid_last_period TYPE BOOLEAN "
        "USING (avoid_last_period <> 0), "
        "ALTER COLUMN avoid_last_period SET DEFAULT FALSE, "
        "ALTER COLUMN avoid_last_period SET NOT NULL"
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN avoid_first_period DROP DEFAULT, "
        "ALTER COLUMN avoid_first_period TYPE BOOLEAN "
        "USING (avoid_first_period <> 0), "
        "ALTER COLUMN avoid_first_period SET DEFAULT FALSE, "
        "ALTER COLUMN avoid_first_period SET NOT NULL"
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN prefer_early_period DROP DEFAULT, "
        "ALTER COLUMN prefer_early_period TYPE BOOLEAN "
        "USING (prefer_early_period <> 0), "
        "ALTER COLUMN prefer_early_period SET DEFAULT FALSE, "
        "ALTER COLUMN prefer_early_period SET NOT NULL"
    )
    op.alter_column(
        "subjects",
        "prefer_early_period",
        new_column_name="prefer_early_periods",
    )
```

The `DROP DEFAULT` step is required before the type cast: Postgres rejects `ALTER COLUMN ... TYPE` on a column with a `DEFAULT` of incompatible type (the existing default is `FALSE`, which does not cast to `INTEGER` automatically).

- [ ] **Step 2.4: Run migration up + down on the test DB**

Run: `mise run db:reset && mise run db:migrate`

Expected: migrations apply cleanly through the new revision. Then test the down-migration manually:

Run: `uv run alembic --config backend/alembic.ini downgrade -1 && uv run alembic --config backend/alembic.ini upgrade head`

Expected: down + up cycle completes without error. The test DB now has the integer columns at the latest revision.

- [ ] **Step 2.5: Update ORM `Subject` model**

In `backend/src/klassenzeit_backend/db/models/subject.py`, replace lines 21-29 with:

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

Add `Integer` to the existing imports from `sqlalchemy` if it's not already imported.

- [ ] **Step 2.6: Update Pydantic schemas**

In `backend/src/klassenzeit_backend/scheduling/schemas/subject.py`, replace lines 15-42 with:

```python
class SubjectCreate(BaseModel):
    name: str
    short_code: str
    color: str
    prefer_early_period: int = Field(0, ge=0, le=10)
    avoid_first_period: int = Field(0, ge=0, le=10)
    avoid_last_period: int = Field(0, ge=0, le=10)


class SubjectUpdate(BaseModel):
    name: str | None = None
    short_code: str | None = None
    color: str | None = None
    prefer_early_period: int | None = Field(None, ge=0, le=10)
    avoid_first_period: int | None = Field(None, ge=0, le=10)
    avoid_last_period: int | None = Field(None, ge=0, le=10)


class SubjectResponse(BaseModel):
    id: uuid.UUID
    name: str
    short_code: str
    color: str
    prefer_early_period: int
    avoid_first_period: int
    avoid_last_period: int
```

(Adjust import block to include `Field` from pydantic if not already; preserve any other fields the existing schema has, e.g. `model_config`.)

- [ ] **Step 2.7: Update routes module**

In `backend/src/klassenzeit_backend/scheduling/routes/subjects.py`, find six round-trip sites listed in the file structure (lines 60-210):

1. Lines 66-68 (create accept): `prefer_early_period=body.prefer_early_period, avoid_first_period=body.avoid_first_period, avoid_last_period=body.avoid_last_period`.
2. Lines 84-86 (response after create): same field-name update.
3. Lines 113-115 (list response): same.
4. Lines 148-150 (single-subject response): same.
5. Lines 185-190 (update apply): `if body.prefer_early_period is not None: subject.prefer_early_period = body.prefer_early_period` plus avoid_first, avoid_last.
6. Lines 204-206 (update response): same field-name update.

Mechanical find-replace `prefer_early_periods → prefer_early_period`. The other two field names stay; only the type changes (Pydantic enforces the range).

- [ ] **Step 2.8: Update `solver_io.build_problem_json`**

In `backend/src/klassenzeit_backend/scheduling/solver_io.py`, replace lines 259-261 with:

```python
"prefer_early_period": s.prefer_early_period,
"avoid_first_period": s.avoid_first_period,
"avoid_last_period": s.avoid_last_period,
```

(Field-name rename for the first key; the other two keys stay; values flow through as integers.)

- [ ] **Step 2.9: Update existing `test_subjects.py` assertions for renamed key**

Run: `grep -n "prefer_early_periods" backend/tests/scheduling/test_subjects.py`

For every match, rename to `prefer_early_period` and confirm the existing `True / False` literal flips: `True → 1`, `False → 0`. (This is the migration mapping applied to test fixtures; behavioural assertions on default-value flow stay the same.)

If any test in `backend/tests/scheduling/test_solver_io.py` constructs a Subject and asserts on the JSON shape, mirror the rename + literal flip there.

- [ ] **Step 2.10: Run pytest, verify tests pass**

Run: `mise run test:py -- backend/tests/scheduling/test_subjects.py backend/tests/scheduling/test_solver_io.py -v`

Expected: green, including the new range-rejection tests from step 2.1 and the existing CRUD coverage.

- [ ] **Step 2.11: Run lint, then commit**

Run: `mise run lint`

Expected: green.

```bash
git add backend/migrations/versions/*subject_preference_weights*.py \
        backend/src/klassenzeit_backend/db/models/subject.py \
        backend/src/klassenzeit_backend/scheduling/schemas/subject.py \
        backend/src/klassenzeit_backend/scheduling/routes/subjects.py \
        backend/src/klassenzeit_backend/scheduling/solver_io.py \
        backend/tests/scheduling/test_subjects.py \
        backend/tests/scheduling/test_solver_io.py
git commit -m "feat(backend): subject preference weights u32 + alembic migration"
```

(The second test file path is included only if step 2.9 found a hit there; otherwise drop it from the `git add`.)

---

## Task 3: Frontend regenerated types + Zod + dialog + i18n + Vitest spec

**Files:**

- Regenerate: `frontend/src/lib/api-types.ts`
- Modify: `frontend/src/features/subjects/schema.ts`
- Modify: `frontend/src/features/subjects/subjects-dialogs.tsx`
- Modify: `frontend/src/i18n/locales/en.json`
- Modify: `frontend/src/i18n/locales/de.json`
- Add: `frontend/src/features/subjects/subjects-dialogs.test.tsx`

- [ ] **Step 3.1: Regenerate api-types.ts**

Run: `mise run fe:types`

Expected: `frontend/src/lib/api-types.ts` regenerates; the three Subject preference fields go from `boolean` to `number`. Inspect the diff with `git diff frontend/src/lib/api-types.ts | head -60` and verify the rename `prefer_early_periods → prefer_early_period` flowed through.

- [ ] **Step 3.2: Update Zod schema**

In `frontend/src/features/subjects/schema.ts`, replace lines 5-15 (or wherever the three fields live) with:

```ts
prefer_early_period: z.number().int().min(0).max(10),
avoid_first_period: z.number().int().min(0).max(10),
avoid_last_period: z.number().int().min(0).max(10),
```

(Field-name rename for the first; type flip for all three. Per `frontend/CLAUDE.md` "Keep Zod schemas flat for RHF forms": no `.coerce()`, no `.default(...)`. Coercion lives in the form's `onChange` handler instead.)

- [ ] **Step 3.3: Update form defaults and dialog markup**

In `frontend/src/features/subjects/subjects-dialogs.tsx`:

1. Lines 50-52: replace the boolean defaults with integer defaults:

```tsx
prefer_early_period: subject?.prefer_early_period ?? 0,
avoid_first_period: subject?.avoid_first_period ?? 0,
avoid_last_period: subject?.avoid_last_period ?? 0,
```

2. Lines 130-187 (three checkbox FormFields): replace each `<Checkbox checked={...} onCheckedChange={...} />` block with a small number input. Pattern:

```tsx
<FormField
  control={form.control}
  name="prefer_early_period"
  render={({ field }) => (
    <FormItem>
      <FormLabel>{t("subjects.fields.preferEarlyPeriod.label")}</FormLabel>
      <FormControl>
        <Input
          type="number"
          inputMode="numeric"
          min={0}
          max={10}
          step={1}
          value={field.value}
          onChange={(e) => field.onChange(Number(e.target.value))}
          className="w-24"
        />
      </FormControl>
      <FormDescription>{t("subjects.fields.preferEarlyPeriod.help")}</FormDescription>
      <FormMessage />
    </FormItem>
  )}
/>
```

Mirror the shape for `avoid_first_period` and `avoid_last_period` rows. Keep the existing import for `Input` from `@/components/ui/input`; remove the `Checkbox` import if no other field still uses it.

The `Number(e.target.value)` coercion on `onChange` keeps the form value typed as `number` for Zod. Empty string from a cleared input coerces to `0` via `Number("")`, which is the desired "input cleared = axis disabled" behaviour.

- [ ] **Step 3.4: Update i18n keys**

In `frontend/src/i18n/locales/en.json`, find the `subjects.fields` block (around line 140) and replace:

```json
"preferEarlyPeriods": {
  "label": "Prefer early periods",
  "help": "Schedule lessons in this subject earlier in the day."
},
```

with:

```json
"preferEarlyPeriod": {
  "label": "Prefer early periods (weight)",
  "help": "Higher values steer this subject to earlier periods more strongly. 0 disables this axis."
},
```

(Key rename plus updated label and help text.)

For `avoidFirstPeriod` and `avoidLastPeriod`, keep the keys unchanged but rewrite their `.label` to append "(weight)" and their `.help` text:

```json
"avoidFirstPeriod": {
  "label": "Avoid first period (weight)",
  "help": "Higher values penalise placement in the first period more strongly. 0 disables this axis."
},
"avoidLastPeriod": {
  "label": "Avoid last period (weight)",
  "help": "Higher values penalise placement in the last period more strongly. 0 disables this axis."
}
```

In `frontend/src/i18n/locales/de.json`, mirror the same key rename and copy updates with German strings:

```json
"preferEarlyPeriod": {
  "label": "Frühe Stunden bevorzugen (Gewicht)",
  "help": "Höhere Werte verschieben dieses Fach stärker in frühe Stunden. 0 deaktiviert diese Achse."
},
"avoidFirstPeriod": {
  "label": "Erste Stunde meiden (Gewicht)",
  "help": "Höhere Werte gewichten die Platzierung in der ersten Stunde stärker negativ. 0 deaktiviert diese Achse."
},
"avoidLastPeriod": {
  "label": "Letzte Stunde meiden (Gewicht)",
  "help": "Höhere Werte gewichten die Platzierung in der letzten Stunde stärker negativ. 0 deaktiviert diese Achse."
}
```

- [ ] **Step 3.5: Add subjects-dialogs.test.tsx**

Create `frontend/src/features/subjects/subjects-dialogs.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeAll, describe, expect, test } from "vitest";
import i18n from "@/i18n/init";
import { SubjectFormDialog } from "./subjects-dialogs";

function wrapSubjectDialog(children: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("SubjectFormDialog weight inputs", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  test("submits with non-binary prefer_early_period weight", async () => {
    let submitted: Record<string, unknown> | null = null;
    render(
      wrapSubjectDialog(
        <SubjectFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Create"
          onSubmit={(values) => {
            submitted = values as Record<string, unknown>;
          }}
        />,
      ),
    );
    await userEvent.type(screen.getByLabelText(/^name$/i), "Mathematik");
    await userEvent.type(screen.getByLabelText(/short code/i), "MA");
    const earlyInput = screen.getByLabelText(/prefer early periods/i);
    await userEvent.clear(earlyInput);
    await userEvent.type(earlyInput, "3");
    await userEvent.click(screen.getByRole("button", { name: /create/i }));
    await waitFor(() => expect(submitted).not.toBeNull());
    expect(submitted).toMatchObject({ prefer_early_period: 3 });
  });
});
```

Note: this assumes `SubjectFormDialog` exposes an `onSubmit` prop matching the rooms-dialogs pattern. If the actual `subjects-dialogs.tsx` exports a different shape (e.g. wires the mutation internally via a hook), adapt the test to use MSW handlers instead: register a POST `/api/subjects` handler in `frontend/tests/msw-handlers.ts` that captures the request body, and assert on the captured body. The rooms-dialogs test (`frontend/src/features/rooms/rooms-dialogs.test.tsx`) uses the MSW pattern; mirror it if needed.

Inspect `frontend/src/features/subjects/subjects-dialogs.tsx` for the actual exported component name and props before writing the test; the import line and rendered element must match. (Common variants: `SubjectFormDialog`, `SubjectsDialogs`, `SubjectFormBody`.)

- [ ] **Step 3.6: Run frontend tests**

Run: `cd frontend && mise exec -- pnpm vitest run src/features/subjects/`

Expected: green, including the new dialog spec and the existing color / picker specs.

- [ ] **Step 3.7: Run frontend type-check + lint**

Run: `cd frontend && mise exec -- pnpm exec tsc --noEmit && mise run fe:lint`

Expected: green.

- [ ] **Step 3.8: Commit**

```bash
git add frontend/src/lib/api-types.ts \
        frontend/src/features/subjects/schema.ts \
        frontend/src/features/subjects/subjects-dialogs.tsx \
        frontend/src/features/subjects/subjects-dialogs.test.tsx \
        frontend/src/i18n/locales/en.json frontend/src/i18n/locales/de.json
git commit -m "feat(frontend): subject preference weight inputs replace checkboxes"
```

---

## Task 4: Demo seed Python rewrites + solvability test re-run

**Files:**

- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule.py`
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py`
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py`

- [ ] **Step 4.1: Update `_SubjectSpec` in `demo_grundschule.py`**

In `backend/src/klassenzeit_backend/seed/demo_grundschule.py` lines 53-73, replace the `_SubjectSpec` definition and the `_SUBJECTS` tuple:

```python
class _SubjectSpec(NamedTuple):
    name: str
    short_code: str
    color: str
    prefer_early_period: int = 0
    avoid_first_period: int = 0
    avoid_last_period: int = 0


_SUBJECTS: tuple[_SubjectSpec, ...] = (
    _SubjectSpec("Deutsch", "D", "chart-1", prefer_early_period=1, avoid_last_period=1),
    _SubjectSpec("Mathematik", "M", "chart-2", prefer_early_period=1, avoid_last_period=1),
    _SubjectSpec("Sachunterricht", "SU", "chart-3"),
    _SubjectSpec("Religion (kath.)", "RK", "chart-4"),
    _SubjectSpec("Religion (ev.)", "RE", "chart-4"),
    _SubjectSpec("Ethik", "ETH", "chart-4"),
    _SubjectSpec("Englisch", "E", "chart-5"),
    _SubjectSpec("Kunst", "KU", "chart-1"),
    _SubjectSpec("Musik", "MU", "chart-3"),
    _SubjectSpec("Sport", "SP", "chart-4", avoid_first_period=1),
    _SubjectSpec("Förderunterricht", "FÖ", "chart-5"),
)
```

(Field rename + type flip; literal `True` becomes `1`.)

Around line 198-205, the Subject construction loop:

```python
prefer_early_period=spec.prefer_early_period,
avoid_first_period=spec.avoid_first_period,
avoid_last_period=spec.avoid_last_period,
```

(First key renames; the loop already maps `spec.<field>` to the ORM kwarg, so the rename matches the rename in the `_SubjectSpec` definition above.)

- [ ] **Step 4.2: Update construction loop in `demo_grundschule_zweizuegig.py`**

In `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py` lines 248-253, the same shape as step 4.1:

```python
prefer_early_period=spec.prefer_early_period,
avoid_first_period=spec.avoid_first_period,
avoid_last_period=spec.avoid_last_period,
```

(`_SubjectSpec` is imported from `demo_grundschule.py`, so the dataclass-side update propagates; only the loop's kwargs need updating.)

- [ ] **Step 4.3: Update construction loop in `demo_grundschule_dreizuegig.py`**

Same edits at lines 384-389.

- [ ] **Step 4.4: Run seed solvability tests**

Run: `mise run test:py -- backend/tests/seed/ -v`

Expected: green. The three demo solvability tests (`test_demo_grundschule_solvability.py`, `test_demo_grundschule_zweizuegig_solvability.py`, `test_demo_grundschule_dreizuegig_solvability.py`) assert "schedule placeable, soft score within budget"; they pass because `1 * weights.<axis> = weights.<axis>` byte-identically with the previous boolean shape.

If any test fails with a soft-score mismatch, audit the seed rewrite for an accidental `True → 0` (boolean false-y bug); the most common failure mode is forgetting to update one of the three `_SubjectSpec` literal lines.

- [ ] **Step 4.5: Run full test suite + lint**

Run: `mise run lint && mise run test`

Expected: green across Rust, Python, frontend.

- [ ] **Step 4.6: Commit**

```bash
git add backend/src/klassenzeit_backend/seed/demo_grundschule.py \
        backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py \
        backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py
git commit -m "chore(seed): rewrite demo seeds with int preference weights"
```

---

## Final verification

- [ ] **Step F.1: Run the full test suite**

Run: `mise run test`

Expected: green across all three runners.

- [ ] **Step F.2: Run the bench**

Run: `mise run bench`

Expected: soft scores identical to baseline; p50 wall-clock within 3% of baseline.

- [ ] **Step F.3: Sanity-check the Playwright smoke spec**

Run: `mise run e2e -- --grep "grundschule"` (or the equivalent filter pinning to the smoke flow).

Expected: the Grundschule generate-and-render flow passes; the dialog change has no visible effect on the smoke path because the smoke spec doesn't open the subject edit dialog.

- [ ] **Step F.4: Inspect git log + diff**

Run: `git log --oneline master..HEAD && git diff master..HEAD --stat`

Expected: four commits (`feat(solver-core): per-subject preference weights u32`, `feat(backend): subject preference weights u32 + alembic migration`, `feat(frontend): subject preference weight inputs replace checkboxes`, `chore(seed): rewrite demo seeds with int preference weights`); diff stats roughly: solver-core ~150 lines changed, backend ~60 lines + new migration, frontend ~80 lines + new test, seeds ~30 lines.

If a commit's content drifted from this plan (e.g. cross-cutting fixes folded into the wrong commit), the four-commit story still reads as one logical step per layer; that's the acceptance bar.
