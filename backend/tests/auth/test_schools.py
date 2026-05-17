"""Tests for school CRUD routes (item 10b)."""

import uuid

from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School


async def _make_school(
    db_session: AsyncSession, *, name: str, short_name: str | None = None
) -> School:
    school = School(name=name, short_name=short_name)
    db_session.add(school)
    await db_session.flush()
    return school


async def test_create_school_admin(client: AsyncClient, create_test_user, login_as) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    response = await client.post(
        "/api/schools",
        json={"name": "Zweite Grundschule", "short_name": "ZWG"},
    )
    assert response.status_code == 201
    body = response.json()
    assert body["name"] == "Zweite Grundschule"
    assert body["short_name"] == "ZWG"
    assert uuid.UUID(body["id"])


async def test_create_school_minimal_no_short_name(
    client: AsyncClient, create_test_user, login_as
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    response = await client.post("/api/schools", json={"name": "Nur Name"})
    assert response.status_code == 201
    assert response.json()["short_name"] is None


async def test_create_school_duplicate_name(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    await _make_school(db_session, name="Doppelte")
    await db_session.commit()
    response = await client.post("/api/schools", json={"name": "Doppelte"})
    assert response.status_code == 409


async def test_create_school_duplicate_short_name(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    await _make_school(db_session, name="Unique 1", short_name="DUP")
    await db_session.commit()
    response = await client.post("/api/schools", json={"name": "Unique 2", "short_name": "DUP"})
    assert response.status_code == 409


async def test_list_schools_admin(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    await _make_school(db_session, name="Beta School")
    await db_session.commit()
    response = await client.get("/api/schools")
    assert response.status_code == 200
    names = [row["name"] for row in response.json()]
    assert "Beta School" in names
    # The seeded default school should also be present.
    assert any(uuid.UUID(row["id"]) == DEFAULT_SCHOOL_ID for row in response.json())


async def test_get_school_admin(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    school = await _make_school(db_session, name="To Fetch", short_name="TF")
    await db_session.commit()
    response = await client.get(f"/api/schools/{school.id}")
    assert response.status_code == 200
    body = response.json()
    assert body["name"] == "To Fetch"
    assert body["short_name"] == "TF"


async def test_get_school_not_found(client: AsyncClient, create_test_user, login_as) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    response = await client.get(f"/api/schools/{uuid.uuid4()}")
    assert response.status_code == 404


async def test_update_school_admin(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    school = await _make_school(db_session, name="Old", short_name="OLD")
    await db_session.commit()
    response = await client.patch(
        f"/api/schools/{school.id}",
        json={"name": "New", "short_name": "NEW"},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["name"] == "New"
    assert body["short_name"] == "NEW"


async def test_update_school_partial(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    school = await _make_school(db_session, name="Stays", short_name="OLD")
    await db_session.commit()
    response = await client.patch(f"/api/schools/{school.id}", json={"short_name": "NEW"})
    assert response.status_code == 200
    body = response.json()
    assert body["name"] == "Stays"
    assert body["short_name"] == "NEW"


async def test_update_school_clear_short_name(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    school = await _make_school(db_session, name="Keeps", short_name="HAD")
    await db_session.commit()
    response = await client.patch(f"/api/schools/{school.id}", json={"short_name": None})
    assert response.status_code == 200
    assert response.json()["short_name"] is None


async def test_update_school_empty_body(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    school = await _make_school(db_session, name="Untouched")
    await db_session.commit()
    response = await client.patch(f"/api/schools/{school.id}", json={})
    assert response.status_code == 422


async def test_update_school_duplicate_name(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    await _make_school(db_session, name="Existing")
    other = await _make_school(db_session, name="Other")
    await db_session.commit()
    response = await client.patch(f"/api/schools/{other.id}", json={"name": "Existing"})
    assert response.status_code == 409


async def test_update_school_not_found(client: AsyncClient, create_test_user, login_as) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    response = await client.patch(f"/api/schools/{uuid.uuid4()}", json={"name": "Anything"})
    assert response.status_code == 404


async def test_update_default_school_rename(
    client: AsyncClient, create_test_user, login_as
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    response = await client.patch(
        f"/api/schools/{DEFAULT_SCHOOL_ID}",
        json={"name": "Renamed Default"},
    )
    assert response.status_code == 200
    assert response.json()["name"] == "Renamed Default"


async def test_delete_school_admin(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    school = await _make_school(db_session, name="To Delete")
    await db_session.commit()
    response = await client.delete(f"/api/schools/{school.id}")
    assert response.status_code == 204


async def test_delete_school_with_users_blocked(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    school = await _make_school(db_session, name="Has Users")
    await db_session.commit()
    await create_test_user(email="admin@test.com", role="admin")
    await create_test_user(email="resident@test.com", role="user", school_id=school.id)
    await login_as("admin@test.com", "testpassword123")
    response = await client.delete(f"/api/schools/{school.id}")
    assert response.status_code == 409
    assert "referenced" in response.json()["detail"].lower()


async def test_delete_school_default_blocked(
    client: AsyncClient, create_test_user, login_as
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    response = await client.delete(f"/api/schools/{DEFAULT_SCHOOL_ID}")
    assert response.status_code == 409
    assert "default" in response.json()["detail"].lower()


async def test_delete_school_not_found(client: AsyncClient, create_test_user, login_as) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    response = await client.delete(f"/api/schools/{uuid.uuid4()}")
    assert response.status_code == 404


async def test_endpoints_require_admin_role(
    client: AsyncClient, create_test_user, login_as, db_session
) -> None:
    school = await _make_school(db_session, name="Gated")
    await db_session.commit()
    await create_test_user(email="user@test.com", role="user")
    await login_as("user@test.com", "testpassword123")

    assert (await client.get("/api/schools")).status_code == 403
    assert (await client.get(f"/api/schools/{school.id}")).status_code == 403
    assert (await client.post("/api/schools", json={"name": "X"})).status_code == 403
    assert (await client.patch(f"/api/schools/{school.id}", json={"name": "Y"})).status_code == 403
    assert (await client.delete(f"/api/schools/{school.id}")).status_code == 403


async def test_endpoints_require_auth(client: AsyncClient) -> None:
    assert (await client.get("/api/schools")).status_code == 401
