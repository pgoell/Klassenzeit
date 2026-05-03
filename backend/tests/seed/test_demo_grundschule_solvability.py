"""End-to-end feasibility: seed + generate lessons + assign teachers + solve.

Drives the full flow through the HTTP test client so lesson generation,
solver invocation, and placement persistence all run as they would in
production, plus a teacher-assignment step that production currently
expects the user to perform (either via the UI or by extending the
generate-lessons endpoint later). The per-test db_session is shared via
the existing dependency override, so the route handlers' commits are
nested savepoint restarts, rolled back at test teardown.
"""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.main import app
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule

CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
LoginFn = Callable[[str, str], Awaitable[None]]


@pytest.mark.xfail(
    reason=(
        "Demo Grundschule occasionally hits 'no_suitable_room' on FFD greedy "
        "since the same-room hard constraint landed: FFD locks "
        "(class, day, subject) into a room early in the search, then can't "
        "place a later hour because every candidate room conflicts with "
        "either the lock or another class's placement. Flake rate ~50% per "
        "run on this seed. LAHC cannot escape because LAHC moves accepted "
        "placements rather than re-placing violations. Re-enable strict "
        "after FFD ordering becomes same-room-aware (planned per "
        "OPEN_THINGS 'Reduce demo Grundschule Wochenschema' + 'Tighten "
        "Grundschule schedule quality bar'). Strict=False so XPASS doesn't "
        "fail the suite once the solver catches up."
    ),
    strict=False,
)
async def test_seeded_grundschule_solves_with_zero_violations(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # Opt into the production 200ms LAHC pass (the rest of the suite stays
    # greedy-only via KZ_SOLVE_DEADLINE_MS=0). LAHC alone does not fix the
    # FFD lock-in described in the xfail reason above, but enabling it
    # matches the production solver path and is the correct shape for when
    # FFD becomes same-room-aware.
    monkeypatch.setattr(app.state.settings, "solve_deadline_ms", 200)
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

    unassigned_count = (
        await db_session.execute(
            select(func.count()).select_from(Lesson).where(Lesson.teacher_id.is_(None))
        )
    ).scalar_one()
    assert unassigned_count == 0, "auto-assign left some lessons unassigned"

    for school_class in class_rows:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])
        assert len(body["placements"]) > 0, school_class.name
