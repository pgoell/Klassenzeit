"""ScheduledLesson.pinned column round-trips False by default and accepts True."""

import uuid

import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

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
        await db_session.execute(
            select(ScheduledLesson).where(ScheduledLesson.lesson_id == lesson_id)
        )
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
        await db_session.execute(
            select(ScheduledLesson).where(ScheduledLesson.lesson_id == lesson_id)
        )
    ).scalar_one()
    assert row.pinned is True
