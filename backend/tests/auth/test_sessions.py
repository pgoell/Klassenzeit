"""Tests for session CRUD functions (DB integration)."""

import uuid
from datetime import UTC, datetime, timedelta

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.passwords import hash_password
from klassenzeit_backend.auth.sessions import (
    cleanup_expired_sessions,
    create_session,
    delete_session,
    delete_user_sessions,
    lookup_session,
)
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.session import UserSession
from klassenzeit_backend.db.models.user import User


async def _make_user(db: AsyncSession, email: str = "sess@test.com") -> User:
    user = User(
        email=email,
        password_hash=hash_password("testpassword123"),
        role="user",
        school_id=DEFAULT_SCHOOL_ID,
    )
    db.add(user)
    await db.flush()
    return user


async def test_create_session_returns_session(db_session: AsyncSession) -> None:
    user = await _make_user(db_session)
    session = await create_session(
        db_session, user.id, active_school_id=DEFAULT_SCHOOL_ID, ttl_days=14
    )
    assert session.user_id == user.id
    assert session.id is not None
    assert session.expires_at > datetime.now(UTC)


async def test_lookup_session_finds_valid(db_session: AsyncSession) -> None:
    user = await _make_user(db_session)
    created = await create_session(
        db_session, user.id, active_school_id=DEFAULT_SCHOOL_ID, ttl_days=14
    )
    found = await lookup_session(db_session, created.id)
    assert found is not None
    assert found.id == created.id


async def test_lookup_session_returns_none_for_expired(db_session: AsyncSession) -> None:
    user = await _make_user(db_session)
    session = UserSession(
        user_id=user.id,
        active_school_id=DEFAULT_SCHOOL_ID,
        expires_at=datetime.now(UTC) - timedelta(hours=1),
    )
    db_session.add(session)
    await db_session.flush()
    found = await lookup_session(db_session, session.id)
    assert found is None


async def test_lookup_session_returns_none_for_missing(db_session: AsyncSession) -> None:
    found = await lookup_session(db_session, uuid.uuid4())
    assert found is None


async def test_delete_session_removes_it(db_session: AsyncSession) -> None:
    user = await _make_user(db_session)
    session = await create_session(
        db_session, user.id, active_school_id=DEFAULT_SCHOOL_ID, ttl_days=14
    )
    await delete_session(db_session, session.id)
    found = await lookup_session(db_session, session.id)
    assert found is None


async def test_delete_user_sessions_removes_all(db_session: AsyncSession) -> None:
    user = await _make_user(db_session)
    await create_session(db_session, user.id, active_school_id=DEFAULT_SCHOOL_ID, ttl_days=14)
    await create_session(db_session, user.id, active_school_id=DEFAULT_SCHOOL_ID, ttl_days=14)
    await delete_user_sessions(db_session, user.id)
    result = await db_session.execute(select(UserSession).where(UserSession.user_id == user.id))
    assert result.scalars().all() == []


async def test_cleanup_expired_sessions(db_session: AsyncSession) -> None:
    user = await _make_user(db_session)
    # One expired
    expired = UserSession(
        user_id=user.id,
        active_school_id=DEFAULT_SCHOOL_ID,
        expires_at=datetime.now(UTC) - timedelta(hours=1),
    )
    db_session.add(expired)
    # One valid
    await create_session(db_session, user.id, active_school_id=DEFAULT_SCHOOL_ID, ttl_days=14)
    await db_session.flush()
    count = await cleanup_expired_sessions(db_session)
    assert count == 1
