"""End-to-end: a soft pin pointing at an infeasible TB surfaces on the response.

Load-bearing scenario: seed a one-class school with two lessons sharing the
same teacher, hard-pin one of them at TB_2, and soft-pin the other at TB_2
too. The teacher cannot teach two lessons simultaneously, so the solver
MUST route the soft-pinned lesson to TB_1 — that is one ``soft_pin_miss``.
This pins the whole wire path: solver_io emits ``kind: "soft"``,
solver-core scores the miss, and the QualityReport surfaces it on
``ScheduleResponse``. ADR 0042.
"""

from datetime import time

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.pin_kind import PinKind
from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.teacher import TeacherQualification
from klassenzeit_backend.main import app


@pytest.mark.asyncio
async def test_post_schedule_routes_soft_pin_through_solver(
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
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A soft pin at an infeasible TB is missed; quality_report.soft_pin_misses >= 1."""
    # Give the solver enough wall-clock to actually iterate on this tiny problem.
    monkeypatch.setitem(app.state.settings.solve_deadline_ms_by_backend, "lahc_rr", 5000)
    await create_test_user(email="admin@soft-pin-int.com", role="admin")
    await login_as("admin@soft-pin-int.com", "testpassword123")

    subject_a = await create_subject()
    subject_b = await create_subject()
    scheme = await create_week_scheme()
    await create_time_block(
        week_scheme_id=scheme.id,
        day_of_week=0,
        position=1,
        start_time=time(8, 0),
        end_time=time(8, 45),
    )
    tb_2 = await create_time_block(
        week_scheme_id=scheme.id,
        day_of_week=0,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    await create_room()
    # One teacher qualified for both subjects: the teacher double-book at TB_2
    # is what makes the soft pin infeasible.
    teacher = await create_teacher()
    db_session.add_all(
        [
            TeacherQualification(teacher_id=teacher.id, subject_id=subject_a.id),
            TeacherQualification(teacher_id=teacher.id, subject_id=subject_b.id),
        ]
    )
    await db_session.flush()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        name="SoftPinClass",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
    )

    lesson_hard = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject_a.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    lesson_soft = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject_b.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add_all([lesson_hard, lesson_soft])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=lesson_hard.id, school_class_id=cls.id),
            LessonSchoolClass(lesson_id=lesson_soft.id, school_class_id=cls.id),
            # Hard-pin lesson_hard at TB_2 so the teacher is occupied there.
            ScheduledLesson(
                lesson_id=lesson_hard.id,
                time_block_id=tb_2.id,
                room_id=(await _first_room_id(db_session)),
                teacher_id=teacher.id,
                pin_kind=PinKind.HARD,
            ),
            # Soft-pin lesson_soft at the same TB; the solver MUST route around
            # it (teacher conflict) so this pin is missed.
            ScheduledLesson(
                lesson_id=lesson_soft.id,
                time_block_id=tb_2.id,
                room_id=(await _first_room_id(db_session)),
                teacher_id=teacher.id,
                pin_kind=PinKind.SOFT,
            ),
        ]
    )
    await db_session.commit()

    response = await client.post(f"/api/classes/{cls.id}/schedule")
    assert response.status_code == 200, response.text
    body = response.json()
    assert "quality_report" in body, "quality_report must be on the wire format"
    assert "soft_pin_misses" in body["quality_report"], (
        "soft_pin_misses must surface on quality_report"
    )
    # The hard pin holds lesson_hard at TB_2; the soft pin asks for lesson_soft
    # at TB_2 too. With one teacher between them, the solver MUST drop
    # lesson_soft to TB_1, which is exactly one soft_pin_miss.
    assert body["quality_report"]["soft_pin_misses"] >= 1, (
        f"expected the infeasible soft pin to register as a miss; "
        f"quality_report={body['quality_report']}"
    )


async def _first_room_id(db_session: AsyncSession):
    """Return the UUID of the one Room created by the ``create_room`` factory."""
    return (await db_session.execute(select(Room.id).limit(1))).scalar_one()
