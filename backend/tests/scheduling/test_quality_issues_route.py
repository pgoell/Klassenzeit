"""Tests for GET /api/classes/{class_id}/quality-issues.

The route surfaces the per-class output of ``compute_quality_issues`` as a
list of :class:`QualityIssueResponse`. It computes on demand from the
persisted ScheduledLesson rows; no per-solve snapshot is stored.
"""

import uuid
from datetime import time

from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson


async def test_get_quality_issues_returns_list_with_room_hop(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    """Seed a class with a room_hop and assert the route returns the issue list.

    Each entry has the QualityIssueResponse wire shape:
    kind, school_class_id, day_of_week, subject_id, detail, cells.
    """
    await create_test_user(email="admin@qi-route-list.com", role="admin")
    await login_as("admin@qi-route-list.com", "testpassword123")

    subject = await create_subject()
    week_scheme = await create_week_scheme()
    tb_1 = await create_time_block(
        week_scheme_id=week_scheme.id,
        day_of_week=0,
        position=1,
    )
    tb_2 = await create_time_block(
        week_scheme_id=week_scheme.id,
        day_of_week=0,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    room_a = await create_room()
    room_b = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=week_scheme.id)

    lesson_1 = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=2,
        preferred_block_size=1,
    )
    lesson_2 = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=2,
        preferred_block_size=1,
    )
    db_session.add_all([lesson_1, lesson_2])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=lesson_1.id, school_class_id=cls.id),
            LessonSchoolClass(lesson_id=lesson_2.id, school_class_id=cls.id),
            ScheduledLesson(
                lesson_id=lesson_1.id,
                time_block_id=tb_1.id,
                room_id=room_a.id,
                teacher_id=teacher.id,
                pin_kind=None,
            ),
            ScheduledLesson(
                lesson_id=lesson_2.id,
                time_block_id=tb_2.id,
                room_id=room_b.id,
                teacher_id=teacher.id,
                pin_kind=None,
            ),
        ]
    )
    await db_session.flush()
    await db_session.commit()

    resp = await client.get(f"/api/classes/{cls.id}/quality-issues")
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert isinstance(body, list)
    kinds = {issue["kind"] for issue in body}
    assert "room_hop" in kinds, f"expected room_hop in {kinds}"
    expected_keys = {"kind", "school_class_id", "day_of_week", "subject_id", "detail", "cells"}
    for issue in body:
        assert set(issue.keys()) == expected_keys, (
            f"QualityIssueResponse field drift: {set(issue.keys()) ^ expected_keys}"
        )
        assert issue["school_class_id"] == str(cls.id)


async def test_get_quality_issues_empty_when_no_scheduled_lessons(
    client: AsyncClient,
    create_test_user,
    login_as,
    create_week_scheme,
    create_stundentafel,
    create_school_class,
) -> None:
    """A class with no ScheduledLesson rows returns an empty list, not 404."""
    await create_test_user(email="admin@qi-route-empty.com", role="admin")
    await login_as("admin@qi-route-empty.com", "testpassword123")
    scheme = await create_week_scheme()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    resp = await client.get(f"/api/classes/{cls.id}/quality-issues")
    assert resp.status_code == 200, resp.text
    assert resp.json() == []


async def test_get_quality_issues_returns_404_for_unknown_class(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    """An unknown class_id returns 404, mirroring the sibling GET schedule route."""
    await create_test_user(email="admin@qi-route-404.com", role="admin")
    await login_as("admin@qi-route-404.com", "testpassword123")
    resp = await client.get(f"/api/classes/{uuid.uuid4()}/quality-issues")
    assert resp.status_code == 404
