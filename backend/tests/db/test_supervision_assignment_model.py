"""Tests for the SupervisionAssignment ORM model.

The model persists one supervisor assignment per Hofpause (break) TimeBlock,
mirroring the solver's ``supervision_assignments`` wire output.
"""

from datetime import time

import pytest
from sqlalchemy.exc import IntegrityError

from klassenzeit_backend.db.models import SupervisionAssignment, Teacher
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.week_scheme import TimeBlock, TimeBlockKind, WeekScheme


async def _make_break_block_and_teacher(db_session) -> tuple[TimeBlock, Teacher]:
    """Insert a WeekScheme + break TimeBlock + Teacher inline (no factories)."""
    scheme = WeekScheme(name="Test Scheme", school_id=DEFAULT_SCHOOL_ID)
    db_session.add(scheme)
    await db_session.flush()

    block = TimeBlock(
        week_scheme_id=scheme.id,
        day_of_week=0,
        position=3,
        start_time=time(9, 30),
        end_time=time(9, 45),
        kind=TimeBlockKind.BREAK,
    )
    db_session.add(block)

    teacher = Teacher(
        first_name="Sup",
        last_name="Visor",
        short_code="SV1",
        max_hours_per_week=24,
        school_id=DEFAULT_SCHOOL_ID,
    )
    db_session.add(teacher)
    await db_session.flush()

    return block, teacher


@pytest.mark.asyncio
async def test_supervision_assignment_round_trip(db_session):
    block, teacher = await _make_break_block_and_teacher(db_session)

    row = SupervisionAssignment(
        time_block_id=block.id, teacher_id=teacher.id, school_id=DEFAULT_SCHOOL_ID
    )
    db_session.add(row)
    await db_session.flush()
    await db_session.refresh(row)

    assert row.id is not None
    assert row.created_at is not None
    assert row.time_block_id == block.id
    assert row.teacher_id == teacher.id


@pytest.mark.asyncio
async def test_supervision_assignment_time_block_unique(db_session):
    block, teacher = await _make_break_block_and_teacher(db_session)

    first = SupervisionAssignment(
        time_block_id=block.id, teacher_id=teacher.id, school_id=DEFAULT_SCHOOL_ID
    )
    db_session.add(first)
    await db_session.flush()

    second_teacher = Teacher(
        first_name="Sup",
        last_name="Visor2",
        short_code="SV2",
        max_hours_per_week=24,
        school_id=DEFAULT_SCHOOL_ID,
    )
    db_session.add(second_teacher)
    await db_session.flush()

    with pytest.raises(IntegrityError):
        async with db_session.begin_nested():
            duplicate = SupervisionAssignment(
                time_block_id=block.id,
                teacher_id=second_teacher.id,
                school_id=DEFAULT_SCHOOL_ID,
            )
            db_session.add(duplicate)
            await db_session.flush()
