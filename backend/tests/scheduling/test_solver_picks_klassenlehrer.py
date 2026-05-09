"""Backend integration test for solver-driven klassenlehrer assignment.

Items 66 (per-(class, subject) uniformity), 67 (prefer_class_teacher soft
cost), 68 (placement-time teacher pick): the full backend -> solver path
must (a) leave Lesson.teacher_id null on generated lessons, (b) hand the
solver a non-empty teacher_candidates set per lesson, (c) get back
placements where every (class, subject) pair is taught by exactly one
teacher, and (d) prefer the class teacher when they are qualified.
"""

from datetime import time

from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.teacher import TeacherQualification


async def test_schedule_post_picks_klassenlehrer_when_qualified(
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
    """End-to-end: solver picks the klt for klt-qualified subjects, falls back otherwise.

    Setup:
      - Klassenlehrer T1, Fachlehrer T2.
      - Subject Mathematik (qualified for both T1 and T2).
      - Subject Kunst (qualified for T2 only).
      - SchoolClass with class_teacher_id=T1.
      - Two Mathematik lessons + two Kunst lessons (each 1h, single-block).
      - Four time blocks on day 0, one room (avoids same-room friction).

    Assertions:
      1. Response 200 and feasible (no violations).
      2. All Mathematik placements use T1 (klassenlehrer soft cost steers the pick).
      3. All Kunst placements use T2 (only qualified).
      4. Per-(class, subject) uniformity: each pair has exactly one teacher.
    """
    await create_test_user(email="admin@kl-pick.com", role="admin")
    await login_as("admin@kl-pick.com", "testpassword123")

    week_scheme = await create_week_scheme()
    for pos in range(4):
        await create_time_block(
            week_scheme_id=week_scheme.id,
            position=pos,
            start_time=time(8 + pos, 0),
            end_time=time(8 + pos, 45),
        )
    await create_room()
    klt = await create_teacher()
    fachlehrer = await create_teacher()
    mathe = await create_subject()
    kunst = await create_subject()
    db_session.add(TeacherQualification(teacher_id=klt.id, subject_id=mathe.id))
    db_session.add(TeacherQualification(teacher_id=fachlehrer.id, subject_id=mathe.id))
    db_session.add(TeacherQualification(teacher_id=fachlehrer.id, subject_id=kunst.id))

    tafel = await create_stundentafel()
    cls = await create_school_class(
        name="3a-kl-pick",
        stundentafel_id=tafel.id,
        week_scheme_id=week_scheme.id,
    )
    cls.class_teacher_id = klt.id
    await db_session.flush()

    # Two Mathematik lessons (klt qualified) + two Kunst lessons (klt not qualified).
    # Lesson.teacher_id stays null (pin-only since item 63), so the solver picks
    # among teacher_candidates per ADR 0036.
    for subject_id in (mathe.id, mathe.id, kunst.id, kunst.id):
        lesson = Lesson(
            subject_id=subject_id,
            hours_per_week=1,
            preferred_block_size=1,
        )
        db_session.add(lesson)
        await db_session.flush()
        db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
        await db_session.flush()

    resp = await client.post(f"/api/classes/{cls.id}/schedule")
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["violations"] == [], f"unexpected violations: {body['violations'][:3]}"
    assert len(body["placements"]) == 4

    # Read the persisted ScheduledLesson rows to inspect teacher picks
    # (PlacementResponse on the wire does not expose teacher_id today).
    cls_id = cls.id
    klt_id = klt.id
    fachlehrer_id = fachlehrer.id
    mathe_id = mathe.id
    kunst_id = kunst.id
    rows = (
        await db_session.execute(
            select(ScheduledLesson.lesson_id, ScheduledLesson.teacher_id, Lesson.subject_id)
            .join(Lesson, Lesson.id == ScheduledLesson.lesson_id)
            .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
            .where(LessonSchoolClass.school_class_id == cls_id)
        )
    ).all()
    assert len(rows) == 4, f"expected 4 persisted placements, got {len(rows)}"
    mathe_teachers = {tid for _lid, tid, sid in rows if sid == mathe_id}
    kunst_teachers = {tid for _lid, tid, sid in rows if sid == kunst_id}
    assert mathe_teachers == {klt_id}, (
        f"expected klassenlehrer T1 for Mathematik, got {mathe_teachers}"
    )
    assert kunst_teachers == {fachlehrer_id}, (
        f"expected fachlehrer T2 for Kunst, got {kunst_teachers}"
    )
