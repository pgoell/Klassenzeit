"""Cross-school tenancy isolation tests for the WeekScheme aggregate."""

import datetime as dt
import uuid

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.week_scheme import TimeBlock, TimeBlockKind, WeekScheme


@pytest.fixture
async def school_b_week_schemes(db_session: AsyncSession) -> School:
    """A second school distinct from DEFAULT_SCHOOL_ID."""
    school = School(name="Schule B (week-schemes)", short_name="SBW")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_week_scheme_duplicate_name_across_schools_allowed(
    db_session: AsyncSession,
    school_b_week_schemes: School,
) -> None:
    """The same scheme name may live in two different schools simultaneously."""
    a = WeekScheme(name="Standard", school_id=DEFAULT_SCHOOL_ID)
    b = WeekScheme(name="Standard", school_id=school_b_week_schemes.id)
    db_session.add_all([a, b])
    await db_session.flush()
    rows = (
        (await db_session.execute(select(WeekScheme).where(WeekScheme.name == "Standard")))
        .scalars()
        .all()
    )
    assert {r.school_id for r in rows} == {DEFAULT_SCHOOL_ID, school_b_week_schemes.id}


async def test_week_scheme_duplicate_name_within_school_rejected(
    db_session: AsyncSession,
) -> None:
    """A second WeekScheme with the same name in the same school violates the
    composite UNIQUE constraint."""
    a = WeekScheme(name="Same Name", school_id=DEFAULT_SCHOOL_ID)
    db_session.add(a)
    await db_session.flush()
    async with db_session.begin_nested():
        with pytest.raises(IntegrityError):
            b = WeekScheme(name="Same Name", school_id=DEFAULT_SCHOOL_ID)
            db_session.add(b)
            await db_session.flush()


async def test_list_week_schemes_excludes_other_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_week_schemes: School,
    create_test_user,
    login_as,
) -> None:
    """GET /week-schemes returns only the current user's school's rows."""
    user, password = await create_test_user(
        email="admin-ws-list@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    own = WeekScheme(name="Own Scheme", school_id=DEFAULT_SCHOOL_ID)
    foreign = WeekScheme(name="Foreign Scheme", school_id=school_b_week_schemes.id)
    db_session.add_all([own, foreign])
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.get("/api/week-schemes")
    assert response.status_code == 200
    names = {row["name"] for row in response.json()}
    assert "Own Scheme" in names
    assert "Foreign Scheme" not in names


async def test_get_week_scheme_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_week_schemes: School,
    create_test_user,
    login_as,
) -> None:
    """GET /week-schemes/{id} where the row is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-ws-get@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign = WeekScheme(name="Foreign Get", school_id=school_b_week_schemes.id)
    db_session.add(foreign)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.get(f"/api/week-schemes/{foreign.id}")
    assert response.status_code == 404


async def test_patch_week_scheme_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_week_schemes: School,
    create_test_user,
    login_as,
) -> None:
    """PATCH /week-schemes/{id} where the row is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-ws-patch@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign = WeekScheme(name="Foreign Patch", school_id=school_b_week_schemes.id)
    db_session.add(foreign)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.patch(f"/api/week-schemes/{foreign.id}", json={"name": "Renamed"})
    assert response.status_code == 404


async def test_delete_week_scheme_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_week_schemes: School,
    create_test_user,
    login_as,
) -> None:
    """DELETE /week-schemes/{id} where the row is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-ws-delete@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign = WeekScheme(name="Foreign Delete", school_id=school_b_week_schemes.id)
    db_session.add(foreign)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.delete(f"/api/week-schemes/{foreign.id}")
    assert response.status_code == 404


async def test_create_week_scheme_stamps_current_user_school_id(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_week_schemes: School,
    create_test_user,
    login_as,
) -> None:
    """POST /week-schemes stamps school_id from the current user."""
    user, password = await create_test_user(
        email="admin-ws-create@test.com",
        role="admin",
        school_id=school_b_week_schemes.id,
    )
    await login_as(user.email, password)
    response = await client.post(
        "/api/week-schemes",
        json={"name": "Brand New Scheme"},
    )
    assert response.status_code == 201
    scheme_id = response.json()["id"]
    scheme = (
        await db_session.execute(select(WeekScheme).where(WeekScheme.id == uuid.UUID(scheme_id)))
    ).scalar_one()
    assert scheme.school_id == school_b_week_schemes.id


async def test_create_time_block_under_foreign_scheme_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_week_schemes: School,
    create_test_user,
    login_as,
) -> None:
    """POST a TimeBlock under a foreign-school parent scheme returns 404."""
    user, password = await create_test_user(
        email="admin-tb-create@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_scheme = WeekScheme(name="Foreign Parent", school_id=school_b_week_schemes.id)
    db_session.add(foreign_scheme)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.post(
        f"/api/week-schemes/{foreign_scheme.id}/time-blocks",
        json={
            "day_of_week": 1,
            "position": 1,
            "start_time": "08:00:00",
            "end_time": "08:45:00",
            "kind": "lesson",
        },
    )
    assert response.status_code == 404


async def test_patch_time_block_under_foreign_scheme_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_week_schemes: School,
    create_test_user,
    login_as,
) -> None:
    """PATCH a TimeBlock under a foreign-school parent scheme returns 404."""
    user, password = await create_test_user(
        email="admin-tb-patch@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_scheme = WeekScheme(name="Foreign Patch Parent", school_id=school_b_week_schemes.id)
    db_session.add(foreign_scheme)
    await db_session.flush()
    foreign_block = TimeBlock(
        week_scheme_id=foreign_scheme.id,
        day_of_week=1,
        position=1,
        start_time=dt.time(8, 0),
        end_time=dt.time(8, 45),
        kind=TimeBlockKind.LESSON,
    )
    db_session.add(foreign_block)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.patch(
        f"/api/week-schemes/{foreign_scheme.id}/time-blocks/{foreign_block.id}",
        json={"position": 2},
    )
    assert response.status_code == 404


async def test_create_school_class_cross_school_week_scheme_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_week_schemes: School,
    create_test_user,
    create_stundentafel,
    login_as,
) -> None:
    """POST /classes with a cross-school week_scheme_id returns 404."""
    user, password = await create_test_user(
        email="admin-ws-xclass@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_scheme = WeekScheme(name="Foreign for class", school_id=school_b_week_schemes.id)
    db_session.add(foreign_scheme)
    await db_session.flush()
    tafel = await create_stundentafel()

    await login_as(user.email, password)
    response = await client.post(
        "/api/classes",
        json={
            "name": "1a",
            "grade_level": 1,
            "stundentafel_id": str(tafel.id),
            "week_scheme_id": str(foreign_scheme.id),
        },
    )
    assert response.status_code == 404


async def test_patch_school_class_cross_school_week_scheme_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_week_schemes: School,
    create_test_user,
    create_school_class,
    create_stundentafel,
    create_week_scheme,
    login_as,
) -> None:
    """PATCH /classes/{id} setting a cross-school week_scheme_id returns 404."""
    user, password = await create_test_user(
        email="admin-ws-xclass-patch@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    own_tafel = await create_stundentafel()
    own_scheme = await create_week_scheme()
    own_class = await create_school_class(
        stundentafel_id=own_tafel.id, week_scheme_id=own_scheme.id
    )
    foreign_scheme = WeekScheme(name="Foreign for patch", school_id=school_b_week_schemes.id)
    db_session.add(foreign_scheme)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.patch(
        f"/api/classes/{own_class.id}",
        json={"week_scheme_id": str(foreign_scheme.id)},
    )
    assert response.status_code == 404
