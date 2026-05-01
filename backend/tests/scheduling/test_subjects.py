"""Integration tests for the Subject CRUD routes."""

import uuid
from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient

from klassenzeit_backend.db.models.user import User

pytestmark = pytest.mark.anyio

# Type aliases matching the factory fixtures defined in conftest.py
type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]


async def test_create_subject(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects creates a new subject and returns 201 with the full body.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    response = await client.post(
        "/api/subjects", json={"name": "Mathematik", "short_name": "Ma", "color": "chart-1"}
    )
    assert response.status_code == 201
    body = response.json()
    assert body["name"] == "Mathematik"
    assert body["short_name"] == "Ma"
    assert "id" in body


async def test_create_subject_duplicate_name(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects with a duplicate name returns 409 Conflict.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin2@test.com", role="admin")
    await login_as("admin2@test.com", "testpassword123")
    await client.post(
        "/api/subjects", json={"name": "Deutsch", "short_name": "De", "color": "chart-2"}
    )
    response = await client.post(
        "/api/subjects", json={"name": "Deutsch", "short_name": "D2", "color": "chart-3"}
    )
    assert response.status_code == 409


async def test_list_subjects(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /subjects returns all subjects.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin3@test.com", role="admin")
    await login_as("admin3@test.com", "testpassword123")
    await client.post(
        "/api/subjects", json={"name": "Englisch", "short_name": "En", "color": "chart-4"}
    )
    await client.post(
        "/api/subjects", json={"name": "Biologie", "short_name": "Bi", "color": "chart-5"}
    )
    response = await client.get("/api/subjects")
    assert response.status_code == 200
    body = response.json()
    assert len(body) >= 2
    names = [s["name"] for s in body]
    assert "Englisch" in names
    assert "Biologie" in names


async def test_get_subject(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /subjects/{id} returns a single subject by ID.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin4@test.com", role="admin")
    await login_as("admin4@test.com", "testpassword123")
    create_resp = await client.post(
        "/api/subjects", json={"name": "Chemie", "short_name": "Ch", "color": "chart-6"}
    )
    subject_id = create_resp.json()["id"]
    response = await client.get(f"/api/subjects/{subject_id}")
    assert response.status_code == 200
    body = response.json()
    assert body["id"] == subject_id
    assert body["name"] == "Chemie"


async def test_get_subject_not_found(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /subjects/{id} returns 404 for an unknown UUID.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin5@test.com", role="admin")
    await login_as("admin5@test.com", "testpassword123")
    response = await client.get(f"/api/subjects/{uuid.uuid4()}")
    assert response.status_code == 404


async def test_update_subject(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /subjects/{id} updates only the fields provided; others remain unchanged.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin6@test.com", role="admin")
    await login_as("admin6@test.com", "testpassword123")
    create_resp = await client.post(
        "/api/subjects", json={"name": "Physik", "short_name": "Ph", "color": "chart-7"}
    )
    subject_id = create_resp.json()["id"]
    response = await client.patch(f"/api/subjects/{subject_id}", json={"name": "Physik neu"})
    assert response.status_code == 200
    body = response.json()
    assert body["name"] == "Physik neu"
    assert body["short_name"] == "Ph"


async def test_delete_subject(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """DELETE /subjects/{id} removes the subject; subsequent GET returns 404.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin7@test.com", role="admin")
    await login_as("admin7@test.com", "testpassword123")
    create_resp = await client.post(
        "/api/subjects", json={"name": "Kunst", "short_name": "Ku", "color": "chart-8"}
    )
    subject_id = create_resp.json()["id"]
    delete_resp = await client.delete(f"/api/subjects/{subject_id}")
    assert delete_resp.status_code == 204
    get_resp = await client.get(f"/api/subjects/{subject_id}")
    assert get_resp.status_code == 404


async def test_delete_subject_not_found(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """DELETE /subjects/{id} returns 404 for an unknown UUID.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin8@test.com", role="admin")
    await login_as("admin8@test.com", "testpassword123")
    response = await client.delete(f"/api/subjects/{uuid.uuid4()}")
    assert response.status_code == 404


async def test_delete_subject_referenced_by_lesson(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """DELETE /subjects/{id} returns 409 when a lesson references it.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    subj_resp = await client.post(
        "/api/subjects", json={"name": "Mathematik", "short_name": "Ma", "color": "chart-9"}
    )
    subject_id = subj_resp.json()["id"]
    scheme_resp = await client.post("/api/week-schemes", json={"name": "Test Scheme"})
    tafel_resp = await client.post(
        "/api/stundentafeln", json={"name": "Test Tafel", "grade_level": 5}
    )
    class_resp = await client.post(
        "/api/classes",
        json={
            "name": "5a",
            "grade_level": 5,
            "stundentafel_id": tafel_resp.json()["id"],
            "week_scheme_id": scheme_resp.json()["id"],
        },
    )
    await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_resp.json()["id"]],
            "subject_id": subject_id,
            "hours_per_week": 4,
        },
    )
    response = await client.delete(f"/api/subjects/{subject_id}")
    assert response.status_code == 409


async def test_subject_requires_admin(client: AsyncClient) -> None:
    """GET /subjects without authentication returns 401 Unauthorized.

    Args:
        client: The async test HTTP client (no session cookie set).
    """
    response = await client.get("/api/subjects")
    assert response.status_code == 401


async def test_subject_create_accepts_preference_flags(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects with preference flags stores and returns them."""
    await create_test_user(email="admin@pref1.com", role="admin")
    await login_as("admin@pref1.com", "testpassword123")
    res = await client.post(
        "/api/subjects",
        json={
            "name": "Test prefer early",
            "short_name": "PE",
            "color": "chart-1",
            "prefer_early_period": 1,
            "avoid_first_period": 0,
        },
    )
    assert res.status_code == 201
    body = res.json()
    assert body["prefer_early_period"] == 1
    assert body["avoid_first_period"] == 0


async def test_subject_create_defaults_preference_flags_to_false(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects without preference flags defaults all three to zero."""
    await create_test_user(email="admin@pref2.com", role="admin")
    await login_as("admin@pref2.com", "testpassword123")
    res = await client.post(
        "/api/subjects",
        json={"name": "Test default", "short_name": "TD", "color": "chart-1"},
    )
    assert res.status_code == 201
    body = res.json()
    assert body["prefer_early_period"] == 0
    assert body["avoid_first_period"] == 0
    assert body["avoid_last_period"] == 0


async def test_subject_update_patches_preference_flags(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /subjects/{id} can set avoid_first_period without touching prefer_early_period."""
    await create_test_user(email="admin@pref3.com", role="admin")
    await login_as("admin@pref3.com", "testpassword123")
    res = await client.post(
        "/api/subjects",
        json={"name": "Test update", "short_name": "TU", "color": "chart-1"},
    )
    subject_id = res.json()["id"]

    res = await client.patch(
        f"/api/subjects/{subject_id}",
        json={"avoid_first_period": 1},
    )
    assert res.status_code == 200
    assert res.json()["avoid_first_period"] == 1
    # prefer_early stays untouched.
    assert res.json()["prefer_early_period"] == 0


async def test_subject_update_can_toggle_avoid_last_period_without_touching_other_flags(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /subjects/{id} sets avoid_last_period alone, leaving the other two flags."""
    await create_test_user(email="admin@pref4.com", role="admin")
    await login_as("admin@pref4.com", "testpassword123")
    res = await client.post(
        "/api/subjects",
        json={"name": "Test avoid last", "short_name": "TL", "color": "chart-1"},
    )
    subject_id = res.json()["id"]

    res = await client.patch(
        f"/api/subjects/{subject_id}",
        json={"avoid_last_period": 1},
    )
    assert res.status_code == 200
    body = res.json()
    assert body["avoid_last_period"] == 1
    # other flags stay untouched.
    assert body["prefer_early_period"] == 0
    assert body["avoid_first_period"] == 0


async def test_create_subject_requires_color(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects without color returns 422 Unprocessable Entity."""
    await create_test_user(email="admin@color1.com", role="admin")
    await login_as("admin@color1.com", "testpassword123")
    response = await client.post("/api/subjects", json={"name": "NoColor", "short_name": "NC"})
    assert response.status_code == 422


async def test_create_subject_rejects_invalid_color(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects with malformed color returns 422."""
    await create_test_user(email="admin@color2.com", role="admin")
    await login_as("admin@color2.com", "testpassword123")
    response = await client.post(
        "/api/subjects", json={"name": "Bad", "short_name": "BD", "color": "not-a-color"}
    )
    assert response.status_code == 422


async def test_patch_subject_color(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /subjects/{id} can update color alone."""
    await create_test_user(email="admin@color3.com", role="admin")
    await login_as("admin@color3.com", "testpassword123")
    create = await client.post(
        "/api/subjects", json={"name": "Color Me", "short_name": "CM", "color": "chart-3"}
    )
    subject_id = create.json()["id"]
    response = await client.patch(f"/api/subjects/{subject_id}", json={"color": "#112233"})
    assert response.status_code == 200
    assert response.json()["color"] == "#112233"


async def test_create_subject_rejects_out_of_range_prefer_early_period(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects with prefer_early_period > 10 returns 422."""
    await create_test_user(email="admin@range1.com", role="admin")
    await login_as("admin@range1.com", "testpassword123")
    response = await client.post(
        "/api/subjects",
        json={
            "name": "Range Math",
            "short_name": "RM",
            "color": "chart-1",
            "prefer_early_period": 11,
            "avoid_first_period": 0,
            "avoid_last_period": 0,
        },
    )
    assert response.status_code == 422
    detail = response.json()["detail"]
    assert any(loc[-1] == "prefer_early_period" for loc in (e.get("loc", []) for e in detail))


async def test_create_subject_rejects_out_of_range_avoid_first_period(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects with avoid_first_period > 10 returns 422."""
    await create_test_user(email="admin@range2.com", role="admin")
    await login_as("admin@range2.com", "testpassword123")
    response = await client.post(
        "/api/subjects",
        json={
            "name": "Range Sport",
            "short_name": "RS",
            "color": "chart-1",
            "prefer_early_period": 0,
            "avoid_first_period": 11,
            "avoid_last_period": 0,
        },
    )
    assert response.status_code == 422


async def test_create_subject_rejects_out_of_range_avoid_last_period(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects with avoid_last_period > 10 returns 422."""
    await create_test_user(email="admin@range3.com", role="admin")
    await login_as("admin@range3.com", "testpassword123")
    response = await client.post(
        "/api/subjects",
        json={
            "name": "Range Deutsch",
            "short_name": "RD",
            "color": "chart-1",
            "prefer_early_period": 0,
            "avoid_first_period": 0,
            "avoid_last_period": 11,
        },
    )
    assert response.status_code == 422


async def test_create_subject_accepts_non_binary_weight(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /subjects with non-binary weight values round-trips through the response."""
    await create_test_user(email="admin@weight.com", role="admin")
    await login_as("admin@weight.com", "testpassword123")
    response = await client.post(
        "/api/subjects",
        json={
            "name": "Weighted Mathematik",
            "short_name": "WM",
            "color": "chart-1",
            "prefer_early_period": 3,
            "avoid_first_period": 0,
            "avoid_last_period": 2,
        },
    )
    assert response.status_code == 201
    body = response.json()
    assert body["prefer_early_period"] == 3
    assert body["avoid_last_period"] == 2
