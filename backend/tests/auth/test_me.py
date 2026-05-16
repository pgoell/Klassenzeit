"""Tests for GET /auth/me and POST /auth/change-password."""

from datetime import UTC, datetime, timedelta

from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.sessions import create_session, lookup_session
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.session import UserSession


async def test_me_returns_user_info(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="me@test.com")
    await login_as("me@test.com", "testpassword123")
    response = await client.get("/api/auth/me")
    assert response.status_code == 200
    body = response.json()
    assert body["email"] == "me@test.com"
    assert body["role"] == "user"
    assert body["force_password_change"] is False
    assert "id" in body


async def test_me_includes_school_info(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    """The /auth/me response carries the requesting user's school id and name."""
    await create_test_user(email="me-school@test.com")
    await login_as("me-school@test.com", "testpassword123")
    response = await client.get("/api/auth/me")
    assert response.status_code == 200
    body = response.json()
    assert body["school_id"] == str(DEFAULT_SCHOOL_ID)
    assert body["school_name"] == "Default Schule"


async def test_me_without_cookie_returns_401(client: AsyncClient) -> None:
    response = await client.get("/api/auth/me")
    assert response.status_code == 401


async def test_me_with_expired_session_returns_401(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
) -> None:
    user, _ = await create_test_user(email="expired@test.com")
    session = UserSession(
        user_id=user.id,
        expires_at=datetime.now(UTC) - timedelta(hours=1),
    )
    db_session.add(session)
    await db_session.flush()
    client.cookies.set("kz_session", str(session.id))
    response = await client.get("/api/auth/me")
    assert response.status_code == 401


async def test_me_shows_force_password_change(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="force@test.com", force_password_change=True)
    await login_as("force@test.com", "testpassword123")
    response = await client.get("/api/auth/me")
    assert response.json()["force_password_change"] is True


async def test_change_password_succeeds(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    _, pw = await create_test_user(email="change@test.com")
    await login_as("change@test.com", pw)
    response = await client.post(
        "/api/auth/change-password",
        json={"current_password": pw, "new_password": "a-brand-new-passphrase"},
    )
    assert response.status_code == 204


async def test_change_password_wrong_current_returns_401(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    _, pw = await create_test_user(email="wrongcur@test.com")
    await login_as("wrongcur@test.com", pw)
    response = await client.post(
        "/api/auth/change-password",
        json={"current_password": "wrongpassword!!", "new_password": "newpassphrase!!"},
    )
    assert response.status_code == 401


async def test_change_password_too_short_returns_422(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    _, pw = await create_test_user(email="short@test.com")
    await login_as("short@test.com", pw)
    response = await client.post(
        "/api/auth/change-password",
        json={"current_password": pw, "new_password": "short"},
    )
    assert response.status_code == 422


async def test_change_password_clears_force_flag(
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    await create_test_user(email="clearflag@test.com", force_password_change=True)
    await login_as("clearflag@test.com", "testpassword123")
    await client.post(
        "/api/auth/change-password",
        json={
            "current_password": "testpassword123",
            "new_password": "a-brand-new-passphrase",
        },
    )
    me = await client.get("/api/auth/me")
    assert me.json()["force_password_change"] is False


async def test_change_password_invalidates_other_sessions(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
) -> None:
    user, pw = await create_test_user(email="killsess@test.com")
    # Create an extra session (simulating another device)
    other_session = await create_session(db_session, user.id, ttl_days=14)
    await db_session.commit()

    await login_as("killsess@test.com", pw)
    await client.post(
        "/api/auth/change-password",
        json={"current_password": pw, "new_password": "a-brand-new-passphrase"},
    )
    # The other session should be gone
    found = await lookup_session(db_session, other_session.id)
    assert found is None
