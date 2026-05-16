"""Integration tests for the Room.is_external field on POST /api/rooms."""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient

from klassenzeit_backend.db.models.user import User

pytestmark = pytest.mark.anyio

# Type aliases matching the factory fixtures defined in conftest.py
type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]


async def test_create_room_with_is_external_true(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /rooms with ``is_external=True`` round-trips the flag."""
    await create_test_user(email="admin@ext1.com", role="admin")
    await login_as("admin@ext1.com", "testpassword123")
    response = await client.post(
        "/api/rooms",
        json={"name": "Schwimmbad", "short_name": "SB", "is_external": True},
    )
    assert response.status_code == 201, response.text
    assert response.json()["is_external"] is True


async def test_create_room_defaults_is_external_false(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /rooms without ``is_external`` defaults the flag to ``False``."""
    await create_test_user(email="admin@ext2.com", role="admin")
    await login_as("admin@ext2.com", "testpassword123")
    response = await client.post(
        "/api/rooms",
        json={"name": "Klassenraum 1A", "short_name": "1A"},
    )
    assert response.status_code == 201, response.text
    assert response.json()["is_external"] is False
