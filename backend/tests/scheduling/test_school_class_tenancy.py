"""Cross-school isolation tests for the SchoolClass aggregate."""

import uuid

import pytest
from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.school_class import SchoolClass

pytestmark = pytest.mark.anyio


@pytest.fixture
async def school_b_classes(db_session: AsyncSession) -> School:
    """A second school distinct from DEFAULT_SCHOOL_ID."""
    school = School(name="Schule B (classes)", short_name="SBC")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_list_school_classes_is_school_scoped(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_classes: School,
    create_test_user,
    login_as,
    create_school_class,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """GET /classes returns only the requesting user's school's classes."""
    user_a, password = await create_test_user(
        email="admin-classes-a@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    await create_school_class(
        name="A 1a",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=DEFAULT_SCHOOL_ID,
    )
    await create_school_class(
        name="B 1a",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=school_b_classes.id,
    )

    await login_as(user_a.email, password)
    response = await client.get("/api/classes")
    assert response.status_code == 200
    body = response.json()
    names = {row["name"] for row in body}
    assert "A 1a" in names
    assert "B 1a" not in names


async def test_get_school_class_returns_404_for_cross_school(
    client: AsyncClient,
    school_b_classes: School,
    create_test_user,
    login_as,
    create_school_class,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """GET /classes/{id} where the class is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-classes-detail@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    other = await create_school_class(
        name="B 2a",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=school_b_classes.id,
    )

    await login_as(user.email, password)
    response = await client.get(f"/api/classes/{other.id}")
    assert response.status_code == 404


async def test_patch_school_class_returns_404_for_cross_school(
    client: AsyncClient,
    school_b_classes: School,
    create_test_user,
    login_as,
    create_school_class,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """PATCH /classes/{id} where the class is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-classes-patch@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    other = await create_school_class(
        name="B 3a",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=school_b_classes.id,
    )

    await login_as(user.email, password)
    response = await client.patch(f"/api/classes/{other.id}", json={"name": "Sneaky rename"})
    assert response.status_code == 404


async def test_delete_school_class_returns_404_for_cross_school(
    client: AsyncClient,
    school_b_classes: School,
    create_test_user,
    login_as,
    create_school_class,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """DELETE /classes/{id} where the class is in another school returns 404."""
    user, password = await create_test_user(
        email="admin-classes-delete@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    other = await create_school_class(
        name="B 4a",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=school_b_classes.id,
    )

    await login_as(user.email, password)
    response = await client.delete(f"/api/classes/{other.id}")
    assert response.status_code == 404


async def test_post_school_class_stamps_users_school_id(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_classes: School,
    create_test_user,
    login_as,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """POST /classes stamps current_user.school_id; a body-supplied school_id is ignored."""
    user, password = await create_test_user(
        email="admin-classes-post@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()

    await login_as(user.email, password)
    response = await client.post(
        "/api/classes",
        json={
            "name": "Fresh Tenanted Class",
            "grade_level": 1,
            "stundentafel_id": str(tafel.id),
            "week_scheme_id": str(scheme.id),
            "school_id": str(school_b_classes.id),
        },
    )
    assert response.status_code == 201
    body = response.json()
    new_class = await db_session.get(SchoolClass, uuid.UUID(body["id"]))
    assert new_class is not None
    assert new_class.school_id == user.school_id == DEFAULT_SCHOOL_ID
    assert new_class.school_id != school_b_classes.id


async def test_two_schools_can_share_class_name(
    school_b_classes: School,
    create_school_class,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """Composite UNIQUE(school_id, name) lets two schools both have a '1a'."""
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    a = await create_school_class(
        name="1a", stundentafel_id=tafel.id, week_scheme_id=scheme.id, school_id=DEFAULT_SCHOOL_ID
    )
    b = await create_school_class(
        name="1a", stundentafel_id=tafel.id, week_scheme_id=scheme.id, school_id=school_b_classes.id
    )
    assert a.id != b.id
    assert a.school_id != b.school_id
    assert a.name == b.name == "1a"


async def test_same_school_duplicate_class_name_rejected(
    client: AsyncClient,
    create_test_user,
    login_as,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """POST /classes with a name already used in the requesting school returns 409."""
    user, password = await create_test_user(
        email="admin-classes-dup@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()

    await login_as(user.email, password)
    payload = {
        "name": "DupClass",
        "grade_level": 1,
        "stundentafel_id": str(tafel.id),
        "week_scheme_id": str(scheme.id),
    }
    first = await client.post("/api/classes", json=payload)
    assert first.status_code == 201
    second = await client.post("/api/classes", json=payload)
    assert second.status_code == 409


async def test_post_schedule_returns_404_for_cross_school_class(
    client: AsyncClient,
    school_b_classes: School,
    create_test_user,
    login_as,
    create_school_class,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """POST /classes/{id}/schedule for a cross-school class id returns 404."""
    user, password = await create_test_user(
        email="admin-classes-schedule@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    other = await create_school_class(
        name="B sched class",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=school_b_classes.id,
    )

    await login_as(user.email, password)
    response = await client.post(f"/api/classes/{other.id}/schedule")
    assert response.status_code == 404


async def test_get_quality_issues_returns_404_for_cross_school_class(
    client: AsyncClient,
    school_b_classes: School,
    create_test_user,
    login_as,
    create_school_class,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """GET /classes/{id}/quality-issues for a cross-school class returns 404."""
    user, password = await create_test_user(
        email="admin-classes-quality@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    other = await create_school_class(
        name="B quality class",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=school_b_classes.id,
    )

    await login_as(user.email, password)
    response = await client.get(f"/api/classes/{other.id}/quality-issues")
    assert response.status_code == 404


async def test_get_schedule_returns_404_for_cross_school_class(
    client: AsyncClient,
    school_b_classes: School,
    create_test_user,
    login_as,
    create_school_class,
    create_stundentafel,
    create_week_scheme,
) -> None:
    """GET /classes/{id}/schedule for a cross-school class id returns 404."""
    user, password = await create_test_user(
        email="admin-classes-read@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    other = await create_school_class(
        name="B read class",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        school_id=school_b_classes.id,
    )

    await login_as(user.email, password)
    response = await client.get(f"/api/classes/{other.id}/schedule")
    assert response.status_code == 404
