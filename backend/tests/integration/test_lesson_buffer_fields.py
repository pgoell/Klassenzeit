"""Integration tests for Lesson travel-buffer fields on POST /api/lessons."""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient

from klassenzeit_backend.db.models.user import User

pytestmark = pytest.mark.anyio

# Type aliases matching the factory fixtures defined in conftest.py
type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]


async def _create_subject_for_buffer_test(client: AsyncClient, name: str, short_name: str) -> str:
    resp = await client.post(
        "/api/subjects",
        json={"name": name, "short_name": short_name, "color": "chart-1"},
    )
    assert resp.status_code == 201, resp.text
    return resp.json()["id"]


async def _create_week_scheme(client: AsyncClient, name: str) -> str:
    resp = await client.post("/api/week-schemes", json={"name": name})
    assert resp.status_code == 201, resp.text
    return resp.json()["id"]


async def _create_stundentafel(client: AsyncClient, name: str, grade_level: int = 3) -> str:
    resp = await client.post("/api/stundentafeln", json={"name": name, "grade_level": grade_level})
    assert resp.status_code == 201, resp.text
    return resp.json()["id"]


async def _create_school_class_for_buffer_test(
    client: AsyncClient,
    name: str,
    grade_level: int,
    stundentafel_id: str,
    week_scheme_id: str,
) -> str:
    resp = await client.post(
        "/api/classes",
        json={
            "name": name,
            "grade_level": grade_level,
            "stundentafel_id": stundentafel_id,
            "week_scheme_id": week_scheme_id,
        },
    )
    assert resp.status_code == 201, resp.text
    return resp.json()["id"]


async def _setup_class_and_subject(
    client: AsyncClient,
    *,
    tag: str,
    subject_short: str,
    grade_level: int = 3,
) -> tuple[str, str]:
    """Create a class + subject and return ``(class_id, subject_id)``."""
    subject_id = await _create_subject_for_buffer_test(client, f"Subject {tag}", subject_short)
    scheme_id = await _create_week_scheme(client, f"Scheme {tag}")
    tafel_id = await _create_stundentafel(client, f"Tafel {tag}", grade_level)
    class_id = await _create_school_class_for_buffer_test(
        client, f"Class {tag}", grade_level, tafel_id, scheme_id
    )
    return class_id, subject_id


async def test_create_lesson_with_buffer_minutes(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons round-trips ``pre_buffer_minutes`` and ``post_buffer_minutes``."""
    await create_test_user(email="admin@buf1.com", role="admin")
    await login_as("admin@buf1.com", "testpassword123")
    class_id, subject_id = await _setup_class_and_subject(client, tag="B1", subject_short="Sw")
    resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subject_id,
            "hours_per_week": 2,
            "preferred_block_size": 2,
            "pre_buffer_minutes": 15,
            "post_buffer_minutes": 15,
        },
    )
    assert resp.status_code == 201, resp.text
    body = resp.json()
    assert body["pre_buffer_minutes"] == 15
    assert body["post_buffer_minutes"] == 15


async def test_create_lesson_defaults_buffer_minutes_zero(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons without buffer fields defaults both to 0."""
    await create_test_user(email="admin@buf2.com", role="admin")
    await login_as("admin@buf2.com", "testpassword123")
    class_id, subject_id = await _setup_class_and_subject(client, tag="B2", subject_short="Ma")
    resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subject_id,
            "hours_per_week": 4,
            "preferred_block_size": 2,
        },
    )
    assert resp.status_code == 201, resp.text
    body = resp.json()
    assert body["pre_buffer_minutes"] == 0
    assert body["post_buffer_minutes"] == 0


async def test_create_lesson_rejects_buffer_above_60(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons with ``pre_buffer_minutes=61`` is rejected with 422."""
    await create_test_user(email="admin@buf3.com", role="admin")
    await login_as("admin@buf3.com", "testpassword123")
    class_id, subject_id = await _setup_class_and_subject(client, tag="B3", subject_short="De")
    resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subject_id,
            "hours_per_week": 2,
            "preferred_block_size": 2,
            "pre_buffer_minutes": 61,
        },
    )
    assert resp.status_code == 422, resp.text


async def test_create_lesson_rejects_negative_buffer(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons with negative ``pre_buffer_minutes`` is rejected with 422."""
    await create_test_user(email="admin@buf4.com", role="admin")
    await login_as("admin@buf4.com", "testpassword123")
    class_id, subject_id = await _setup_class_and_subject(client, tag="B4", subject_short="En")
    resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subject_id,
            "hours_per_week": 2,
            "preferred_block_size": 2,
            "pre_buffer_minutes": -1,
        },
    )
    assert resp.status_code == 422, resp.text
