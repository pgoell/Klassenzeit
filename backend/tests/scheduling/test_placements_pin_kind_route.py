"""Three-state cycle integration tests for ``PATCH /api/placements/.../pin``.

Covers ``{"pin_kind": "hard"}``, ``{"pin_kind": "soft"}``, and
``{"pin_kind": null}``: the route accepts each, persists the value, and
surfaces both ``pin_kind`` and the computed ``pinned`` flag on the
response. ADR 0042.
"""

import pytest
from httpx import AsyncClient

from tests.scheduling.conftest import SeededMovablePlacement


@pytest.mark.asyncio
async def test_pin_route_accepts_hard_kind(
    client: AsyncClient,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    """PATCH with ``{"pin_kind": "hard"}`` writes HARD and reports ``pinned=True``."""
    await create_test_user(email="admin@pin-hard.com", role="admin")
    await login_as("admin@pin-hard.com", "testpassword123")
    fixture = seeded_movable_placement
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin",
        json={"pin_kind": "hard"},
    )
    assert response.status_code == 200, response.text
    body = response.json()
    assert body["pin_kind"] == "hard"
    assert body["pinned"] is True


@pytest.mark.asyncio
async def test_pin_route_accepts_soft_kind(
    client: AsyncClient,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    """PATCH with ``{"pin_kind": "soft"}`` writes SOFT and reports ``pinned=True``."""
    await create_test_user(email="admin@pin-soft.com", role="admin")
    await login_as("admin@pin-soft.com", "testpassword123")
    fixture = seeded_movable_placement
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin",
        json={"pin_kind": "soft"},
    )
    assert response.status_code == 200, response.text
    body = response.json()
    assert body["pin_kind"] == "soft"
    assert body["pinned"] is True


@pytest.mark.asyncio
async def test_pin_route_clears_pin(
    client: AsyncClient,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    """PATCH with ``{"pin_kind": null}`` clears the pin and reports ``pinned=False``."""
    await create_test_user(email="admin@pin-null.com", role="admin")
    await login_as("admin@pin-null.com", "testpassword123")
    fixture = seeded_movable_placement
    # First set to hard so we can observe the null clear.
    response_set = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin",
        json={"pin_kind": "hard"},
    )
    assert response_set.status_code == 200, response_set.text
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin",
        json={"pin_kind": None},
    )
    assert response.status_code == 200, response.text
    body = response.json()
    assert body["pin_kind"] is None
    assert body["pinned"] is False
