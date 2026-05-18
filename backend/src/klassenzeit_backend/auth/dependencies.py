"""FastAPI auth dependencies.

``get_current_user`` reads the ``kz_session`` cookie, looks up the
session in the DB, loads the user, and returns it. Raises 401 if
anything is missing or invalid.

``require_admin`` accepts admin OR super-admin users (super-admin is a
strict superset of admin within the currently scoped school).

``require_super_admin`` accepts only super-admin users; exported for
super-admin-only endpoints.

``get_scope_school_id`` resolves the per-request operating school. For
non-super-admin users the ``school_id`` query parameter is ignored and
the user's home school is returned. For super-admin users the parameter
selects the operating school (404 if it points at a nonexistent row);
absent, the home school is returned.
"""

import uuid
from typing import Annotated

from fastapi import Cookie, Depends, HTTPException, Query, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.sessions import lookup_session
from klassenzeit_backend.db.models.school import School
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.models.user_school_membership import UserSchoolMembership
from klassenzeit_backend.db.session import get_session


async def get_current_user(
    db: Annotated[AsyncSession, Depends(get_session)],
    kz_session: Annotated[str | None, Cookie()] = None,
) -> User:
    """Return the authenticated user or raise 401."""
    if kz_session is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)

    try:
        session_id = uuid.UUID(kz_session)
    except ValueError:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED) from None

    session = await lookup_session(db, session_id)
    if session is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)

    user = await db.get(User, session.user_id)
    if user is None or not user.is_active:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)

    return user


def is_super_admin(user: User) -> bool:
    """True if the user's role is ``super_admin``."""
    return user.role == "super_admin"


async def require_admin(
    user: Annotated[User, Depends(get_current_user)],
) -> User:
    """Return the user if their role is ``admin`` or ``super_admin``; else raise 403."""
    if user.role not in ("admin", "super_admin"):
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN)
    return user


async def require_super_admin(
    user: Annotated[User, Depends(get_current_user)],
) -> User:
    """Return the user if their role is ``super_admin``; else raise 403."""
    if not is_super_admin(user):
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN)
    return user


async def get_scope_school_id(
    user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
    school_id: Annotated[uuid.UUID | None, Query()] = None,
) -> uuid.UUID:
    """Resolve the per-request operating school.

    Non-super-admin users: the ``school_id`` query parameter is ignored
    and ``user.school_id`` (the user's home school) is returned. The
    admin gate runs via ``require_admin``.

    Super-admin users: if ``school_id`` is provided, the row is loaded
    (404 if absent) and that id is returned. If not provided, the home
    school is returned.

    Never mutates ``user``; ``user.school_id`` stays as the authenticated
    user's home school for audit purposes. The returned UUID is the
    per-request operating scope.
    """
    if not is_super_admin(user):
        return user.school_id
    if school_id is None:
        return user.school_id
    target = await db.get(School, school_id)
    if target is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return school_id


async def _load_membership_school_ids(
    db: AsyncSession,
    user_id: uuid.UUID,
) -> set[uuid.UUID]:
    """Return the set of school_ids in ``user_school_memberships`` for ``user_id``."""
    result = await db.execute(
        select(UserSchoolMembership.school_id).where(UserSchoolMembership.user_id == user_id)
    )
    return set(result.scalars().all())


async def load_accessible_schools(
    db: AsyncSession,
    user: User,
) -> list[School]:
    """Return all schools the user can access.

    Super-admins see every school. Regular users see their home school
    plus any explicit memberships.
    """
    if is_super_admin(user):
        result = await db.execute(select(School).order_by(School.name))
        return list(result.scalars().all())

    accessible_ids: set[uuid.UUID] = {user.school_id} | (
        await _load_membership_school_ids(db, user.id)
    )
    result = await db.execute(
        select(School).where(School.id.in_(accessible_ids)).order_by(School.name)
    )
    return list(result.scalars().all())


async def is_accessible_school(
    db: AsyncSession,
    user: User,
    school_id: uuid.UUID,
) -> bool:
    """True if ``user`` can operate within ``school_id``.

    Super-admins may operate in any *existing* school. Regular users
    are limited to their home school plus explicit memberships.
    """
    if is_super_admin(user):
        target = await db.get(School, school_id)
        return target is not None

    if school_id == user.school_id:
        return True
    membership_school_ids = await _load_membership_school_ids(db, user.id)
    return school_id in membership_school_ids
