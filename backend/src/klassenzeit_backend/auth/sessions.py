"""Session CRUD operations.

Functions in this module add/modify/delete objects but do NOT commit.
The caller (route handler) is responsible for committing the
transaction.
"""

import uuid
from datetime import UTC, datetime, timedelta

from sqlalchemy import delete, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.session import UserSession


async def create_session(
    db: AsyncSession,
    user_id: uuid.UUID,
    *,
    active_school_id: uuid.UUID,
    ttl_days: int = 14,
) -> UserSession:
    """Create a new session for the given user with an initial active school."""
    session = UserSession(
        user_id=user_id,
        active_school_id=active_school_id,
        expires_at=datetime.now(UTC) + timedelta(days=ttl_days),
    )
    db.add(session)
    await db.flush()
    return session


async def lookup_session(
    db: AsyncSession,
    session_id: uuid.UUID,
) -> UserSession | None:
    """Find a non-expired session by ID."""
    result = await db.execute(
        select(UserSession).where(
            UserSession.id == session_id,
            UserSession.expires_at > datetime.now(UTC),
        )
    )
    return result.scalar_one_or_none()


async def delete_session(db: AsyncSession, session_id: uuid.UUID) -> None:
    """Delete a single session."""
    await db.execute(delete(UserSession).where(UserSession.id == session_id))
    await db.flush()


async def delete_user_sessions(
    db: AsyncSession,
    user_id: uuid.UUID,
    *,
    exclude_session_id: uuid.UUID | None = None,
) -> None:
    """Delete all sessions for a user, optionally keeping one."""
    stmt = delete(UserSession).where(UserSession.user_id == user_id)
    if exclude_session_id is not None:
        stmt = stmt.where(UserSession.id != exclude_session_id)
    await db.execute(stmt)
    await db.flush()


async def cleanup_expired_sessions(db: AsyncSession) -> int:
    """Delete all expired sessions. Returns the number deleted."""
    result = await db.execute(
        delete(UserSession).where(
            UserSession.expires_at <= datetime.now(UTC),
        )
    )
    await db.flush()
    # rowcount is available on CursorResult returned by DML statements;
    # ty sees Result[Any] (the base class), so we access it via getattr.
    return int(getattr(result, "rowcount", 0))
