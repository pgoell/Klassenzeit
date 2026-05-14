"""ScheduledLesson.pin_kind column round-trips HARD, SOFT, and None.

Pins the runtime column shape so a future regression (e.g., someone
restoring the legacy ``pinned: bool`` column for compat, or a SQLAlchemy
enum-roundtrip drift) trips here. The HARD/SOFT/None triad mirrors the
three-state ``PinKind | None`` enum defined in ADR 0042.
"""

import uuid

import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.pin_kind import PinKind
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson


@pytest.mark.asyncio
async def test_scheduled_lesson_pin_kind_defaults_none(
    db_session: AsyncSession,
    seeded_lesson_for_pinning: tuple[uuid.UUID, uuid.UUID, uuid.UUID, uuid.UUID],
) -> None:
    lesson_id, time_block_id, room_id, teacher_id = seeded_lesson_for_pinning
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson_id,
            time_block_id=time_block_id,
            room_id=room_id,
            teacher_id=teacher_id,
        )
    )
    await db_session.flush()
    row = (
        await db_session.execute(
            select(ScheduledLesson).where(ScheduledLesson.lesson_id == lesson_id)
        )
    ).scalar_one()
    assert row.pin_kind is None


@pytest.mark.asyncio
async def test_scheduled_lesson_pin_kind_round_trips_hard(
    db_session: AsyncSession,
    seeded_lesson_for_pinning: tuple[uuid.UUID, uuid.UUID, uuid.UUID, uuid.UUID],
) -> None:
    lesson_id, time_block_id, room_id, teacher_id = seeded_lesson_for_pinning
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson_id,
            time_block_id=time_block_id,
            room_id=room_id,
            teacher_id=teacher_id,
            pin_kind=PinKind.HARD,
        )
    )
    await db_session.flush()
    row = (
        await db_session.execute(
            select(ScheduledLesson).where(ScheduledLesson.lesson_id == lesson_id)
        )
    ).scalar_one()
    assert row.pin_kind is PinKind.HARD


@pytest.mark.asyncio
async def test_scheduled_lesson_pin_kind_round_trips_soft(
    db_session: AsyncSession,
    seeded_lesson_for_pinning: tuple[uuid.UUID, uuid.UUID, uuid.UUID, uuid.UUID],
) -> None:
    """Setting ``pin_kind = SOFT`` persists and re-loads as ``PinKind.SOFT``."""
    lesson_id, time_block_id, room_id, teacher_id = seeded_lesson_for_pinning
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson_id,
            time_block_id=time_block_id,
            room_id=room_id,
            teacher_id=teacher_id,
            pin_kind=PinKind.SOFT,
        )
    )
    await db_session.flush()
    row = (
        await db_session.execute(
            select(ScheduledLesson).where(ScheduledLesson.lesson_id == lesson_id)
        )
    ).scalar_one()
    assert row.pin_kind is PinKind.SOFT
