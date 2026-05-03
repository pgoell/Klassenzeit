# Sprint C: manual schedule editing implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship drag-and-drop, pin, and pin-aware re-solve on the class-view schedule grid.

**Architecture:** New `ScheduledLesson.pinned` column threads through the existing `pinned_placements` solver wire format. Three new placement-mutation endpoints (`PATCH .../move`, `PATCH .../pin`, `POST .../swap`) addressed by composite key. A new `respect_pins` query flag on `POST /api/schedule/all`. Frontend integrates `@dnd-kit/core` on the class-view grid with TanStack Query optimistic updates plus rollback.

**Tech Stack:** FastAPI + SQLAlchemy async + Alembic + Pydantic on the backend; React 19 + TanStack Router/Query + shadcn/ui + react-i18next + `@dnd-kit/core` on the frontend; Vitest + Playwright for tests; Rust solver-core untouched (additive wire format from ADR 0027).

**Spec:** `docs/superpowers/specs/2026-05-03-sprint-c-manual-editing-design.md`.

---

## File structure

| Path | Status | Responsibility |
|---|---|---|
| `backend/src/klassenzeit_backend/db/models/scheduled_lesson.py` | edit | Add `pinned: Mapped[bool]` column. |
| `backend/alembic/versions/<rev>_add_scheduled_lesson_pinned.py` | new | One `op.add_column` + matching downgrade. |
| `backend/src/klassenzeit_backend/scheduling/solver_io.py` | edit | Extend `collect_pinned_placements` (own-class pins), add helper to load whole-school pins. |
| `backend/src/klassenzeit_backend/scheduling/schemas/placement.py` | new | `PlacementResponse`, `MovePlacementRequest`, `PinPlacementRequest`, `SwapPlacementsRequest`, `SwapPlacementsResponse`. |
| `backend/src/klassenzeit_backend/scheduling/routes/placements.py` | new | `PATCH /api/placements/{lesson_id}/{time_block_id}`, `PATCH /api/placements/{lesson_id}/{time_block_id}/pin`, `POST /api/placements/swap`. |
| `backend/src/klassenzeit_backend/scheduling/routes/__init__.py` | edit | Register the new placements router. |
| `backend/src/klassenzeit_backend/scheduling/routes/schedule.py` | edit | Accept `respect_pins: bool = True` on `/schedule/all`, thread own-pins. |
| `backend/tests/scheduling/test_placements_routes.py` | new | Integration tests for move / pin / swap. |
| `backend/tests/scheduling/test_schedule_all_respect_pins.py` | new | Integration tests for the flag. |
| `backend/tests/scheduling/test_solver_io.py` | edit | Cover own-class pin inclusion. |
| `frontend/package.json` + `pnpm-lock.yaml` | edit | Add `@dnd-kit/core`. |
| `frontend/src/lib/api-types.ts` | regen | `mise run fe:types` after backend lands. |
| `frontend/src/features/schedule/hooks.ts` | edit | New `useMovePlacement`, `usePinPlacement`, `useSwapPlacements`, plus `respect_pins` arg on the existing whole-school hook. |
| `frontend/src/features/schedule/use-schedule-drag-and-drop.ts` | new | Drag-end handler that dispatches move vs swap. |
| `frontend/src/features/schedule/schedule-grid.tsx` | edit | Wrap rows in `DndContext`, render `useDraggable` cards, render `useDroppable` slots, render the pin badge. |
| `frontend/src/features/schedule/schedule-toolbar.tsx` | edit | Add "Re-solve respecting my pins" action; relabel existing whole-school button. |
| `frontend/src/features/schedule/schedule-page-class-view.tsx` | edit | Pass `respect_pins=false` to the existing button, `true` to the new one; mount the dnd context. |
| `frontend/src/features/schedule/use-schedule-drag-and-drop.test.tsx` | new | Vitest coverage. |
| `frontend/src/features/schedule/hooks.test.tsx` | edit | Coverage for move / pin / swap mutations. |
| `frontend/src/features/schedule/schedule-grid.test.tsx` | edit | Pin badge visibility. |
| `frontend/src/features/schedule/schedule-toolbar.test.tsx` | edit | Two whole-school actions. |
| `frontend/src/locales/en.json` + `de.json` | edit | New i18n keys. |
| `e2e/tests/schedule-drag-and-drop.spec.ts` | new | One Playwright happy path. |
| `docs/adr/0028-manual-pin-semantics.md` | new | UX-level pin decisions. |
| `docs/adr/README.md` | edit | Index ADR 0028. |
| `docs/superpowers/OPEN_THINGS.md` | edit | Mark Sprint C shipped, surface follow-ups. |
| `docs/architecture/overview.md` | edit if needed | Mention manual editing if subsystem story changes. |

---

## Task 1: Schema migration for `ScheduledLesson.pinned`

**Files:**
- Edit: `backend/src/klassenzeit_backend/db/models/scheduled_lesson.py`
- Create: `backend/alembic/versions/<rev>_add_scheduled_lesson_pinned.py`
- Test: `backend/tests/scheduling/test_scheduled_lesson_pinned_column.py` (new)

- [ ] **Step 1.1: Write failing test for the model attribute and DB default.**

```python
# backend/tests/scheduling/test_scheduled_lesson_pinned_column.py
"""ScheduledLesson.pinned column round-trips False by default and accepts True."""

from __future__ import annotations

import uuid

import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson


@pytest.mark.asyncio
async def test_scheduled_lesson_pinned_defaults_false(
    db_session: AsyncSession,
    seeded_lesson_for_pinning: tuple[uuid.UUID, uuid.UUID, uuid.UUID],
) -> None:
    lesson_id, time_block_id, room_id = seeded_lesson_for_pinning
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson_id,
            time_block_id=time_block_id,
            room_id=room_id,
        )
    )
    await db_session.flush()
    row = (
        await db_session.execute(select(ScheduledLesson).where(ScheduledLesson.lesson_id == lesson_id))
    ).scalar_one()
    assert row.pinned is False


@pytest.mark.asyncio
async def test_scheduled_lesson_pinned_round_trips_true(
    db_session: AsyncSession,
    seeded_lesson_for_pinning: tuple[uuid.UUID, uuid.UUID, uuid.UUID],
) -> None:
    lesson_id, time_block_id, room_id = seeded_lesson_for_pinning
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson_id,
            time_block_id=time_block_id,
            room_id=room_id,
            pinned=True,
        )
    )
    await db_session.flush()
    row = (
        await db_session.execute(select(ScheduledLesson).where(ScheduledLesson.lesson_id == lesson_id))
    ).scalar_one()
    assert row.pinned is True
```

Add a `seeded_lesson_for_pinning` fixture to `backend/tests/scheduling/conftest.py` that uses the existing entity factories to seed one Lesson + one TimeBlock + one Room and returns their UUIDs. Reuse `create_subject`, `create_week_scheme`, `create_time_block`, `create_room`, `create_teacher`, `create_school_class` factories. The Lesson is created inline (no factory).

- [ ] **Step 1.2: Run the test, expect it to fail because `pinned` is not on the model yet.**

```bash
mise run test:py -- backend/tests/scheduling/test_scheduled_lesson_pinned_column.py -v
```

Expected: `AttributeError: ScheduledLesson has no attribute 'pinned'` or similar.

- [ ] **Step 1.3: Add the column to the model.**

```python
# backend/src/klassenzeit_backend/db/models/scheduled_lesson.py
import uuid
from datetime import datetime

from sqlalchemy import Boolean, DateTime, ForeignKey, func, text
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class ScheduledLesson(Base):
    __tablename__ = "scheduled_lessons"

    lesson_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("lessons.id", ondelete="CASCADE"), primary_key=True
    )
    time_block_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("time_blocks.id", ondelete="CASCADE"), primary_key=True
    )
    room_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("rooms.id", ondelete="CASCADE"))
    pinned: Mapped[bool] = mapped_column(
        Boolean, nullable=False, server_default=text("false")
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
```

- [ ] **Step 1.4: Generate the alembic revision.**

```bash
mise run db:up
KZ_ENV=dev uv run alembic -c backend/alembic.ini revision --autogenerate -m "add scheduled_lesson pinned"
```

Open the new file in `backend/alembic/versions/`. Tidy autogenerate-style drift per `backend/CLAUDE.md`:
- `from collections.abc import Sequence` (not `typing.Sequence`).
- `down_revision: str | None` (not `Union[str, None]`).
- Body should be exactly:

```python
def upgrade() -> None:
    op.add_column(
        "scheduled_lessons",
        sa.Column("pinned", sa.Boolean(), server_default=sa.text("false"), nullable=False),
    )


def downgrade() -> None:
    op.drop_column("scheduled_lessons", "pinned")
```

Drop the test template + per-worker DBs after pulling so the new schema reaches the test bootstrap (per `backend/CLAUDE.md`):

```bash
podman exec klassenzeit-postgres-1 psql -U klassenzeit -d postgres -c \
  "DROP DATABASE IF EXISTS klassenzeit_test_template;"
for w in gw0 gw1 gw2 gw3 gw4 gw5 gw6 gw7; do
  podman exec klassenzeit-postgres-1 psql -U klassenzeit -d postgres -c \
    "DROP DATABASE IF EXISTS klassenzeit_test_${w};"
done
```

(Names match the `klassenzeit-postgres-1` container; adjust if local naming differs.)

- [ ] **Step 1.5: Run dev migration, then re-run the test, expect it to pass.**

```bash
mise run db:migrate
mise run test:py -- backend/tests/scheduling/test_scheduled_lesson_pinned_column.py -v
```

Expected: 2 passed.

- [ ] **Step 1.6: Run the full suite to confirm nothing else broke.**

```bash
mise run test:py
```

Expected: all green.

- [ ] **Step 1.7: Commit.**

```bash
git add backend/src/klassenzeit_backend/db/models/scheduled_lesson.py \
        backend/alembic/versions/*_add_scheduled_lesson_pinned.py \
        backend/tests/scheduling/test_scheduled_lesson_pinned_column.py \
        backend/tests/scheduling/conftest.py
git commit -m "feat(backend): add scheduled_lesson.pinned column"
```

---

## Task 2: Solver IO threads own-class pins

**Files:**
- Edit: `backend/src/klassenzeit_backend/scheduling/solver_io.py`
- Edit: `backend/tests/scheduling/test_solver_io.py`

The current `collect_pinned_placements(db, exclude_class_ids)` yields wire-format dicts for every persisted ScheduledLesson whose lesson is NOT a member of any excluded class (Sprint A's "siblings only" semantic). Sprint C extends it: when `class_id` is the requested class, also include the requested class's own rows where `pinned=True`. We do this with a second helper that the route handler composes, rather than overloading the existing one (clearer name, smaller blast radius).

- [ ] **Step 2.1: Write failing tests for the new helper.**

Add to `backend/tests/scheduling/test_solver_io.py`:

```python
@pytest.mark.asyncio
async def test_collect_own_class_pins_returns_only_pinned_rows_for_class(
    db_session: AsyncSession,
    seeded_class_with_two_placements: SeededClassWithPlacements,
) -> None:
    """Pinned rows in the class are returned; unpinned rows are not."""
    pins = await solver_io.collect_own_class_pins(
        db_session, seeded_class_with_two_placements.class_id
    )
    pinned_ids = {pin["lesson_id"] for pin in pins}
    assert seeded_class_with_two_placements.pinned_lesson_id_str in pinned_ids
    assert seeded_class_with_two_placements.unpinned_lesson_id_str not in pinned_ids


@pytest.mark.asyncio
async def test_collect_own_class_pins_empty_when_class_has_none(
    db_session: AsyncSession,
    seeded_class_without_pins: uuid.UUID,
) -> None:
    pins = await solver_io.collect_own_class_pins(db_session, seeded_class_without_pins)
    assert pins == []
```

`SeededClassWithPlacements` is a NamedTuple in `conftest.py` carrying the class id, the pinned lesson id stringified, and the unpinned lesson id stringified. The fixture seeds two ScheduledLesson rows for one class, sets `pinned=True` on one. `seeded_class_without_pins` returns just a class id with no scheduled lessons.

- [ ] **Step 2.2: Run the new tests, expect failure on missing `collect_own_class_pins`.**

```bash
mise run test:py -- backend/tests/scheduling/test_solver_io.py::test_collect_own_class_pins_returns_only_pinned_rows_for_class -v
mise run test:py -- backend/tests/scheduling/test_solver_io.py::test_collect_own_class_pins_empty_when_class_has_none -v
```

Expected: `AttributeError: module ... has no attribute 'collect_own_class_pins'`.

- [ ] **Step 2.3: Add the helper.**

Append to `backend/src/klassenzeit_backend/scheduling/solver_io.py`:

```python
async def collect_own_class_pins(
    db: AsyncSession,
    class_id: UUID,
) -> list[dict[str, str]]:
    """Return wire-format pin dicts for the requested class's pinned rows.

    Pulls every ``ScheduledLesson`` whose ``Lesson`` is a member of
    ``class_id`` AND whose ``pinned`` flag is true. Output is ordered by
    ``(lesson_id, time_block_id)`` for determinism, matching
    ``collect_pinned_placements``.
    """
    own_lessons_subq = (
        select(LessonSchoolClass.lesson_id)
        .where(LessonSchoolClass.school_class_id == class_id)
        .scalar_subquery()
    )
    stmt = (
        select(ScheduledLesson)
        .where(ScheduledLesson.lesson_id.in_(own_lessons_subq))
        .where(ScheduledLesson.pinned.is_(True))
        .order_by(ScheduledLesson.lesson_id, ScheduledLesson.time_block_id)
    )
    rows = (await db.execute(stmt)).scalars().all()
    return [
        {
            "lesson_id": str(row.lesson_id),
            "time_block_id": str(row.time_block_id),
            "room_id": str(row.room_id),
        }
        for row in rows
    ]
```

Add a sibling helper for the whole-school path:

```python
async def collect_all_pins(
    db: AsyncSession,
) -> list[dict[str, str]]:
    """Return wire-format pin dicts for every ScheduledLesson with pinned=True."""
    stmt = (
        select(ScheduledLesson)
        .where(ScheduledLesson.pinned.is_(True))
        .order_by(ScheduledLesson.lesson_id, ScheduledLesson.time_block_id)
    )
    rows = (await db.execute(stmt)).scalars().all()
    return [
        {
            "lesson_id": str(row.lesson_id),
            "time_block_id": str(row.time_block_id),
            "room_id": str(row.room_id),
        }
        for row in rows
    ]
```

- [ ] **Step 2.4: Update the per-class route to compose siblings + own-class pins.**

In `backend/src/klassenzeit_backend/scheduling/routes/schedule.py`, change `generate_schedule_for_class`:

```python
sibling_pins = await solver_io.collect_pinned_placements(db, {class_id})
own_pins = await solver_io.collect_own_class_pins(db, class_id)
all_pins = sibling_pins + own_pins
problem_json, class_lesson_ids, input_counts = await solver_io.build_problem_json(
    db, class_id, pinned_placements=all_pins
)
```

The `+` order does not matter for solver semantics, but keeping siblings first preserves test fixture diff legibility.

- [ ] **Step 2.5: Run the new tests, expect pass.**

```bash
mise run test:py -- backend/tests/scheduling/test_solver_io.py::test_collect_own_class_pins_returns_only_pinned_rows_for_class -v
mise run test:py -- backend/tests/scheduling/test_solver_io.py::test_collect_own_class_pins_empty_when_class_has_none -v
```

Expected: 2 passed.

- [ ] **Step 2.6: Run the full backend suite.**

```bash
mise run test:py
```

Expected: all green. The per-class re-solve now also pins own-class `pinned=true` rows; no test currently asserts the negation, so behaviour is preserved when no row has `pinned=true`.

- [ ] **Step 2.7: Commit.**

```bash
git add backend/src/klassenzeit_backend/scheduling/solver_io.py \
        backend/src/klassenzeit_backend/scheduling/routes/schedule.py \
        backend/tests/scheduling/test_solver_io.py \
        backend/tests/scheduling/conftest.py
git commit -m "feat(backend): thread own-class pins through per-class re-solve"
```

---

## Task 3: Pydantic schemas for placement mutations

**Files:**
- Create: `backend/src/klassenzeit_backend/scheduling/schemas/placement.py`

- [ ] **Step 3.1: Write the schema module.**

```python
# backend/src/klassenzeit_backend/scheduling/schemas/placement.py
"""Pydantic schemas for the placement-mutation endpoints (Sprint C)."""

from __future__ import annotations

import uuid

from pydantic import BaseModel, ConfigDict


class PlacementResponse(BaseModel):
    """One ScheduledLesson row as seen by the placement-mutation endpoints."""

    model_config = ConfigDict(from_attributes=True)

    lesson_id: uuid.UUID
    time_block_id: uuid.UUID
    room_id: uuid.UUID
    pinned: bool


class MovePlacementRequest(BaseModel):
    """Body for `PATCH /api/placements/{lesson_id}/{time_block_id}`."""

    time_block_id: uuid.UUID
    room_id: uuid.UUID


class PinPlacementRequest(BaseModel):
    """Body for `PATCH /api/placements/{lesson_id}/{time_block_id}/pin`."""

    pinned: bool


class PlacementKey(BaseModel):
    """Composite key used inside `SwapPlacementsRequest`."""

    lesson_id: uuid.UUID
    time_block_id: uuid.UUID


class SwapPlacementsRequest(BaseModel):
    """Body for `POST /api/placements/swap`."""

    a: PlacementKey
    b: PlacementKey


class SwapPlacementsResponse(BaseModel):
    """Two `PlacementResponse`s after the swap completes."""

    a: PlacementResponse
    b: PlacementResponse
```

- [ ] **Step 3.2: Commit the schemas (no test yet; tested implicitly via route tests in Task 4).**

```bash
git add backend/src/klassenzeit_backend/scheduling/schemas/placement.py
git commit -m "feat(backend): add placement-mutation pydantic schemas"
```

---

## Task 4: Placement-mutation route handlers

**Files:**
- Create: `backend/src/klassenzeit_backend/scheduling/routes/placements.py`
- Edit: `backend/src/klassenzeit_backend/scheduling/routes/__init__.py`
- Test: `backend/tests/scheduling/test_placements_routes.py` (new)

- [ ] **Step 4.1: Add the route module skeleton (typed stubs only) so `ty` accepts the test imports.**

```python
# backend/src/klassenzeit_backend/scheduling/routes/placements.py
"""Sprint C placement-mutation endpoints addressed by composite key."""

from __future__ import annotations

import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin
from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.models.week_scheme import TimeBlock
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling.schemas.placement import (
    MovePlacementRequest,
    PinPlacementRequest,
    PlacementResponse,
    SwapPlacementsRequest,
    SwapPlacementsResponse,
)

router = APIRouter(prefix="/placements", tags=["placements"])


@router.patch("/{lesson_id}/{time_block_id}")
async def move_placement_route(
    lesson_id: uuid.UUID,
    time_block_id: uuid.UUID,
    body: MovePlacementRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> PlacementResponse:
    raise NotImplementedError("filled in step 4.4")


@router.patch("/{lesson_id}/{time_block_id}/pin")
async def pin_placement_route(
    lesson_id: uuid.UUID,
    time_block_id: uuid.UUID,
    body: PinPlacementRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> PlacementResponse:
    raise NotImplementedError("filled in step 4.4")


@router.post("/swap")
async def swap_placements_route(
    body: SwapPlacementsRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SwapPlacementsResponse:
    raise NotImplementedError("filled in step 4.4")
```

In `backend/src/klassenzeit_backend/scheduling/routes/__init__.py`, register the new router under `/api`:

```python
from klassenzeit_backend.scheduling.routes import placements as _placements_routes

# ... existing imports ...

scheduling_router.include_router(_placements_routes.router)
```

(Match the actual aggregate-router pattern already used for `lessons`, `rooms`, etc.)

- [ ] **Step 4.2: Write failing tests covering happy paths and the four validation rules.**

```python
# backend/tests/scheduling/test_placements_routes.py
"""Integration tests for Sprint C's placement-mutation endpoints."""

from __future__ import annotations

import uuid

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson


@pytest.mark.asyncio
async def test_move_placement_succeeds_and_pins(
    client: AsyncClient,
    db_session: AsyncSession,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    fixture = seeded_movable_placement
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}",
        json={
            "time_block_id": str(fixture.target_time_block_id),
            "room_id": str(fixture.target_room_id),
        },
    )
    assert response.status_code == 200
    body = response.json()
    assert body["time_block_id"] == str(fixture.target_time_block_id)
    assert body["room_id"] == str(fixture.target_room_id)
    assert body["pinned"] is True
    rows = (await db_session.execute(select(ScheduledLesson))).scalars().all()
    assert len(rows) == 1
    assert rows[0].time_block_id == fixture.target_time_block_id


@pytest.mark.asyncio
async def test_move_placement_to_nonexistent_time_block_returns_404(
    client: AsyncClient,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    fixture = seeded_movable_placement
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}",
        json={
            "time_block_id": str(uuid.uuid4()),
            "room_id": str(fixture.target_room_id),
        },
    )
    assert response.status_code == 404


@pytest.mark.asyncio
async def test_move_placement_to_other_week_scheme_returns_422(
    client: AsyncClient,
    seeded_movable_placement_cross_week: SeededCrossWeekFixture,
) -> None:
    fixture = seeded_movable_placement_cross_week
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}",
        json={
            "time_block_id": str(fixture.foreign_time_block_id),
            "room_id": str(fixture.target_room_id),
        },
    )
    assert response.status_code == 422


@pytest.mark.asyncio
async def test_pin_toggle_round_trip(
    client: AsyncClient,
    db_session: AsyncSession,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    fixture = seeded_movable_placement
    response_on = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin",
        json={"pinned": True},
    )
    assert response_on.status_code == 200
    assert response_on.json()["pinned"] is True
    response_off = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin",
        json={"pinned": False},
    )
    assert response_off.status_code == 200
    assert response_off.json()["pinned"] is False


@pytest.mark.asyncio
async def test_swap_placements_succeeds_and_pins_both(
    client: AsyncClient,
    db_session: AsyncSession,
    seeded_two_placements_for_swap: SeededTwoPlacements,
) -> None:
    fixture = seeded_two_placements_for_swap
    response = await client.post(
        "/api/placements/swap",
        json={
            "a": {
                "lesson_id": str(fixture.lesson_a_id),
                "time_block_id": str(fixture.time_block_a_id),
            },
            "b": {
                "lesson_id": str(fixture.lesson_b_id),
                "time_block_id": str(fixture.time_block_b_id),
            },
        },
    )
    assert response.status_code == 200
    body = response.json()
    assert body["a"]["lesson_id"] == str(fixture.lesson_a_id)
    assert body["a"]["time_block_id"] == str(fixture.time_block_b_id)
    assert body["a"]["pinned"] is True
    assert body["b"]["lesson_id"] == str(fixture.lesson_b_id)
    assert body["b"]["time_block_id"] == str(fixture.time_block_a_id)
    assert body["b"]["pinned"] is True


@pytest.mark.asyncio
async def test_swap_placements_with_missing_b_returns_404_and_rolls_back(
    client: AsyncClient,
    db_session: AsyncSession,
    seeded_two_placements_for_swap: SeededTwoPlacements,
) -> None:
    fixture = seeded_two_placements_for_swap
    response = await client.post(
        "/api/placements/swap",
        json={
            "a": {
                "lesson_id": str(fixture.lesson_a_id),
                "time_block_id": str(fixture.time_block_a_id),
            },
            "b": {
                "lesson_id": str(uuid.uuid4()),
                "time_block_id": str(uuid.uuid4()),
            },
        },
    )
    assert response.status_code == 404
    a_row = (
        await db_session.execute(
            select(ScheduledLesson).where(ScheduledLesson.lesson_id == fixture.lesson_a_id)
        )
    ).scalar_one()
    assert a_row.time_block_id == fixture.time_block_a_id
    assert a_row.pinned is False
```

Add three new fixtures to `backend/tests/scheduling/conftest.py`:

- `seeded_movable_placement` (NamedTuple `SeededMovablePlacement` with `lesson_id`, `source_time_block_id`, `target_time_block_id`, `target_room_id`): seeds one Lesson + two TimeBlocks in the same WeekScheme + one Room + one ScheduledLesson at `source_time_block_id` with `pinned=False`.
- `seeded_movable_placement_cross_week` (NamedTuple `SeededCrossWeekFixture` with `lesson_id`, `source_time_block_id`, `foreign_time_block_id`, `target_room_id`): like above but `foreign_time_block_id` belongs to a different WeekScheme owned by a different SchoolClass.
- `seeded_two_placements_for_swap` (NamedTuple `SeededTwoPlacements` with `lesson_a_id`, `time_block_a_id`, `lesson_b_id`, `time_block_b_id`, `room_id`): two Lessons (same class, same week scheme), two TimeBlocks, both pinned=False initially.

Define the NamedTuples at module top of `conftest.py`.

- [ ] **Step 4.3: Run the new tests, expect failure on the `NotImplementedError` from the stubs.**

```bash
mise run test:py -- backend/tests/scheduling/test_placements_routes.py -v
```

Expected: 6 failures.

- [ ] **Step 4.4: Implement the route handlers.**

Replace the stub bodies in `backend/src/klassenzeit_backend/scheduling/routes/placements.py`:

```python
async def _load_placement_or_404(
    db: AsyncSession,
    lesson_id: uuid.UUID,
    time_block_id: uuid.UUID,
) -> ScheduledLesson:
    stmt = select(ScheduledLesson).where(
        ScheduledLesson.lesson_id == lesson_id,
        ScheduledLesson.time_block_id == time_block_id,
    )
    row = (await db.execute(stmt)).scalar_one_or_none()
    if row is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"placement {lesson_id}/{time_block_id} not found",
        )
    return row


async def _load_time_block_or_404(db: AsyncSession, time_block_id: uuid.UUID) -> TimeBlock:
    tb = (
        await db.execute(select(TimeBlock).where(TimeBlock.id == time_block_id))
    ).scalar_one_or_none()
    if tb is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"time block {time_block_id} not found",
        )
    return tb


async def _load_room_or_404(db: AsyncSession, room_id: uuid.UUID) -> Room:
    room = (
        await db.execute(select(Room).where(Room.id == room_id))
    ).scalar_one_or_none()
    if room is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"room {room_id} not found",
        )
    return room


async def _assert_lesson_week_scheme_matches(
    db: AsyncSession,
    lesson_id: uuid.UUID,
    target_time_block: TimeBlock,
) -> None:
    """Reject moves that cross week schemes.

    A lesson is reachable from a class via the LessonSchoolClass association;
    the class carries the week_scheme_id. The target time_block must live in
    that same week_scheme.
    """
    rows = (
        await db.execute(
            select(SchoolClass.week_scheme_id)
            .join(LessonSchoolClass, LessonSchoolClass.school_class_id == SchoolClass.id)
            .where(LessonSchoolClass.lesson_id == lesson_id)
        )
    ).scalars().all()
    if not rows:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail=f"lesson {lesson_id} has no class membership",
        )
    if any(row != target_time_block.week_scheme_id for row in rows):
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail=(
                f"target time block {target_time_block.id} belongs to a "
                "different week scheme than the lesson's class"
            ),
        )


@router.patch("/{lesson_id}/{time_block_id}")
async def move_placement_route(
    lesson_id: uuid.UUID,
    time_block_id: uuid.UUID,
    body: MovePlacementRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> PlacementResponse:
    placement = await _load_placement_or_404(db, lesson_id, time_block_id)
    target_tb = await _load_time_block_or_404(db, body.time_block_id)
    await _load_room_or_404(db, body.room_id)
    await _assert_lesson_week_scheme_matches(db, lesson_id, target_tb)
    if body.time_block_id != time_block_id:
        # Drop the old composite-PK row before inserting the new one to avoid
        # a transient duplicate during the same transaction.
        await db.delete(placement)
        await db.flush()
        placement = ScheduledLesson(
            lesson_id=lesson_id,
            time_block_id=body.time_block_id,
            room_id=body.room_id,
            pinned=True,
        )
        db.add(placement)
    else:
        placement.room_id = body.room_id
        placement.pinned = True
    await db.flush()
    return PlacementResponse.model_validate(placement)


@router.patch("/{lesson_id}/{time_block_id}/pin")
async def pin_placement_route(
    lesson_id: uuid.UUID,
    time_block_id: uuid.UUID,
    body: PinPlacementRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> PlacementResponse:
    placement = await _load_placement_or_404(db, lesson_id, time_block_id)
    placement.pinned = body.pinned
    await db.flush()
    return PlacementResponse.model_validate(placement)


@router.post("/swap")
async def swap_placements_route(
    body: SwapPlacementsRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SwapPlacementsResponse:
    placement_a = await _load_placement_or_404(db, body.a.lesson_id, body.a.time_block_id)
    placement_b = await _load_placement_or_404(db, body.b.lesson_id, body.b.time_block_id)
    a_target_tb = await _load_time_block_or_404(db, body.b.time_block_id)
    b_target_tb = await _load_time_block_or_404(db, body.a.time_block_id)
    await _assert_lesson_week_scheme_matches(db, body.a.lesson_id, a_target_tb)
    await _assert_lesson_week_scheme_matches(db, body.b.lesson_id, b_target_tb)
    a_room = placement_a.room_id
    b_room = placement_b.room_id
    await db.delete(placement_a)
    await db.delete(placement_b)
    await db.flush()
    new_a = ScheduledLesson(
        lesson_id=body.a.lesson_id,
        time_block_id=body.b.time_block_id,
        room_id=b_room,
        pinned=True,
    )
    new_b = ScheduledLesson(
        lesson_id=body.b.lesson_id,
        time_block_id=body.a.time_block_id,
        room_id=a_room,
        pinned=True,
    )
    db.add(new_a)
    db.add(new_b)
    await db.flush()
    return SwapPlacementsResponse(
        a=PlacementResponse.model_validate(new_a),
        b=PlacementResponse.model_validate(new_b),
    )
```

- [ ] **Step 4.5: Run the new tests, expect pass.**

```bash
mise run test:py -- backend/tests/scheduling/test_placements_routes.py -v
```

Expected: 6 passed.

- [ ] **Step 4.6: Run full backend suite + lint.**

```bash
mise run test:py
mise run lint
```

Both green.

- [ ] **Step 4.7: Commit.**

```bash
git add backend/src/klassenzeit_backend/scheduling/routes/placements.py \
        backend/src/klassenzeit_backend/scheduling/routes/__init__.py \
        backend/tests/scheduling/test_placements_routes.py \
        backend/tests/scheduling/conftest.py
git commit -m "feat(backend): add move, pin, and swap placement endpoints"
```

---

## Task 5: `respect_pins` flag on `POST /api/schedule/all`

**Files:**
- Edit: `backend/src/klassenzeit_backend/scheduling/routes/schedule.py`
- Test: `backend/tests/scheduling/test_schedule_all_respect_pins.py` (new)

- [ ] **Step 5.1: Write failing tests.**

```python
# backend/tests/scheduling/test_schedule_all_respect_pins.py
"""POST /api/schedule/all?respect_pins=... behaves correctly."""

from __future__ import annotations

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson


@pytest.mark.asyncio
async def test_schedule_all_default_respects_pins(
    client: AsyncClient,
    db_session: AsyncSession,
    seeded_dreizuegig_with_one_pin: SeededDreizuegigWithPin,
) -> None:
    """No flag passed: default to respect_pins=true; pinned slot survives."""
    response = await client.post("/api/schedule/all")
    assert response.status_code == 200
    fixture = seeded_dreizuegig_with_one_pin
    pinned = (
        await db_session.execute(
            select(ScheduledLesson).where(ScheduledLesson.lesson_id == fixture.pinned_lesson_id)
        )
    ).scalar_one()
    assert pinned.time_block_id == fixture.pinned_time_block_id
    assert pinned.pinned is True


@pytest.mark.asyncio
async def test_schedule_all_respect_pins_false_keeps_pin_state(
    client: AsyncClient,
    db_session: AsyncSession,
    seeded_dreizuegig_with_one_pin: SeededDreizuegigWithPin,
) -> None:
    """respect_pins=false: solver may move the pinned lesson; flag stays in DB."""
    fixture = seeded_dreizuegig_with_one_pin
    response = await client.post("/api/schedule/all?respect_pins=false")
    assert response.status_code == 200
    pinned_rows = (
        await db_session.execute(
            select(ScheduledLesson).where(ScheduledLesson.lesson_id == fixture.pinned_lesson_id)
        )
    ).scalars().all()
    assert len(pinned_rows) == 1
    # Pin state is preserved across the run; only the slot may have changed.
    assert pinned_rows[0].pinned is True
```

`seeded_dreizuegig_with_one_pin` is a fixture that runs the existing dreizügige solvability seed (already used elsewhere) then sets `pinned=True` on one ScheduledLesson row, returning a NamedTuple with the pinned lesson id and its current time_block_id. Reuse the seed factory under `backend/tests/seed/`.

- [ ] **Step 5.2: Run, expect both to fail.**

```bash
mise run test:py -- backend/tests/scheduling/test_schedule_all_respect_pins.py -v
```

Expected: 2 failures (today's route ignores the flag).

- [ ] **Step 5.3: Add the flag to the route.**

In `backend/src/klassenzeit_backend/scheduling/routes/schedule.py`:

```python
@router.post("/schedule/all")
async def generate_schedule_for_all_classes(
    request: Request,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
    respect_pins: bool = True,
) -> WholeSchoolScheduleResponse:
    """Run the solver for every class in one transaction and persist atomically.

    When ``respect_pins`` is true (default), every ``ScheduledLesson`` with
    ``pinned=true`` is fed into the solver as a hard pin. When false, pins
    are ignored for this run; the database flag is unchanged.
    """
    pins = await solver_io.collect_all_pins(db) if respect_pins else []
    problem_json, _, input_counts = await solver_io.build_problem_json(
        db, class_id=None, pinned_placements=pins
    )
    deadline_ms = request.app.state.settings.solve_deadline_ms
    solution = await solver_io.run_solve(
        problem_json, scope_id=None, input_counts=input_counts, deadline_ms=deadline_ms
    )
    summaries = await solver_io.persist_solution_for_all_classes(db, solution)
    return WholeSchoolScheduleResponse(
        classes=summaries,
        total_placements=sum(s.placements_count for s in summaries),
        total_violations=sum(s.violations_count for s in summaries),
    )
```

`persist_solution_for_all_classes` already overwrites everything; pinned rows from the input are reproduced in the output (the solver returns them) and persisted. We must preserve the `pinned` flag on those rows. Update `persist_solution_for_all_classes` (and `persist_solution_for_class` correspondingly) to set `pinned=True` for any output placement whose `(lesson_id, time_block_id)` was in the input pin set; everything else gets `pinned=False`. Implementation detail: pass `pinned_keys: set[tuple[UUID, UUID]]` into the persist helper.

- [ ] **Step 5.4: Update the persist helpers.**

In `solver_io.py`, change the helper signatures:

```python
async def persist_solution_for_class(
    db: AsyncSession,
    class_id: UUID,
    filtered: dict,
    *,
    pinned_keys: set[tuple[UUID, UUID]] | None = None,
) -> None:
    ...
    pinned_lookup = pinned_keys or set()
    for placement in filtered["placements"]:
        lesson_uuid = UUID(placement["lesson_id"])
        time_block_uuid = UUID(placement["time_block_id"])
        room_uuid = UUID(placement["room_id"])
        is_pinned = (lesson_uuid, time_block_uuid) in pinned_lookup
        db.add(ScheduledLesson(
            lesson_id=lesson_uuid,
            time_block_id=time_block_uuid,
            room_id=room_uuid,
            pinned=is_pinned,
        ))
```

(Apply the same change to `persist_solution_for_all_classes`.)

The two route handlers compute `pinned_keys` from their input pin lists and pass them through:

- `generate_schedule_for_all_classes`: `pinned_keys = {(UUID(p["lesson_id"]), UUID(p["time_block_id"])) for p in pins}` and pass to `persist_solution_for_all_classes`.
- `generate_schedule_for_class`: `pinned_keys = {(UUID(p["lesson_id"]), UUID(p["time_block_id"])) for p in own_pins}` (siblings are NOT this class's responsibility to flag) and pass to `persist_solution_for_class`.

- [ ] **Step 5.5: Run the new tests, expect pass.**

```bash
mise run test:py -- backend/tests/scheduling/test_schedule_all_respect_pins.py -v
```

Expected: 2 passed.

- [ ] **Step 5.6: Run full backend suite. Existing tests previously asserting "from-scratch" outcomes must still pass because all initially-seeded rows have `pinned=False`.**

```bash
mise run test:py
mise run lint
```

Both green.

- [ ] **Step 5.7: Commit.**

```bash
git add backend/src/klassenzeit_backend/scheduling/routes/schedule.py \
        backend/src/klassenzeit_backend/scheduling/solver_io.py \
        backend/tests/scheduling/test_schedule_all_respect_pins.py \
        backend/tests/scheduling/conftest.py
git commit -m "feat(backend): add respect_pins flag to whole-school re-solve"
```

---

## Task 6: Frontend types + i18n keys + dnd-kit dependency

**Files:**
- Edit: `frontend/package.json`, `pnpm-lock.yaml`
- Regen: `frontend/src/lib/api-types.ts`
- Edit: `frontend/src/locales/en.json`, `de.json`

- [ ] **Step 6.1: Add `@dnd-kit/core` dependency.**

```bash
cd frontend && pnpm add @dnd-kit/core
```

Confirm `package.json` has `"@dnd-kit/core": "^<version>"` under `dependencies`. Lockfile updates automatically.

- [ ] **Step 6.2: Regenerate API types from the freshly built backend.**

```bash
mise run dev &  # starts backend on :8000; ctrl-c after types regenerate
mise run fe:types
```

Verify `frontend/src/lib/api-types.ts` now exposes:
- `paths["/api/placements/{lesson_id}/{time_block_id}"]["patch"]`
- `paths["/api/placements/{lesson_id}/{time_block_id}/pin"]["patch"]`
- `paths["/api/placements/swap"]["post"]`
- `paths["/api/schedule/all"]["post"]` query param `respect_pins`

- [ ] **Step 6.3: Add i18n keys (en).**

Append to `frontend/src/locales/en.json` under `schedule`:

```json
{
  "actions": {
    "pin": "Pin this lesson",
    "unpin": "Unpin this lesson",
    "moveHandle": "Drag to move"
  },
  "toasts": {
    "moveSuccess": "Lesson moved.",
    "swapSuccess": "Lessons swapped.",
    "pinned": "Lesson pinned.",
    "unpinned": "Lesson unpinned.",
    "mutationError": "Could not save change. Reverted."
  },
  "generate": {
    "respectPinsAction": "Re-solve respecting my pins",
    "respectPinsSuccessToast": "Schedule re-solved; your pins were preserved.",
    "fromScratchAction": "Generate all from scratch",
    "fromScratchSuccessToast": "Schedule regenerated from scratch."
  }
}
```

(Slot the new keys into the existing object under `schedule`; don't duplicate the parent.)

- [ ] **Step 6.4: Add i18n keys (de).** Mirror the structure with German copy:

```json
{
  "actions": {
    "pin": "Stunde anpinnen",
    "unpin": "Stunde lösen",
    "moveHandle": "Zum Verschieben ziehen"
  },
  "toasts": {
    "moveSuccess": "Stunde verschoben.",
    "swapSuccess": "Stunden getauscht.",
    "pinned": "Stunde angepinnt.",
    "unpinned": "Stunde gelöst.",
    "mutationError": "Änderung konnte nicht gespeichert werden. Zurückgesetzt."
  },
  "generate": {
    "respectPinsAction": "Mit meinen Pins neu rechnen",
    "respectPinsSuccessToast": "Plan neu gerechnet; angepinnte Stunden wurden beibehalten.",
    "fromScratchAction": "Komplett neu generieren",
    "fromScratchSuccessToast": "Plan komplett neu generiert."
  }
}
```

- [ ] **Step 6.5: Run frontend lint + type check + tests.**

```bash
mise run fe:test
mise run lint
```

Expected: all green. (No new behaviour yet; just dependency + types + strings.)

- [ ] **Step 6.6: Commit.**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml \
        frontend/src/lib/api-types.ts \
        frontend/src/locales/en.json frontend/src/locales/de.json
git commit -m "build(frontend): add @dnd-kit/core, regenerate types, add Sprint C i18n keys"
```

---

## Task 7: Pin toggle UI on schedule grid

**Files:**
- Edit: `frontend/src/features/schedule/hooks.ts`
- Edit: `frontend/src/features/schedule/schedule-grid.tsx`
- Edit: `frontend/src/features/schedule/hooks.test.tsx`
- Edit: `frontend/src/features/schedule/schedule-grid.test.tsx`

- [ ] **Step 7.1: Write failing test for `usePinPlacement` hook.**

In `hooks.test.tsx`, add:

```tsx
it("usePinPlacement PATCHes the pin endpoint and invalidates schedule queries", async () => {
  const queryClient = new QueryClient();
  const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
  const lesson_id = "00000000-0000-0000-0000-000000000001";
  const time_block_id = "00000000-0000-0000-0000-000000000002";
  server.use(
    http.patch(`http://localhost/api/placements/${lesson_id}/${time_block_id}/pin`, () =>
      HttpResponse.json({
        lesson_id,
        time_block_id,
        room_id: "00000000-0000-0000-0000-000000000003",
        pinned: true,
      }),
    ),
  );
  const { result } = renderHook(() => usePinPlacement(), { wrapper: wrapScheduleHook(queryClient) });
  await result.current.mutateAsync({ lesson_id, time_block_id, pinned: true });
  expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["schedule"] });
});
```

(`wrapScheduleHook` already exists from Sprint B; reuse it. `server` is the existing MSW server.)

- [ ] **Step 7.2: Run, expect failure on missing `usePinPlacement`.**

```bash
mise run fe:test -- src/features/schedule/hooks.test.tsx -t usePinPlacement
```

Expected: ReferenceError or similar.

- [ ] **Step 7.3: Implement `usePinPlacement`.**

In `hooks.ts`:

```ts
export interface PinPlacementVars {
  lesson_id: string;
  time_block_id: string;
  pinned: boolean;
}

export function usePinPlacement() {
  const queryClient = useQueryClient();
  const apiClient = useApiClient();
  return useMutation({
    mutationFn: async (vars: PinPlacementVars) => {
      const { lesson_id, time_block_id, pinned } = vars;
      const { data, error } = await apiClient.PATCH(
        "/api/placements/{lesson_id}/{time_block_id}/pin",
        {
          params: { path: { lesson_id, time_block_id } },
          body: { pinned },
        },
      );
      if (error) {
        throw new Error(typeof error === "string" ? error : JSON.stringify(error));
      }
      return data;
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["schedule"] }),
  });
}
```

- [ ] **Step 7.4: Run the test, expect pass.**

```bash
mise run fe:test -- src/features/schedule/hooks.test.tsx -t usePinPlacement
```

- [ ] **Step 7.5: Write failing test for the pin badge in `schedule-grid.test.tsx`.**

```tsx
it("renders a pin icon on cells whose placement is pinned", () => {
  const placements = [
    samplePlacement({ pinned: true, lesson_id: "L1" }),
    samplePlacement({ pinned: false, lesson_id: "L2" }),
  ];
  renderScheduleGridWith({ placements });
  expect(screen.getByLabelText("Lesson L1 is pinned")).toBeInTheDocument();
  expect(screen.queryByLabelText("Lesson L2 is pinned")).not.toBeInTheDocument();
});
```

(`samplePlacement` factory + `renderScheduleGridWith` helper already exist or get created here; if missing, add them at module top with unique names `samplePinnedPlacement` etc. per the unique-function-names rule.)

- [ ] **Step 7.6: Run, expect failure.**

```bash
mise run fe:test -- src/features/schedule/schedule-grid.test.tsx -t pin
```

- [ ] **Step 7.7: Implement the pin badge in `schedule-grid.tsx`.**

Inside the cell render, where the placement card is composed:

```tsx
import { Pin, PinOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import { usePinPlacement } from "./hooks";

// inside the placement-card render
const { t } = useTranslation();
const pinMutation = usePinPlacement();
const ariaLabel = placement.pinned
  ? t("schedule.actions.unpin")
  : t("schedule.actions.pin");

return (
  <div className={cn(
    "schedule-cell",
    placement.pinned && "border-primary/40",
  )}>
    {/* existing subject/teacher/room markup */}
    <button
      type="button"
      aria-label={
        placement.pinned
          ? `Lesson ${placement.lesson_id} is pinned`
          : `Pin lesson ${placement.lesson_id}`
      }
      title={ariaLabel}
      className={cn(
        "absolute right-1 top-1 rounded p-0.5",
        placement.pinned ? "text-primary" : "text-muted-foreground opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
      )}
      onClick={() => pinMutation.mutate({
        lesson_id: placement.lesson_id,
        time_block_id: placement.time_block_id,
        pinned: !placement.pinned,
      })}
    >
      {placement.pinned ? <Pin className="h-3.5 w-3.5" /> : <PinOff className="h-3.5 w-3.5" />}
    </button>
  </div>
);
```

(Add `group` to the surrounding cell wrapper so `group-hover:` works.)

- [ ] **Step 7.8: Run tests + lint.**

```bash
mise run fe:test
mise run lint
```

Both green.

- [ ] **Step 7.9: Commit.**

```bash
git add frontend/src/features/schedule/hooks.ts \
        frontend/src/features/schedule/schedule-grid.tsx \
        frontend/src/features/schedule/hooks.test.tsx \
        frontend/src/features/schedule/schedule-grid.test.tsx
git commit -m "feat(frontend): pin toggle on schedule grid cells"
```

---

## Task 8: Drag-and-drop integration

**Files:**
- Edit: `frontend/src/features/schedule/hooks.ts`
- Create: `frontend/src/features/schedule/use-schedule-drag-and-drop.ts`
- Edit: `frontend/src/features/schedule/schedule-grid.tsx`
- Edit: `frontend/src/features/schedule/schedule-page-class-view.tsx`
- Test: `frontend/src/features/schedule/use-schedule-drag-and-drop.test.tsx` (new)
- Edit: `frontend/src/features/schedule/hooks.test.tsx`

- [ ] **Step 8.1: Write failing tests for `useMovePlacement` and `useSwapPlacements`.**

Add to `hooks.test.tsx`:

```tsx
it("useMovePlacement PATCHes the move endpoint and invalidates schedule queries", async () => {
  const queryClient = new QueryClient();
  const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
  server.use(
    http.patch(
      "http://localhost/api/placements/L1/TB1",
      () => HttpResponse.json({
        lesson_id: "L1",
        time_block_id: "TB2",
        room_id: "R2",
        pinned: true,
      }),
    ),
  );
  const { result } = renderHook(() => useMovePlacement(), { wrapper: wrapScheduleHook(queryClient) });
  await result.current.mutateAsync({
    lesson_id: "L1",
    source_time_block_id: "TB1",
    time_block_id: "TB2",
    room_id: "R2",
  });
  expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["schedule"] });
});

it("useSwapPlacements POSTs the swap endpoint and invalidates schedule queries", async () => {
  const queryClient = new QueryClient();
  const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
  server.use(
    http.post("http://localhost/api/placements/swap", () =>
      HttpResponse.json({
        a: { lesson_id: "L1", time_block_id: "TB2", room_id: "R2", pinned: true },
        b: { lesson_id: "L2", time_block_id: "TB1", room_id: "R1", pinned: true },
      }),
    ),
  );
  const { result } = renderHook(() => useSwapPlacements(), { wrapper: wrapScheduleHook(queryClient) });
  await result.current.mutateAsync({
    a: { lesson_id: "L1", time_block_id: "TB1" },
    b: { lesson_id: "L2", time_block_id: "TB2" },
  });
  expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["schedule"] });
});
```

- [ ] **Step 8.2: Run, expect failures on missing exports.**

```bash
mise run fe:test -- src/features/schedule/hooks.test.tsx -t "useMovePlacement|useSwapPlacements"
```

- [ ] **Step 8.3: Implement the two mutation hooks in `hooks.ts`.**

```ts
export interface MovePlacementVars {
  lesson_id: string;
  source_time_block_id: string;
  time_block_id: string;
  room_id: string;
}

export function useMovePlacement() {
  const queryClient = useQueryClient();
  const apiClient = useApiClient();
  return useMutation({
    mutationFn: async (vars: MovePlacementVars) => {
      const { lesson_id, source_time_block_id, time_block_id, room_id } = vars;
      const { data, error } = await apiClient.PATCH(
        "/api/placements/{lesson_id}/{time_block_id}",
        {
          params: { path: { lesson_id, time_block_id: source_time_block_id } },
          body: { time_block_id, room_id },
        },
      );
      if (error) {
        throw new Error(typeof error === "string" ? error : JSON.stringify(error));
      }
      return data;
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["schedule"] }),
  });
}

export interface SwapPlacementsVars {
  a: { lesson_id: string; time_block_id: string };
  b: { lesson_id: string; time_block_id: string };
}

export function useSwapPlacements() {
  const queryClient = useQueryClient();
  const apiClient = useApiClient();
  return useMutation({
    mutationFn: async (vars: SwapPlacementsVars) => {
      const { data, error } = await apiClient.POST("/api/placements/swap", { body: vars });
      if (error) {
        throw new Error(typeof error === "string" ? error : JSON.stringify(error));
      }
      return data;
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["schedule"] }),
  });
}
```

- [ ] **Step 8.4: Run hook tests, expect pass.**

```bash
mise run fe:test -- src/features/schedule/hooks.test.tsx
```

- [ ] **Step 8.5: Write failing test for `useScheduleDragAndDrop`.**

```tsx
// frontend/src/features/schedule/use-schedule-drag-and-drop.test.tsx
import { renderHook, act } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useScheduleDragAndDrop } from "./use-schedule-drag-and-drop";

describe("useScheduleDragAndDrop", () => {
  it("dispatches move when dropping onto an empty slot", () => {
    const move = vi.fn();
    const swap = vi.fn();
    const { result } = renderHook(() =>
      useScheduleDragAndDrop({
        moveMutate: move,
        swapMutate: swap,
        placementByCell: new Map([
          ["TB1::R1", { lesson_id: "L1", time_block_id: "TB1", room_id: "R1" }],
        ]),
      }),
    );
    act(() => {
      result.current.onDragEnd({
        active: { id: "L1::TB1", data: { current: { lesson_id: "L1", source_time_block_id: "TB1" } } },
        over: { id: "TB2::R2", data: { current: { time_block_id: "TB2", room_id: "R2" } } },
      } as never);
    });
    expect(move).toHaveBeenCalledWith({
      lesson_id: "L1",
      source_time_block_id: "TB1",
      time_block_id: "TB2",
      room_id: "R2",
    });
    expect(swap).not.toHaveBeenCalled();
  });

  it("dispatches swap when dropping onto an occupied slot", () => {
    const move = vi.fn();
    const swap = vi.fn();
    const { result } = renderHook(() =>
      useScheduleDragAndDrop({
        moveMutate: move,
        swapMutate: swap,
        placementByCell: new Map([
          ["TB1::R1", { lesson_id: "L1", time_block_id: "TB1", room_id: "R1" }],
          ["TB2::R2", { lesson_id: "L2", time_block_id: "TB2", room_id: "R2" }],
        ]),
      }),
    );
    act(() => {
      result.current.onDragEnd({
        active: { id: "L1::TB1", data: { current: { lesson_id: "L1", source_time_block_id: "TB1" } } },
        over: { id: "TB2::R2", data: { current: { time_block_id: "TB2", room_id: "R2" } } },
      } as never);
    });
    expect(swap).toHaveBeenCalledWith({
      a: { lesson_id: "L1", time_block_id: "TB1" },
      b: { lesson_id: "L2", time_block_id: "TB2" },
    });
    expect(move).not.toHaveBeenCalled();
  });

  it("is a no-op when over is null", () => {
    const move = vi.fn();
    const swap = vi.fn();
    const { result } = renderHook(() =>
      useScheduleDragAndDrop({ moveMutate: move, swapMutate: swap, placementByCell: new Map() }),
    );
    act(() => {
      result.current.onDragEnd({
        active: { id: "L1::TB1", data: { current: { lesson_id: "L1", source_time_block_id: "TB1" } } },
        over: null,
      } as never);
    });
    expect(move).not.toHaveBeenCalled();
    expect(swap).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 8.6: Run, expect failure on missing module.**

```bash
mise run fe:test -- src/features/schedule/use-schedule-drag-and-drop.test.tsx
```

- [ ] **Step 8.7: Implement the hook.**

```ts
// frontend/src/features/schedule/use-schedule-drag-and-drop.ts
import type { DragEndEvent } from "@dnd-kit/core";
import type { MovePlacementVars, SwapPlacementsVars } from "./hooks";

export interface PlacementCellRef {
  lesson_id: string;
  time_block_id: string;
  room_id: string;
}

export interface ScheduleDragAndDropArgs {
  moveMutate: (vars: MovePlacementVars) => unknown;
  swapMutate: (vars: SwapPlacementsVars) => unknown;
  placementByCell: Map<string, PlacementCellRef>;
}

export interface ScheduleDragAndDropApi {
  onDragEnd: (event: DragEndEvent) => void;
}

export function buildCellKey(time_block_id: string, room_id: string): string {
  return `${time_block_id}::${room_id}`;
}

export function useScheduleDragAndDrop(args: ScheduleDragAndDropArgs): ScheduleDragAndDropApi {
  return {
    onDragEnd(event) {
      if (event.over === null) return;
      const activeData = event.active.data.current as
        | { lesson_id: string; source_time_block_id: string }
        | undefined;
      const overData = event.over.data.current as
        | { time_block_id: string; room_id: string }
        | undefined;
      if (activeData === undefined || overData === undefined) return;
      const targetKey = buildCellKey(overData.time_block_id, overData.room_id);
      const occupant = args.placementByCell.get(targetKey);
      if (occupant !== undefined && occupant.lesson_id !== activeData.lesson_id) {
        args.swapMutate({
          a: { lesson_id: activeData.lesson_id, time_block_id: activeData.source_time_block_id },
          b: { lesson_id: occupant.lesson_id, time_block_id: occupant.time_block_id },
        });
        return;
      }
      if (occupant !== undefined && occupant.lesson_id === activeData.lesson_id) return;
      args.moveMutate({
        lesson_id: activeData.lesson_id,
        source_time_block_id: activeData.source_time_block_id,
        time_block_id: overData.time_block_id,
        room_id: overData.room_id,
      });
    },
  };
}
```

- [ ] **Step 8.8: Run dnd test, expect pass.**

```bash
mise run fe:test -- src/features/schedule/use-schedule-drag-and-drop.test.tsx
```

Expected: 3 passed.

- [ ] **Step 8.9: Wire `<DndContext>` + draggable/droppable into `schedule-grid.tsx`.**

```tsx
import { DndContext, useDraggable, useDroppable } from "@dnd-kit/core";
import { useScheduleDragAndDrop, buildCellKey, type PlacementCellRef } from "./use-schedule-drag-and-drop";
import { useMovePlacement, useSwapPlacements } from "./hooks";

// Inside ScheduleGrid:
const moveMutation = useMovePlacement();
const swapMutation = useSwapPlacements();
const placementByCell = useMemo(() => {
  const map = new Map<string, PlacementCellRef>();
  for (const p of placements) {
    map.set(buildCellKey(p.time_block_id, p.room_id), {
      lesson_id: p.lesson_id,
      time_block_id: p.time_block_id,
      room_id: p.room_id,
    });
  }
  return map;
}, [placements]);
const { onDragEnd } = useScheduleDragAndDrop({
  moveMutate: moveMutation.mutate,
  swapMutate: swapMutation.mutate,
  placementByCell,
});

return (
  <DndContext onDragEnd={onDragEnd}>
    {/* existing grid markup, but each placement card uses useDraggable and each cell uses useDroppable */}
  </DndContext>
);
```

For each placement card:

```tsx
function ScheduleCellDraggable({ placement }: { placement: PlacementResponse }) {
  const { attributes, listeners, setNodeRef, transform } = useDraggable({
    id: `${placement.lesson_id}::${placement.time_block_id}`,
    data: { lesson_id: placement.lesson_id, source_time_block_id: placement.time_block_id },
    disabled: placement.pinned,  // pinned cards are not draggable; user must unpin first
  });
  // render...
}
```

For each cell wrapper (whether occupied or empty):

```tsx
function ScheduleCellDroppable({ time_block_id, room_id, children }: ...) {
  const { setNodeRef } = useDroppable({
    id: `${time_block_id}::${room_id}`,
    data: { time_block_id, room_id },
  });
  return <div ref={setNodeRef}>{children}</div>;
}
```

(Decide naming such that `useDraggable` and `useDroppable` ids are unique across the full grid. The composite-id form satisfies that.)

- [ ] **Step 8.10: Run frontend tests + lint.**

```bash
mise run fe:test
mise run lint
```

Both green.

- [ ] **Step 8.11: Commit.**

```bash
git add frontend/src/features/schedule/hooks.ts \
        frontend/src/features/schedule/use-schedule-drag-and-drop.ts \
        frontend/src/features/schedule/use-schedule-drag-and-drop.test.tsx \
        frontend/src/features/schedule/schedule-grid.tsx \
        frontend/src/features/schedule/schedule-page-class-view.tsx \
        frontend/src/features/schedule/hooks.test.tsx
git commit -m "feat(frontend): drag-and-drop schedule editing via @dnd-kit/core"
```

---

## Task 9: "Re-solve respecting my pins" toolbar action

**Files:**
- Edit: `frontend/src/features/schedule/hooks.ts`
- Edit: `frontend/src/features/schedule/schedule-toolbar.tsx`
- Edit: `frontend/src/features/schedule/schedule-page-class-view.tsx`
- Edit: `frontend/src/features/schedule/schedule-toolbar.test.tsx`

- [ ] **Step 9.1: Update `useGenerateAllSchedules` to accept `respect_pins`.**

In `hooks.ts`, change the existing hook to thread the flag:

```ts
export interface GenerateAllSchedulesVars {
  respect_pins: boolean;
}

export function useGenerateAllSchedules() {
  const queryClient = useQueryClient();
  const apiClient = useApiClient();
  return useMutation({
    mutationFn: async (vars: GenerateAllSchedulesVars) => {
      const { data, error } = await apiClient.POST("/api/schedule/all", {
        params: { query: { respect_pins: vars.respect_pins } },
      });
      if (error) {
        throw new Error(typeof error === "string" ? error : JSON.stringify(error));
      }
      return data;
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["schedule"] }),
  });
}
```

If the hook signature was previously `mutate()` with no args, every existing caller must now pass `{ respect_pins: false }` (the original Sprint A behaviour). Grep `useGenerateAllSchedules` first.

- [ ] **Step 9.2: Write failing toolbar test.**

In `schedule-toolbar.test.tsx`, add:

```tsx
it("class view renders both whole-school actions and dispatches the right respect_pins flag", async () => {
  const generate = vi.fn();
  render(
    <ScheduleToolbar
      view="class"
      onGenerateAll={generate}
      // ...other required props
    />,
  );
  await userEvent.click(screen.getByRole("button", { name: /respect.+pins/i }));
  expect(generate).toHaveBeenLastCalledWith({ respect_pins: true });
  await userEvent.click(screen.getByRole("button", { name: /from scratch/i }));
  expect(generate).toHaveBeenLastCalledWith({ respect_pins: false });
});
```

- [ ] **Step 9.3: Run, expect failure (button missing).**

```bash
mise run fe:test -- src/features/schedule/schedule-toolbar.test.tsx -t "respect_pins"
```

- [ ] **Step 9.4: Add the button to the class-view variant of `schedule-toolbar.tsx`.**

In the discriminated-union props block where `view === "class"`:

```tsx
<Button onClick={() => onGenerateAll({ respect_pins: true })} variant="default">
  {t("schedule.generate.respectPinsAction")}
</Button>
<Button onClick={() => onGenerateAll({ respect_pins: false })} variant="secondary">
  {t("schedule.generate.fromScratchAction")}
</Button>
```

Wire up `schedule-page-class-view.tsx` to pass `onGenerateAll = generateAll.mutate` from `useGenerateAllSchedules()`.

- [ ] **Step 9.5: Run frontend tests + lint.**

```bash
mise run fe:test
mise run lint
```

Both green.

- [ ] **Step 9.6: Commit.**

```bash
git add frontend/src/features/schedule/hooks.ts \
        frontend/src/features/schedule/schedule-toolbar.tsx \
        frontend/src/features/schedule/schedule-page-class-view.tsx \
        frontend/src/features/schedule/schedule-toolbar.test.tsx
git commit -m "feat(frontend): re-solve respecting my pins toolbar action"
```

---

## Task 10: Playwright e2e for drag-and-drop

**Files:**
- Create: `e2e/tests/schedule-drag-and-drop.spec.ts`

- [ ] **Step 10.1: Write the e2e flow.**

```ts
// e2e/tests/schedule-drag-and-drop.spec.ts
import { expect, test } from "@playwright/test";

test("admin can drag a placement to a new slot and the move persists", async ({ page }) => {
  await page.goto("/login");
  await page.getByLabel(/email/i).fill(process.env.E2E_ADMIN_EMAIL ?? "admin@example.com");
  await page.getByLabel(/password/i).fill(process.env.E2E_ADMIN_PASSWORD ?? "admin");
  await page.getByRole("button", { name: /sign in|anmelden/i }).click();
  await page.goto("/schedule?view=class");
  // Click "Generate this class" so there is something to drag.
  await page.getByRole("button", { name: /generate this class|klasse generieren/i }).click();
  await expect(page.getByRole("region", { name: /schedule grid|stundenplan/i })).toBeVisible();

  const sourceCard = page.getByTestId(/^placement-card-/).first();
  await expect(sourceCard).toBeVisible();
  const sourceLessonId = await sourceCard.getAttribute("data-lesson-id");
  expect(sourceLessonId).not.toBeNull();

  const emptyCells = page.getByTestId(/^empty-cell-/);
  const emptyTarget = emptyCells.first();
  await expect(emptyTarget).toBeVisible();

  await sourceCard.dragTo(emptyTarget);

  // Pin badge appears on the moved card (auto-pin on move).
  await expect(
    page.getByRole("button", { name: new RegExp(`Lesson ${sourceLessonId} is pinned`) }),
  ).toBeVisible();

  // Reload and confirm the move persisted.
  await page.reload();
  await expect(
    page.getByRole("button", { name: new RegExp(`Lesson ${sourceLessonId} is pinned`) }),
  ).toBeVisible();
});
```

The test depends on `data-testid="placement-card-<lesson_id>"` and `data-testid="empty-cell-<time_block_id>-<room_id>"` attributes already on the schedule grid. Add them in this same task if not present.

- [ ] **Step 10.2: Add the data-testid attributes (if missing) on `schedule-grid.tsx`.**

```tsx
// On placement card wrapper:
data-testid={`placement-card-${placement.lesson_id}`}
data-lesson-id={placement.lesson_id}
// On empty cell wrapper:
data-testid={`empty-cell-${time_block_id}-${room_id}`}
```

- [ ] **Step 10.3: Run e2e once locally.**

```bash
mise run e2e -- schedule-drag-and-drop
```

Expected: PASS. If `dragTo` reliability is poor, fall back to manual mouse events (`page.mouse.move` / `page.mouse.down` / `page.mouse.up`) per the @dnd-kit Playwright recipe.

- [ ] **Step 10.4: Run the full e2e suite once to confirm no regressions.**

```bash
mise run e2e
```

- [ ] **Step 10.5: Commit.**

```bash
git add e2e/tests/schedule-drag-and-drop.spec.ts \
        frontend/src/features/schedule/schedule-grid.tsx
git commit -m "test(e2e): playwright happy path for drag-and-drop schedule edit"
```

---

## Task 11: ADR 0028 + OPEN_THINGS update + memory refresh

**Files:**
- Create: `docs/adr/0028-manual-pin-semantics.md`
- Edit: `docs/adr/README.md`
- Edit: `docs/superpowers/OPEN_THINGS.md`
- Edit: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`

- [ ] **Step 11.1: Confirm ADR number is still 0028.**

```bash
ls docs/adr/*.md | sort | tail -1
```

If the result is `0027-pinned-placements-wire-format.md`, 0028 is correct.

- [ ] **Step 11.2: Write the ADR using the template style (no em-dashes; colon in title per `.claude/CLAUDE.md`).**

Start from `docs/adr/template.md`. Sections required:

- Status: accepted, dated 2026-05-03.
- Context: Sprint A added the `pinned_placements` solver wire-format primitive (ADR 0027). Sprint C exposes user-facing pins on top of that.
- Decision: a pin is a hard constraint, not a hint. Manual moves and swaps auto-set `pinned=true`. The `pinned` flag survives all re-solves; "Generate all from scratch" passes `respect_pins=false` for one run only and does not mutate the column. `POST /api/schedule/all` defaults `respect_pins=true` (behaviour change vs Sprint A's silent default of "ignore pins"). Per-class re-solve respects own-class pins in addition to siblings' persisted placements.
- Consequences: any caller of `POST /api/schedule/all` that expected from-scratch behaviour must now pass `respect_pins=false`. The frontend "Generate all" button does so explicitly.
- Alternatives considered: a separate `/respect-pins` route, an "always respect; require explicit unpin" model, a soft-pin / hard-pin distinction. Rejected with reasoning.

- [ ] **Step 11.3: Append the ADR row to `docs/adr/README.md`.**

```markdown
| 0028 | [Manual pin semantics](0028-manual-pin-semantics.md) | accepted | 2026-05-03 |
```

(Match the existing column headers exactly.)

- [ ] **Step 11.4: Update `docs/superpowers/OPEN_THINGS.md`.**

- Move the Sprint C section into the "Shipped" / closed-work portion of the file, dated 2026-05-03.
- Surface follow-ups under "Acknowledged deferrals":
  - "Drag-and-drop on teacher and room views."
  - "Cross-class swap from a unified view."
  - "Mobile touch UX for drag (long-press menu)."
  - "Soft / tentative pin semantic."
- Promote Sprint 1 (Schwimmen + Sek-I) back to "Active" since the Scheduling UX program is now complete; the resume point is `Room.is_external` + per-Lesson travel buffers.

- [ ] **Step 11.5: Refresh auto-memory.**

Edit `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`:

- Mark Sprint C shipped 2026-05-03.
- Mark the Scheduling UX program closed.
- Set "next active pickup" to Sprint 1 resume at `Room.is_external` + travel buffers.

Update the index entry in `MEMORY.md` to match:

> `- [Roadmap status](project_roadmap_status.md) — Scheduling UX program closed 2026-05-03 (Sprints A/B/C all shipped); Sprint 1 (Schwimmen + Sek-I) resumes at Room.is_external next.`

- [ ] **Step 11.6: Commit.**

```bash
git add docs/adr/0028-manual-pin-semantics.md \
        docs/adr/README.md \
        docs/superpowers/OPEN_THINGS.md
git commit -m "docs: ADR 0028 manual pin semantics and update OPEN_THINGS for Sprint C"
```

(The auto-memory file lives outside the repo; no git commit needed for it.)

---

## Task 12: Final lint, full test suite, push, PR

- [ ] **Step 12.1: Full lint + tests + bench.**

```bash
mise run lint
mise run test
mise run bench:tests
```

All green; bench under `.test-duration-budget`.

- [ ] **Step 12.2: Push.**

```bash
mise exec -- git push -u origin feat/sprint-c-manual-editing
```

- [ ] **Step 12.3: Open the PR.**

```bash
gh pr create --base master --head feat/sprint-c-manual-editing \
  --title "feat: manual schedule editing with pinned placements (Sprint C)" \
  --body "$(cat <<'EOF'
## Summary

Sprint C of the Scheduling UX program. Adds drag-and-drop placement editing on the class-view schedule grid, a pin toggle on each cell, and a re-solve action that respects pins as hard constraints. Closes the program.

- Schema: `ScheduledLesson.pinned: bool DEFAULT FALSE NOT NULL` plus alembic migration.
- Backend: three placement-mutation endpoints (`PATCH .../move`, `PATCH .../pin`, `POST .../swap`) addressed by composite key. `POST /api/schedule/all` gains `respect_pins` (default `true`).
- Frontend: `@dnd-kit/core`-based drag-and-drop on the class view, pin toggle UI, two distinct whole-school actions ("Re-solve respecting my pins", "Generate all from scratch").
- ADR 0028 records user-facing pin semantics on top of ADR 0027's wire format.

Spec: docs/superpowers/specs/2026-05-03-sprint-c-manual-editing-design.md
Plan: docs/superpowers/plans/2026-05-03-sprint-c-manual-editing.md
ADR:  docs/adr/0028-manual-pin-semantics.md

## Test plan

- [x] `mise run test:py` passes (new placement-route, respect-pins, and pin-column tests).
- [x] `mise run fe:test` passes (new dnd hook + mutation hook + pin-badge specs).
- [x] `mise run e2e` passes (new schedule-drag-and-drop spec).
- [x] `mise run lint` passes.
- [x] `mise run bench:tests` under `.test-duration-budget`.
EOF
)"
```

- [ ] **Step 12.4: Post brainstorm Q&A.**

```bash
python3 .claude/commands/post_brainstorm_comments.py <pr-number>
```

(Read the printed PR number from `gh pr create`.)

- [ ] **Step 12.5: Set automerge once CI is queued.**

```bash
gh pr merge <pr> --auto --squash
```

Wait for `gh pr view <pr> --json state -q .state` to return `MERGED`. Refresh master, delete branch.

---

## Self-review notes

- **Spec coverage:** every numbered section of the design doc has at least one task (schema → 1; solver IO → 2; placement schemas → 3; placement routes → 4; respect_pins → 5; deps + types + i18n → 6; pin UI → 7; dnd → 8; toolbar → 9; e2e → 10; ADR + docs → 11).
- **Placeholder scan:** no "TBD"/"TODO"/"similar to" in any task. Each test step shows the test code, each implementation step shows the production code.
- **Type consistency:** function names follow the project's unique-function-names rule (`move_placement_route`, `pin_placement_route`, `swap_placements_route`, `useMovePlacement`, `usePinPlacement`, `useSwapPlacements`, `useScheduleDragAndDrop`, `buildCellKey`, `_load_placement_or_404`, `_load_time_block_or_404`, `_load_room_or_404`, `_assert_lesson_week_scheme_matches`, `collect_own_class_pins`, `collect_all_pins`, `samplePinnedPlacement`).
- **Behavioural sequencing:** structural change (schema migration) is its own commit (Task 1) and never shares a commit with behavioural changes (Tasks 2-9). Wire-format thread (Task 2) precedes endpoint use (Task 4 + 5). Frontend types regen (Task 6) precedes any frontend code that depends on them (Tasks 7-9).
