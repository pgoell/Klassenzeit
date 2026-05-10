"""Integration tests for the Teacher CRUD routes with qualifications and availability."""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient

from klassenzeit_backend.db.models.user import User

pytestmark = pytest.mark.anyio

# Type aliases matching the factory fixtures defined in conftest.py
type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]


async def test_create_teacher(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /teachers creates a teacher and returns 201 with the teacher data.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher1.com", role="admin")
    await login_as("admin@teacher1.com", "testpassword123")
    response = await client.post(
        "/api/teachers",
        json={
            "first_name": "Anna",
            "last_name": "Müller",
            "short_code": "AMU",
            "max_hours_per_week": 24,
        },
    )
    assert response.status_code == 201
    body = response.json()
    assert body["first_name"] == "Anna"
    assert body["last_name"] == "Müller"
    assert body["short_code"] == "AMU"
    assert body["max_hours_per_week"] == 24
    assert body["is_active"] is True
    assert "id" in body


async def test_create_teacher_duplicate_short_code(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /teachers with a duplicate short_code returns 409 Conflict.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher2.com", role="admin")
    await login_as("admin@teacher2.com", "testpassword123")
    await client.post(
        "/api/teachers",
        json={
            "first_name": "Bob",
            "last_name": "Smith",
            "short_code": "BSM",
            "max_hours_per_week": 20,
        },
    )
    response = await client.post(
        "/api/teachers",
        json={
            "first_name": "Brian",
            "last_name": "Stone",
            "short_code": "BSM",
            "max_hours_per_week": 18,
        },
    )
    assert response.status_code == 409


async def test_list_teachers(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /teachers returns all teachers without nested qualifications or availability.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher3.com", role="admin")
    await login_as("admin@teacher3.com", "testpassword123")
    await client.post(
        "/api/teachers",
        json={
            "first_name": "Clara",
            "last_name": "Zander",
            "short_code": "CZA",
            "max_hours_per_week": 22,
        },
    )
    await client.post(
        "/api/teachers",
        json={
            "first_name": "David",
            "last_name": "Appel",
            "short_code": "DAP",
            "max_hours_per_week": 20,
        },
    )
    response = await client.get("/api/teachers")
    assert response.status_code == 200
    body = response.json()
    assert len(body) >= 2
    last_names = [t["last_name"] for t in body]
    assert "Zander" in last_names
    assert "Appel" in last_names
    for item in body:
        assert "qualifications" not in item
        assert "availability" not in item


async def test_list_teachers_filter_active(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /teachers?active=true returns only active teachers after one is soft-deleted.

    Creates two teachers, deactivates one via DELETE, then verifies the filter
    returns only the remaining active teacher.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher4.com", role="admin")
    await login_as("admin@teacher4.com", "testpassword123")
    r1 = await client.post(
        "/api/teachers",
        json={
            "first_name": "Eva",
            "last_name": "FilterA",
            "short_code": "EFA",
            "max_hours_per_week": 20,
        },
    )
    r2 = await client.post(
        "/api/teachers",
        json={
            "first_name": "Frank",
            "last_name": "FilterB",
            "short_code": "FFB",
            "max_hours_per_week": 20,
        },
    )
    teacher1_id = r1.json()["id"]

    # Soft-delete teacher1
    delete_resp = await client.delete(f"/api/teachers/{teacher1_id}")
    assert delete_resp.status_code == 204

    # Filter active=true: should only see teacher2
    active_resp = await client.get("/api/teachers?active=true")
    assert active_resp.status_code == 200
    active_ids = [t["id"] for t in active_resp.json()]
    assert teacher1_id not in active_ids
    assert r2.json()["id"] in active_ids

    # Filter active=false: should only see teacher1
    inactive_resp = await client.get("/api/teachers?active=false")
    assert inactive_resp.status_code == 200
    inactive_ids = [t["id"] for t in inactive_resp.json()]
    assert teacher1_id in inactive_ids


async def test_get_teacher_detail(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /teachers/{id} returns teacher detail with qualifications and availability.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher5.com", role="admin")
    await login_as("admin@teacher5.com", "testpassword123")

    # Create the teacher
    teacher_resp = await client.post(
        "/api/teachers",
        json={
            "first_name": "Grace",
            "last_name": "Detail",
            "short_code": "GDE",
            "max_hours_per_week": 25,
        },
    )
    teacher_id = teacher_resp.json()["id"]

    # Create a subject and assign as qualification
    subj_resp = await client.post(
        "/api/subjects", json={"name": "Biology", "short_name": "BIO", "color": "chart-5"}
    )
    subject_id = subj_resp.json()["id"]
    await client.put(
        f"/api/teachers/{teacher_id}/qualifications", json={"subject_ids": [subject_id]}
    )

    # Create a week scheme + time block for availability
    ws_resp = await client.post("/api/week-schemes", json={"name": "Teacher Detail Scheme"})
    ws_id = ws_resp.json()["id"]
    tb_resp = await client.post(
        f"/api/week-schemes/{ws_id}/time-blocks",
        json={"day_of_week": 2, "position": 1, "start_time": "08:00:00", "end_time": "08:45:00"},
    )
    tb_id = tb_resp.json()["id"]
    await client.put(
        f"/api/teachers/{teacher_id}/availability",
        json={"entries": [{"time_block_id": tb_id, "status": "available"}]},
    )

    response = await client.get(f"/api/teachers/{teacher_id}")
    assert response.status_code == 200
    body = response.json()
    assert body["id"] == teacher_id
    assert body["first_name"] == "Grace"
    assert body["last_name"] == "Detail"
    assert "qualifications" in body
    assert len(body["qualifications"]) == 1
    assert body["qualifications"][0]["name"] == "Biology"
    assert "availability" in body
    assert len(body["availability"]) == 1
    assert body["availability"][0]["time_block_id"] == tb_id
    assert body["availability"][0]["day_of_week"] == 2
    assert body["availability"][0]["position"] == 1
    assert body["availability"][0]["status"] == "available"


async def test_update_teacher(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /teachers/{id} updates only the supplied fields and returns 200.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher6.com", role="admin")
    await login_as("admin@teacher6.com", "testpassword123")
    teacher_resp = await client.post(
        "/api/teachers",
        json={
            "first_name": "Hans",
            "last_name": "Original",
            "short_code": "HOR",
            "max_hours_per_week": 20,
        },
    )
    teacher_id = teacher_resp.json()["id"]
    response = await client.patch(
        f"/api/teachers/{teacher_id}",
        json={"last_name": "Updated", "max_hours_per_week": 28},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["first_name"] == "Hans"
    assert body["last_name"] == "Updated"
    assert body["short_code"] == "HOR"
    assert body["max_hours_per_week"] == 28


async def test_delete_teacher_soft_deletes(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """DELETE /teachers/{id} sets is_active=False; teacher remains accessible via GET.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher7.com", role="admin")
    await login_as("admin@teacher7.com", "testpassword123")
    teacher_resp = await client.post(
        "/api/teachers",
        json={
            "first_name": "Iris",
            "last_name": "SoftDel",
            "short_code": "ISD",
            "max_hours_per_week": 22,
        },
    )
    teacher_id = teacher_resp.json()["id"]

    delete_resp = await client.delete(f"/api/teachers/{teacher_id}")
    assert delete_resp.status_code == 204

    # Teacher should still be accessible (soft delete, not hard delete)
    get_resp = await client.get(f"/api/teachers/{teacher_id}")
    assert get_resp.status_code == 200
    body = get_resp.json()
    assert body["is_active"] is False


async def test_replace_qualifications(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PUT /teachers/{id}/qualifications replaces the qualification list and returns detail.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher8.com", role="admin")
    await login_as("admin@teacher8.com", "testpassword123")

    teacher_resp = await client.post(
        "/api/teachers",
        json={
            "first_name": "Jonas",
            "last_name": "Qual",
            "short_code": "JQU",
            "max_hours_per_week": 20,
        },
    )
    teacher_id = teacher_resp.json()["id"]

    # Create two subjects
    hist_resp = await client.post(
        "/api/subjects", json={"name": "History", "short_name": "HIS", "color": "chart-6"}
    )
    geo_resp = await client.post(
        "/api/subjects", json={"name": "Geography", "short_name": "GEO", "color": "chart-7"}
    )
    hist_id = hist_resp.json()["id"]
    geo_id = geo_resp.json()["id"]

    # Set initial qualifications to History only
    first_put = await client.put(
        f"/api/teachers/{teacher_id}/qualifications", json={"subject_ids": [hist_id]}
    )
    assert first_put.status_code == 200
    first_body = first_put.json()
    assert len(first_body["qualifications"]) == 1
    assert first_body["qualifications"][0]["name"] == "History"

    # Replace with Geography only — History should be gone
    second_put = await client.put(
        f"/api/teachers/{teacher_id}/qualifications", json={"subject_ids": [geo_id]}
    )
    assert second_put.status_code == 200
    second_body = second_put.json()
    assert len(second_body["qualifications"]) == 1
    assert second_body["qualifications"][0]["name"] == "Geography"

    # Confirm via GET
    detail = await client.get(f"/api/teachers/{teacher_id}")
    assert len(detail.json()["qualifications"]) == 1
    assert detail.json()["qualifications"][0]["id"] == geo_id


async def test_replace_teacher_availability(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PUT /teachers/{id}/availability replaces the availability list and returns detail.

    Creates a week scheme with two time blocks via the API, then verifies that
    the teacher's availability is replaced correctly with status values.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher9.com", role="admin")
    await login_as("admin@teacher9.com", "testpassword123")

    teacher_resp = await client.post(
        "/api/teachers",
        json={
            "first_name": "Klara",
            "last_name": "Avail",
            "short_code": "KAV",
            "max_hours_per_week": 24,
        },
    )
    teacher_id = teacher_resp.json()["id"]

    # Create week scheme and two time blocks
    ws_resp = await client.post("/api/week-schemes", json={"name": "Teacher Avail Scheme"})
    ws_id = ws_resp.json()["id"]
    tb1_resp = await client.post(
        f"/api/week-schemes/{ws_id}/time-blocks",
        json={"day_of_week": 0, "position": 1, "start_time": "08:00:00", "end_time": "08:45:00"},
    )
    tb2_resp = await client.post(
        f"/api/week-schemes/{ws_id}/time-blocks",
        json={"day_of_week": 0, "position": 2, "start_time": "09:00:00", "end_time": "09:45:00"},
    )
    tb1_id = tb1_resp.json()["id"]
    tb2_id = tb2_resp.json()["id"]

    # Set availability to both time blocks with different statuses
    first_put = await client.put(
        f"/api/teachers/{teacher_id}/availability",
        json={
            "entries": [
                {"time_block_id": tb1_id, "status": "preferred"},
                {"time_block_id": tb2_id, "status": "unavailable"},
            ]
        },
    )
    assert first_put.status_code == 200
    first_body = first_put.json()
    assert len(first_body["availability"]) == 2

    # Replace with only the first time block as available
    second_put = await client.put(
        f"/api/teachers/{teacher_id}/availability",
        json={"entries": [{"time_block_id": tb1_id, "status": "available"}]},
    )
    assert second_put.status_code == 200
    second_body = second_put.json()
    assert len(second_body["availability"]) == 1
    assert second_body["availability"][0]["time_block_id"] == tb1_id
    assert second_body["availability"][0]["status"] == "available"

    # Confirm via GET
    detail = await client.get(f"/api/teachers/{teacher_id}")
    avail = detail.json()["availability"]
    assert len(avail) == 1
    assert avail[0]["time_block_id"] == tb1_id
    assert avail[0]["day_of_week"] == 0
    assert avail[0]["position"] == 1


async def test_replace_availability_invalid_status(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PUT /teachers/{id}/availability with an invalid status returns 422.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@teacher10.com", role="admin")
    await login_as("admin@teacher10.com", "testpassword123")

    teacher_resp = await client.post(
        "/api/teachers",
        json={
            "first_name": "Lars",
            "last_name": "BadStatus",
            "short_code": "LBS",
            "max_hours_per_week": 20,
        },
    )
    teacher_id = teacher_resp.json()["id"]

    ws_resp = await client.post("/api/week-schemes", json={"name": "Bad Status Scheme"})
    ws_id = ws_resp.json()["id"]
    tb_resp = await client.post(
        f"/api/week-schemes/{ws_id}/time-blocks",
        json={"day_of_week": 0, "position": 1, "start_time": "08:00:00", "end_time": "08:45:00"},
    )
    tb_id = tb_resp.json()["id"]

    response = await client.put(
        f"/api/teachers/{teacher_id}/availability",
        json={"entries": [{"time_block_id": tb_id, "status": "maybe"}]},
    )
    assert response.status_code == 422


async def test_teacher_requires_admin(client: AsyncClient) -> None:
    """GET /teachers without authentication returns 401 Unauthorized.

    Args:
        client: The async test HTTP client (no session cookie set).
    """
    response = await client.get("/api/teachers")
    assert response.status_code == 401


async def test_list_teachers_includes_subject_ids(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /teachers returns each teacher's qualified-subject IDs."""
    await create_test_user(email="admin@teacher-subject-ids.com", role="admin")
    await login_as("admin@teacher-subject-ids.com", "testpassword123")

    teacher_resp = await client.post(
        "/api/teachers",
        json={
            "first_name": "Eva",
            "last_name": "Quali",
            "short_code": "EQU",
            "max_hours_per_week": 24,
        },
    )
    teacher_id = teacher_resp.json()["id"]

    ma_resp = await client.post(
        "/api/subjects",
        json={"name": "Mathematik", "short_name": "MA", "color": "chart-1"},
    )
    de_resp = await client.post(
        "/api/subjects",
        json={"name": "Deutsch", "short_name": "DE", "color": "chart-2"},
    )
    ma_id = ma_resp.json()["id"]
    de_id = de_resp.json()["id"]

    qual_resp = await client.put(
        f"/api/teachers/{teacher_id}/qualifications",
        json={"subject_ids": [ma_id, de_id]},
    )
    assert qual_resp.status_code == 200

    list_resp = await client.get("/api/teachers")
    assert list_resp.status_code == 200
    rows = list_resp.json()
    row = next(t for t in rows if t["id"] == teacher_id)
    assert sorted(row["subject_ids"]) == sorted([ma_id, de_id])
