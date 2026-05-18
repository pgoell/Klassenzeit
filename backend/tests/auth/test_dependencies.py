"""Tests for get_current_user and require_admin dependencies."""

import uuid

import pytest
from fastapi import HTTPException
from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import (
    get_scope_school_id,
    is_super_admin,
    require_super_admin,
)
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.user import User


async def test_unauthenticated_returns_401(client: AsyncClient) -> None:
    response = await client.get("/api/auth/me")
    assert response.status_code == 401


async def test_invalid_session_cookie_returns_401(client: AsyncClient) -> None:
    client.cookies.set("kz_session", "not-a-uuid")
    response = await client.get("/api/auth/me")
    assert response.status_code == 401


async def test_nonexistent_session_returns_401(client: AsyncClient) -> None:
    client.cookies.set("kz_session", str(uuid.uuid4()))
    response = await client.get("/api/auth/me")
    assert response.status_code == 401


async def test_inactive_user_returns_401(
    client: AsyncClient,
    create_test_user,
) -> None:
    _, pw = await create_test_user(email="inactive@test.com", is_active=False)
    response = await client.post(
        "/api/auth/login",
        json={"email": "inactive@test.com", "password": pw},
    )
    assert response.status_code == 401


async def _make_scoped_user(role: str, school_id) -> User:
    return User(
        email=f"scope-{role}@test.com",
        password_hash="x",  # noqa: S106
        role=role,
        school_id=school_id,
    )


@pytest.mark.anyio
async def test_is_super_admin_returns_true_only_for_super_admin_role() -> None:
    user = await _make_scoped_user(role="user", school_id=DEFAULT_SCHOOL_ID)
    admin = await _make_scoped_user(role="admin", school_id=DEFAULT_SCHOOL_ID)
    sa = await _make_scoped_user(role="super_admin", school_id=DEFAULT_SCHOOL_ID)
    assert is_super_admin(user) is False
    assert is_super_admin(admin) is False
    assert is_super_admin(sa) is True


@pytest.mark.anyio
async def test_scope_school_id_for_non_super_admin_ignores_query_param(
    db_session: AsyncSession,
) -> None:
    """Non-super-admin: school_id query param is ignored; returns current_user.school_id."""
    school_b = School(name="Schule B", short_name="SB")
    db_session.add(school_b)
    await db_session.flush()
    admin = await _make_scoped_user(role="admin", school_id=DEFAULT_SCHOOL_ID)

    # No param.
    result = await get_scope_school_id(user=admin, db=db_session, school_id=None)
    assert result == DEFAULT_SCHOOL_ID
    # Param pointing at other school — ignored.
    result = await get_scope_school_id(user=admin, db=db_session, school_id=school_b.id)
    assert result == DEFAULT_SCHOOL_ID


@pytest.mark.anyio
async def test_scope_school_id_for_super_admin_no_param_returns_home(
    db_session: AsyncSession,
) -> None:
    sa = await _make_scoped_user(role="super_admin", school_id=DEFAULT_SCHOOL_ID)
    result = await get_scope_school_id(user=sa, db=db_session, school_id=None)
    assert result == DEFAULT_SCHOOL_ID


@pytest.mark.anyio
async def test_scope_school_id_for_super_admin_with_other_school_returns_other(
    db_session: AsyncSession,
) -> None:
    school_b = School(name="Schule B 2", short_name="SB2")
    db_session.add(school_b)
    await db_session.flush()
    sa = await _make_scoped_user(role="super_admin", school_id=DEFAULT_SCHOOL_ID)
    result = await get_scope_school_id(user=sa, db=db_session, school_id=school_b.id)
    assert result == school_b.id


@pytest.mark.anyio
async def test_scope_school_id_for_super_admin_with_nonexistent_school_raises_404(
    db_session: AsyncSession,
) -> None:
    sa = await _make_scoped_user(role="super_admin", school_id=DEFAULT_SCHOOL_ID)
    with pytest.raises(HTTPException) as exc:
        await get_scope_school_id(user=sa, db=db_session, school_id=uuid.uuid4())
    assert exc.value.status_code == 404


@pytest.mark.anyio
async def test_require_super_admin_rejects_admin(
    db_session: AsyncSession,
) -> None:
    admin = await _make_scoped_user(role="admin", school_id=DEFAULT_SCHOOL_ID)
    with pytest.raises(HTTPException) as exc:
        await require_super_admin(user=admin)
    assert exc.value.status_code == 403


@pytest.mark.anyio
async def test_require_super_admin_accepts_super_admin(
    db_session: AsyncSession,
) -> None:
    sa = await _make_scoped_user(role="super_admin", school_id=DEFAULT_SCHOOL_ID)
    result = await require_super_admin(user=sa)
    assert result is sa
