"""End-to-end feasibility: seed + generate lessons + solve, no pinning.

Drives the production HTTP-route flow through the test client without any
per-Lesson teacher pinning, so the solver picks teachers per ADR 0036 from
each Lesson's `teacher_candidates` list. The per-test db_session is shared
via the existing dependency override, so the route handlers' commits are
nested savepoint restarts, rolled back at test teardown.

`Lesson.teacher_id` is pin-only since item 63; `POST /generate-lessons`
no longer pre-assigns a teacher, and the deleted
`auto_assign_teachers_for_lessons` function (item 69) is therefore not in
the call chain.
"""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.main import app
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule

CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
LoginFn = Callable[[str, str], Awaitable[None]]


async def test_seeded_grundschule_solves_without_pinned_teachers(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # Production deadline: 5000 ms LAHC budget per ADR 0033. Test env
    # default is KZ_SOLVE_DEADLINE_MS=0 (greedy-only); this opts back in
    # so the test exercises the production solver path on the
    # solver-driven teacher pick.
    monkeypatch.setattr(app.state.settings, "solve_deadline_ms", 5000)
    await seed_demo_grundschule(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-seedtest@example.com",
        password="seed-test-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    class_rows = (
        (await db_session.execute(select(SchoolClass).order_by(SchoolClass.grade_level)))
        .scalars()
        .all()
    )
    assert [c.name for c in class_rows] == ["1a", "2a", "3a", "4a"]

    for school_class in class_rows:
        gen_resp = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
        assert gen_resp.status_code == 201, gen_resp.text
        lessons = gen_resp.json()
        assert len(lessons) in (8, 9), (school_class.name, len(lessons))

    for school_class in class_rows:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])
        assert len(body["placements"]) > 0, school_class.name
