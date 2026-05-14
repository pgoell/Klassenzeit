# Klassenzeit backend: rules

Stack: FastAPI + SQLAlchemy async, Alembic, Pydantic. Served under `klassenzeit_backend`. On top of `.claude/CLAUDE.md`.

## Layout (`backend/src/`)

Routes and route handlers live next to the aggregate they serve. Runtime state (engine, session factory, settings, rate limiter) lives on `app.state`, set in `lifespan`. No module-level globals.

## Error handling

- **No bare catchalls.** No `except:` / `except Exception`. Catch the specific error; let the rest propagate.
- **Use `status.HTTP_422_UNPROCESSABLE_CONTENT`** for 422 responses (the `_ENTITY` alias is deprecated).

## Type checking

- **`ty` does not honor `# type: ignore[...]` pragmas.** Use concrete types (NamedTuple, TypedDict, dataclass) over `dict[str, object]`.
- **Pre-commit `ty check` blocks a strict "red test with missing module" TDD start.** Land a stub module (typed signature, body `raise NotImplementedError(...)`) in the red commit; replace in the next.
- **Don't add `from __future__ import annotations` to test files or backend Pydantic schema modules.** Ruff `TC001/TC002/TC003` then flags every annotation-only import.
- **Pydantic-settings `env_file=` only injects into BOUND fields.** Declare a scalar field for every env var; `.env.test` values are silently ignored if no matching field exists. Assemble derived views in `@model_validator(mode="after")`, not by reading `os.environ` from a validator.

## Data access

- **No raw SQL outside the abstraction layer.** All queries go through SQLAlchemy.
- **Alembic autogenerate style drift.** Replace `typing.Sequence` with `collections.abc.Sequence` and `typing.Union[X, Y]` with PEP 604 unions in new revisions.
- **Native Postgres `ENUM` columns with a Python `enum.StrEnum` need `values_callable`.** SQLAlchemy 2 sends `member.name` by default; the Postgres `CREATE TYPE` carries `member.value`, the round-trip errors. Pin: `PG_ENUM(MyEnum, name="...", create_type=False, native_enum=True, values_callable=lambda cls: [m.value for m in cls])` plus `server_default=MyEnum.<DEFAULT>.value`. Migration uses explicit `.create(op.get_bind(), checkfirst=True)` and `.drop(...)` on downgrade. See `db/models/stundentafel.py`.
- **`ALTER COLUMN ... TYPE` needs `DROP DEFAULT` first when the existing default has incompatible type.** Canonical shape: `DROP DEFAULT, TYPE <new> USING (<expr>), SET DEFAULT <new>, SET NOT NULL` in one `op.execute(...)` per column.
- **`AsyncSession.execute(delete/update).rowcount`.** `ty` sees `Result[Any]`; access via `int(getattr(result, "rowcount", 0) or 0)`.
- **PATCH for nullable columns uses `body.model_fields_set`,** not `is not None`. Gate on `if "foo" in body.model_fields_set` for nullable FKs so explicit nulls clear the column. Keep `is not None` for NOT NULL columns.
- **Routes that mutate DB state must explicitly `await db.commit()` + `db.refresh(orm_row)` before returning.** The test conftest's savepoint-wrapped session masks missing commits; production fails because each request gets a fresh session.

## Testing

- **Test fixtures, not imports.** `pytest --import-mode=importlib` requires shared helpers be fixtures in `conftest.py`, not plain imports.
- **Scheduling entity factories** live in `backend/tests/scheduling/conftest.py`. Join-table rows have no factory; construct inline with `db_session.add(...)` + `flush()`. `Lesson.preferred_block_size` is NOT NULL with no server default; always set it.
- **`TimeBlock.start_time` / `end_time` are `datetime.time`,** not strings.
- **`Teacher` has no `qualifications` relationship; query `TeacherQualification` directly** (see `_build_teacher_detail` in `scheduling/routes/teachers.py`). Same for `TeacherAvailability`.
- **`async with db_session.begin_nested():` for tests that expect `IntegrityError`.** A bare `rollback()` in `finally:` escapes the outer savepoint and drops setup rows.
- **`Lesson.teacher_id` is pin-only.** The solver picks among `teacher_candidates` when null. `POST /generate-lessons` validates qualified-teacher coverage up front and returns 422 (`code="missing_qualified_teacher"`, `subject_ids`, `subject_short_names`) if any subject has no qualified teacher.
- **Solver post-condition validators raise `ValueError`, not soft violations.** `solver_io.run_solve` catches `(ValueError, RuntimeError)` and re-raises; tests must assert `status_code == 200` BEFORE asserting on `body["violations"]`.
- **`caplog` records expose `extra=` keys via `record.__dict__[key]`,** not `record.key`.
- **Settings tests construct `Settings` directly,** not `get_settings()`. Use `Settings(_env_file=None)  # ty: ignore[missing-argument, unknown-argument]`.
- **Ruff `PLC0415` rejects in-function imports.** Hoist to module top-level.
- **Ruff `S101` rejects `assert` in non-test source.** Restructure the data flow so the narrowed value travels through the data structure (carry the discriminator as a tuple key element) rather than asserting after a filter.
- **`QualityIssue.detail` is `dict[str, object]`,** not `str`.
- **Middleware integration tests use `httpx.ASGITransport(app=build_app(env="dev"))`** + `AsyncClient` + `caplog.set_level(...)`.
- **Cross-test-module helpers import as `from tests.<module>`,** not `from backend.tests.<module>` (no `backend/__init__.py`).
- **Conftest module-load env mutations need `# noqa: E402`** on trailing `klassenzeit_backend` imports.
- **`app.state.<name>` set in `lifespan` must be mirrored in the test `client` fixture** in `backend/tests/conftest.py`. `httpx.ASGITransport` skips `lifespan`, so without the mirror request handlers raise `AttributeError`. Pattern: `settings`, `rate_limiter`, `solver_progress`.
- **Per-request long-running state lives in an `app.state.<name>` dict + a `@contextmanager` lifecycle.** Pattern: `app.state.<registry> = {}` set in `lifespan`; a `register_<thing>(registry, *, key, ...)` context manager registers an entry, `yield`s it, and removes it in `finally:` so a crashing handler still leaves the registry clean. Sibling read / cancel routes look up the key and return 404 when absent. Template: `backend/src/klassenzeit_backend/scheduling/progress.py` (`register_progress` + `RegistrationEntry`). ADR 0039.
- **Backend capability gaps surface as a structured `logger.warning(...)` + fall-through, not a 4xx.** When the backend wires a new capability through one solver path (live progress on the three LAHC backends) but not yet through another (CP-SAT), the unsupported path logs `solver.solve.<capability>_unsupported` with `extra={"backend": ...}` and continues with the legacy code path so the request succeeds. The route is uniform; the capability degrades. Pattern: `progress_handle is not None` branch in `solver_io.run_solve`'s `case "cpsat":`. ADR 0039.
- **New required fields on solver `Solution` flow through `filter_solution_for_class`** to land on `ScheduleResponse`. Adding a field on solver-core's `Solution` plus the matching field on the Pydantic response model is insufficient if `filter_solution_for_class` doesn't pass it through; the route's `model_validate(filtered)` call silently drops missing fields back to their default.
- **`ScheduleReadResponse` (GET /teachers|rooms|classes/{id}/schedule) is distinct from `ScheduleResponse` (POST /classes/{id}/schedule).** A new field surfacing on solve-time responses lands on `ScheduleResponse` via `filter_solution_for_class`; the same field on read-time responses needs explicit addition to `ScheduleReadResponse` plus a populating query at the manual-construction site (`read_schedule_for_*_route`). Pinning tests like `test_get_schedule_for_teacher_empty_placements_response` assert the exact dict shape and break when a field is added without default; widen those in the same commit.
- **pytest-xdist gives each worker its own database** via `backend/tests/_xdist_db.py` (ADR 0019). Two concurrent `mise run test:py` invocations conflict on the suffix-less `klassenzeit_test`; run flake loops sequentially. Alembic migrations cache via a Postgres template DB; schema-changing PRs must drop the template and per-worker DBs first (`DROP DATABASE IF EXISTS klassenzeit_test_template; klassenzeit_test_gw0; ...; klassenzeit_test`).
- **Per-backend deadlines in `.env.test`.** Zeroes `KZ_SOLVE_DEADLINE_MS_*` for all solver backends; one-test override via `monkeypatch.setitem(app.state.settings.solve_deadline_ms_by_backend, "<backend>", 5000)`. ADR 0038.
- **`KZ_SOLVER_BACKEND` selects the backend** (`Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"]`, default `lahc_rr`). Default pinned by `test_solver_backend_default_is_production_choice`; flip assertion and ADR in lockstep. ADR 0030 / 0031 / 0032 / 0037.
- **Quality-bar tests the system does not yet meet ship as `pytest.mark.xfail(strict=False)`** paired with a sibling item in `docs/OPEN_THINGS.md`.
- **Route handlers that construct response objects manually** (subject CRUD, `read_schedule_for_class`, `read_schedule_for_teacher`, `read_schedule_for_room`) must include any new schema fields explicitly. `model_validate(orm_row)` callers auto-flow.
- **Structured 422 detail dicts use `{"code": str, ...domain fields}`** for machine-readable error codes. FastAPI accepts `HTTPException(detail=<dict>)` natively; the `code` discriminator drives frontend i18n.
- **`scheduling/schemas/__init__.py` is intentionally bare;** every schema is imported directly from its submodule.
- **`ViolationResponse.kind` Literal lives in `scheduling/schemas/schedule.py`.** Adding a new `ViolationKind`: widen the Literal, `mise run fe:types`, extend the frontend exhaustive switch + i18n keys, update the closed-enum tests. Land all four in one commit.
- **`QualityReport` field-set is pinned exhaustively in two tests:** `tests/scheduling/test_schedule_route.py::test_schedule_post_response_carries_quality_report` and `tests/scheduling/test_schedule_all_respect_pins.py::test_schedule_all_response_carries_quality_report` both maintain a frozen `expected_fields` set-equality assertion. Adding a `QualityReport` field touches both tests in lockstep with the Pydantic mirror (`schemas/quality_report.py`); `extra="forbid"` on `QualityReportResponse` 422s the route if the mirror drifts.
- **Quality predicates read raw `TimeBlock.position`; project to lesson ordinals when breaks interleave.** `check_interior_gaps` / `check_day_length` in `quality_checks.py` and the integration test `test_grundschule_schedule_quality.py` operate on `Placement.position`. After ADR 0040 (TimeBlock.kind), lesson positions are sparse (einzügig day: 1,2,4,5,7,8). The test pre-projects via `_build_lesson_ordinal_map(time_blocks)` (walks LESSON-kind rows per day, builds raw→ordinal map) before scoring. Any new admin-facing quality endpoint that reuses these predicates must fold the projection in, or surface phantom gaps at every break slot.
- **Seed-shape tests pinning `[b.position for b in day_blocks] == list(range(1, N))` are a known sensitivity surface.** When seed positions change (e.g. 6 → 8 with breaks), audit `backend/tests/seed/test_demo_*_shape.py` for `range(1, 7)` / `range(1, 9)` patterns and `TimeBlock` count assertions; update in the same commit as the seed change.
- **Alembic head check before writing `down_revision`.** `ls backend/alembic/versions/ | sort | tail -1` can lag the real head when intermediate revisions land. Verify with `cd backend && mise exec -- alembic heads`; pin `down_revision` to the actual head, not the file-sort tail, or `alembic upgrade head` errors with "multiple head revisions".

## Logging

- **Structured logs use `logger.info("event.name", extra={...})`.** `JsonFormatter` merges `extra=` keys at top level (ADR 0016). Reserve `event` as a stable identifier.
- **Per-request `request_id` propagates via the `request_id_var` ContextVar.** A `RequestIdFilter` injects it onto any `LogRecord` in the request scope; routes do not thread `request_id` through `extra=` manually.
