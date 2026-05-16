"""Cross-school isolation tests for the Room aggregate."""

import uuid

import pytest
from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School

pytestmark = pytest.mark.anyio


@pytest.fixture
async def school_b(db_session: AsyncSession) -> School:
    """A second school distinct from DEFAULT_SCHOOL_ID."""
    school = School(name="Schule B", short_name="SB")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_get_rooms_is_school_scoped(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b: School,
    create_test_user,
    login_as,
) -> None:
    """GET /rooms returns only the requesting user's school's rooms."""
    user_a, password = await create_test_user(
        email="admin-tenancy-a@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )

    room_in_a = Room(name="A Room", short_name="A1", school_id=DEFAULT_SCHOOL_ID)
    room_in_b = Room(name="B Room", short_name="B1", school_id=school_b.id)
    db_session.add_all([room_in_a, room_in_b])
    await db_session.flush()

    await login_as(user_a.email, password)
    response = await client.get("/api/rooms")
    assert response.status_code == 200
    body = response.json()
    names = {row["name"] for row in body}
    assert "A Room" in names
    assert "B Room" not in names


async def test_get_room_detail_returns_404_for_cross_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b: School,
    create_test_user,
    login_as,
) -> None:
    """GET /rooms/{id} where the room is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-detail@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    room_in_b = Room(name="B Room 2", short_name="B2", school_id=school_b.id)
    db_session.add(room_in_b)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.get(f"/api/rooms/{room_in_b.id}")
    assert response.status_code == 404


async def test_patch_room_returns_404_for_cross_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b: School,
    create_test_user,
    login_as,
) -> None:
    """PATCH /rooms/{id} where the room is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-patch@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    room_in_b = Room(name="B Room 3", short_name="B3", school_id=school_b.id)
    db_session.add(room_in_b)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.patch(f"/api/rooms/{room_in_b.id}", json={"name": "Sneaky rename"})
    assert response.status_code == 404


async def test_delete_room_returns_404_for_cross_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b: School,
    create_test_user,
    login_as,
) -> None:
    """DELETE /rooms/{id} where the room is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-delete@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    room_in_b = Room(name="B Room 4", short_name="B4", school_id=school_b.id)
    db_session.add(room_in_b)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.delete(f"/api/rooms/{room_in_b.id}")
    assert response.status_code == 404


async def test_post_room_stamps_users_school_id(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    """POST /rooms creates a Room whose school_id equals current_user.school_id."""
    user, password = await create_test_user(
        email="admin-post@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    await login_as(user.email, password)
    response = await client.post(
        "/api/rooms", json={"name": "Fresh Tenanted Room", "short_name": "FTR"}
    )
    assert response.status_code == 201
    body = response.json()
    new_room = await db_session.get(Room, uuid.UUID(body["id"]))
    assert new_room is not None
    assert new_room.school_id == user.school_id == DEFAULT_SCHOOL_ID


async def test_put_suitability_returns_404_for_cross_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b: School,
    create_test_user,
    login_as,
) -> None:
    """PUT /rooms/{id}/suitability where the room belongs to another school returns 404."""
    user, password = await create_test_user(
        email="admin-suit@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    room_in_b = Room(name="B Room 5", short_name="B5", school_id=school_b.id)
    db_session.add(room_in_b)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.put(
        f"/api/rooms/{room_in_b.id}/suitability",
        json={"subject_ids": []},
    )
    assert response.status_code == 404


async def test_put_availability_returns_404_for_cross_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b: School,
    create_test_user,
    login_as,
) -> None:
    """PUT /rooms/{id}/availability where the room belongs to another school returns 404."""
    user, password = await create_test_user(
        email="admin-avail@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    room_in_b = Room(name="B Room 6", short_name="B6", school_id=school_b.id)
    db_session.add(room_in_b)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.put(
        f"/api/rooms/{room_in_b.id}/availability",
        json={"time_block_ids": []},
    )
    assert response.status_code == 404
