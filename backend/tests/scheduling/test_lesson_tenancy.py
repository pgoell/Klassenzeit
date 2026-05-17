"""Cross-school tenancy isolation tests for the Lesson aggregate."""

import json
import uuid

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.pin_kind import PinKind
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.teacher import TeacherQualification
from klassenzeit_backend.scheduling.solver_io import build_problem_json

pytestmark = pytest.mark.anyio


@pytest.fixture
async def school_b_lessons(db_session: AsyncSession) -> School:
    """A second school distinct from DEFAULT_SCHOOL_ID."""
    school = School(name="Schule B (lessons)", short_name="SBL")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_list_lessons_excludes_other_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_lessons: School,
    create_test_user,
    login_as,
    create_subject,
    create_week_scheme,
    create_stundentafel,
    create_school_class,
) -> None:
    user, password = await create_test_user(
        email="admin-lesson-list@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    own_subj = await create_subject(name="Mathe-own", short_name="Mo")
    foreign_subj = await create_subject(
        name="Mathe-foreign", short_name="Mf", school_id=school_b_lessons.id
    )
    own_ws = await create_week_scheme(name="WS-own")
    foreign_ws = await create_week_scheme(name="WS-foreign", school_id=school_b_lessons.id)
    own_tafel = await create_stundentafel(name="Tafel-own")
    foreign_tafel = await create_stundentafel(name="Tafel-foreign", school_id=school_b_lessons.id)
    own_class = await create_school_class(
        name="1a-own", stundentafel_id=own_tafel.id, week_scheme_id=own_ws.id
    )
    foreign_class = await create_school_class(
        name="1a-foreign",
        stundentafel_id=foreign_tafel.id,
        week_scheme_id=foreign_ws.id,
        school_id=school_b_lessons.id,
    )

    own_lesson = Lesson(
        subject_id=own_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=DEFAULT_SCHOOL_ID,
    )
    foreign_lesson = Lesson(
        subject_id=foreign_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=school_b_lessons.id,
    )
    db_session.add_all([own_lesson, foreign_lesson])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=own_lesson.id, school_class_id=own_class.id),
            LessonSchoolClass(lesson_id=foreign_lesson.id, school_class_id=foreign_class.id),
        ]
    )
    await db_session.commit()

    await login_as(user.email, password)
    response = await client.get("/api/lessons")
    assert response.status_code == 200
    returned_ids = {row["id"] for row in response.json()}
    assert str(own_lesson.id) in returned_ids
    assert str(foreign_lesson.id) not in returned_ids


async def test_get_lesson_in_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_lessons: School,
    create_test_user,
    login_as,
    create_subject,
) -> None:
    user, password = await create_test_user(
        email="admin-lesson-get@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_subj = await create_subject(
        name="Foreign-get", short_name="Fge", school_id=school_b_lessons.id
    )
    foreign = Lesson(
        subject_id=foreign_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=school_b_lessons.id,
    )
    db_session.add(foreign)
    await db_session.commit()

    await login_as(user.email, password)
    response = await client.get(f"/api/lessons/{foreign.id}")
    assert response.status_code == 404


async def test_patch_lesson_in_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_lessons: School,
    create_test_user,
    login_as,
    create_subject,
) -> None:
    user, password = await create_test_user(
        email="admin-lesson-patch@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_subj = await create_subject(
        name="Foreign-patch", short_name="Fpa", school_id=school_b_lessons.id
    )
    foreign = Lesson(
        subject_id=foreign_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=school_b_lessons.id,
    )
    db_session.add(foreign)
    await db_session.commit()

    await login_as(user.email, password)
    response = await client.patch(f"/api/lessons/{foreign.id}", json={"hours_per_week": 4})
    assert response.status_code == 404


async def test_delete_lesson_in_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_lessons: School,
    create_test_user,
    login_as,
    create_subject,
) -> None:
    user, password = await create_test_user(
        email="admin-lesson-del@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_subj = await create_subject(
        name="Foreign-del", short_name="Fdl", school_id=school_b_lessons.id
    )
    foreign = Lesson(
        subject_id=foreign_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=school_b_lessons.id,
    )
    db_session.add(foreign)
    await db_session.commit()

    await login_as(user.email, password)
    response = await client.delete(f"/api/lessons/{foreign.id}")
    assert response.status_code == 404


async def test_create_lesson_stamps_current_user_school_id(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    create_subject,
    create_week_scheme,
    create_stundentafel,
    create_school_class,
) -> None:
    user, password = await create_test_user(
        email="admin-lesson-create@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    subj = await create_subject(name="Mathe-create", short_name="Mcr")
    ws = await create_week_scheme(name="WS-create")
    tafel = await create_stundentafel(name="Tafel-create")
    cls = await create_school_class(
        name="1a-create", stundentafel_id=tafel.id, week_scheme_id=ws.id
    )
    await db_session.commit()
    await login_as(user.email, password)
    response = await client.post(
        "/api/lessons",
        json={
            "subject_id": str(subj.id),
            "teacher_id": None,
            "school_class_ids": [str(cls.id)],
            "hours_per_week": 2,
            "preferred_block_size": 1,
        },
    )
    assert response.status_code == 201
    created_id = uuid.UUID(response.json()["id"])
    row = (
        await db_session.execute(select(Lesson.school_id).where(Lesson.id == created_id))
    ).scalar_one()
    assert row == DEFAULT_SCHOOL_ID


async def test_generate_lessons_from_stundentafel_stamps_school_id(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    create_subject,
    create_teacher,
    create_week_scheme,
    create_stundentafel,
    create_stundentafel_entry,
    create_school_class,
) -> None:
    user, password = await create_test_user(
        email="admin-lesson-gen@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    subj = await create_subject(name="Mathe-gen", short_name="Mge")
    teacher = await create_teacher(short_code="TGE")

    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subj.id))
    await db_session.flush()
    ws = await create_week_scheme(name="WS-gen")
    tafel = await create_stundentafel(name="Tafel-gen")
    await create_stundentafel_entry(
        stundentafel_id=tafel.id, subject_id=subj.id, hours_per_week=2, preferred_block_size=1
    )
    cls = await create_school_class(name="1a-gen", stundentafel_id=tafel.id, week_scheme_id=ws.id)
    await db_session.commit()

    await login_as(user.email, password)
    response = await client.post(f"/api/classes/{cls.id}/generate-lessons")
    assert response.status_code == 201
    created_ids = [uuid.UUID(row["id"]) for row in response.json()]
    assert created_ids
    rows = (
        (await db_session.execute(select(Lesson.school_id).where(Lesson.id.in_(created_ids))))
        .scalars()
        .all()
    )
    assert all(school_id == DEFAULT_SCHOOL_ID for school_id in rows)


async def test_move_placement_with_cross_school_lesson_id_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_lessons: School,
    create_test_user,
    login_as,
    create_subject,
    create_teacher,
    create_week_scheme,
    create_time_block,
    create_room,
    create_stundentafel,
    create_school_class,
) -> None:
    """Moving a placement whose lesson belongs to another school returns 404.

    Seeds a foreign-school lesson with a foreign-school placement (something a
    well-behaved writer would never produce, but the path defends against
    direct DB writes / leaks). The move route loads the placement scoped to
    the requester's school and 404s because the lesson is in another tenant.
    """
    user, password = await create_test_user(
        email="admin-lesson-move@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_subj = await create_subject(
        name="Foreign-move", short_name="Fmv", school_id=school_b_lessons.id
    )
    foreign_teacher = await create_teacher(short_code="FMV", school_id=school_b_lessons.id)
    foreign_ws = await create_week_scheme(name="WS-move-f", school_id=school_b_lessons.id)
    foreign_tb_a = await create_time_block(week_scheme_id=foreign_ws.id)
    foreign_room = await create_room(school_id=school_b_lessons.id)
    foreign_tafel = await create_stundentafel(name="Tafel-move-f", school_id=school_b_lessons.id)
    foreign_class = await create_school_class(
        name="cls-move-f",
        stundentafel_id=foreign_tafel.id,
        week_scheme_id=foreign_ws.id,
        school_id=school_b_lessons.id,
    )
    foreign_lesson = Lesson(
        subject_id=foreign_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=school_b_lessons.id,
    )
    db_session.add(foreign_lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=foreign_lesson.id, school_class_id=foreign_class.id))
    db_session.add(
        ScheduledLesson(
            lesson_id=foreign_lesson.id,
            time_block_id=foreign_tb_a.id,
            room_id=foreign_room.id,
            teacher_id=foreign_teacher.id,
            pin_kind=None,
        )
    )
    # Also seed an own-school room so the move body has a valid target room.
    own_room = await create_room(name="own-room-move", short_name="ORM")
    own_ws = await create_week_scheme(name="WS-move-own")
    own_tb = await create_time_block(week_scheme_id=own_ws.id)
    await db_session.commit()

    await login_as(user.email, password)
    response = await client.patch(
        f"/api/placements/{foreign_lesson.id}/{foreign_tb_a.id}",
        json={
            "time_block_id": str(own_tb.id),
            "room_id": str(own_room.id),
        },
    )
    assert response.status_code == 404


async def test_swap_placements_rejects_cross_school_lesson_id_with_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_lessons: School,
    create_test_user,
    login_as,
    create_subject,
    create_teacher,
    create_week_scheme,
    create_time_block,
    create_room,
    create_stundentafel,
    create_school_class,
) -> None:
    """Even when ONE side of the swap references a foreign-school lesson, the
    whole swap aborts with 404. We seed both sides as foreign placements so
    the swap loader sees the cross-tenant references and 404s before mutating.
    """
    user, password = await create_test_user(
        email="admin-lesson-swap@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_subj = await create_subject(
        name="Foreign-swap", short_name="Fsw", school_id=school_b_lessons.id
    )
    foreign_teacher = await create_teacher(short_code="FSW", school_id=school_b_lessons.id)
    foreign_ws = await create_week_scheme(name="WS-swap-f", school_id=school_b_lessons.id)
    foreign_tb_a = await create_time_block(week_scheme_id=foreign_ws.id, position=1)
    foreign_tb_b = await create_time_block(week_scheme_id=foreign_ws.id, position=2)
    foreign_room = await create_room(school_id=school_b_lessons.id)
    foreign_tafel = await create_stundentafel(name="Tafel-swap-f", school_id=school_b_lessons.id)
    foreign_class = await create_school_class(
        name="cls-swap-f",
        stundentafel_id=foreign_tafel.id,
        week_scheme_id=foreign_ws.id,
        school_id=school_b_lessons.id,
    )
    foreign_lesson_a = Lesson(
        subject_id=foreign_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=school_b_lessons.id,
    )
    foreign_lesson_b = Lesson(
        subject_id=foreign_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=school_b_lessons.id,
    )
    db_session.add_all([foreign_lesson_a, foreign_lesson_b])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=foreign_lesson_a.id, school_class_id=foreign_class.id),
            LessonSchoolClass(lesson_id=foreign_lesson_b.id, school_class_id=foreign_class.id),
            ScheduledLesson(
                lesson_id=foreign_lesson_a.id,
                time_block_id=foreign_tb_a.id,
                room_id=foreign_room.id,
                teacher_id=foreign_teacher.id,
                pin_kind=PinKind.HARD,
            ),
            ScheduledLesson(
                lesson_id=foreign_lesson_b.id,
                time_block_id=foreign_tb_b.id,
                room_id=foreign_room.id,
                teacher_id=foreign_teacher.id,
                pin_kind=PinKind.HARD,
            ),
        ]
    )
    await db_session.commit()

    await login_as(user.email, password)
    response = await client.post(
        "/api/placements/swap",
        json={
            "a": {
                "lesson_id": str(foreign_lesson_a.id),
                "time_block_id": str(foreign_tb_a.id),
            },
            "b": {
                "lesson_id": str(foreign_lesson_b.id),
                "time_block_id": str(foreign_tb_b.id),
            },
        },
    )
    assert response.status_code == 404


async def test_build_problem_json_omits_other_school_lessons(
    db_session: AsyncSession,
    school_b_lessons: School,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_stundentafel,
    create_school_class,
) -> None:
    """The solver input for a class in school A must not see any school-B lessons,
    even if a LessonSchoolClass row links them (impossible under normal writers,
    but the defensive filter protects against any future cross-school join leakage).
    """
    own_subj = await create_subject(name="Own-bpj", short_name="Obp")
    foreign_subj = await create_subject(
        name="Foreign-bpj", short_name="Fbp", school_id=school_b_lessons.id
    )
    ws = await create_week_scheme(name="WS-bpj")
    # Solver needs at least one TimeBlock and one Room to build the problem.
    await create_time_block(week_scheme_id=ws.id, position=1)
    await create_room(name="bpj-room", short_name="BPJ")
    tafel = await create_stundentafel(name="Tafel-bpj")
    cls = await create_school_class(name="1a-bpj", stundentafel_id=tafel.id, week_scheme_id=ws.id)
    own_lesson = Lesson(
        subject_id=own_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=DEFAULT_SCHOOL_ID,
    )
    foreign_lesson = Lesson(
        subject_id=foreign_subj.id,
        hours_per_week=2,
        preferred_block_size=1,
        school_id=school_b_lessons.id,
    )
    db_session.add_all([own_lesson, foreign_lesson])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=own_lesson.id, school_class_id=cls.id),
            LessonSchoolClass(lesson_id=foreign_lesson.id, school_class_id=cls.id),
        ]
    )
    await db_session.commit()

    problem_json, _class_lesson_ids, _counts = await build_problem_json(
        db_session, cls.id, school_id=DEFAULT_SCHOOL_ID
    )
    problem = json.loads(problem_json)
    lesson_ids_in_problem = {lesson["id"] for lesson in problem["lessons"]}
    assert str(own_lesson.id) in lesson_ids_in_problem
    assert str(foreign_lesson.id) not in lesson_ids_in_problem


async def test_lesson_school_id_required_after_default_drop(
    db_session: AsyncSession,
    create_subject,
) -> None:
    """After commit B drops the model server_default, omitting `school_id` on a
    direct Lesson(...) constructor IntegrityErrors. Pins the contract so future
    contributors can't silently drop into the default tenant."""
    subj = await create_subject(name="Required-school", short_name="Rsc")
    async with db_session.begin_nested():
        with pytest.raises(IntegrityError):
            no_school = Lesson(subject_id=subj.id, hours_per_week=2, preferred_block_size=1)
            db_session.add(no_school)
            await db_session.flush()
