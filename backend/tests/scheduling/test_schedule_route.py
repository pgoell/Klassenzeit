"""Tests for POST /api/classes/{class_id}/schedule."""

import logging
import uuid
from datetime import time

import pytest
from httpx import AsyncClient
from sqlalchemy import delete, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.teacher import TeacherQualification
from klassenzeit_backend.db.models.week_scheme import TimeBlock, WeekScheme


async def _seed_solvable_class(
    db_session,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
    *,
    class_name: str,
    scheme: WeekScheme | None = None,
    position: int = 0,
) -> tuple[SchoolClass, WeekScheme]:
    """Seed one class with one teacher, one subject, one room, one time block, one lesson.

    Returns ``(SchoolClass, WeekScheme)``. If ``scheme`` is provided, reuses it
    (useful for seeding a second class on the same scheme); in that case the
    caller must pass a ``position`` that doesn't collide with existing blocks
    on that scheme (the unique key is ``(week_scheme_id, day_of_week, position)``).
    """
    subject = await create_subject()
    week_scheme = scheme if scheme is not None else await create_week_scheme()
    hour_offset = position
    await create_time_block(
        week_scheme_id=week_scheme.id,
        position=position,
        start_time=time(8 + hour_offset, 0),
        end_time=time(8 + hour_offset, 45),
    )
    await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        name=class_name,
        stundentafel_id=tafel.id,
        week_scheme_id=week_scheme.id,
    )
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
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
    return cls, week_scheme


async def test_schedule_post_returns_404_for_unknown_class(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="admin@sched-404.com", role="admin")
    await login_as("admin@sched-404.com", "testpassword123")
    resp = await client.post(f"/api/classes/{uuid.uuid4()}/schedule")
    assert resp.status_code == 404


async def test_schedule_post_returns_200_with_placements_on_happy_path(
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
    await create_test_user(email="admin@sched-ok.com", role="admin")
    await login_as("admin@sched-ok.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-sched-ok",
    )
    resp = await client.post(f"/api/classes/{cls.id}/schedule")
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert len(body["placements"]) == 1
    assert body["violations"] == []
    assert "soft_score" in body
    assert body["soft_score"] >= 0


async def test_schedule_post_returns_422_without_time_blocks(
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
    await create_test_user(email="admin@sched-422-tb.com", role="admin")
    await login_as("admin@sched-422-tb.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-sched-notb",
    )
    await db_session.execute(delete(TimeBlock))
    await db_session.flush()
    resp = await client.post(f"/api/classes/{cls.id}/schedule")
    assert resp.status_code == 422


async def test_schedule_post_returns_422_when_rooms_table_empty(
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
    await create_test_user(email="admin@sched-422-rooms.com", role="admin")
    await login_as("admin@sched-422-rooms.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-sched-norooms",
    )
    await db_session.execute(delete(Room))
    await db_session.flush()
    resp = await client.post(f"/api/classes/{cls.id}/schedule")
    assert resp.status_code == 422


async def test_schedule_post_filters_out_other_classes_placements(
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
    await create_test_user(email="admin@sched-filter.com", role="admin")
    await login_as("admin@sched-filter.com", "testpassword123")
    cls_a, scheme = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-filter",
    )
    # Second class on the same scheme, second time block so both can be placed.
    await create_time_block(
        week_scheme_id=scheme.id,
        position=1,
        start_time=time(8, 50),
        end_time=time(9, 35),
    )
    subject_b = await create_subject()
    teacher_b = await create_teacher()
    db_session.add(TeacherQualification(teacher_id=teacher_b.id, subject_id=subject_b.id))
    tafel_b = await create_stundentafel()
    cls_b = await create_school_class(
        name="1b-filter",
        stundentafel_id=tafel_b.id,
        week_scheme_id=scheme.id,
    )
    lesson_b = Lesson(
        subject_id=subject_b.id,
        teacher_id=teacher_b.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson_b)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson_b.id, school_class_id=cls_b.id))
    await db_session.flush()
    resp = await client.post(f"/api/classes/{cls_a.id}/schedule")
    assert resp.status_code == 200, resp.text
    body = resp.json()
    class_a_lesson_ids = {
        str(lesson.id)
        for lesson in (
            await db_session.execute(
                select(Lesson)
                .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
                .where(LessonSchoolClass.school_class_id == cls_a.id)
            )
        )
        .scalars()
        .all()
    }
    for placement in body["placements"]:
        assert placement["lesson_id"] in class_a_lesson_ids


async def test_schedule_post_logs_solve_start_and_done(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    caplog: pytest.LogCaptureFixture,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    await create_test_user(email="admin@sched-log.com", role="admin")
    await login_as("admin@sched-log.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-sched-log",
    )
    caplog.set_level(logging.INFO, logger="klassenzeit_backend.scheduling.solver_io")
    resp = await client.post(f"/api/classes/{cls.id}/schedule")
    assert resp.status_code == 200, resp.text
    events = {r.message for r in caplog.records}
    assert "solver.solve.start" in events
    assert "solver.solve.done" in events


async def test_schedule_get_returns_404_for_unknown_class(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="admin@sched-get-404.com", role="admin")
    await login_as("admin@sched-get-404.com", "testpassword123")
    resp = await client.get(f"/api/classes/{uuid.uuid4()}/schedule")
    assert resp.status_code == 404


async def test_schedule_get_returns_empty_list_for_never_solved_class(
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
    await create_test_user(email="admin@sched-get-empty.com", role="admin")
    await login_as("admin@sched-get-empty.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-sched-get-empty",
    )
    resp = await client.get(f"/api/classes/{cls.id}/schedule")
    assert resp.status_code == 200, resp.text
    assert resp.json() == {"placements": [], "supervision_assignments": []}


async def test_schedule_post_then_get_returns_same_placements(
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
    await create_test_user(email="admin@sched-post-get.com", role="admin")
    await login_as("admin@sched-post-get.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-sched-post-get",
    )
    post_resp = await client.post(f"/api/classes/{cls.id}/schedule")
    assert post_resp.status_code == 200, post_resp.text
    post_placements = post_resp.json()["placements"]
    assert len(post_placements) == 1

    get_resp = await client.get(f"/api/classes/{cls.id}/schedule")
    assert get_resp.status_code == 200, get_resp.text
    get_placements = get_resp.json()["placements"]
    assert get_placements == post_placements


async def test_schedule_post_twice_second_call_replaces_first(
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
    await create_test_user(email="admin@sched-post-twice.com", role="admin")
    await login_as("admin@sched-post-twice.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-sched-post-twice",
    )
    first = await client.post(f"/api/classes/{cls.id}/schedule")
    second = await client.post(f"/api/classes/{cls.id}/schedule")
    assert first.status_code == 200
    assert second.status_code == 200
    # The problem is deterministic, so both solves should produce identical placements.
    assert first.json()["placements"] == second.json()["placements"]

    get_resp = await client.get(f"/api/classes/{cls.id}/schedule")
    assert len(get_resp.json()["placements"]) == 1


async def test_schedule_post_for_class_a_does_not_persist_for_class_b(
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
    await create_test_user(email="admin@sched-ab-isolation.com", role="admin")
    await login_as("admin@sched-ab-isolation.com", "testpassword123")
    cls_a, scheme = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-sched-ab-A",
    )
    cls_b, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1b-sched-ab-B",
        scheme=scheme,
        position=1,
    )
    post_a = await client.post(f"/api/classes/{cls_a.id}/schedule")
    assert post_a.status_code == 200

    get_b = await client.get(f"/api/classes/{cls_b.id}/schedule")
    assert get_b.status_code == 200
    assert get_b.json() == {"placements": [], "supervision_assignments": []}


async def test_read_teacher_schedule_returns_404_for_unknown_teacher(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="admin@teacher-sched-404.com", role="admin")
    await login_as("admin@teacher-sched-404.com", "testpassword123")
    resp = await client.get(f"/api/teachers/{uuid.uuid4()}/schedule")
    assert resp.status_code == 404


async def test_read_teacher_schedule_returns_empty_for_unscheduled_teacher(
    client: AsyncClient,
    create_test_user,
    login_as,
    create_teacher,
) -> None:
    await create_test_user(email="admin@teacher-sched-empty.com", role="admin")
    await login_as("admin@teacher-sched-empty.com", "testpassword123")
    teacher = await create_teacher()
    resp = await client.get(f"/api/teachers/{teacher.id}/schedule")
    assert resp.status_code == 200
    assert resp.json() == {"placements": [], "supervision_assignments": []}


async def test_read_teacher_schedule_returns_placements_after_solve(
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
    await create_test_user(email="admin@teacher-sched-ok.com", role="admin")
    await login_as("admin@teacher-sched-ok.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-teacher-sched-ok",
    )
    post = await client.post(f"/api/classes/{cls.id}/schedule")
    assert post.status_code == 200, post.text
    placements_via_class = post.json()["placements"]
    assert len(placements_via_class) == 1
    teacher_id_str = (
        await db_session.execute(
            select(Lesson.teacher_id).where(Lesson.id == placements_via_class[0]["lesson_id"])
        )
    ).scalar_one()
    resp = await client.get(f"/api/teachers/{teacher_id_str}/schedule")
    assert resp.status_code == 200, resp.text
    placements_via_teacher = resp.json()["placements"]
    assert placements_via_teacher == placements_via_class


async def test_read_room_schedule_returns_404_for_unknown_room(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="admin@room-sched-404.com", role="admin")
    await login_as("admin@room-sched-404.com", "testpassword123")
    resp = await client.get(f"/api/rooms/{uuid.uuid4()}/schedule")
    assert resp.status_code == 404


async def test_read_room_schedule_returns_empty_for_unscheduled_room(
    client: AsyncClient,
    create_test_user,
    login_as,
    create_room,
) -> None:
    await create_test_user(email="admin@room-sched-empty.com", role="admin")
    await login_as("admin@room-sched-empty.com", "testpassword123")
    room = await create_room()
    resp = await client.get(f"/api/rooms/{room.id}/schedule")
    assert resp.status_code == 200
    assert resp.json() == {"placements": [], "supervision_assignments": []}


async def test_read_room_schedule_returns_placements_after_solve(
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
    await create_test_user(email="admin@room-sched-ok.com", role="admin")
    await login_as("admin@room-sched-ok.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-room-sched-ok",
    )
    post = await client.post(f"/api/classes/{cls.id}/schedule")
    assert post.status_code == 200, post.text
    placements_via_class = post.json()["placements"]
    assert len(placements_via_class) == 1
    room_id_str = placements_via_class[0]["room_id"]
    resp = await client.get(f"/api/rooms/{room_id_str}/schedule")
    assert resp.status_code == 200, resp.text
    placements_via_room = resp.json()["placements"]
    assert placements_via_room == placements_via_class


async def test_schedule_post_response_surfaces_solver_picked_teacher_id(
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
    """POST /api/classes/{id}/schedule response carries teacher_id per placement.

    Per OPEN_THINGS item 69, ``PlacementResponse.teacher_id`` mirrors the
    persisted ``ScheduledLesson.teacher_id`` column (non-null since item 63).
    A one-class / one-lesson / one-qualified-teacher problem makes the solver
    pick deterministic.
    """
    await create_test_user(email="admin@sched-tid.com", role="admin")
    await login_as("admin@sched-tid.com", "testpassword123")
    subject = await create_subject()
    week_scheme = await create_week_scheme()
    await create_time_block(
        week_scheme_id=week_scheme.id,
        position=0,
        start_time=time(8, 0),
        end_time=time(8, 45),
    )
    await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        name="1a-sched-tid",
        stundentafel_id=tafel.id,
        week_scheme_id=week_scheme.id,
    )
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=None,
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
    assert len(body["placements"]) == 1
    assert body["placements"][0]["teacher_id"] == str(teacher.id)


async def test_schedule_get_returns_placements_with_teacher_id_after_post(
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
    """GET /api/classes/{id}/schedule surfaces persisted teacher_id per placement.

    Per OPEN_THINGS item 69, the persisted-read paths (`read_schedule_for_class`
    / `read_schedule_for_teacher` / `read_schedule_for_room`) must thread
    ``ScheduledLesson.teacher_id`` through their direct ``PlacementResponse``
    constructors. POST then GET asserts the round-trip.
    """
    await create_test_user(email="admin@sched-tid-get.com", role="admin")
    await login_as("admin@sched-tid-get.com", "testpassword123")
    cls, _ = await _seed_solvable_class(
        db_session,
        create_subject,
        create_week_scheme,
        create_time_block,
        create_room,
        create_teacher,
        create_stundentafel,
        create_school_class,
        class_name="1a-sched-tid-get",
    )
    post_resp = await client.post(f"/api/classes/{cls.id}/schedule")
    assert post_resp.status_code == 200, post_resp.text

    get_resp = await client.get(f"/api/classes/{cls.id}/schedule")
    assert get_resp.status_code == 200, get_resp.text
    body = get_resp.json()
    assert len(body["placements"]) == 1
    placement = body["placements"][0]
    assert "teacher_id" in placement
    assert isinstance(placement["teacher_id"], str)
    # The seeded teacher is the only candidate; solver pick is deterministic.
    assert uuid.UUID(placement["teacher_id"])


async def test_schedule_post_response_carries_quality_report(
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
    """POST /api/classes/{id}/schedule response carries a quality_report payload.

    Item 58 wire-format extension; the report decomposes ``soft_score`` into
    its per-axis raw counts so admins can debug why a soft score is the
    number it is. The ``weighted_score == soft_score`` parity invariant is
    pinned on the Rust side (``solver-core/tests/solution_quality_report_json.rs``).
    Mirrors the inline one-class shape of
    ``test_schedule_post_response_surfaces_solver_picked_teacher_id``.
    """
    await create_test_user(email="admin@sched-qr.com", role="admin")
    await login_as("admin@sched-qr.com", "testpassword123")
    subject = await create_subject()
    week_scheme = await create_week_scheme()
    await create_time_block(
        week_scheme_id=week_scheme.id,
        position=0,
        start_time=time(8, 0),
        end_time=time(8, 45),
    )
    await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        name="1a-sched-qr",
        stundentafel_id=tafel.id,
        week_scheme_id=week_scheme.id,
    )
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=None,
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
    assert "quality_report" in body, "quality_report must be on the wire format"
    qr = body["quality_report"]
    # Mirrors QualityReport in solver-core/src/quality.rs (18 fields total).
    expected_fields = {
        "hard_violations",
        "unplaced_hours",
        "class_gap_hours",
        "class_gap_hours_by_class",
        "teacher_gap_hours",
        "teacher_gap_hours_by_teacher",
        "class_day_balance_cost",
        "class_day_balance_cost_by_class",
        "home_room_misses",
        "home_room_misses_by_class",
        "prefer_early_units",
        "avoid_first_units",
        "avoid_last_units",
        "prefer_late_units",
        "prefer_class_teacher_misses",
        "weighted_score",
        "worst_per_class_spread",
        "worst_per_class_interior_gaps",
    }
    assert expected_fields == set(qr.keys()), (
        f"quality_report fields drift from solver-core: "
        f"missing={expected_fields - set(qr.keys())}, "
        f"extra={set(qr.keys()) - expected_fields}"
    )
    assert qr["weighted_score"] == body["soft_score"], (
        "weighted_score == soft_score parity invariant (pinned in "
        "solver-core/tests/solution_quality_report_json.rs)"
    )
    # Per-axis attribution map shape: dict[str (UUID), int (count)].
    for map_field in (
        "class_gap_hours_by_class",
        "teacher_gap_hours_by_teacher",
        "class_day_balance_cost_by_class",
        "home_room_misses_by_class",
    ):
        assert isinstance(qr[map_field], dict), f"{map_field} must be a dict"
        for k, v in qr[map_field].items():
            assert isinstance(k, str), f"{map_field} keys must be UUID strings"
            assert isinstance(v, int) and v > 0, (
                f"{map_field}[{k}] must be a positive int (skip-zero invariant)"
            )

    # Sum-equals-legacy invariant per axis (mirrors solver-core proptest).
    for legacy, map_field in (
        ("class_gap_hours", "class_gap_hours_by_class"),
        ("teacher_gap_hours", "teacher_gap_hours_by_teacher"),
        ("class_day_balance_cost", "class_day_balance_cost_by_class"),
        ("home_room_misses", "home_room_misses_by_class"),
    ):
        assert sum(qr[map_field].values()) == qr[legacy], (
            f"sum({map_field}.values()) must equal {legacy}"
        )
