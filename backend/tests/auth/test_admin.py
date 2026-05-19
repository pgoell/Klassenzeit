"""Tests for admin user management routes."""

from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.session import UserSession
from klassenzeit_backend.db.models.user import User


async def test_create_user(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="admin@test.com", role="admin")
    await login_as("admin@test.com", "testpassword123")
    response = await client.post(
        "/api/auth/admin/users",
        json={
            "email": "newuser@test.com",
            "password": "a-secure-passphrase",
        },
    )
    assert response.status_code == 201
    body = response.json()
    assert body["email"] == "newuser@test.com"
    assert body["role"] == "user"
    assert "id" in body


async def test_create_user_with_admin_role(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="admin2@test.com", role="admin")
    await login_as("admin2@test.com", "testpassword123")
    response = await client.post(
        "/api/auth/admin/users",
        json={
            "email": "newadmin@test.com",
            "password": "a-secure-passphrase",
            "role": "admin",
        },
    )
    assert response.status_code == 201
    assert response.json()["role"] == "admin"


async def test_create_user_duplicate_email_returns_409(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="dupadmin@test.com", role="admin")
    await login_as("dupadmin@test.com", "testpassword123")
    await client.post(
        "/api/auth/admin/users",
        json={"email": "dup@test.com", "password": "a-secure-passphrase"},
    )
    response = await client.post(
        "/api/auth/admin/users",
        json={"email": "dup@test.com", "password": "another-passphrase!"},
    )
    assert response.status_code == 409


async def test_create_user_weak_password_returns_422(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="weakadmin@test.com", role="admin")
    await login_as("weakadmin@test.com", "testpassword123")
    response = await client.post(
        "/api/auth/admin/users",
        json={"email": "weak@test.com", "password": "short"},
    )
    assert response.status_code == 422


async def test_non_admin_returns_403(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="regular@test.com", role="user")
    await login_as("regular@test.com", "testpassword123")
    response = await client.post(
        "/api/auth/admin/users",
        json={"email": "x@test.com", "password": "a-secure-passphrase"},
    )
    assert response.status_code == 403


async def test_list_users(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="listadmin@test.com", role="admin")
    await login_as("listadmin@test.com", "testpassword123")
    await client.post(
        "/api/auth/admin/users",
        json={"email": "listme@test.com", "password": "a-secure-passphrase"},
    )
    response = await client.get("/api/auth/admin/users")
    assert response.status_code == 200
    emails = [u["email"] for u in response.json()]
    assert "listadmin@test.com" in emails
    assert "listme@test.com" in emails


async def test_list_users_filter_active(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="filteradmin@test.com", role="admin")
    await login_as("filteradmin@test.com", "testpassword123")
    await client.post(
        "/api/auth/admin/users",
        json={"email": "willdeactivate@test.com", "password": "a-secure-passphrase"},
    )
    users = (await client.get("/api/auth/admin/users")).json()
    uid = next(u["id"] for u in users if u["email"] == "willdeactivate@test.com")
    await client.post(f"/api/auth/admin/users/{uid}/deactivate")

    active = await client.get("/api/auth/admin/users?active=true")
    active_emails = [u["email"] for u in active.json()]
    assert "willdeactivate@test.com" not in active_emails

    inactive = await client.get("/api/auth/admin/users?active=false")
    inactive_emails = [u["email"] for u in inactive.json()]
    assert "willdeactivate@test.com" in inactive_emails


async def test_reset_password(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="resetadmin@test.com", role="admin")
    await login_as("resetadmin@test.com", "testpassword123")
    create_resp = await client.post(
        "/api/auth/admin/users",
        json={"email": "resetme@test.com", "password": "a-secure-passphrase"},
    )
    uid = create_resp.json()["id"]
    response = await client.post(
        f"/api/auth/admin/users/{uid}/reset-password",
        json={"new_password": "a-new-secure-passphrase"},
    )
    assert response.status_code == 204


async def test_reset_password_sets_force_flag(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="forceadmin@test.com", role="admin")
    await login_as("forceadmin@test.com", "testpassword123")
    create_resp = await client.post(
        "/api/auth/admin/users",
        json={"email": "forcereset@test.com", "password": "a-secure-passphrase"},
    )
    uid = create_resp.json()["id"]
    await client.post(
        f"/api/auth/admin/users/{uid}/reset-password",
        json={"new_password": "a-new-secure-passphrase"},
    )
    # Verify force flag directly via DB query
    result = await db_session.execute(select(User).where(User.email == "forcereset@test.com"))
    user = result.scalar_one()
    assert user.force_password_change is True


async def test_deactivate_user(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="deactadmin@test.com", role="admin")
    await login_as("deactadmin@test.com", "testpassword123")
    create_resp = await client.post(
        "/api/auth/admin/users",
        json={"email": "deactme@test.com", "password": "a-secure-passphrase"},
    )
    uid = create_resp.json()["id"]
    response = await client.post(f"/api/auth/admin/users/{uid}/deactivate")
    assert response.status_code == 204


async def test_deactivated_user_cannot_login(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="deactlogin@test.com", role="admin")
    await login_as("deactlogin@test.com", "testpassword123")
    create_resp = await client.post(
        "/api/auth/admin/users",
        json={"email": "blocked@test.com", "password": "a-secure-passphrase"},
    )
    uid = create_resp.json()["id"]
    await client.post(f"/api/auth/admin/users/{uid}/deactivate")
    login_resp = await client.post(
        "/api/auth/login",
        json={"email": "blocked@test.com", "password": "a-secure-passphrase"},
    )
    assert login_resp.status_code == 401


async def test_activate_user(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="actadmin@test.com", role="admin")
    await login_as("actadmin@test.com", "testpassword123")
    create_resp = await client.post(
        "/api/auth/admin/users",
        json={"email": "reactivate@test.com", "password": "a-secure-passphrase"},
    )
    uid = create_resp.json()["id"]
    await client.post(f"/api/auth/admin/users/{uid}/deactivate")
    response = await client.post(f"/api/auth/admin/users/{uid}/activate")
    assert response.status_code == 204


async def test_set_role_promote_admin_to_super_admin(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="root1@test.com", role="super_admin")
    await create_test_user(email="promoteme@test.com", role="admin")
    await login_as("root1@test.com", "testpassword123")
    target = (
        await db_session.execute(select(User).where(User.email == "promoteme@test.com"))
    ).scalar_one()
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/role",
        json={"role": "super_admin"},
    )
    assert response.status_code == 200, response.text
    body = response.json()
    assert body["role"] == "super_admin"
    assert body["email"] == "promoteme@test.com"
    await db_session.refresh(target)
    assert target.role == "super_admin"


async def test_set_role_promote_user_to_admin(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="root2@test.com", role="super_admin")
    await create_test_user(email="newadmin@test.com", role="user")
    await login_as("root2@test.com", "testpassword123")
    target = (
        await db_session.execute(select(User).where(User.email == "newadmin@test.com"))
    ).scalar_one()
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/role",
        json={"role": "admin"},
    )
    assert response.status_code == 200, response.text
    await db_session.refresh(target)
    assert target.role == "admin"


async def test_set_role_demote_super_admin_to_admin(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="root3@test.com", role="super_admin")
    await create_test_user(email="peer@test.com", role="super_admin")
    await login_as("root3@test.com", "testpassword123")
    target = (
        await db_session.execute(select(User).where(User.email == "peer@test.com"))
    ).scalar_one()
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/role",
        json={"role": "admin"},
    )
    assert response.status_code == 200, response.text
    await db_session.refresh(target)
    assert target.role == "admin"


async def test_set_role_demote_self_when_another_super_admin_active(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    actor, _ = await create_test_user(email="rotateme@test.com", role="super_admin")
    await create_test_user(email="stayed@test.com", role="super_admin")
    await login_as("rotateme@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{actor.id}/role",
        json={"role": "admin"},
    )
    assert response.status_code == 200, response.text
    await db_session.refresh(actor)
    assert actor.role == "admin"


async def test_set_role_last_super_admin_returns_409(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    actor, _ = await create_test_user(email="onlyone@test.com", role="super_admin")
    await login_as("onlyone@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{actor.id}/role",
        json={"role": "admin"},
    )
    assert response.status_code == 409, response.text
    assert response.json()["detail"] == {"code": "last_super_admin"}


async def test_set_role_last_super_admin_ignores_deactivated_peer(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    actor, _ = await create_test_user(email="lonelyactive@test.com", role="super_admin")
    await create_test_user(
        email="deadpeer@test.com",
        role="super_admin",
        is_active=False,
    )
    await login_as("lonelyactive@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{actor.id}/role",
        json={"role": "admin"},
    )
    assert response.status_code == 409, response.text
    assert response.json()["detail"] == {"code": "last_super_admin"}


async def test_set_role_idempotent_no_op_keeps_sessions(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    actor, _ = await create_test_user(email="noopme@test.com", role="super_admin")
    await login_as("noopme@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{actor.id}/role",
        json={"role": "super_admin"},
    )
    assert response.status_code == 200, response.text
    sessions = (
        (await db_session.execute(select(UserSession).where(UserSession.user_id == actor.id)))
        .scalars()
        .all()
    )
    assert len(sessions) >= 1


async def test_set_role_non_super_admin_returns_403(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="onlyadmin@test.com", role="admin")
    await create_test_user(email="target403@test.com", role="user")
    await login_as("onlyadmin@test.com", "testpassword123")
    target = (
        await db_session.execute(select(User).where(User.email == "target403@test.com"))
    ).scalar_one()
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/role",
        json={"role": "admin"},
    )
    assert response.status_code == 403, response.text


async def test_set_role_unauthenticated_returns_401(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
) -> None:
    await create_test_user(email="target401@test.com", role="user")
    target = (
        await db_session.execute(select(User).where(User.email == "target401@test.com"))
    ).scalar_one()
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/role",
        json={"role": "admin"},
    )
    assert response.status_code == 401, response.text


async def test_set_role_invalid_role_returns_422(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="root422@test.com", role="super_admin")
    await create_test_user(email="target422@test.com", role="user")
    await login_as("root422@test.com", "testpassword123")
    target = (
        await db_session.execute(select(User).where(User.email == "target422@test.com"))
    ).scalar_one()
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/role",
        json={"role": "owner"},
    )
    assert response.status_code == 422, response.text


async def test_set_role_unknown_user_returns_404(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="root404@test.com", role="super_admin")
    await login_as("root404@test.com", "testpassword123")
    response = await client.post(
        "/api/auth/admin/users/00000000-0000-0000-0000-000000000404/role",
        json={"role": "admin"},
    )
    assert response.status_code == 404, response.text


async def test_set_role_invalidates_target_sessions(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="rootkill@test.com", role="super_admin")
    target, _ = await create_test_user(email="killmysessions@test.com", role="admin")
    # Login as target first to create a session row, then re-login as the
    # actor; the target's row stays in the DB even after the cookie is
    # replaced on the shared client.
    await login_as("killmysessions@test.com", "testpassword123")
    pre = (
        (await db_session.execute(select(UserSession).where(UserSession.user_id == target.id)))
        .scalars()
        .all()
    )
    assert len(pre) == 1

    await login_as("rootkill@test.com", "testpassword123")
    response = await client.post(
        f"/api/auth/admin/users/{target.id}/role",
        json={"role": "super_admin"},
    )
    assert response.status_code == 200, response.text

    post = (
        (await db_session.execute(select(UserSession).where(UserSession.user_id == target.id)))
        .scalars()
        .all()
    )
    assert post == []
