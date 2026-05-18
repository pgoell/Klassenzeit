"""Tests for the extended /auth/me payload and POST /auth/switch-school.

These are HTTP-shaped end-to-end tests against the ASGI client; the
narrower dependency tests live in ``test_dependencies.py`` and the
unchanged /auth/me happy-path tests live in ``test_me.py``.
"""

import uuid

from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.sessions import create_session
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.user_school_membership import UserSchoolMembership

# --- /auth/me extended payload (Task 4) -------------------------------------


async def test_auth_me_returns_extended_payload_for_single_school_user(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    """A single-school user sees their home school as the only accessible one."""
    await create_test_user(email="me-ext-single@test.com")
    await login_as("me-ext-single@test.com", "testpassword123")

    response = await client.get("/api/auth/me")
    assert response.status_code == 200
    body = response.json()
    assert body["school_id"] == str(DEFAULT_SCHOOL_ID)
    assert body["active_school_id"] == str(DEFAULT_SCHOOL_ID)
    assert isinstance(body["active_school_name"], str)
    assert body["active_school_name"] == "Default Schule"
    assert isinstance(body["accessible_schools"], list)
    ids = [s["id"] for s in body["accessible_schools"]]
    assert ids == [str(DEFAULT_SCHOOL_ID)]


async def test_auth_me_returns_extended_payload_for_multi_school_user(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    """A user with a membership row sees both schools in accessible_schools."""
    school_b = School(name="Schule B (me-ext)", short_name="SBM")
    db_session.add(school_b)
    await db_session.flush()
    user, _ = await create_test_user(email="me-ext-multi@test.com")
    db_session.add(UserSchoolMembership(user_id=user.id, school_id=school_b.id))
    await db_session.flush()
    await db_session.commit()

    await login_as("me-ext-multi@test.com", "testpassword123")
    response = await client.get("/api/auth/me")
    assert response.status_code == 200
    body = response.json()
    ids = {s["id"] for s in body["accessible_schools"]}
    assert ids == {str(DEFAULT_SCHOOL_ID), str(school_b.id)}
    assert body["active_school_id"] == str(DEFAULT_SCHOOL_ID)


async def test_auth_me_super_admin_sees_all_schools(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    """Super-admin's accessible_schools is the full schools table."""
    school_b = School(name="Schule B (sa-me)", short_name="SBSA")
    db_session.add(school_b)
    await db_session.flush()
    await db_session.commit()

    await create_test_user(email="sa-ext@test.com", role="super_admin")
    await login_as("sa-ext@test.com", "testpassword123")

    response = await client.get("/api/auth/me")
    assert response.status_code == 200
    body = response.json()
    schools_in_db = (await db_session.execute(select(School))).scalars().all()
    expected_ids = {str(s.id) for s in schools_in_db}
    actual_ids = {s["id"] for s in body["accessible_schools"]}
    assert actual_ids == expected_ids


# --- POST /auth/switch-school (Task 5) --------------------------------------


async def test_switch_school_to_membership_school_succeeds(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    """A user with a membership can switch the session to that school."""
    school_b = School(name="Schule B (switch-mem)", short_name="SBSM")
    db_session.add(school_b)
    await db_session.flush()
    user, _ = await create_test_user(email="switch-mem@test.com")
    db_session.add(UserSchoolMembership(user_id=user.id, school_id=school_b.id))
    await db_session.flush()
    await db_session.commit()

    await login_as("switch-mem@test.com", "testpassword123")
    response = await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})
    assert response.status_code == 200
    body = response.json()
    assert body["active_school_id"] == str(school_b.id)
    assert body["active_school_name"] == "Schule B (switch-mem)"


async def test_switch_school_to_home_succeeds(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    """Switching to one's own home school is a no-op success."""
    user, _ = await create_test_user(email="switch-home@test.com")
    await login_as("switch-home@test.com", "testpassword123")

    response = await client.post("/api/auth/switch-school", json={"school_id": str(user.school_id)})
    assert response.status_code == 200
    body = response.json()
    assert body["active_school_id"] == str(user.school_id)


async def test_switch_school_to_non_accessible_school_403(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    """A regular user without a membership row for school_b is rejected with 403."""
    school_b = School(name="Schule B (switch-403)", short_name="SB403")
    db_session.add(school_b)
    await db_session.flush()
    await db_session.commit()

    await create_test_user(email="switch-403@test.com")
    await login_as("switch-403@test.com", "testpassword123")

    response = await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})
    assert response.status_code == 403


async def test_switch_school_super_admin_to_any_school_succeeds(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    """Super-admin can switch to any existing school without a membership row."""
    school_b = School(name="Schule B (switch-sa)", short_name="SBSAS")
    db_session.add(school_b)
    await db_session.flush()
    await db_session.commit()

    await create_test_user(email="switch-sa@test.com", role="super_admin")
    await login_as("switch-sa@test.com", "testpassword123")

    response = await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})
    assert response.status_code == 200
    body = response.json()
    assert body["active_school_id"] == str(school_b.id)


async def test_switch_school_to_nonexistent_school_404_for_super_admin(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    """A nonexistent school id yields 404 (not 403), even for a super-admin."""
    await create_test_user(email="switch-404@test.com", role="super_admin")
    await login_as("switch-404@test.com", "testpassword123")

    bogus = uuid.uuid4()
    response = await client.post("/api/auth/switch-school", json={"school_id": str(bogus)})
    assert response.status_code == 404


async def test_switch_school_persists_active_school_on_session(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    """After a successful switch, /auth/me reflects the new active school."""
    school_b = School(name="Schule B (switch-persist)", short_name="SBSP")
    db_session.add(school_b)
    await db_session.flush()
    user, _ = await create_test_user(email="switch-persist@test.com")
    db_session.add(UserSchoolMembership(user_id=user.id, school_id=school_b.id))
    await db_session.flush()
    await db_session.commit()

    await login_as("switch-persist@test.com", "testpassword123")
    await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})

    me = await client.get("/api/auth/me")
    assert me.status_code == 200
    assert me.json()["active_school_id"] == str(school_b.id)


async def test_switch_school_without_cookie_returns_401(client: AsyncClient) -> None:
    """An unauthenticated POST returns 401."""
    response = await client.post("/api/auth/switch-school", json={"school_id": str(uuid.uuid4())})
    assert response.status_code == 401


async def test_switch_school_create_session_path_kept_consistent(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
) -> None:
    """Sanity check that the create_session signature is honored end-to-end."""
    user, _ = await create_test_user(email="switch-sanity@test.com")
    session = await create_session(db_session, user.id, active_school_id=user.school_id)
    await db_session.commit()
    client.cookies.set("kz_session", str(session.id))
    response = await client.get("/api/auth/me")
    assert response.status_code == 200
    assert response.json()["active_school_id"] == str(user.school_id)
