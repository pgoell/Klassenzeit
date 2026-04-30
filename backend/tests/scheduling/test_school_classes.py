"""Integration tests for the SchoolClass CRUD routes."""

import uuid
from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient

from klassenzeit_backend.db.models.user import User

pytestmark = pytest.mark.anyio

# Type aliases matching the factory fixtures defined in conftest.py
type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]


async def _setup_stundentafel_for_classes(
    client: AsyncClient, name: str, grade_level: int = 5
) -> str:
    """Create a Stundentafel via the API and return its ID.

    Args:
        client: The async test HTTP client (must already be authenticated).
        name: Unique name for the Stundentafel.
        grade_level: Grade level to assign (default 5).

    Returns:
        The UUID string of the created Stundentafel.
    """
    resp = await client.post(
        "/api/stundentafeln",
        json={"name": name, "grade_level": grade_level},
    )
    assert resp.status_code == 201
    return resp.json()["id"]


async def _setup_week_scheme_for_classes(client: AsyncClient, name: str) -> str:
    """Create a WeekScheme via the API and return its ID.

    Args:
        client: The async test HTTP client (must already be authenticated).
        name: Unique name for the WeekScheme.

    Returns:
        The UUID string of the created WeekScheme.
    """
    resp = await client.post("/api/week-schemes", json={"name": name})
    assert resp.status_code == 201
    return resp.json()["id"]


async def test_create_school_class(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /classes creates a new school class and returns 201 with the full body.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@sc1.com", role="admin")
    await login_as("admin@sc1.com", "testpassword123")
    tafel_id = await _setup_stundentafel_for_classes(client, "Tafel SC1", 5)
    scheme_id = await _setup_week_scheme_for_classes(client, "Scheme SC1")
    response = await client.post(
        "/api/classes",
        json={
            "name": "5a",
            "grade_level": 5,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
        },
    )
    assert response.status_code == 201
    body = response.json()
    assert body["name"] == "5a"
    assert body["grade_level"] == 5
    assert body["stundentafel_id"] == tafel_id
    assert body["week_scheme_id"] == scheme_id
    assert "id" in body
    assert "created_at" in body
    assert "updated_at" in body


async def test_create_school_class_duplicate_name(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /classes with a duplicate name returns 409 Conflict.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@sc2.com", role="admin")
    await login_as("admin@sc2.com", "testpassword123")
    tafel_id = await _setup_stundentafel_for_classes(client, "Tafel SC2", 6)
    scheme_id = await _setup_week_scheme_for_classes(client, "Scheme SC2")
    await client.post(
        "/api/classes",
        json={
            "name": "6b",
            "grade_level": 6,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
        },
    )
    response = await client.post(
        "/api/classes",
        json={
            "name": "6b",
            "grade_level": 6,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
        },
    )
    assert response.status_code == 409


async def test_list_school_classes(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /classes returns all school classes.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@sc3.com", role="admin")
    await login_as("admin@sc3.com", "testpassword123")
    tafel_id = await _setup_stundentafel_for_classes(client, "Tafel SC3", 7)
    scheme_id = await _setup_week_scheme_for_classes(client, "Scheme SC3")
    await client.post(
        "/api/classes",
        json={
            "name": "7a",
            "grade_level": 7,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
        },
    )
    await client.post(
        "/api/classes",
        json={
            "name": "7b",
            "grade_level": 7,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
        },
    )
    response = await client.get("/api/classes")
    assert response.status_code == 200
    body = response.json()
    assert len(body) >= 2
    names = [sc["name"] for sc in body]
    assert "7a" in names
    assert "7b" in names


async def test_get_school_class(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /classes/{id} returns a single school class by ID.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@sc4.com", role="admin")
    await login_as("admin@sc4.com", "testpassword123")
    tafel_id = await _setup_stundentafel_for_classes(client, "Tafel SC4", 8)
    scheme_id = await _setup_week_scheme_for_classes(client, "Scheme SC4")
    create_resp = await client.post(
        "/api/classes",
        json={
            "name": "8c",
            "grade_level": 8,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
        },
    )
    class_id = create_resp.json()["id"]
    response = await client.get(f"/api/classes/{class_id}")
    assert response.status_code == 200
    body = response.json()
    assert body["id"] == class_id
    assert body["name"] == "8c"
    assert body["grade_level"] == 8


async def test_update_school_class(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /classes/{id} updates only the fields provided; others remain unchanged.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@sc5.com", role="admin")
    await login_as("admin@sc5.com", "testpassword123")
    tafel_id = await _setup_stundentafel_for_classes(client, "Tafel SC5", 9)
    scheme_id = await _setup_week_scheme_for_classes(client, "Scheme SC5")
    create_resp = await client.post(
        "/api/classes",
        json={
            "name": "9a",
            "grade_level": 9,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
        },
    )
    class_id = create_resp.json()["id"]
    response = await client.patch(f"/api/classes/{class_id}", json={"name": "9a-updated"})
    assert response.status_code == 200
    body = response.json()
    assert body["name"] == "9a-updated"
    assert body["grade_level"] == 9
    assert body["stundentafel_id"] == tafel_id
    assert body["week_scheme_id"] == scheme_id


async def test_delete_school_class(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """DELETE /classes/{id} removes the school class; subsequent GET returns 404.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@sc6.com", role="admin")
    await login_as("admin@sc6.com", "testpassword123")
    tafel_id = await _setup_stundentafel_for_classes(client, "Tafel SC6", 10)
    scheme_id = await _setup_week_scheme_for_classes(client, "Scheme SC6")
    create_resp = await client.post(
        "/api/classes",
        json={
            "name": "10a",
            "grade_level": 10,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
        },
    )
    class_id = create_resp.json()["id"]
    delete_resp = await client.delete(f"/api/classes/{class_id}")
    assert delete_resp.status_code == 204
    get_resp = await client.get(f"/api/classes/{class_id}")
    assert get_resp.status_code == 404


async def test_school_class_requires_admin(client: AsyncClient) -> None:
    """GET /classes without authentication returns 401 Unauthorized.

    Args:
        client: The async test HTTP client (no session cookie set).
    """
    response = await client.get("/api/classes")
    assert response.status_code == 401


async def test_get_school_class_not_found(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /classes/{id} returns 404 for an unknown UUID.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@sc7.com", role="admin")
    await login_as("admin@sc7.com", "testpassword123")
    response = await client.get(f"/api/classes/{uuid.uuid4()}")
    assert response.status_code == 404


async def test_create_school_class_with_home_room_id(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /classes with home_room_id stores the FK and echoes it on the response."""
    await create_test_user(email="admin@sc-hr1.com", role="admin")
    await login_as("admin@sc-hr1.com", "testpassword123")
    tafel_id = await _setup_stundentafel_for_classes(client, "Tafel SC-HR1", 1)
    scheme_id = await _setup_week_scheme_for_classes(client, "Scheme SC-HR1")
    room_response = await client.post(
        "/api/rooms",
        json={"name": "Klasse 1a SC-HR1", "short_name": "1a-HR1"},
    )
    assert room_response.status_code == 201
    room_id = room_response.json()["id"]
    response = await client.post(
        "/api/classes",
        json={
            "name": "1a-HR1",
            "grade_level": 1,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
            "home_room_id": room_id,
        },
    )
    assert response.status_code == 201
    body = response.json()
    assert body["home_room_id"] == room_id


async def test_create_school_class_accepts_null_home_room_id(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /classes accepts null home_room_id and stores None."""
    await create_test_user(email="admin@sc-hr2.com", role="admin")
    await login_as("admin@sc-hr2.com", "testpassword123")
    tafel_id = await _setup_stundentafel_for_classes(client, "Tafel SC-HR2", 2)
    scheme_id = await _setup_week_scheme_for_classes(client, "Scheme SC-HR2")
    response = await client.post(
        "/api/classes",
        json={
            "name": "2a-HR2",
            "grade_level": 2,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
            "home_room_id": None,
        },
    )
    assert response.status_code == 201
    assert response.json()["home_room_id"] is None


async def test_update_school_class_clears_home_room_id_when_patched_null(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /classes/{id} with explicit null home_room_id clears the FK."""
    await create_test_user(email="admin@sc-hr3.com", role="admin")
    await login_as("admin@sc-hr3.com", "testpassword123")
    tafel_id = await _setup_stundentafel_for_classes(client, "Tafel SC-HR3", 3)
    scheme_id = await _setup_week_scheme_for_classes(client, "Scheme SC-HR3")
    room_response = await client.post(
        "/api/rooms",
        json={"name": "Klasse 3a SC-HR3", "short_name": "3a-HR3"},
    )
    room_id = room_response.json()["id"]
    create_response = await client.post(
        "/api/classes",
        json={
            "name": "3a-HR3",
            "grade_level": 3,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_id,
            "home_room_id": room_id,
        },
    )
    class_id = create_response.json()["id"]
    patch_response = await client.patch(
        f"/api/classes/{class_id}",
        json={"home_room_id": None},
    )
    assert patch_response.status_code == 200
    assert patch_response.json()["home_room_id"] is None
