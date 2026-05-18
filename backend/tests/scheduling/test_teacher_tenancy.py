"""Cross-school isolation tests for the Teacher aggregate."""

import json
import uuid

import pytest
from fastapi import HTTPException
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.session import UserSession
from klassenzeit_backend.db.models.stundentafel import StundentafelEntry
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.scheduling.quality_checks import (
    compute_quality_attribution_for_teacher,
)
from klassenzeit_backend.scheduling.solver_io import build_problem_json

pytestmark = pytest.mark.anyio


@pytest.fixture
async def school_b_teachers(db_session: AsyncSession) -> School:
    """A second school distinct from DEFAULT_SCHOOL_ID."""
    school = School(name="Schule B (teachers)", short_name="SBT")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_duplicate_short_code_within_school_rejected(
    db_session: AsyncSession,
    create_teacher,
) -> None:
    """A second teacher with the same short_code in the same school violates the
    composite UNIQUE constraint."""
    await create_teacher(short_code="ABC", school_id=DEFAULT_SCHOOL_ID)
    async with db_session.begin_nested():
        with pytest.raises(IntegrityError):
            await create_teacher(short_code="ABC", school_id=DEFAULT_SCHOOL_ID)
            await db_session.flush()


async def test_duplicate_short_code_across_schools_allowed(
    db_session: AsyncSession,
    create_teacher,
    school_b_teachers: School,
) -> None:
    """The same short_code may live in two different schools simultaneously."""
    a = await create_teacher(short_code="ABC", school_id=DEFAULT_SCHOOL_ID)
    b = await create_teacher(short_code="ABC", school_id=school_b_teachers.id)
    assert a.id != b.id
    rows = (
        (await db_session.execute(select(Teacher).where(Teacher.short_code == "ABC")))
        .scalars()
        .all()
    )
    assert {r.school_id for r in rows} == {DEFAULT_SCHOOL_ID, school_b_teachers.id}


async def test_list_teachers_excludes_other_school(
    client: AsyncClient,
    school_b_teachers: School,
    create_test_user,
    login_as,
    create_teacher,
) -> None:
    """GET /teachers returns only the requesting user's school's teachers."""
    user_a, password = await create_test_user(
        email="admin-teachers-a@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    await create_teacher(short_code="AAA", school_id=DEFAULT_SCHOOL_ID)
    await create_teacher(short_code="BBB", school_id=school_b_teachers.id)

    await login_as(user_a.email, password)
    response = await client.get("/api/teachers")
    assert response.status_code == 200
    body = response.json()
    short_codes = {row["short_code"] for row in body}
    assert "AAA" in short_codes
    assert "BBB" not in short_codes


async def test_get_teacher_returns_404_for_cross_school(
    client: AsyncClient,
    school_b_teachers: School,
    create_test_user,
    login_as,
    create_teacher,
) -> None:
    """GET /teachers/{id} where the teacher is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-teachers-detail@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    other = await create_teacher(short_code="BCD", school_id=school_b_teachers.id)

    await login_as(user.email, password)
    response = await client.get(f"/api/teachers/{other.id}")
    assert response.status_code == 404


async def test_patch_teacher_returns_404_for_cross_school(
    client: AsyncClient,
    school_b_teachers: School,
    create_test_user,
    login_as,
    create_teacher,
) -> None:
    """PATCH /teachers/{id} where the teacher is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-teachers-patch@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    other = await create_teacher(short_code="BEF", school_id=school_b_teachers.id)

    await login_as(user.email, password)
    response = await client.patch(f"/api/teachers/{other.id}", json={"first_name": "Hacker"})
    assert response.status_code == 404


async def test_delete_teacher_returns_404_for_cross_school(
    client: AsyncClient,
    school_b_teachers: School,
    create_test_user,
    login_as,
    create_teacher,
) -> None:
    """DELETE /teachers/{id} where the teacher is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-teachers-del@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    other = await create_teacher(short_code="BGH", school_id=school_b_teachers.id)

    await login_as(user.email, password)
    response = await client.delete(f"/api/teachers/{other.id}")
    assert response.status_code == 404


async def test_create_teacher_stamps_current_user_school_id(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    school_b_teachers: School,
) -> None:
    """POST /teachers stamps school_id from the current user, not from the body."""
    user, password = await create_test_user(
        email="admin-teachers-create@test.com",
        role="admin",
        school_id=school_b_teachers.id,
    )
    await login_as(user.email, password)
    response = await client.post(
        "/api/teachers",
        json={
            "first_name": "Neu",
            "last_name": "Lehrkraft",
            "short_code": "NEW",
            "max_hours_per_week": 28,
        },
    )
    assert response.status_code == 201
    teacher_id = response.json()["id"]
    teacher = (
        await db_session.execute(select(Teacher).where(Teacher.id == uuid.UUID(teacher_id)))
    ).scalar_one()
    assert teacher.school_id == school_b_teachers.id


async def test_put_teacher_qualifications_returns_404_for_cross_school(
    client: AsyncClient,
    school_b_teachers: School,
    create_test_user,
    login_as,
    create_teacher,
) -> None:
    """PUT /teachers/{id}/qualifications where the teacher is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-teachers-qual@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    other = await create_teacher(short_code="BIJ", school_id=school_b_teachers.id)

    await login_as(user.email, password)
    response = await client.put(
        f"/api/teachers/{other.id}/qualifications", json={"subject_ids": []}
    )
    assert response.status_code == 404


async def test_put_teacher_availability_returns_404_for_cross_school(
    client: AsyncClient,
    school_b_teachers: School,
    create_test_user,
    login_as,
    create_teacher,
) -> None:
    """PUT /teachers/{id}/availability where the teacher is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-teachers-avail@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    other = await create_teacher(short_code="BKL", school_id=school_b_teachers.id)

    await login_as(user.email, password)
    response = await client.put(f"/api/teachers/{other.id}/availability", json={"entries": []})
    assert response.status_code == 404


async def test_read_schedule_for_teacher_returns_404_for_cross_school(
    client: AsyncClient,
    school_b_teachers: School,
    create_test_user,
    login_as,
    create_teacher,
) -> None:
    """GET /teachers/{id}/schedule for a cross-school teacher returns 404."""
    user, password = await create_test_user(
        email="admin-teachers-sched@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    other = await create_teacher(short_code="BMN", school_id=school_b_teachers.id)

    await login_as(user.email, password)
    response = await client.get(f"/api/teachers/{other.id}/schedule")
    assert response.status_code == 404


async def test_quality_attribution_for_teacher_returns_404_for_cross_school(
    db_session: AsyncSession,
    school_b_teachers: School,
    create_teacher,
) -> None:
    """compute_quality_attribution_for_teacher 404s before any placement-load work
    when the teacher id belongs to a different school."""
    other = await create_teacher(short_code="BQA", school_id=school_b_teachers.id)
    with pytest.raises(HTTPException) as excinfo:
        await compute_quality_attribution_for_teacher(
            db_session, other.id, school_id=DEFAULT_SCHOOL_ID
        )
    assert excinfo.value.status_code == 404


async def test_build_problem_json_excludes_other_school_teachers(
    db_session: AsyncSession,
    school_b_teachers: School,
    create_teacher,
    create_subject,
    create_stundentafel,
    create_week_scheme,
    create_time_block,
    create_school_class,
    create_room,
) -> None:
    """build_problem_json on a School A class must not include School B teachers."""
    subject = await create_subject()
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    await create_time_block(week_scheme_id=scheme.id)
    school_class = await create_school_class(
        name="A 3a",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=DEFAULT_SCHOOL_ID,
    )
    await create_room(school_id=DEFAULT_SCHOOL_ID)
    school_a_teacher = await create_teacher(short_code="SAA", school_id=DEFAULT_SCHOOL_ID)
    school_b_teacher = await create_teacher(short_code="SBB", school_id=school_b_teachers.id)
    db_session.add(TeacherQualification(teacher_id=school_a_teacher.id, subject_id=subject.id))
    db_session.add(TeacherQualification(teacher_id=school_b_teacher.id, subject_id=subject.id))
    lesson = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=None,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=school_class.id))
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(
        db_session, school_class.id, school_id=DEFAULT_SCHOOL_ID
    )
    payload = json.loads(problem_json)
    teacher_ids = {t["id"] for t in payload["teachers"]}
    assert str(school_a_teacher.id) in teacher_ids
    assert str(school_b_teacher.id) not in teacher_ids


async def test_generate_lessons_qualified_teacher_check_scoped_to_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_teachers: School,
    create_test_user,
    login_as,
    create_teacher,
    create_subject,
    create_stundentafel,
    create_week_scheme,
    create_school_class,
) -> None:
    """POST /classes/{id}/generate-lessons rejects the call when the school's own
    teachers do not qualify for a curriculum subject, even if a different school
    has a qualified teacher for that subject."""
    subject = await create_subject(name="Mathematik", short_name="Ma")
    school_b_math_teacher = await create_teacher(short_code="BMT", school_id=school_b_teachers.id)
    db_session.add(TeacherQualification(teacher_id=school_b_math_teacher.id, subject_id=subject.id))

    tafel = await create_stundentafel()
    db_session.add(
        StundentafelEntry(stundentafel_id=tafel.id, subject_id=subject.id, hours_per_week=4)
    )
    scheme = await create_week_scheme()
    school_class = await create_school_class(
        name="A 4a",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=DEFAULT_SCHOOL_ID,
    )
    await db_session.flush()

    user, password = await create_test_user(
        email="admin-genless-school-a@test.com",
        role="admin",
        school_id=DEFAULT_SCHOOL_ID,
    )
    await login_as(user.email, password)
    response = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
    assert response.status_code == 422
    detail = response.json()["detail"]
    assert detail["code"] == "missing_qualified_teacher"
    assert str(subject.id) in detail["subject_ids"]


async def test_super_admin_with_active_school_sees_other_teachers(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_teachers,
    create_teacher,
    create_test_user,
    login_as,
) -> None:
    """Super-admin with session.active_school_id=<other> sees the other school's teachers."""
    sa, password = await create_test_user(
        email="sa-teacher-other@test.com", role="super_admin", school_id=DEFAULT_SCHOOL_ID
    )
    home_teacher = await create_teacher(short_code="HOM")
    other_teacher = await create_teacher(short_code="OTH", school_id=school_b_teachers.id)
    await login_as(sa.email, password)
    # Mutate the cookie session's active_school_id (simulates POST /auth/switch-school
    # before that endpoint ships in a follow-up bundle).
    cookie = client.cookies.get("kz_session")
    assert cookie is not None
    session = await db_session.get(UserSession, uuid.UUID(cookie))
    assert session is not None
    session.active_school_id = school_b_teachers.id
    await db_session.flush()

    response = await client.get("/api/teachers")
    assert response.status_code == 200
    ids = {row["id"] for row in response.json()}
    assert str(other_teacher.id) in ids
    assert str(home_teacher.id) not in ids


async def test_super_admin_no_param_sees_home_teachers_only(
    client: AsyncClient,
    school_b_teachers,
    create_teacher,
    create_test_user,
    login_as,
) -> None:
    """Super-admin without ?school_id sees home teachers only."""
    sa, password = await create_test_user(
        email="sa-teacher-home@test.com", role="super_admin", school_id=DEFAULT_SCHOOL_ID
    )
    home_teacher = await create_teacher(short_code="HM2")
    other_teacher = await create_teacher(short_code="OT2", school_id=school_b_teachers.id)
    await login_as(sa.email, password)
    response = await client.get("/api/teachers")
    assert response.status_code == 200
    ids = {row["id"] for row in response.json()}
    assert str(home_teacher.id) in ids
    assert str(other_teacher.id) not in ids


async def test_admin_with_other_school_param_is_ignored_on_teachers(
    client: AsyncClient,
    school_b_teachers,
    create_teacher,
    create_test_user,
    login_as,
) -> None:
    """Plain admin with ?school_id=<other> still sees home school's teachers only."""
    admin, password = await create_test_user(
        email="admin-teacher-ignore@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    home_teacher = await create_teacher(short_code="AHM")
    other_teacher = await create_teacher(short_code="AOT", school_id=school_b_teachers.id)
    await login_as(admin.email, password)
    response = await client.get(f"/api/teachers?school_id={school_b_teachers.id}")
    assert response.status_code == 200
    ids = {row["id"] for row in response.json()}
    assert str(home_teacher.id) in ids
    assert str(other_teacher.id) not in ids
