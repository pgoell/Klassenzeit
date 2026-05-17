"""Tests for the ScheduledLesson ORM model school_id column."""

from datetime import time

import pytest
from sqlalchemy.exc import IntegrityError

from klassenzeit_backend.db.models import (
    Lesson,
    Room,
    ScheduledLesson,
    Subject,
    Teacher,
)
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.week_scheme import (
    TimeBlock,
    TimeBlockKind,
    WeekScheme,
)


async def _make_placement_fixture(db_session) -> tuple[Lesson, TimeBlock, Room, Teacher]:
    """Insert the minimal aggregate set needed for a ScheduledLesson row."""
    scheme = WeekScheme(name="SL Test Scheme", school_id=DEFAULT_SCHOOL_ID)
    db_session.add(scheme)
    await db_session.flush()
    block = TimeBlock(
        week_scheme_id=scheme.id,
        day_of_week=0,
        position=1,
        start_time=time(8, 0),
        end_time=time(8, 45),
        kind=TimeBlockKind.LESSON,
    )
    db_session.add(block)
    subject = Subject(
        name="ScheduledLesson Test Subject",
        short_name="SLT",
        color="#FFFFFF",
        school_id=DEFAULT_SCHOOL_ID,
    )
    db_session.add(subject)
    teacher = Teacher(
        first_name="SL",
        last_name="Test",
        short_code="SLT",
        max_hours_per_week=24,
        school_id=DEFAULT_SCHOOL_ID,
    )
    db_session.add(teacher)
    room = Room(name="SL Test Room", short_name="SLR", school_id=DEFAULT_SCHOOL_ID)
    db_session.add(room)
    await db_session.flush()
    lesson = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    return lesson, block, room, teacher


@pytest.mark.asyncio
async def test_scheduled_lesson_round_trip_with_school_id(db_session):
    lesson, block, room, teacher = await _make_placement_fixture(db_session)
    row = ScheduledLesson(
        lesson_id=lesson.id,
        time_block_id=block.id,
        room_id=room.id,
        teacher_id=teacher.id,
        school_id=DEFAULT_SCHOOL_ID,
    )
    db_session.add(row)
    await db_session.flush()
    await db_session.refresh(row)
    assert row.school_id == DEFAULT_SCHOOL_ID
    assert row.lesson_id == lesson.id
    assert row.time_block_id == block.id


@pytest.mark.asyncio
async def test_scheduled_lesson_rejects_null_school_id(db_session):
    lesson, block, room, teacher = await _make_placement_fixture(db_session)
    with pytest.raises(IntegrityError):
        async with db_session.begin_nested():
            db_session.add(
                ScheduledLesson(
                    lesson_id=lesson.id,
                    time_block_id=block.id,
                    room_id=room.id,
                    teacher_id=teacher.id,
                )
            )
            await db_session.flush()
