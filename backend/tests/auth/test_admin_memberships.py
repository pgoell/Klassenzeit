"""Tests for super-admin school-membership endpoints (item 10j)."""

import logging

from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.session import UserSession
from klassenzeit_backend.db.models.super_admin_audit_log import SuperAdminAuditLog
from klassenzeit_backend.db.models.user_school_membership import UserSchoolMembership

UNKNOWN_USER_ID = "00000000-0000-0000-0000-000000000404"
UNKNOWN_SCHOOL_ID = "00000000-0000-0000-0000-0000000004f0"


async def _insert_school_b(db_session: AsyncSession) -> School:
    school = School(name="Sekundarschule B", short_name="B")
    db_session.add(school)
    await db_session.flush()
    return school


# ── role gating ────────────────────────────────────────────────────────────


async def test_list_memberships_non_super_admin_returns_403(
    client: AsyncClient, create_test_user, login_as
) -> None:
    await create_test_user(email="ma403@test.com", role="admin")
    target, _ = await create_test_user(email="targ403@test.com", role="user")
    await login_as("ma403@test.com", "testpassword123")
    response = await client.get(f"/api/auth/admin/users/{target.id}/memberships")
    assert response.status_code == 403, response.text


async def test_grant_membership_non_super_admin_returns_403(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="mg403@test.com", role="admin")
    target, _ = await create_test_user(email="targg403@test.com", role="user")
    school_b_memberships = await _insert_school_b(db_session)
    await login_as("mg403@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/memberships",
        json={"school_id": str(school_b_memberships.id)},
    )
    assert response.status_code == 403, response.text


async def test_revoke_membership_non_super_admin_returns_403(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="mr403@test.com", role="admin")
    target, _ = await create_test_user(email="targr403@test.com", role="user")
    school_b_memberships = await _insert_school_b(db_session)
    await login_as("mr403@test.com", "testpassword123")
    response = await client.delete(
        f"/api/auth/admin/users/{target.id}/memberships/{school_b_memberships.id}"
    )
    assert response.status_code == 403, response.text


async def test_list_memberships_unauthenticated_returns_401(
    client: AsyncClient,
) -> None:
    response = await client.get(f"/api/auth/admin/users/{UNKNOWN_USER_ID}/memberships")
    assert response.status_code == 401, response.text


async def test_grant_membership_unauthenticated_returns_401(
    client: AsyncClient,
) -> None:
    response = await client.post(
        f"/api/auth/admin/users/{UNKNOWN_USER_ID}/memberships",
        json={"school_id": UNKNOWN_SCHOOL_ID},
    )
    assert response.status_code == 401, response.text


async def test_revoke_membership_unauthenticated_returns_401(
    client: AsyncClient,
) -> None:
    response = await client.delete(
        f"/api/auth/admin/users/{UNKNOWN_USER_ID}/memberships/{UNKNOWN_SCHOOL_ID}"
    )
    assert response.status_code == 401, response.text


# ── GET ────────────────────────────────────────────────────────────────────


async def test_list_memberships_empty(client: AsyncClient, create_test_user, login_as) -> None:
    await create_test_user(email="rootlist@test.com", role="super_admin")
    target, _ = await create_test_user(email="empty@test.com", role="user")
    await login_as("rootlist@test.com", "testpassword123")
    response = await client.get(f"/api/auth/admin/users/{target.id}/memberships")
    assert response.status_code == 200, response.text
    assert response.json() == []


async def test_list_memberships_two_rows_sorted_by_school_name(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="rootlist2@test.com", role="super_admin")
    target, _ = await create_test_user(email="twomemberships@test.com", role="user")
    school_a = School(name="Aachen Schule", short_name="A")
    school_z = School(name="Zellertal Schule", short_name="Z")
    db_session.add_all([school_a, school_z])
    await db_session.flush()
    db_session.add_all(
        [
            UserSchoolMembership(user_id=target.id, school_id=school_z.id),
            UserSchoolMembership(user_id=target.id, school_id=school_a.id),
        ]
    )
    await db_session.flush()
    await login_as("rootlist2@test.com", "testpassword123")
    response = await client.get(f"/api/auth/admin/users/{target.id}/memberships")
    assert response.status_code == 200, response.text
    body = response.json()
    assert [row["school_name"] for row in body] == ["Aachen Schule", "Zellertal Schule"]
    assert body[0]["school_id"] == str(school_a.id)


async def test_list_memberships_unknown_user_returns_404(
    client: AsyncClient, create_test_user, login_as
) -> None:
    await create_test_user(email="rootlist404@test.com", role="super_admin")
    await login_as("rootlist404@test.com", "testpassword123")
    response = await client.get(f"/api/auth/admin/users/{UNKNOWN_USER_ID}/memberships")
    assert response.status_code == 404, response.text


# ── POST grant ─────────────────────────────────────────────────────────────


async def test_grant_membership_happy_path(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="rootgrant@test.com", role="super_admin")
    target, _ = await create_test_user(email="grantme@test.com", role="user")
    school_b_memberships = await _insert_school_b(db_session)
    await login_as("rootgrant@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/memberships",
        json={"school_id": str(school_b_memberships.id)},
    )
    assert response.status_code == 201, response.text
    body = response.json()
    assert body == {
        "user_id": str(target.id),
        "school_id": str(school_b_memberships.id),
        "school_name": "Sekundarschule B",
    }
    rows = (
        (
            await db_session.execute(
                select(UserSchoolMembership).where(
                    UserSchoolMembership.user_id == target.id,
                    UserSchoolMembership.school_id == school_b_memberships.id,
                )
            )
        )
        .scalars()
        .all()
    )
    assert len(rows) == 1


async def test_grant_membership_duplicate_returns_409(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="rootdup@test.com", role="super_admin")
    target, _ = await create_test_user(email="dupme@test.com", role="user")
    school_b_memberships = await _insert_school_b(db_session)
    db_session.add(UserSchoolMembership(user_id=target.id, school_id=school_b_memberships.id))
    await db_session.flush()
    await login_as("rootdup@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/memberships",
        json={"school_id": str(school_b_memberships.id)},
    )
    assert response.status_code == 409, response.text
    assert response.json()["detail"] == {"code": "membership_exists"}


async def test_grant_membership_home_school_returns_409(
    client: AsyncClient, create_test_user, login_as
) -> None:
    await create_test_user(email="roothome@test.com", role="super_admin")
    target, _ = await create_test_user(email="homedup@test.com", role="user")
    await login_as("roothome@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/memberships",
        json={"school_id": str(DEFAULT_SCHOOL_ID)},
    )
    assert response.status_code == 409, response.text
    assert response.json()["detail"] == {"code": "membership_redundant_home_school"}


async def test_grant_membership_unknown_user_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="rootg404u@test.com", role="super_admin")
    school_b_memberships = await _insert_school_b(db_session)
    await login_as("rootg404u@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{UNKNOWN_USER_ID}/memberships",
        json={"school_id": str(school_b_memberships.id)},
    )
    assert response.status_code == 404, response.text


async def test_grant_membership_unknown_school_returns_404(
    client: AsyncClient, create_test_user, login_as
) -> None:
    await create_test_user(email="rootg404s@test.com", role="super_admin")
    target, _ = await create_test_user(email="targ404s@test.com", role="user")
    await login_as("rootg404s@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/memberships",
        json={"school_id": UNKNOWN_SCHOOL_ID},
    )
    assert response.status_code == 404, response.text


async def test_grant_membership_does_not_invalidate_sessions(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="rootnokill@test.com", role="super_admin")
    target, _ = await create_test_user(email="keepmysessions@test.com", role="user")
    school_b_memberships = await _insert_school_b(db_session)
    # Seed the target's session by logging in first
    await login_as("keepmysessions@test.com", "testpassword123")
    pre = (
        (await db_session.execute(select(UserSession).where(UserSession.user_id == target.id)))
        .scalars()
        .all()
    )
    assert len(pre) == 1
    # Then re-login as the actor (replaces the cookie on the shared client)
    await login_as("rootnokill@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/memberships",
        json={"school_id": str(school_b_memberships.id)},
    )
    assert response.status_code == 201, response.text
    post = (
        (await db_session.execute(select(UserSession).where(UserSession.user_id == target.id)))
        .scalars()
        .all()
    )
    assert len(post) == 1
    assert post[0].id == pre[0].id


# ── DELETE revoke ──────────────────────────────────────────────────────────


async def test_revoke_membership_happy_path_invalidates_sessions(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    caplog,
) -> None:
    await create_test_user(email="rootrev@test.com", role="super_admin")
    target, _ = await create_test_user(email="killmemberships@test.com", role="user")
    school_b_memberships = await _insert_school_b(db_session)
    db_session.add(UserSchoolMembership(user_id=target.id, school_id=school_b_memberships.id))
    await db_session.flush()
    await login_as("killmemberships@test.com", "testpassword123")
    pre = (
        (await db_session.execute(select(UserSession).where(UserSession.user_id == target.id)))
        .scalars()
        .all()
    )
    assert len(pre) == 1
    await login_as("rootrev@test.com", "testpassword123")
    caplog.clear()
    with caplog.at_level(logging.INFO, logger="klassenzeit_backend.auth.routes.admin"):
        response = await client.delete(
            f"/api/auth/admin/users/{target.id}/memberships/{school_b_memberships.id}"
        )
    assert response.status_code == 204, response.text
    rows = (
        (
            await db_session.execute(
                select(UserSchoolMembership).where(
                    UserSchoolMembership.user_id == target.id,
                    UserSchoolMembership.school_id == school_b_memberships.id,
                )
            )
        )
        .scalars()
        .all()
    )
    assert rows == []
    post = (
        (await db_session.execute(select(UserSession).where(UserSession.user_id == target.id)))
        .scalars()
        .all()
    )
    assert post == []
    revoke_events = [r for r in caplog.records if r.message == "admin.user_membership.revoke"]
    assert len(revoke_events) == 1
    event = revoke_events[0]
    assert event.__dict__["target_id"] == str(target.id)
    assert event.__dict__["school_id"] == str(school_b_memberships.id)
    assert event.__dict__["sessions_invalidated"] is True


async def test_revoke_membership_unknown_user_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="rootr404u@test.com", role="super_admin")
    school_b_memberships = await _insert_school_b(db_session)
    await login_as("rootr404u@test.com", "testpassword123")
    response = await client.delete(
        f"/api/auth/admin/users/{UNKNOWN_USER_ID}/memberships/{school_b_memberships.id}"
    )
    assert response.status_code == 404, response.text


async def test_revoke_membership_unknown_school_returns_404(
    client: AsyncClient, create_test_user, login_as
) -> None:
    await create_test_user(email="rootr404s@test.com", role="super_admin")
    target, _ = await create_test_user(email="targr404s@test.com", role="user")
    await login_as("rootr404s@test.com", "testpassword123")
    response = await client.delete(
        f"/api/auth/admin/users/{target.id}/memberships/{UNKNOWN_SCHOOL_ID}"
    )
    assert response.status_code == 404, response.text


async def test_revoke_membership_absent_row_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="rootr404m@test.com", role="super_admin")
    target, _ = await create_test_user(email="targr404m@test.com", role="user")
    school_b_memberships = await _insert_school_b(db_session)
    await login_as("rootr404m@test.com", "testpassword123")
    response = await client.delete(
        f"/api/auth/admin/users/{target.id}/memberships/{school_b_memberships.id}"
    )
    assert response.status_code == 404, response.text


# ── audit middleware sanity ────────────────────────────────────────────────


async def test_grant_membership_does_not_insert_audit_row(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    """The /api/auth/ exemption in SuperAdminAuditMiddleware (ADR 0048)
    means grant/revoke do NOT create super_admin_audit_log rows.
    """
    await create_test_user(email="rootaudit@test.com", role="super_admin")
    target, _ = await create_test_user(email="targaudit@test.com", role="user")
    school_b_memberships = await _insert_school_b(db_session)
    await login_as("rootaudit@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/memberships",
        json={"school_id": str(school_b_memberships.id)},
    )
    assert response.status_code == 201, response.text
    rows = (await db_session.execute(select(SuperAdminAuditLog))).scalars().all()
    assert rows == []
