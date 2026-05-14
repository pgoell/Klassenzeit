"""Integration tests for Sprint C's placement-mutation endpoints."""

import uuid

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from tests.scheduling.conftest import (
    SeededCrossWeekFixture,
    SeededMovablePlacement,
    SeededTwoPlacements,
)


@pytest.mark.asyncio
async def test_move_placement_succeeds_and_pins(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    await create_test_user(email="admin@move-ok.com", role="admin")
    await login_as("admin@move-ok.com", "testpassword123")
    fixture = seeded_movable_placement
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}",
        json={
            "time_block_id": str(fixture.target_time_block_id),
            "room_id": str(fixture.target_room_id),
        },
    )
    assert response.status_code == 200, response.text
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
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    await create_test_user(email="admin@move-404.com", role="admin")
    await login_as("admin@move-404.com", "testpassword123")
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
    create_test_user,
    login_as,
    seeded_movable_placement_cross_week: SeededCrossWeekFixture,
) -> None:
    await create_test_user(email="admin@move-422.com", role="admin")
    await login_as("admin@move-422.com", "testpassword123")
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
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    await create_test_user(email="admin@pin-toggle.com", role="admin")
    await login_as("admin@pin-toggle.com", "testpassword123")
    fixture = seeded_movable_placement
    response_on = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin",
        json={"pin_kind": "hard"},
    )
    assert response_on.status_code == 200, response_on.text
    assert response_on.json()["pin_kind"] == "hard"
    assert response_on.json()["pinned"] is True
    response_off = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin",
        json={"pin_kind": None},
    )
    assert response_off.status_code == 200, response_off.text
    assert response_off.json()["pin_kind"] is None
    assert response_off.json()["pinned"] is False


@pytest.mark.asyncio
async def test_swap_placements_succeeds_and_pins_both(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    seeded_two_placements_for_swap: SeededTwoPlacements,
) -> None:
    await create_test_user(email="admin@swap-ok.com", role="admin")
    await login_as("admin@swap-ok.com", "testpassword123")
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
    assert response.status_code == 200, response.text
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
    create_test_user,
    login_as,
    seeded_two_placements_for_swap: SeededTwoPlacements,
) -> None:
    await create_test_user(email="admin@swap-404.com", role="admin")
    await login_as("admin@swap-404.com", "testpassword123")
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
    assert a_row.pin_kind is None
