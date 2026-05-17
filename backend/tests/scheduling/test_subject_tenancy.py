"""Cross-school tenancy isolation tests for the Subject aggregate."""

import uuid

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.subject import Subject


@pytest.fixture
async def school_b_subjects(db_session: AsyncSession) -> School:
    """A second school distinct from DEFAULT_SCHOOL_ID."""
    school = School(name="Schule B (subjects)", short_name="SBS")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_duplicate_short_name_across_schools_allowed(
    db_session: AsyncSession,
    school_b_subjects: School,
) -> None:
    """The same short_name may live in two different schools simultaneously."""
    a = Subject(name="Sport A", short_name="Sp", color="chart-1", school_id=DEFAULT_SCHOOL_ID)
    b = Subject(name="Sport B", short_name="Sp", color="chart-2", school_id=school_b_subjects.id)
    db_session.add_all([a, b])
    await db_session.flush()
    rows = (
        (await db_session.execute(select(Subject).where(Subject.short_name == "Sp")))
        .scalars()
        .all()
    )
    assert {r.school_id for r in rows} == {DEFAULT_SCHOOL_ID, school_b_subjects.id}


async def test_duplicate_short_name_within_school_rejected(
    db_session: AsyncSession,
) -> None:
    """A second subject with the same short_name in the same school violates the
    composite UNIQUE constraint."""
    a = Subject(name="Subject A", short_name="SX", color="chart-1", school_id=DEFAULT_SCHOOL_ID)
    db_session.add(a)
    await db_session.flush()
    async with db_session.begin_nested():
        with pytest.raises(IntegrityError):
            b = Subject(
                name="Subject B", short_name="SX", color="chart-2", school_id=DEFAULT_SCHOOL_ID
            )
            db_session.add(b)
            await db_session.flush()


async def test_list_subjects_excludes_other_school(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_subjects: School,
    create_test_user,
    login_as,
) -> None:
    """GET /subjects returns only the requesting user's school's subjects."""
    user_a, password = await create_test_user(
        email="admin-subjects-list@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    own = Subject(name="Own Sport", short_name="OS", color="chart-1", school_id=DEFAULT_SCHOOL_ID)
    foreign = Subject(
        name="Foreign Sport", short_name="FS", color="chart-2", school_id=school_b_subjects.id
    )
    db_session.add_all([own, foreign])
    await db_session.flush()

    await login_as(user_a.email, password)
    response = await client.get("/api/subjects")
    assert response.status_code == 200
    names = {row["name"] for row in response.json()}
    assert "Own Sport" in names
    assert "Foreign Sport" not in names


async def test_get_subject_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_subjects: School,
    create_test_user,
    login_as,
) -> None:
    """GET /subjects/{id} where the subject is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-subjects-get@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign = Subject(
        name="Foreign Music", short_name="FM", color="chart-1", school_id=school_b_subjects.id
    )
    db_session.add(foreign)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.get(f"/api/subjects/{foreign.id}")
    assert response.status_code == 404


async def test_patch_subject_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_subjects: School,
    create_test_user,
    login_as,
) -> None:
    """PATCH /subjects/{id} where the subject is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-subjects-patch@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign = Subject(
        name="Foreign English", short_name="FE", color="chart-2", school_id=school_b_subjects.id
    )
    db_session.add(foreign)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.patch(f"/api/subjects/{foreign.id}", json={"name": "Renamed"})
    assert response.status_code == 404


async def test_delete_subject_other_school_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_subjects: School,
    create_test_user,
    login_as,
) -> None:
    """DELETE /subjects/{id} where the subject is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-subjects-delete@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign = Subject(
        name="Foreign Art", short_name="FA", color="chart-3", school_id=school_b_subjects.id
    )
    db_session.add(foreign)
    await db_session.flush()

    await login_as(user.email, password)
    response = await client.delete(f"/api/subjects/{foreign.id}")
    assert response.status_code == 404


async def test_create_subject_stamps_current_user_school_id(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_subjects: School,
    create_test_user,
    login_as,
) -> None:
    """POST /subjects stamps school_id from the current user."""
    user, password = await create_test_user(
        email="admin-subjects-create@test.com",
        role="admin",
        school_id=school_b_subjects.id,
    )
    await login_as(user.email, password)
    response = await client.post(
        "/api/subjects",
        json={"name": "Brand New", "short_name": "BN", "color": "chart-6"},
    )
    assert response.status_code == 201
    subject_id = response.json()["id"]
    subject = (
        await db_session.execute(select(Subject).where(Subject.id == uuid.UUID(subject_id)))
    ).scalar_one()
    assert subject.school_id == school_b_subjects.id
