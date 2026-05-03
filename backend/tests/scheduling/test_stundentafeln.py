"""Integration tests for the Stundentafel and StundentafelEntry CRUD routes."""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient

from klassenzeit_backend.db.models.user import User

pytestmark = pytest.mark.anyio

# Type aliases matching the factory fixtures defined in conftest.py
type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]


async def test_create_stundentafel(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /stundentafeln creates a new Stundentafel and returns 201 with the full body.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st1.com", role="admin")
    await login_as("admin@st1.com", "testpassword123")
    response = await client.post(
        "/api/stundentafeln",
        json={"name": "Gymnasium Klasse 5", "grade_level": 5},
    )
    assert response.status_code == 201
    body = response.json()
    assert body["name"] == "Gymnasium Klasse 5"
    assert body["grade_level"] == 5
    assert "id" in body
    assert "created_at" in body
    assert "updated_at" in body


async def test_create_stundentafel_duplicate_name(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /stundentafeln with a duplicate name returns 409 Conflict.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st2.com", role="admin")
    await login_as("admin@st2.com", "testpassword123")
    await client.post("/api/stundentafeln", json={"name": "Duplicate Tafel", "grade_level": 6})
    response = await client.post(
        "/api/stundentafeln", json={"name": "Duplicate Tafel", "grade_level": 7}
    )
    assert response.status_code == 409


async def test_list_stundentafeln(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /stundentafeln returns all Stundentafeln without entries.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st3.com", role="admin")
    await login_as("admin@st3.com", "testpassword123")
    await client.post("/api/stundentafeln", json={"name": "Alpha Tafel", "grade_level": 5})
    await client.post("/api/stundentafeln", json={"name": "Beta Tafel", "grade_level": 6})
    response = await client.get("/api/stundentafeln")
    assert response.status_code == 200
    body = response.json()
    assert len(body) >= 2
    names = [t["name"] for t in body]
    assert "Alpha Tafel" in names
    assert "Beta Tafel" in names
    # List response must not contain entries key
    for item in body:
        assert "entries" not in item


async def test_get_stundentafel_with_entries(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /stundentafeln/{id} returns the Stundentafel with nested entries including subject data.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st4.com", role="admin")
    await login_as("admin@st4.com", "testpassword123")
    # Create a subject first
    subj_resp = await client.post(
        "/api/subjects", json={"name": "Mathematik ST4", "short_name": "MaST4", "color": "chart-1"}
    )
    assert subj_resp.status_code == 201
    subject_id = subj_resp.json()["id"]
    # Create Stundentafel
    tafel_resp = await client.post(
        "/api/stundentafeln", json={"name": "Detail Tafel", "grade_level": 5}
    )
    assert tafel_resp.status_code == 201
    tafel_id = tafel_resp.json()["id"]
    # Add entry
    entry_resp = await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 4, "preferred_block_size": 2},
    )
    assert entry_resp.status_code == 201
    # Fetch detail
    response = await client.get(f"/api/stundentafeln/{tafel_id}")
    assert response.status_code == 200
    body = response.json()
    assert body["id"] == tafel_id
    assert body["name"] == "Detail Tafel"
    assert body["grade_level"] == 5
    assert "entries" in body
    assert len(body["entries"]) == 1
    entry = body["entries"][0]
    assert entry["hours_per_week"] == 4
    assert entry["preferred_block_size"] == 2
    assert entry["subject"]["id"] == subject_id
    assert entry["subject"]["name"] == "Mathematik ST4"
    assert entry["subject"]["short_name"] == "MaST4"


async def test_update_stundentafel(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /stundentafeln/{id} updates only the supplied fields.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st5.com", role="admin")
    await login_as("admin@st5.com", "testpassword123")
    create_resp = await client.post(
        "/api/stundentafeln", json={"name": "Old Tafel Name", "grade_level": 5}
    )
    tafel_id = create_resp.json()["id"]
    response = await client.patch(f"/api/stundentafeln/{tafel_id}", json={"name": "New Tafel Name"})
    assert response.status_code == 200
    body = response.json()
    assert body["name"] == "New Tafel Name"
    assert body["grade_level"] == 5


async def test_delete_stundentafel(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """DELETE /stundentafeln/{id} removes the Stundentafel; subsequent GET returns 404.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st6.com", role="admin")
    await login_as("admin@st6.com", "testpassword123")
    create_resp = await client.post(
        "/api/stundentafeln", json={"name": "To Delete Tafel", "grade_level": 8}
    )
    tafel_id = create_resp.json()["id"]
    delete_resp = await client.delete(f"/api/stundentafeln/{tafel_id}")
    assert delete_resp.status_code == 204
    get_resp = await client.get(f"/api/stundentafeln/{tafel_id}")
    assert get_resp.status_code == 404


async def test_create_entry(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /stundentafeln/{id}/entries creates an entry and returns 201 with subject data.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st7.com", role="admin")
    await login_as("admin@st7.com", "testpassword123")
    subj_resp = await client.post(
        "/api/subjects", json={"name": "Deutsch ST7", "short_name": "DeST7", "color": "chart-2"}
    )
    subject_id = subj_resp.json()["id"]
    tafel_resp = await client.post(
        "/api/stundentafeln", json={"name": "Entry Tafel", "grade_level": 6}
    )
    tafel_id = tafel_resp.json()["id"]
    response = await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 5, "preferred_block_size": 1},
    )
    assert response.status_code == 201
    body = response.json()
    assert body["hours_per_week"] == 5
    assert body["preferred_block_size"] == 1
    assert body["subject"]["id"] == subject_id
    assert "id" in body


async def test_create_entry_duplicate_subject(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /stundentafeln/{id}/entries with a duplicate subject returns 409 Conflict.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st8.com", role="admin")
    await login_as("admin@st8.com", "testpassword123")
    subj_resp = await client.post(
        "/api/subjects", json={"name": "Sport ST8", "short_name": "SpST8", "color": "chart-3"}
    )
    subject_id = subj_resp.json()["id"]
    tafel_resp = await client.post(
        "/api/stundentafeln", json={"name": "Dup Entry Tafel", "grade_level": 7}
    )
    tafel_id = tafel_resp.json()["id"]
    await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 3},
    )
    response = await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 4},
    )
    assert response.status_code == 409


async def test_update_entry(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /stundentafeln/{id}/entries/{entry_id} updates the specified fields.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st9.com", role="admin")
    await login_as("admin@st9.com", "testpassword123")
    subj_resp = await client.post(
        "/api/subjects", json={"name": "Englisch ST9", "short_name": "EnST9", "color": "chart-4"}
    )
    subject_id = subj_resp.json()["id"]
    tafel_resp = await client.post(
        "/api/stundentafeln", json={"name": "Update Entry Tafel", "grade_level": 5}
    )
    tafel_id = tafel_resp.json()["id"]
    entry_resp = await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 3, "preferred_block_size": 1},
    )
    entry_id = entry_resp.json()["id"]
    response = await client.patch(
        f"/api/stundentafeln/{tafel_id}/entries/{entry_id}",
        json={"hours_per_week": 4, "preferred_block_size": 2},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["hours_per_week"] == 4
    assert body["preferred_block_size"] == 2
    assert body["subject"]["id"] == subject_id


async def test_delete_entry(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """DELETE /stundentafeln/{id}/entries/{entry_id} removes the entry; detail no longer shows it.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st10.com", role="admin")
    await login_as("admin@st10.com", "testpassword123")
    subj_resp = await client.post(
        "/api/subjects", json={"name": "Kunst ST10", "short_name": "KuST10", "color": "chart-5"}
    )
    subject_id = subj_resp.json()["id"]
    tafel_resp = await client.post(
        "/api/stundentafeln", json={"name": "Delete Entry Tafel", "grade_level": 9}
    )
    tafel_id = tafel_resp.json()["id"]
    entry_resp = await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 2},
    )
    entry_id = entry_resp.json()["id"]
    delete_resp = await client.delete(f"/api/stundentafeln/{tafel_id}/entries/{entry_id}")
    assert delete_resp.status_code == 204
    get_resp = await client.get(f"/api/stundentafeln/{tafel_id}")
    body = get_resp.json()
    entry_ids = [e["id"] for e in body["entries"]]
    assert entry_id not in entry_ids


async def test_delete_stundentafel_referenced_by_class(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """DELETE /stundentafeln/{id} returns 409 when a class references it.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@st11.com", role="admin")
    await login_as("admin@st11.com", "testpassword123")
    scheme_resp = await client.post("/api/week-schemes", json={"name": "Test Scheme ST11"})
    tafel_resp = await client.post(
        "/api/stundentafeln", json={"name": "Test Tafel ST11", "grade_level": 5}
    )
    tafel_id = tafel_resp.json()["id"]
    await client.post(
        "/api/classes",
        json={
            "name": "5a",
            "grade_level": 5,
            "stundentafel_id": tafel_id,
            "week_scheme_id": scheme_resp.json()["id"],
        },
    )
    response = await client.delete(f"/api/stundentafeln/{tafel_id}")
    assert response.status_code == 409


async def test_stundentafel_requires_admin(client: AsyncClient) -> None:
    """GET /stundentafeln without authentication returns 401 Unauthorized.

    Args:
        client: The async test HTTP client (no session cookie set).
    """
    response = await client.get("/api/stundentafeln")
    assert response.status_code == 401


async def test_create_stundentafel_entry_rejects_odd_hours_with_block_size_two(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /stundentafeln/{id}/entries returns 422 when h is not divisible by n."""
    await create_test_user(email="admin@stf-dop1.com", role="admin")
    await login_as("admin@stf-dop1.com", "testpassword123")
    subj_resp = await client.post(
        "/api/subjects",
        json={"name": "Sport STDop1", "short_name": "SpDop1", "color": "chart-1"},
    )
    subject_id = subj_resp.json()["id"]
    tafel_resp = await client.post(
        "/api/stundentafeln", json={"name": "Tafel Dop Entry", "grade_level": 5}
    )
    tafel_id = tafel_resp.json()["id"]

    response = await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 3, "preferred_block_size": 2},
    )
    assert response.status_code == 422, response.text
    assert "preferred_block_size" in response.text


async def test_update_stundentafel_entry_rejects_block_size_change_breaking_divisibility(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH entry returns 422 when the merged row's hours are not divisible by block size."""
    await create_test_user(email="admin@stf-dop2.com", role="admin")
    await login_as("admin@stf-dop2.com", "testpassword123")
    subj_resp = await client.post(
        "/api/subjects",
        json={"name": "Sport STDop2", "short_name": "SpDop2", "color": "chart-2"},
    )
    subject_id = subj_resp.json()["id"]
    tafel_resp = await client.post(
        "/api/stundentafeln", json={"name": "Tafel Dop Entry Update", "grade_level": 6}
    )
    tafel_id = tafel_resp.json()["id"]
    entry_resp = await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 3, "preferred_block_size": 1},
    )
    assert entry_resp.status_code == 201, entry_resp.text
    entry_id = entry_resp.json()["id"]

    response = await client.patch(
        f"/api/stundentafeln/{tafel_id}/entries/{entry_id}",
        json={"preferred_block_size": 2},
    )
    assert response.status_code == 422, response.text
    assert "preferred_block_size" in response.text


async def test_create_stundentafel_default_school_type(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /stundentafeln without school_type defaults to Grundschule on the response."""
    await create_test_user(email="admin@stf-st-default.com", role="admin")
    await login_as("admin@stf-st-default.com", "testpassword123")
    response = await client.post(
        "/api/stundentafeln",
        json={"name": "Tafel default", "grade_level": 1},
    )
    assert response.status_code == 201
    body = response.json()
    assert body["school_type"] == "Grundschule"


async def test_create_stundentafel_explicit_school_type(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /stundentafeln with an explicit school_type echoes it back."""
    await create_test_user(email="admin@stf-st-explicit.com", role="admin")
    await login_as("admin@stf-st-explicit.com", "testpassword123")
    response = await client.post(
        "/api/stundentafeln",
        json={"name": "Gymnasium Tafel", "grade_level": 5, "school_type": "Gymnasium"},
    )
    assert response.status_code == 201
    body = response.json()
    assert body["school_type"] == "Gymnasium"
    assert body["grade_level"] == 5


async def test_create_stundentafel_grade_level_too_high(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /stundentafeln with grade_level above 13 returns 422."""
    await create_test_user(email="admin@stf-st-toohigh.com", role="admin")
    await login_as("admin@stf-st-toohigh.com", "testpassword123")
    response = await client.post(
        "/api/stundentafeln",
        json={"name": "OverGymnasium", "grade_level": 14},
    )
    assert response.status_code == 422


async def test_create_stundentafel_invalid_school_type(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /stundentafeln with an unknown school_type value returns 422."""
    await create_test_user(email="admin@stf-st-invalid.com", role="admin")
    await login_as("admin@stf-st-invalid.com", "testpassword123")
    response = await client.post(
        "/api/stundentafeln",
        json={"name": "BadTypeTafel", "grade_level": 5, "school_type": "FH"},
    )
    assert response.status_code == 422


async def test_update_stundentafel_school_type(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /stundentafeln/{id} updates school_type and the GET reflects it."""
    await create_test_user(email="admin@stf-st-patch.com", role="admin")
    await login_as("admin@stf-st-patch.com", "testpassword123")
    create = await client.post(
        "/api/stundentafeln",
        json={"name": "PatchTafel", "grade_level": 6},
    )
    tafel_id = create.json()["id"]

    patch = await client.patch(
        f"/api/stundentafeln/{tafel_id}",
        json={"school_type": "Gymnasium"},
    )
    assert patch.status_code == 200
    assert patch.json()["school_type"] == "Gymnasium"

    detail = await client.get(f"/api/stundentafeln/{tafel_id}")
    assert detail.json()["school_type"] == "Gymnasium"
