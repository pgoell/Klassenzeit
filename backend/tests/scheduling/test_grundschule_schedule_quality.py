"""Integration test: demo Grundschule schedule must clear the quality bar.

Seeds the demo Grundschule, drives lesson generation and the per-class
solve through the production HTTP routes, then calls the production
`compute_quality_issues` orchestrator per class and asserts it returns
no issues for any class.

This guards against future solver / weight / seed changes producing
visually bad schedules without a hard-violation gate to catch them.
The test opts into the production 60000 ms ``lahc_rr`` pass (matching
the production-default backend deadline per ADR 0038) because the soft
costs the new constraints rely on are LAHC-driven; greedy alone
produces a lopsided baseline that cannot pass the bar. The full
production budget is required to clear ``interior_gap`` reliably on
einzuegig (item 87 closure: ADR 0049).
"""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.main import app
from klassenzeit_backend.scheduling.quality_checks import compute_quality_issues
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule

CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
LoginFn = Callable[[str, str], Awaitable[None]]


async def test_grundschule_schedule_meets_quality_bar(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setitem(app.state.settings.solve_deadline_ms_by_backend, "lahc_rr", 60000)
    await seed_demo_grundschule(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-quality@example.com",
        password="quality-test-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    classes = (
        (await db_session.execute(select(SchoolClass).order_by(SchoolClass.grade_level)))
        .scalars()
        .all()
    )
    assert [c.name for c in classes] == ["1a", "2a", "3a", "4a"]

    for school_class in classes:
        gen_resp = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
        assert gen_resp.status_code == 201, gen_resp.text

    for school_class in classes:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])

    for school_class in classes:
        issues = await compute_quality_issues(
            db_session, school_class.id, school_id=DEFAULT_SCHOOL_ID
        )
        assert issues == [], (
            f"demo Grundschule class {school_class.name} failed quality checks:\n"
            + "\n".join(f"  - {issue}" for issue in issues)
        )
