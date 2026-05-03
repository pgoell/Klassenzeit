# 0026: Stundentafel school-type enum and Sek I/II grade-range expansion

- **Status:** Accepted
- **Date:** 2026-05-03

## Context

Sprint item 1 (schema phase, P0) of the "Schwimmen + Sek-I foundations" sprint. Until now `Stundentafel` rows have been Grundschule-shaped (`grade_level` in the implicit 1..4 range), with no field telling consumers which Schulform a curriculum describes. The upcoming sprint sequence (Sprint 2 Gymnasium einzügig 5..10, Sprint 4 Realschule, Sprint 6 Gesamtschule, Sprint 7 G9 5..13) needs both a Schulform label and a wider grade-level domain.

## Decision

1. **Native Postgres `ENUM` `school_type`** with values `Grundschule`, `Hauptschule`, `Realschule`, `Gymnasium`, `Gesamtschule`. Co-located `enum.StrEnum` `SchoolType` in `backend/src/klassenzeit_backend/db/models/stundentafel.py`. The Alembic migration creates the type explicitly with `school_type_enum.create(op.get_bind(), checkfirst=True)`; `server_default="Grundschule"` plus `nullable=False` materialises every existing row at `ALTER TABLE` time. Future Schulformen (`Mittelstufenschule`, `Förderschule`, `Berufsschule`) land later as one-line `ALTER TYPE school_type ADD VALUE`.
2. **`values_callable=lambda enum_cls: [m.value for m in enum_cls]`** on the SQLAlchemy `PG_ENUM` column. SQLAlchemy 2's default for Python enums sends `member.name` (`GYMNASIUM`) instead of `member.value` (`Gymnasium`); the explicit callable makes the wire and catalog representation match.
3. **Widen `grade_level` validators on Stundentafel and SchoolClass to `Field(ge=1, le=13)`.** Underlying `SmallInteger` already holds 1..13; the change is a Pydantic / Zod widening on Stundentafel and a tightening on SchoolClass (today bare `int`). Frontend Zod mirrors with `.max(13, "Grade cannot exceed 13")`.
4. **Solver wire format unchanged.** `Stundentafel.school_type` is metadata for UI and future fixtures, not a constraint input. `solver-core`, `solver-py`, and `BASELINE.md` are untouched.

## Alternatives considered

- **`VARCHAR` + `CHECK` constraint instead of native `ENUM`.** Easier to evolve, weaker type signal in `\d`, less precise OpenAPI emit. The native-enum precedent locks in the pattern for Sprint 5's Sek II phase enum.
- **Application-level Python enum + plain `String` column.** Drift-prone once anyone bypasses the ORM.
- **Split the enum and grade-range work into two PRs.** Cleaner per-PR diffs, but the second PR would be sub-10 lines of Pydantic / Zod widening. Bundling matches OPEN_THINGS' single-item scoping.
- **Localised English glossary labels (Primary / Lower-secondary).** Deferred. The dropdown reads cleaner with verbatim German Schulform names because that is the user's domain language.

## Consequences

Easier: Stundentafel rows now self-describe their Schulform. Sprint 2 can seed a Gymnasium-tagged Stundentafel without further schema work; future fixture seeds follow the same pattern. Sek I and Sek II grade levels are representable end-to-end. The dialog form gains a field that conditions the user's mental model of the grade range.

Harder: every literal `Stundentafel(...)` construction in seeds and tests now passes `school_type=`; future axis additions (further Schulformen, the upcoming Sek II phase enum) compound the maintenance cost. The native ENUM is harder to migrate than a string column if the variant set ever needs to shrink, but `ALTER TYPE` lets us grow it with one line per addition.

Revisit when Sprint 5 introduces the Sek II phase enum (Einführungsphase / Qualifikationsphase) and the codebase has two Postgres ENUMs to maintain; if a third lands, factor out `db/models/enums.py`.
