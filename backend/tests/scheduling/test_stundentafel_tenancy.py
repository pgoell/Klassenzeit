"""Cross-school tenancy isolation tests for the Stundentafel aggregate."""

import uuid

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.stundentafel import Stundentafel


@pytest.fixture
async def school_b_stundentafeln(db_session: AsyncSession) -> School:
    """A second school distinct from DEFAULT_SCHOOL_ID."""
    school = School(name="Schule B (stundentafeln)", short_name="SBS")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_duplicate_name_across_schools_allowed(
    db_session: AsyncSession,
    school_b_stundentafeln: School,
) -> None:
    """The same name may live in two different schools simultaneously."""
    a = Stundentafel(name="Klasse 3", grade_level=3, school_id=DEFAULT_SCHOOL_ID)
    b = Stundentafel(name="Klasse 3", grade_level=3, school_id=school_b_stundentafeln.id)
    db_session.add_all([a, b])
    await db_session.flush()
    rows = (
        (await db_session.execute(select(Stundentafel).where(Stundentafel.name == "Klasse 3")))
        .scalars()
        .all()
    )
    assert {r.school_id for r in rows} == {DEFAULT_SCHOOL_ID, school_b_stundentafeln.id}


async def test_duplicate_name_within_school_rejected(
    db_session: AsyncSession,
) -> None:
    """A second Stundentafel with the same name in the same school violates the
    composite UNIQUE constraint."""
    a = Stundentafel(name="Same Name", grade_level=3, school_id=DEFAULT_SCHOOL_ID)
    db_session.add(a)
    await db_session.flush()
    async with db_session.begin_nested():
        with pytest.raises(IntegrityError):
            b = Stundentafel(name="Same Name", grade_level=4, school_id=DEFAULT_SCHOOL_ID)
            db_session.add(b)
            await db_session.flush()


async def test_list_stundentafeln_excludes_other_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_stundentafeln: School,
    create_test_user,
    login_as,
) -> None:
    """GET /stundentafeln returns only the requesting user's school's rows."""
    user, password = await create_test_user(
        email="admin-tafeln-list@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    own = Stundentafel(name="Own K3", grade_level=3, school_id=DEFAULT_SCHOOL_ID)
    foreign = Stundentafel(name="Foreign K3", grade_level=3, school_id=school_b_stundentafeln.id)
    db_session.add_all([own, foreign])
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.get("/api/stundentafeln")
    assert response.status_code == 200
    names = {row["name"] for row in response.json()}
    assert "Own K3" in names
    assert "Foreign K3" not in names


async def test_get_stundentafel_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_stundentafeln: School,
    create_test_user,
    login_as,
) -> None:
    """GET /stundentafeln/{id} where the row is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-tafeln-get@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign = Stundentafel(name="Foreign K4", grade_level=4, school_id=school_b_stundentafeln.id)
    db_session.add(foreign)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.get(f"/api/stundentafeln/{foreign.id}")
    assert response.status_code == 404


async def test_patch_stundentafel_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_stundentafeln: School,
    create_test_user,
    login_as,
) -> None:
    """PATCH /stundentafeln/{id} where the row is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-tafeln-patch@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign = Stundentafel(name="Foreign Patch", grade_level=4, school_id=school_b_stundentafeln.id)
    db_session.add(foreign)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.patch(f"/api/stundentafeln/{foreign.id}", json={"name": "Renamed"})
    assert response.status_code == 404


async def test_delete_stundentafel_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_stundentafeln: School,
    create_test_user,
    login_as,
) -> None:
    """DELETE /stundentafeln/{id} where the row is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-tafeln-delete@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign = Stundentafel(
        name="Foreign Delete", grade_level=4, school_id=school_b_stundentafeln.id
    )
    db_session.add(foreign)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.delete(f"/api/stundentafeln/{foreign.id}")
    assert response.status_code == 404


async def test_create_stundentafel_stamps_current_user_school_id(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_stundentafeln: School,
    create_test_user,
    login_as,
) -> None:
    """POST /stundentafeln stamps school_id from the current user."""
    user, password = await create_test_user(
        email="admin-tafeln-create@test.com",
        role="admin",
        school_id=school_b_stundentafeln.id,
    )
    await login_as(user.email, password)
    response = await client.post(
        "/api/stundentafeln",
        json={"name": "Brand New K2", "grade_level": 2, "school_type": "Grundschule"},
    )
    assert response.status_code == 201
    tafel_id = response.json()["id"]
    tafel = (
        await db_session.execute(select(Stundentafel).where(Stundentafel.id == uuid.UUID(tafel_id)))
    ).scalar_one()
    assert tafel.school_id == school_b_stundentafeln.id


async def test_create_school_class_cross_school_stundentafel_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_stundentafeln: School,
    create_test_user,
    create_week_scheme,
    login_as,
) -> None:
    """POST /classes with a cross-school stundentafel_id returns 404."""
    user, password = await create_test_user(
        email="admin-tafeln-xclass@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_tafel = Stundentafel(
        name="Foreign Curriculum", grade_level=3, school_id=school_b_stundentafeln.id
    )
    db_session.add(foreign_tafel)
    await db_session.flush()
    week_scheme = await create_week_scheme()

    await login_as(user.email, password)
    response = await client.post(
        "/api/classes",
        json={
            "name": "1a",
            "grade_level": 1,
            "stundentafel_id": str(foreign_tafel.id),
            "week_scheme_id": str(week_scheme.id),
        },
    )
    assert response.status_code == 404
