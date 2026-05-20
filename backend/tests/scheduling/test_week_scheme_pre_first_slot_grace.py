"""Tests for WeekScheme.pre_first_slot_grace_minutes wiring.

Covers the ORM default, the create/patch routes, the Pydantic clamp, and the
``build_problem_json`` passthrough so the value reaches the solver wire format.
"""

import json
from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import Stundentafel
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.models.week_scheme import TimeBlock, WeekScheme
from klassenzeit_backend.scheduling.solver_io import build_problem_json

pytestmark = pytest.mark.anyio

type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]
type CreateSubjectFn = Callable[..., Awaitable[Subject]]
type CreateWeekSchemeFn = Callable[..., Awaitable[WeekScheme]]
type CreateTimeBlockFn = Callable[..., Awaitable[TimeBlock]]
type CreateRoomFn = Callable[..., Awaitable[object]]
type CreateTeacherFn = Callable[..., Awaitable[Teacher]]
type CreateStundentafelFn = Callable[..., Awaitable[Stundentafel]]
type CreateSchoolClassFn = Callable[..., Awaitable[SchoolClass]]


async def test_week_scheme_default_grace_is_zero(db_session: AsyncSession) -> None:
    """The ORM default for ``pre_first_slot_grace_minutes`` is 0."""
    scheme = WeekScheme(name="Default Grace Scheme", school_id=DEFAULT_SCHOOL_ID)
    db_session.add(scheme)
    await db_session.flush()
    await db_session.refresh(scheme)
    assert scheme.pre_first_slot_grace_minutes == 0


async def test_create_week_scheme_persists_grace(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /week-schemes accepts and persists pre_first_slot_grace_minutes."""
    await create_test_user(email="admin@grace1.com", role="admin")
    await login_as("admin@grace1.com", "testpassword123")
    response = await client.post(
        "/api/week-schemes",
        json={"name": "Grace 30", "pre_first_slot_grace_minutes": 30},
    )
    assert response.status_code == 201, response.text
    assert response.json()["pre_first_slot_grace_minutes"] == 30


async def test_patch_week_scheme_updates_grace(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /week-schemes/{id} updates pre_first_slot_grace_minutes in place."""
    await create_test_user(email="admin@grace2.com", role="admin")
    await login_as("admin@grace2.com", "testpassword123")
    created = (
        await client.post(
            "/api/week-schemes",
            json={"name": "Grace Patch", "pre_first_slot_grace_minutes": 0},
        )
    ).json()
    response = await client.patch(
        f"/api/week-schemes/{created['id']}",
        json={"pre_first_slot_grace_minutes": 20},
    )
    assert response.status_code == 200, response.text
    assert response.json()["pre_first_slot_grace_minutes"] == 20


async def test_create_rejects_out_of_range_grace(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /week-schemes with pre_first_slot_grace_minutes > 60 returns 422."""
    await create_test_user(email="admin@grace3.com", role="admin")
    await login_as("admin@grace3.com", "testpassword123")
    response = await client.post(
        "/api/week-schemes",
        json={"name": "Grace Too High", "pre_first_slot_grace_minutes": 200},
    )
    assert response.status_code == 422


async def test_build_problem_json_emits_grace(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """build_problem_json stamps pre_first_slot_grace_minutes from the WeekScheme row."""
    scheme = WeekScheme(
        name="grace-io-test",
        school_id=DEFAULT_SCHOOL_ID,
        pre_first_slot_grace_minutes=25,
    )
    db_session.add(scheme)
    await db_session.flush()

    subject = await create_subject()
    await create_time_block(week_scheme_id=scheme.id, position=1)
    await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    lesson = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, cls.id, school_id=DEFAULT_SCHOOL_ID)
    problem = json.loads(problem_json)
    assert problem["pre_first_slot_grace_minutes"] == 25
