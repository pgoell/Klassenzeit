"""Integration tests for POST /schedule supervision_assignments persistence.

Drives the production HTTP route end-to-end against the einzügig
Grundschule seed, asserts the response carries one entry per break-kind
TimeBlock (5 days x 2 Hofpausen = 10 rows), and verifies the
delete-and-rewrite contract: a second POST does not append duplicates.
"""

from collections.abc import Awaitable, Callable

from httpx import AsyncClient
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.supervision_assignment import SupervisionAssignment
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule

CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
LoginFn = Callable[[str, str], Awaitable[None]]


async def test_post_schedule_returns_supervision_assignments(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /schedule response body carries one supervision row per break TimeBlock.

    The einzügig seed places 2 Hofpausen on each of 5 weekdays. Every break
    slot has at least one eligible supervisor in the seed, so the solver's
    supervision pass emits 10 assignments with no SupervisionGap rows.
    """
    await seed_demo_grundschule(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-supervision@example.com",
        password="supervision-test-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    cls = (
        await db_session.execute(select(SchoolClass).where(SchoolClass.name == "1a"))
    ).scalar_one()

    gen_resp = await client.post(f"/api/classes/{cls.id}/generate-lessons")
    assert gen_resp.status_code == 201, gen_resp.text

    sched_resp = await client.post(f"/api/classes/{cls.id}/schedule")
    assert sched_resp.status_code == 200, sched_resp.text
    body = sched_resp.json()

    assert "supervision_assignments" in body
    assignments = body["supervision_assignments"]
    assert len(assignments) == 10, assignments
    for entry in assignments:
        assert isinstance(entry["time_block_id"], str)
        assert isinstance(entry["teacher_id"], str)


async def test_resolve_overwrites_existing_supervision_assignments(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """A second POST replaces rather than appends supervision rows.

    Mirrors the ScheduledLesson clear-and-rewrite contract: the persist
    helper deletes every supervision_assignment scoped to the affected
    WeekScheme before inserting the new solver output, so the row count
    stays at 10 regardless of how many times the route is invoked.
    """
    await seed_demo_grundschule(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-supervision-twice@example.com",
        password="supervision-test-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    cls = (
        await db_session.execute(select(SchoolClass).where(SchoolClass.name == "1a"))
    ).scalar_one()

    gen_resp = await client.post(f"/api/classes/{cls.id}/generate-lessons")
    assert gen_resp.status_code == 201, gen_resp.text

    first = await client.post(f"/api/classes/{cls.id}/schedule")
    assert first.status_code == 200, first.text
    count_after_first = (
        await db_session.execute(select(func.count()).select_from(SupervisionAssignment))
    ).scalar_one()
    assert count_after_first == 10

    second = await client.post(f"/api/classes/{cls.id}/schedule")
    assert second.status_code == 200, second.text
    count_after_second = (
        await db_session.execute(select(func.count()).select_from(SupervisionAssignment))
    ).scalar_one()
    assert count_after_second == 10
