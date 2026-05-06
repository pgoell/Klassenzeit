"""End-to-end solvability check for demo_grundschule_zweizuegig.

The flow: seed -> per-class POST /api/classes/{id}/generate-lessons ->
overwrite Lesson.teacher_id from _TEACHER_ASSIGNMENTS_ZWEIZUEGIG (so the
teacher allocation stays stable as auto-assign evolves) -> per-class
POST /api/classes/{id}/schedule -> assert each class produces violations
== [] and the union of placements totals 196.

196 is the source of truth shared with the Rust bench fixture in
solver/solver-core/benches/solver_fixtures.rs::zweizuegig_fixture; drift
in either side breaks one test.

Mirrors the HTTP-route pattern from test_demo_grundschule_solvability.py
(no standalone ``generate_lessons_for_class`` helper exists; lesson
generation is a route handler that auto-commits).
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


@pytest.mark.xfail(
    strict=False,
    reason=(
        "auto_assign_teachers_for_lessons distribution intermittently "
        "produces a teacher allocation that the production solver path "
        "(lahc_rr_kempe at 5000 ms) cannot recover from on class 3a; "
        "observed kinds rotate between no_suitable_room and "
        "no_free_time_block (~3/5 fail rate in isolation). Tracked under "
        "OPEN_THINGS item 4 (subject UUID order leak in scarcity-first "
        "auto-assign tiebreak)."
    ),
)
async def test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnZw,
    login_as: LoginFnZw,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Production-route mirror: skips the canonical _TEACHER_ASSIGNMENTS_ZWEIZUEGIG
    UPDATE so the test exercises whatever teacher distribution
    auto_assign_teachers_for_lessons produces inside the generate-lessons route.

    Item 32: a feasibility regression that only manifests on the
    auto-assign distribution would slip through the canonical-pin sibling.
    """
    monkeypatch.setattr(app.state.settings, "solve_deadline_ms", 5000)
    await seed_demo_grundschule_zweizuegig(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-zw-autoassign-seedtest@example.com",
        password="seed-zw-autoassign-test-password-12345",  # noqa: S106
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
