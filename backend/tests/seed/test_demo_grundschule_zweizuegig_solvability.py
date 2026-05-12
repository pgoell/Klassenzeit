"""End-to-end solvability check for demo_grundschule_zweizuegig.

Two flows: (a) the canonical pin-and-solve test
(`_solves_with_zero_violations`) seeds the fixture, generates lessons,
applies the `_TEACHER_ASSIGNMENTS_ZWEIZUEGIG` UPDATE to pin teacher
allocations matching the Rust bench fixture, then schedules each class
and asserts the union of placements totals 196.

(b) the production-route mirror (`_solves_without_pinned_teachers`)
seeds the fixture, generates lessons, then schedules each class WITHOUT
the canonical UPDATE so the solver picks teachers per ADR 0036 from each
Lesson's `teacher_candidates` list. `Lesson.teacher_id` is pin-only
since item 63, so `POST /generate-lessons` produces unpinned Lessons by
default.

196 is the source of truth shared with the Rust bench fixture in
solver/solver-core/benches/solver_fixtures.rs::zweizuegig_fixture; drift
in either side breaks one test.
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
from klassenzeit_backend.seed.demo_grundschule_zweizuegig import (
    _TEACHER_ASSIGNMENTS_ZWEIZUEGIG,
    seed_demo_grundschule_zweizuegig,
)

CreateUserFnZw = Callable[..., Awaitable[tuple[User, str]]]
LoginFnZw = Callable[[str, str], Awaitable[None]]

EXPECTED_PLACEMENTS_ZWEIZUEGIG = 196


async def test_seeded_grundschule_zweizuegig_solves_with_zero_violations(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnZw,
    login_as: LoginFnZw,
) -> None:
    await seed_demo_grundschule_zweizuegig(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-zw-seedtest@example.com",
        password="seed-zw-test-password-12345",  # noqa: S106
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
        "2a",
        "2b",
        "3a",
        "3b",
        "4a",
        "4b",
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
    # auto-assign chose, so the bench has stable placement counts
    # regardless of how auto_assign_teachers_for_lessons evolves).
    for (class_name, subject_short), teacher_short in _TEACHER_ASSIGNMENTS_ZWEIZUEGIG.items():
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

    total_placements = 0
    for school_class in class_rows:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])
        assert len(body["placements"]) > 0, school_class.name
        total_placements += len(body["placements"])

    assert total_placements == EXPECTED_PLACEMENTS_ZWEIZUEGIG, (
        f"expected {EXPECTED_PLACEMENTS_ZWEIZUEGIG} placements across all "
        f"classes, got {total_placements}"
    )


async def test_seeded_grundschule_zweizuegig_solves_without_pinned_teachers(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnZw,
    login_as: LoginFnZw,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Production-route mirror: skips the canonical
    _TEACHER_ASSIGNMENTS_ZWEIZUEGIG UPDATE so the test exercises the
    solver-driven teacher pick path (per ADR 0036) end-to-end.

    Item 32: a feasibility regression that only manifests on unpinned
    teachers would slip through the canonical-pin sibling.
    """
    monkeypatch.setattr(app.state.settings, "solve_deadline_ms", 5000)
    await seed_demo_grundschule_zweizuegig(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-zw-unpinned-seedtest@example.com",
        password="seed-zw-unpinned-test-password-12345",  # noqa: S106
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
