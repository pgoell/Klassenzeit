"""Switch the active school for the current session.

``POST /auth/switch-school`` mutates ``session.active_school_id`` so the
next request scopes to the new school. Validation runs here at switch
time (and at login, when the session is created); ``get_scope_school_id``
trusts the stored value and does not re-validate per request.

The endpoint returns the full ``/auth/me`` payload so the frontend can
update the sidebar and any user-state caches in one round trip.
"""

import uuid
from typing import Annotated

from fastapi import APIRouter, Cookie, Depends, HTTPException, status
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import (
    get_current_user,
    is_accessible_school,
    load_accessible_schools,
)
from klassenzeit_backend.auth.schemas.me import AccessibleSchool, MeResponse
from klassenzeit_backend.auth.schemas.switch_school import SwitchSchoolRequest
from klassenzeit_backend.auth.sessions import lookup_session
from klassenzeit_backend.db.models.school import School
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session

router = APIRouter(prefix="/auth", tags=["auth"])


@router.post("/switch-school")
async def switch_school(
    body: SwitchSchoolRequest,
    user: Annotated[User, Depends(get_current_user)],
    db: Annotated[AsyncSession, Depends(get_session)],
    kz_session: Annotated[str | None, Cookie()] = None,
) -> MeResponse:
    """Set the active school for the cookie session, then return /auth/me payload.

    Returns 404 when the school row does not exist (regardless of role)
    and 403 when the school exists but the user lacks access; the
    existence-check fires first so callers can distinguish the two.
    """
    if kz_session is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)
    session = await lookup_session(db, uuid.UUID(kz_session))
    if session is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)

    target = await db.get(School, body.school_id)
    if target is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    if not await is_accessible_school(db, user, body.school_id):
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN)

    session.active_school_id = body.school_id
    await db.commit()
    await db.refresh(session)

    home_school = await db.get(School, user.school_id)
    if home_school is None:
        raise RuntimeError(
            f"User {user.id} has school_id={user.school_id} but School row is missing"
        )
    accessible = await load_accessible_schools(db, user)

    return MeResponse(
        id=user.id,
        email=user.email,
        role=user.role,
        force_password_change=user.force_password_change,
        school_id=user.school_id,
        school_name=home_school.name,
        active_school_id=target.id,
        active_school_name=target.name,
        accessible_schools=[AccessibleSchool(id=s.id, name=s.name) for s in accessible],
    )
