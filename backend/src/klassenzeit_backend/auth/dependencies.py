"""FastAPI auth dependencies.

``get_current_user`` reads the ``kz_session`` cookie, looks up the
session in the DB, loads the user, and returns it. Raises 401 if
anything is missing or invalid.

``require_admin`` accepts admin OR super-admin users (super-admin is a
strict superset of admin within the currently scoped school).

``require_super_admin`` accepts only super-admin users; exported for
super-admin-only endpoints.

``get_scope_school_id`` resolves the per-request operating school from
``session.active_school_id``. Validation against accessible schools
runs at session-create (login) and at every switch (POST
``/auth/switch-school``); no per-request re-validation is performed.
"""

import uuid
from typing import Annotated

from fastapi import Cookie, Depends, HTTPException, status
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
    kz_session: Annotated[str | None, Cookie()] = None,
) -> uuid.UUID:
    """Resolve the per-request operating school from the user's session.

    Reads ``session.active_school_id``. The session was validated
    against accessible schools at creation (login) and at every switch
    via ``POST /auth/switch-school``, so no per-request re-validation
    is performed.

    The ``?school_id=<uuid>`` URL pattern that used to live here was
    superseded by the sidebar school picker (item 10c). Removing it
    keeps a single source of truth for the request scope.

    ``user`` is unused inside the body but is kept in the signature to
    drive the transitive ``require_admin`` dependency (which itself
    chains ``get_current_user``); FastAPI caches the chain so there is
    no duplicate DB load.
    """
    del user  # documented above; the dependency is for the admin gate
    if kz_session is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)
    try:
        session_id = uuid.UUID(kz_session)
    except ValueError:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED) from None
    session = await lookup_session(db, session_id)
    if session is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)
    return session.active_school_id


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
