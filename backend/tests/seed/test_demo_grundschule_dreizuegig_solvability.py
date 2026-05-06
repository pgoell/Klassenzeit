"""End-to-end solvability check for demo_grundschule_dreizuegig.

The flow: seed (which itself inserts the cross-class Religion trio with
teacher_ids pinned) -> per-class POST /api/classes/{id}/generate-lessons
(creates non-Religion lessons; Religion subjects are skipped because the
seed already linked them via LessonSchoolClass) -> overwrite
Lesson.teacher_id from _TEACHER_ASSIGNMENTS_DREIZUEGIG (pins teacher
allocation for the non-Religion lessons so the bench-stable layout does
not drift as auto_assign_teachers_for_lessons evolves) -> per-class
POST /api/classes/{id}/schedule -> assert each class produces
violations == [] and at least one placement.

Mirrors the HTTP-route pattern from test_demo_grundschule_solvability.py
and test_demo_grundschule_zweizuegig_solvability.py.
"""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient
from sqlalchemy import select, update
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.main import app
from klassenzeit_backend.seed.demo_grundschule_dreizuegig import (
    _TEACHER_ASSIGNMENTS_DREIZUEGIG,
    seed_demo_grundschule_dreizuegig,
)

CreateUserFnDr = Callable[..., Awaitable[tuple[User, str]]]
LoginFnDr = Callable[[str, str], Awaitable[None]]


async def test_seeded_grundschule_dreizuegig_solves_with_zero_violations(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnDr,
    login_as: LoginFnDr,
) -> None:
    await seed_demo_grundschule_dreizuegig(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-dr-seedtest@example.com",
        password="seed-dr-test-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    class_rows = (
        (
            await db_session.execute(
                select(SchoolClass).order_by(SchoolClass.grade_level, SchoolClass.name)
            )
        )
        .scalars()
        .all()
    )
    assert [c.name for c in class_rows] == [
        "1a",
        "1b",
        "1c",
        "2a",
        "2b",
        "2c",
        "3a",
        "3b",
        "3c",
        "4a",
        "4b",
        "4c",
    ]

    for school_class in class_rows:
        gen_resp = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
        assert gen_resp.status_code == 201, gen_resp.text

    teachers_by_short = {
        t.short_code: t for t in (await db_session.execute(select(Teacher))).scalars().all()
    }
    subjects_by_short = {
        s.short_name: s for s in (await db_session.execute(select(Subject))).scalars().all()
    }
    classes_by_name = {c.name: c for c in class_rows}

    # Pin teacher_id from the authored mapping (overrides whatever
    # auto-assign chose, so the layout stays stable regardless of how
    # auto_assign_teachers_for_lessons evolves). Religion lessons were
    # pinned at seed time and are not in the mapping.
    for (class_name, subject_short), teacher_short in _TEACHER_ASSIGNMENTS_DREIZUEGIG.items():
        school_class = classes_by_name[class_name]
        await db_session.execute(
            update(Lesson)
            .where(
                Lesson.id.in_(
                    select(LessonSchoolClass.lesson_id).where(
                        LessonSchoolClass.school_class_id == school_class.id
                    )
                ),
                Lesson.subject_id == subjects_by_short[subject_short].id,
            )
            .values(teacher_id=teachers_by_short[teacher_short].id)
        )
    await db_session.flush()

    for school_class in class_rows:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])
        assert len(body["placements"]) > 0, school_class.name


@pytest.mark.xfail(
    strict=False,
    reason=(
        "auto_assign_teachers_for_lessons distribution intermittently "
        "produces a teacher allocation that the production solver path "
        "(lahc_rr_kempe at 5000 ms) cannot recover from; the failing class "
        "rotates across the cohort (1a, 2a, 3a, 4b observed) with "
        "lesson_group_split as the dominant violation kind and "
        "no_suitable_room as a secondary mode (~4/5 fail rate in "
        "isolation on the dreizuegige fixture). Tracked under OPEN_THINGS "
        "item 4 (subject UUID order leak in scarcity-first auto-assign "
        "tiebreak)."
    ),
)
async def test_seeded_grundschule_dreizuegig_solves_with_auto_assigned_teachers(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnDr,
    login_as: LoginFnDr,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Production-route mirror for the dreizuegige seed: skips the canonical
    _TEACHER_ASSIGNMENTS_DREIZUEGIG UPDATE so the test exercises the
    auto-assign distribution end-to-end. Item 32.

    The cross-class Religion trio is still pinned at seed time (it has to
    be, because LessonSchoolClass relationships drive auto-assign
    eligibility); only the per-class non-Religion teacher allocation
    differs from the canonical sibling.
    """
    monkeypatch.setattr(app.state.settings, "solve_deadline_ms", 5000)
    await seed_demo_grundschule_dreizuegig(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-dr-autoassign-seedtest@example.com",
        password="seed-dr-autoassign-test-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    class_rows = (
        (
            await db_session.execute(
                select(SchoolClass).order_by(SchoolClass.grade_level, SchoolClass.name)
            )
        )
        .scalars()
        .all()
    )

    for school_class in class_rows:
        gen_resp = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
        assert gen_resp.status_code == 201, gen_resp.text

    for school_class in class_rows:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])
        assert len(body["placements"]) > 0, school_class.name
