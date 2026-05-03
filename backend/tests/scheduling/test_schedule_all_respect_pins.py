"""POST /api/schedule/all?respect_pins=... behaves correctly.

Sprint C Task 5. Two cases:
- ``respect_pins=true`` (default): the pinned slot survives and the row keeps
  ``pinned=True``.
- ``respect_pins=false``: the solver may move the pinned lesson, but the
  ``pinned`` flag in the DB is unchanged (per spec section 3.3).
"""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.user import User
from tests.scheduling.conftest import SeededDreizuegigWithPin

CreateUserFnPins = Callable[..., Awaitable[tuple[User, str]]]
LoginFnPins = Callable[[str, str], Awaitable[None]]


@pytest.mark.asyncio
async def test_schedule_all_default_respects_pins(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user: CreateUserFnPins,
    login_as: LoginFnPins,
    seeded_dreizuegig_with_one_pin: SeededDreizuegigWithPin,
) -> None:
    """No flag passed: default to respect_pins=true; pinned slot survives."""
    await create_test_user(email="admin@all-pins-default.com", role="admin")
    await login_as("admin@all-pins-default.com", "testpassword123")
    fixture = seeded_dreizuegig_with_one_pin
    response = await client.post("/api/schedule/all")
    assert response.status_code == 200, response.text
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
    create_test_user: CreateUserFnPins,
    login_as: LoginFnPins,
    seeded_dreizuegig_with_one_pin: SeededDreizuegigWithPin,
) -> None:
    """respect_pins=false: solver may move the pinned lesson; flag stays in DB."""
    await create_test_user(email="admin@all-pins-false.com", role="admin")
    await login_as("admin@all-pins-false.com", "testpassword123")
    fixture = seeded_dreizuegig_with_one_pin
    response = await client.post("/api/schedule/all?respect_pins=false")
    assert response.status_code == 200, response.text
    pinned_rows = (
        (
            await db_session.execute(
                select(ScheduledLesson).where(ScheduledLesson.lesson_id == fixture.pinned_lesson_id)
            )
        )
        .scalars()
        .all()
    )
    assert len(pinned_rows) == 1
    # Pin state is preserved across the run; only the slot may have changed.
    assert pinned_rows[0].pinned is True
