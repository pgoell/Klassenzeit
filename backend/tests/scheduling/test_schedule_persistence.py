"""Tests for the persist_solution_for_class helper.

Exercises the DB-side replace-then-insert semantics directly against the
shared per-test session. Route-level integration lives in
test_schedule_route.py; those tests run after Task 3 adds the GET endpoint.
"""

import uuid
from datetime import time

import pytest
from fastapi import HTTPException
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.pin_kind import PinKind
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.scheduling.solver_io import (
    persist_solution_for_class,
    read_schedule_for_class,
)


async def _seed_class_with_lesson(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
    *,
    class_name: str,
) -> tuple[uuid.UUID, uuid.UUID, uuid.UUID, uuid.UUID, uuid.UUID]:
    """Seed one class with one lesson, one time_block, one room.

    Returns ``(class_id, lesson_id, time_block_id, room_id, teacher_id)``.
    """
    subject = await create_subject()
    week_scheme = await create_week_scheme()
    tb = await create_time_block(
        week_scheme_id=week_scheme.id,
        position=0,
        start_time=time(8, 0),
        end_time=time(8, 45),
    )
    room = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        name=class_name,
        stundentafel_id=tafel.id,
        week_scheme_id=week_scheme.id,
    )
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    await db_session.flush()
    return cls.id, lesson.id, tb.id, room.id, teacher.id


async def test_persist_writes_rows_for_placements(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    class_id, lesson_id, tb_id, room_id, teacher_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-persist-writes",
    )
    filtered = {
        "placements": [
            {
                "lesson_id": str(lesson_id),
                "time_block_id": str(tb_id),
                "room_id": str(room_id),
                "teacher_id": str(teacher_id),
            },
        ],
        "violations": [],
    }
    await persist_solution_for_class(db_session, class_id, filtered)
    await db_session.flush()
    rows = (await db_session.execute(select(ScheduledLesson))).scalars().all()
    assert len(rows) == 1
    assert rows[0].lesson_id == lesson_id
    assert rows[0].time_block_id == tb_id
    assert rows[0].room_id == room_id


async def test_persist_replaces_existing_rows_for_class(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    class_id, lesson_id, tb_id, room_id, teacher_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-persist-replace",
    )
    other_room = await create_room()
    first = {
        "placements": [
            {
                "lesson_id": str(lesson_id),
                "time_block_id": str(tb_id),
                "room_id": str(room_id),
                "teacher_id": str(teacher_id),
            },
        ],
        "violations": [],
    }
    second = {
        "placements": [
            {
                "lesson_id": str(lesson_id),
                "time_block_id": str(tb_id),
                "room_id": str(other_room.id),
                "teacher_id": str(teacher_id),
            },
        ],
        "violations": [],
    }
    await persist_solution_for_class(db_session, class_id, first)
    await db_session.flush()
    await persist_solution_for_class(db_session, class_id, second)
    await db_session.flush()
    rows = (await db_session.execute(select(ScheduledLesson))).scalars().all()
    assert len(rows) == 1
    assert rows[0].room_id == other_room.id


async def test_persist_empty_result_clears_rows(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    class_id, lesson_id, tb_id, room_id, teacher_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-persist-empty",
    )
    filtered_first = {
        "placements": [
            {
                "lesson_id": str(lesson_id),
                "time_block_id": str(tb_id),
                "room_id": str(room_id),
                "teacher_id": str(teacher_id),
            },
        ],
        "violations": [],
    }
    filtered_empty = {"placements": [], "violations": []}
    await persist_solution_for_class(db_session, class_id, filtered_first)
    await db_session.flush()
    await persist_solution_for_class(db_session, class_id, filtered_empty)
    await db_session.flush()
    rows = (await db_session.execute(select(ScheduledLesson))).scalars().all()
    assert rows == []


async def test_persist_class_a_does_not_touch_class_b_rows(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    class_a_id, lesson_a_id, tb_a_id, room_a_id, teacher_a_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-persist-isolate-A",
    )
    class_b_id, lesson_b_id, tb_b_id, room_b_id, teacher_b_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1b-persist-isolate-B",
    )
    filtered_a = {
        "placements": [
            {
                "lesson_id": str(lesson_a_id),
                "time_block_id": str(tb_a_id),
                "room_id": str(room_a_id),
                "teacher_id": str(teacher_a_id),
            },
        ],
        "violations": [],
    }
    filtered_b = {
        "placements": [
            {
                "lesson_id": str(lesson_b_id),
                "time_block_id": str(tb_b_id),
                "room_id": str(room_b_id),
                "teacher_id": str(teacher_b_id),
            },
        ],
        "violations": [],
    }
    await persist_solution_for_class(db_session, class_a_id, filtered_a)
    await db_session.flush()
    await persist_solution_for_class(db_session, class_b_id, filtered_b)
    await db_session.flush()
    # Replace A with empty. B must survive.
    await persist_solution_for_class(db_session, class_a_id, {"placements": [], "violations": []})
    await db_session.flush()
    rows = (await db_session.execute(select(ScheduledLesson))).scalars().all()
    assert len(rows) == 1
    assert rows[0].lesson_id == lesson_b_id


async def test_read_returns_empty_list_for_never_scheduled_class(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    class_id, _lesson_id, _tb_id, _room_id, _teacher_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-read-empty",
    )
    result = await read_schedule_for_class(db_session, class_id, school_id=DEFAULT_SCHOOL_ID)
    assert result == []


async def test_read_returns_only_rows_for_requested_class(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    class_a_id, lesson_a_id, tb_a_id, room_a_id, teacher_a_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-read-scope-A",
    )
    class_b_id, lesson_b_id, tb_b_id, room_b_id, teacher_b_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1b-read-scope-B",
    )
    await persist_solution_for_class(
        db_session,
        class_a_id,
        {
            "placements": [
                {
                    "lesson_id": str(lesson_a_id),
                    "time_block_id": str(tb_a_id),
                    "room_id": str(room_a_id),
                    "teacher_id": str(teacher_a_id),
                },
            ],
            "violations": [],
        },
    )
    await persist_solution_for_class(
        db_session,
        class_b_id,
        {
            "placements": [
                {
                    "lesson_id": str(lesson_b_id),
                    "time_block_id": str(tb_b_id),
                    "room_id": str(room_b_id),
                    "teacher_id": str(teacher_b_id),
                },
            ],
            "violations": [],
        },
    )
    await db_session.flush()
    result_a = await read_schedule_for_class(db_session, class_a_id, school_id=DEFAULT_SCHOOL_ID)
    assert len(result_a) == 1
    assert result_a[0].lesson_id == lesson_a_id


async def test_read_raises_404_for_missing_class(db_session: AsyncSession) -> None:
    with pytest.raises(HTTPException) as excinfo:
        await read_schedule_for_class(db_session, uuid.uuid4(), school_id=DEFAULT_SCHOOL_ID)
    assert excinfo.value.status_code == 404


async def test_scheduled_lesson_persists_teacher_id_from_placement(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    """``persist_solution_for_class`` writes ``Placement.teacher_id`` onto the row.

    Item 65 contract: every ScheduledLesson row carries the teacher the solver
    picked. Today this matches Lesson.teacher_id because auto-assign runs at
    the route handler boundary, but the persistence layer always reads the
    value out of the wire-format placement, not the lesson row.
    """
    class_id, lesson_id, tb_id, room_id, teacher_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-persist-teacher",
    )
    filtered = {
        "placements": [
            {
                "lesson_id": str(lesson_id),
                "time_block_id": str(tb_id),
                "room_id": str(room_id),
                "teacher_id": str(teacher_id),
            },
        ],
        "violations": [],
    }
    await persist_solution_for_class(db_session, class_id, filtered)
    await db_session.flush()
    rows = (await db_session.execute(select(ScheduledLesson))).scalars().all()
    assert len(rows) == 1
    assert rows[0].teacher_id == teacher_id


async def test_read_schedule_surfaces_pinned_flag(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    """``read_schedule_for_class`` must propagate ``ScheduledLesson.pinned``.

    Regression guard: the helper previously dropped the field when building
    ``PlacementResponse``, masking pin state from per-class GET responses.
    """
    class_id, lesson_id, tb_id, room_id, teacher_id = await _seed_class_with_lesson(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-read-pinned",
    )
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson_id,
            time_block_id=tb_id,
            room_id=room_id,
            teacher_id=teacher_id,
            pin_kind=PinKind.HARD,
        )
    )
    await db_session.flush()
    result = await read_schedule_for_class(db_session, class_id, school_id=DEFAULT_SCHOOL_ID)
    assert len(result) == 1
    assert result[0].pinned is True
    assert result[0].pin_kind is PinKind.HARD
