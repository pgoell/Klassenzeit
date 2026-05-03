# `Stundentafel.school_type` enum and Sek I/II grade-range expansion

**Sprint:** Schwimmen + Sek-I foundations (schema phase, P0).

**Closes (in `docs/superpowers/OPEN_THINGS.md`):** active sprint item 1.

**ADR:** [0026: school-type enum and grade-range expansion](../../adr/0026-stundentafel-school-type-and-grade-range.md), added in this PR.

## Goal

Two semantically-linked schema changes ride together:

1. Add a `Stundentafel.school_type: SchoolType` column with the Hessen Schulform values `Grundschule`, `Hauptschule`, `Realschule`, `Gymnasium`, `Gesamtschule`. Native Postgres `ENUM` named `school_type`. Backfill existing rows to `Grundschule` via `server_default`.
2. Widen the `grade_level` Pydantic and Zod validators on `StundentafelCreate` / `StundentafelUpdate` and `SchoolClassCreate` / `SchoolClassUpdate` from the implicit Grundschule ceiling (1..4 in practice) to the explicit `Field(ge=1, le=13)` so future Sek I and Sek II fixtures fit. The underlying `SmallInteger` already holds 1..13.

The two changes share one PR because they share the same Stundentafel surfaces (Pydantic schemas, Zod schema, dialog form), share one ADR, and would otherwise produce a no-op DB PR (a Pydantic-only widening) on the heels of an enum PR. Bundling matches the OPEN_THINGS line ("`Stundentafel.school_type` enum + grade-range expansion") which scopes them as one sprint item.

## Non-goals

- **Solver wire format change.** The solver's `Problem` JSON already operates per Lesson / SchoolClass / Subject / Teacher / Room / TimeBlock; `school_type` is metadata for UI and future fixtures, not a constraint input. `solver-core` and `solver-py` are untouched. No `BASELINE.md` refresh.
- **Renderer for the new field on the list page.** The Stundentafel list page is unchanged. The dropdown ships only on the Create and Edit dialogs.
- **Localised label divergence.** EN and DE both render the German Schulform names (`Grundschule`, …) verbatim. A future English-language Schulform glossary (Primary / Lower-secondary / etc.) is filed under acknowledged deferrals.
- **Per-Klasse override or per-school-type behaviour switch.** Adding a UI toggle that hides Religion options when school_type is "Gesamtschule integriert", etc., is Sprint 6 territory. This PR ships the field, not the switch.
- **Migration of existing dev / staging data beyond the column-level default.** Every Stundentafel row in dev/staging today (and every fixture row in tests) describes a Grundschule curriculum; the `server_default="Grundschule"` materialises the right value at `ALTER TABLE`. No data migration script.
- **Adding `Mittelstufenschule`, `Berufsschule`, `Förderschule`.** Out of scope for Sprints 2-7. Future addition is a one-line `ALTER TYPE school_type ADD VALUE 'Förderschule'` Alembic step.

## Architecture changes

### Database (Alembic migration)

New native Postgres ENUM type plus a non-nullable column on `stundentafeln`:

```python
school_type_enum = postgresql.ENUM(
    "Grundschule",
    "Hauptschule",
    "Realschule",
    "Gymnasium",
    "Gesamtschule",
    name="school_type",
    create_type=False,
)


def upgrade() -> None:
    school_type_enum.create(op.get_bind(), checkfirst=True)
    op.add_column(
        "stundentafeln",
        sa.Column(
            "school_type",
            school_type_enum,
            server_default="Grundschule",
            nullable=False,
        ),
    )


def downgrade() -> None:
    op.drop_column("stundentafeln", "school_type")
    school_type_enum.drop(op.get_bind(), checkfirst=True)
```

Migration filename: `<rev>_add_stundentafel_school_type.py`. The `create_type=False` on the column-side definition plus the explicit `school_type_enum.create(..., checkfirst=True)` is the safer pattern under the test-template DB cache (idempotent reruns) and matches the project's pattern of writing Alembic ops explicitly. The `server_default="Grundschule"` materialises the value into existing rows during `ALTER TABLE ... ADD COLUMN`, so no separate `op.execute("UPDATE ...")` is needed.

### `klassenzeit_backend.db.models.stundentafel`

Add a `SchoolType` `StrEnum` next to the ORM class and the new column on `Stundentafel`:

```python
import enum
from sqlalchemy.dialects.postgresql import ENUM as PG_ENUM


class SchoolType(enum.StrEnum):
    """Hessen Schulform classification on a curriculum (Stundentafel)."""

    GRUNDSCHULE = "Grundschule"
    HAUPTSCHULE = "Hauptschule"
    REALSCHULE = "Realschule"
    GYMNASIUM = "Gymnasium"
    GESAMTSCHULE = "Gesamtschule"


class Stundentafel(Base):
    ...
    school_type: Mapped[SchoolType] = mapped_column(
        PG_ENUM(SchoolType, name="school_type", create_type=False, native_enum=True),
        nullable=False,
        server_default=SchoolType.GRUNDSCHULE.value,
        default=SchoolType.GRUNDSCHULE,
    )
```

`StrEnum` lets Pydantic v2 serialise members as bare strings on the wire, lets `openapi-typescript` emit a TypeScript string-literal union for the field, and lets f-strings render the value verbatim. `create_type=False` on the SQLAlchemy column keeps Alembic from trying to recreate the type on every metadata operation.

### `klassenzeit_backend.scheduling.schemas.stundentafel`

Three sites mirror the existing `name` / `grade_level` pattern:

```python
from klassenzeit_backend.db.models.stundentafel import SchoolType


class StundentafelCreate(BaseModel):
    name: str
    grade_level: int = Field(ge=1, le=13)
    school_type: SchoolType = SchoolType.GRUNDSCHULE


class StundentafelUpdate(BaseModel):
    name: str | None = None
    grade_level: int | None = Field(default=None, ge=1, le=13)
    school_type: SchoolType | None = None


class StundentafelListResponse(BaseModel):
    id: uuid.UUID
    name: str
    grade_level: int
    school_type: SchoolType
    ...


class StundentafelDetailResponse(BaseModel):
    ...same plus school_type: SchoolType
```

The `Field(le=13)` is purely additive on the wire (existing clients submitting 1..4 stay valid), and the `school_type` Create default keeps existing API clients that POST `{name, grade_level}` working unchanged.

### `klassenzeit_backend.scheduling.schemas.school_class`

Tighten the bare `int` validator to match:

```python
class SchoolClassCreate(BaseModel):
    name: str
    grade_level: int = Field(ge=1, le=13)
    ...


class SchoolClassUpdate(BaseModel):
    ...
    grade_level: int | None = Field(default=None, ge=1, le=13)
    ...
```

### `klassenzeit_backend.scheduling.routes.stundentafeln`

Six sites in the existing CRUD module:

- `create_stundentafel_route` reads `body.school_type` (always present, defaults applied), constructs the ORM instance with it, returns it on the response.
- `update_stundentafel_route` adds `if body.school_type is not None: tafel.school_type = body.school_type` next to the existing two PATCH lines (the field is `NOT NULL`, so the `is not None` guard is right; PATCH semantics here mean "do not change", consistent with `name` and `grade_level`).
- `list_stundentafeln`, `get_stundentafel` add `school_type=t.school_type` (or `tafel.school_type`) to every response constructor.

### `klassenzeit_backend.seed.{demo_grundschule, demo_grundschule_zweizuegig, demo_grundschule_dreizuegig}`

Every `Stundentafel(name=..., grade_level=...)` construction site gains `school_type=SchoolType.GRUNDSCHULE`:

```python
tafel = Stundentafel(
    name=f"Grundschule {grade}",
    grade_level=grade,
    school_type=SchoolType.GRUNDSCHULE,
)
```

Three call sites total (one per seed module). Every existing seed shape and solvability test stays green; the only field that changes shape on the wire is the response payload, which the seed tests do not assert on.

### Solver wire format

Unchanged. `scheduling/solver_io.build_problem_json` does not see `school_type`; the Rust `Problem` struct has no Stundentafel-shaped data; `Lesson`, `SchoolClass`, `Subject`, `Teacher`, `Room`, `TimeBlock` are the only entities the solver consumes and none of them gain a field in this PR. `BASELINE.md` not refreshed.

### Frontend types and Zod

Run `mise run fe:types` after the backend feat lands; `frontend/src/lib/api-types.ts` regenerates with `school_type: "Grundschule" | "Hauptschule" | "Realschule" | "Gymnasium" | "Gesamtschule"` on every Stundentafel-shaped schema.

Update `frontend/src/features/stundentafeln/schema.ts`:

```ts
export const SchoolTypeValues = [
  "Grundschule",
  "Hauptschule",
  "Realschule",
  "Gymnasium",
  "Gesamtschule",
] as const;

export const StundentafelFormSchema = z.object({
  name: z.string().min(1, "Name is required"),
  grade_level: z.number().int().min(1, "Grade must be at least 1").max(13),
  school_type: z.enum(SchoolTypeValues),
});
```

Update `frontend/src/features/school-classes/schema.ts`:

```ts
grade_level: z.number().int().min(1, "Grade is required").max(13, "Grade cannot exceed 13"),
```

### Frontend dialog (`features/stundentafeln/stundentafeln-dialogs.tsx`)

The `StundentafelFormDialog` and `StundentafelEditDialog` each gain a `<FormField name="school_type">` block placed below the `name` field and above `grade_level`, rendering a shadcn `<Select>` with the five `SelectItem`s, mirroring the `preferred_block_size` Select pattern already in `EntryFormDialog`. Default value: `"Grundschule"` for the Create dialog; the loaded Stundentafel's value for the Edit dialog. The submit handler threads `values.school_type` into the `StundentafelCreate` / `StundentafelUpdate` body.

### Frontend i18n (`frontend/src/i18n/locales/{en,de}.json`)

Add new keys under `stundentafeln.fields`:

- `schoolTypeLabel` ("School type" / "Schulform").
- `schoolType.Grundschule`, `schoolType.Hauptschule`, `schoolType.Realschule`, `schoolType.Gymnasium`, `schoolType.Gesamtschule` (DE renders the German labels verbatim; EN renders the same German labels by design, matching the user's domain language. The English-glossary divergence is an acknowledged deferral.)

`src/i18n/types.d.ts` re-exports `en.json`'s shape (`resources: { translation: typeof en }`), so adding a key to `en.json` extends the typed `t()` resources automatically; no separate codegen step.

## Tests

### Backend

- `tests/db/test_models.py` gains `test_stundentafel_school_type_round_trip`: build a Stundentafel with `school_type=SchoolType.GYMNASIUM`, flush, re-read, assert.
- `tests/scheduling/test_stundentafeln.py` extends existing tests:
  - `test_create_stundentafel_default_school_type`: POST without `school_type`, response has `school_type == "Grundschule"`.
  - `test_create_stundentafel_explicit_school_type`: POST with `school_type=Gymnasium`, response and DB row both Gymnasium.
  - `test_create_stundentafel_grade_level_too_high`: POST `grade_level=14`, expect 422.
  - `test_create_stundentafel_invalid_school_type`: POST `school_type="FH"`, expect 422.
  - `test_update_stundentafel_school_type`: PATCH from Grundschule to Gymnasium, response and re-read both reflect the change.
- `tests/scheduling/test_school_classes.py`: `test_create_school_class_grade_level_too_high` (POST `grade_level=14`, expect 422).
- `tests/seed/test_demo_grundschule_shape.py` and the `_zweizuegig_shape` / `_dreizuegig` siblings: assert each persisted Stundentafel has `school_type == SchoolType.GRUNDSCHULE`. The dreizuegige variant has its own shape file; the zweizuegige test file already covers Stundentafel structure.

### Frontend (Vitest)

- New spec or extension to `frontend/src/features/stundentafeln/stundentafeln-dialogs.test.tsx` (create the file if absent; the feature has hooks and forms but no unit test file today). Three behaviours:
  - StundentafelFormDialog renders the `schoolType` Select with five options; selecting Gymnasium and submitting POSTs `school_type: "Gymnasium"`.
  - StundentafelEditDialog seeds the dropdown to the loaded stundentafel's `school_type`.
  - Grade-level input rejects 14 with the Zod max-13 validation message; rejects 0 with the existing min-1 message.
- Existing entity-page specs that assert dialog body submissions get the new `school_type` field added to expected payloads where they assert on it.

## Verification

Per the autopilot workflow:

- `mise run lint` (ruff, ty, vulture, biome, machete, cargo fmt, clippy, actionlint).
- `mise run test:py -- backend/tests/db/test_models.py backend/tests/scheduling/test_stundentafeln.py backend/tests/scheduling/test_school_classes.py backend/tests/seed/test_demo_grundschule_shape.py backend/tests/seed/test_demo_grundschule_zweizuegig_shape.py backend/tests/seed/test_demo_grundschule_dreizuegig.py` first as a fast iteration loop, then full `mise run test:py` once green.
- `mise run fe:test` for the new dialog spec.
- `mise run fe:types` regenerates `api-types.ts` after the backend feat lands; the regenerated file is committed in the same frontend commit.
- `mise run fe:build` then `cd frontend && mise exec -- pnpm exec tsc --noEmit` (CI's tighter typecheck) to catch the noUncheckedIndexedAccess corners the frontend rule names.
- The CLAUDE.md "Schema-changing PRs" recovery applies: drop `klassenzeit_test_template` and `klassenzeit_test_gw*` before the first local test run on the new branch so xdist workers see the new schema.
- `mise run e2e` is unaffected (no Playwright spec asserts on Stundentafel creation copy beyond what the backend exposes).

## Risks and trade-offs

- **Hessen Schulform list completeness.** The five values cover Sprints 2-7 (Gymnasium, Wahlpflicht via Gymnasium, Differenzierung via Realschule, Gesamtschule full Sek I, Gymnasium full G9). `Mittelstufenschule`, `Förderschule`, `Berufsschule` are not covered; future addition is one-line `ALTER TYPE`. Documented in ADR 0026.
- **`StrEnum` in the ORM column.** SQLAlchemy 2's PG_ENUM type with `StrEnum` reads back as the `StrEnum` member (not a bare string); existing tests that compare `tafel.school_type == "Grundschule"` work because `StrEnum` members are strings. The route response constructors pass `school_type=tafel.school_type`, which Pydantic v2 accepts as the matching `SchoolType` and serialises as the bare value.
- **Test-template DB drift.** Per CLAUDE.md "Schema-changing PRs": the cached `klassenzeit_test_template` was migrated against the previous schema. The first local `mise run test:py` after checkout will fail with `relation "school_type" does not exist`-style errors per worker until the template and per-worker DBs are dropped. The plan calls this out as a step.
- **Bundling enum + grade-range.** Splitting them into two PRs would produce a sub-10-line follow-up PR with no schema change; the bundle keeps round-trip-through-CI cost down. Per OPEN_THINGS the two are scoped as one sprint item.
- **The new ADR number.** `ls docs/adr/*.md | sort | tail -1` confirms 0025 is the highest before this work; 0026 is the next available number, per CLAUDE.md "Always `ls docs/adr/*.md | sort | tail -1` before assigning the next number".
