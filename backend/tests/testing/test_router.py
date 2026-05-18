"""Integration tests for the test-only router."""

import uuid

from httpx import AsyncClient
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models import Room as _Room
from klassenzeit_backend.db.models import SchoolClass, Subject, Teacher, User
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School


async def test_health_returns_ok(client: AsyncClient) -> None:
    """GET /__test__/health returns 200 with a simple body."""
    response = await client.get("/__test__/health")
    assert response.status_code == 200
    assert response.json() == {"status": "ok"}


async def test_reset_truncates_entity_tables(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    """POST /__test__/reset wipes subjects (and other entity tables)."""
    # Need an admin user to call /subjects
    await create_test_user(email="admin@reset-test.com", role="admin")
    await login_as("admin@reset-test.com", "testpassword123")

    subject = Subject(name="Temp", short_name="TMP", color="chart-1", school_id=DEFAULT_SCHOOL_ID)
    db_session.add(subject)
    await db_session.flush()

    # Confirm the row is visible through the app.
    pre_resp = await client.get("/api/subjects")
    assert pre_resp.status_code == 200
    assert any(s["name"] == "Temp" for s in pre_resp.json())

    response = await client.post("/__test__/reset")
    assert response.status_code == 204

    # After reset, expire the session cache so we see the actual DB state.
    db_session.expire_all()

    post_resp = await client.get("/api/subjects")
    assert post_resp.status_code == 200
    assert post_resp.json() == []


async def test_reset_preserves_users_and_sessions(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
) -> None:
    """POST /__test__/reset does NOT truncate users or sessions."""
    await create_test_user(email="keep@test.com")
    await db_session.commit()

    response = await client.post("/__test__/reset")
    assert response.status_code == 204

    db_session.expire_all()
    result = await db_session.execute(select(User).where(User.email == "keep@test.com"))
    assert result.scalar_one_or_none() is not None


async def test_seed_grundschule_creates_expected_rows(
    client: AsyncClient,
    db_session: AsyncSession,
) -> None:
    """POST /__test__/seed-grundschule seeds a Hessen Grundschule."""
    response = await client.post("/__test__/seed-grundschule")
    assert response.status_code == 204

    db_session.expire_all()

    class_count = (
        await db_session.execute(select(func.count()).select_from(SchoolClass))
    ).scalar_one()
    teacher_count = (
        await db_session.execute(select(func.count()).select_from(Teacher))
    ).scalar_one()
    room_count = (await db_session.execute(select(func.count()).select_from(_Room))).scalar_one()

    assert class_count == 4
    assert teacher_count == 6
    assert room_count == 7


async def test_seed_school_b_returns_seed_ids(
    client: AsyncClient,
    db_session: AsyncSession,
) -> None:
    """POST /__test__/seed-school-b seeds Schule B and returns row ids."""
    response = await client.post("/__test__/seed-school-b")
    assert response.status_code == 200, response.text
    body = response.json()
    assert "school_b_id" in body
    assert "room_b1_id" in body
    assert "room_b2_id" in body

    db_session.expire_all()

    school_b_id = uuid.UUID(body["school_b_id"])
    result = await db_session.execute(select(School).where(School.id == school_b_id))
    school = result.scalar_one()
    assert school.name == "Schule B"
    assert school.short_name == "SB"

    rooms_result = await db_session.execute(
        select(_Room).where(_Room.school_id == school_b_id).order_by(_Room.name)
    )
    rooms = list(rooms_result.scalars().all())
    assert [r.name for r in rooms] == ["SB Raum 1", "SB Raum 2"]

    admin_b_result = await db_session.execute(
        select(User).where(User.email == "admin-b@example.com")
    )
    admin_b = admin_b_result.scalar_one()
    assert admin_b.role == "admin"
    assert admin_b.school_id == school_b_id

    super_admin_result = await db_session.execute(
        select(User).where(User.email == "super-admin@example.com")
    )
    super_admin = super_admin_result.scalar_one()
    assert super_admin.role == "super_admin"


async def test_seed_school_b_is_idempotent(
    client: AsyncClient,
    db_session: AsyncSession,
) -> None:
    """POST /__test__/seed-school-b twice returns the same ids and does not duplicate."""
    first = await client.post("/__test__/seed-school-b")
    assert first.status_code == 200
    first_body = first.json()

    second = await client.post("/__test__/seed-school-b")
    assert second.status_code == 200
    second_body = second.json()

    assert second_body == first_body

    db_session.expire_all()

    school_count = (
        await db_session.execute(
            select(func.count()).select_from(School).where(School.name == "Schule B")
        )
    ).scalar_one()
    assert school_count == 1

    school_b_id = uuid.UUID(first_body["school_b_id"])
    room_count = (
        await db_session.execute(
            select(func.count()).select_from(_Room).where(_Room.school_id == school_b_id)
        )
    ).scalar_one()
    assert room_count == 2

    admin_b_count = (
        await db_session.execute(
            select(func.count()).select_from(User).where(User.email == "admin-b@example.com")
        )
    ).scalar_one()
    assert admin_b_count == 1
