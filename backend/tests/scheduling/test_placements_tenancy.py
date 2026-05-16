"""Cross-school isolation tests for the placements aggregate."""

import pytest
from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from tests.scheduling.conftest import SeededMovablePlacement

pytestmark = pytest.mark.anyio


@pytest.fixture
async def school_b_for_placements(db_session: AsyncSession) -> School:
    """A second school distinct from DEFAULT_SCHOOL_ID."""
    school = School(name="Schule B Placements", short_name="SBP")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_move_placement_with_cross_school_room_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_for_placements: School,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    """PATCH /placements/{lesson_id}/{tb_id} with a cross-school room returns 404."""
    await create_test_user(
        email="admin-place-cross@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    room_in_b = Room(
        name="Cross-school room",
        short_name="CSX",
        school_id=school_b_for_placements.id,
    )
    db_session.add(room_in_b)
    await db_session.flush()
    await login_as("admin-place-cross@test.com", "testpassword123")
    fixture = seeded_movable_placement

    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}",
        json={
            "time_block_id": str(fixture.target_time_block_id),
            "room_id": str(room_in_b.id),
        },
    )
    assert response.status_code == 404
