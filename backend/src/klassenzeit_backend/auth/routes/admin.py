"""Admin user management routes."""

import logging
import uuid
from typing import TYPE_CHECKING, Annotated

from fastapi import APIRouter, Depends, HTTPException, Request, status
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin, require_super_admin
from klassenzeit_backend.auth.passwords import (
    PasswordValidationError,
    hash_password,
    validate_password,
)
from klassenzeit_backend.auth.schemas.admin import (
    CreateUserRequest,
    MembershipGrantRequest,
    MembershipListItem,
    MembershipResponse,
    ResetPasswordRequest,
    SetRoleRequest,
    UserListItem,
    UserResponse,
)
from klassenzeit_backend.auth.sessions import delete_user_sessions
from klassenzeit_backend.db.models.school import School
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.models.user_school_membership import UserSchoolMembership
from klassenzeit_backend.db.session import get_session

if TYPE_CHECKING:
    from klassenzeit_backend.core.settings import Settings

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/auth/admin", tags=["auth-admin"])


@router.post("/users", status_code=status.HTTP_201_CREATED)
async def admin_create_user(
    body: CreateUserRequest,
    request: Request,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> UserResponse:
    """Create a new user account. Requires admin role."""
    settings: Settings = request.app.state.settings
    email = body.email.lower()

    existing = await db.execute(select(User).where(User.email == email))
    if existing.scalar_one_or_none() is not None:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A user with this email already exists",
        )

    try:
        validate_password(body.password, min_length=settings.password_min_length)
    except PasswordValidationError as exc:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail=str(exc),
        ) from exc

    user = User(
        email=email,
        password_hash=hash_password(body.password),
        role=body.role,
        school_id=_admin.school_id,
    )
    db.add(user)
    await db.commit()

    return UserResponse(id=user.id, email=user.email, role=user.role)


@router.get("/users")
async def admin_list_users(
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
    active: bool | None = None,
) -> list[UserListItem]:
    """List all users, optionally filtered by active status."""
    stmt = select(User, School.name).join(School, School.id == User.school_id)
    if active is not None:
        stmt = stmt.where(User.is_active == active)
    result = await db.execute(stmt.order_by(User.created_at))
    return [
        UserListItem(
            id=u.id,
            email=u.email,
            role=u.role,
            is_active=u.is_active,
            last_login_at=u.last_login_at,
            school_id=u.school_id,
            school_name=school_name,
        )
        for u, school_name in result.all()
    ]


async def _get_target_user(db: AsyncSession, user_id: uuid.UUID) -> User:
    """Load a user by ID or raise 404."""
    user = await db.get(User, user_id)
    if user is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return user


async def _get_school_or_404(db: AsyncSession, school_id: uuid.UUID) -> School:
    """Load a school by ID or raise 404."""
    school = await db.get(School, school_id)
    if school is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return school


@router.post("/users/{user_id}/reset-password", status_code=status.HTTP_204_NO_CONTENT)
async def admin_reset_password(
    user_id: uuid.UUID,
    body: ResetPasswordRequest,
    request: Request,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Reset a user's password and force a password change on next login."""
    settings: Settings = request.app.state.settings
    try:
        validate_password(body.new_password, min_length=settings.password_min_length)
    except PasswordValidationError as exc:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail=str(exc),
        ) from exc

    user = await _get_target_user(db, user_id)
    user.password_hash = hash_password(body.new_password)
    user.force_password_change = True
    await delete_user_sessions(db, user.id)
    await db.commit()


@router.post("/users/{user_id}/deactivate", status_code=status.HTTP_204_NO_CONTENT)
async def admin_deactivate_user(
    user_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Deactivate a user and invalidate all their sessions."""
    user = await _get_target_user(db, user_id)
    user.is_active = False
    await delete_user_sessions(db, user.id)
    await db.commit()


@router.post("/users/{user_id}/activate", status_code=status.HTTP_204_NO_CONTENT)
async def admin_activate_user(
    user_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Re-activate a deactivated user account."""
    user = await _get_target_user(db, user_id)
    user.is_active = True
    await db.commit()


@router.post("/users/{user_id}/role")
async def admin_set_user_role(
    user_id: uuid.UUID,
    body: SetRoleRequest,
    _admin: Annotated[User, Depends(require_super_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> UserResponse:
    """Set the target user's role. Requires super-admin.

    Idempotent when the new role matches the current role. Enforces a
    last-active-super-admin guard: refuses with 409
    ``{"code": "last_super_admin"}`` when the change would drop the
    count of active super-admins below 1. Invalidates the target's
    sessions on actual change so a demoted super-admin loses any
    previously-set ``active_school_id`` immediately.
    """
    target = await _get_target_user(db, user_id)
    prev_role = target.role
    new_role = body.role

    if prev_role == new_role:
        return UserResponse(id=target.id, email=target.email, role=target.role)

    if prev_role == "super_admin" and target.is_active and new_role != "super_admin":
        count_stmt = select(func.count(User.id)).where(
            User.role == "super_admin",
            User.is_active.is_(True),
        )
        active_super_admin_count = (await db.execute(count_stmt)).scalar_one()
        if active_super_admin_count <= 1:
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail={"code": "last_super_admin"},
            )

    target.role = new_role
    await delete_user_sessions(db, target.id)
    await db.commit()
    await db.refresh(target)

    logger.info(
        "admin.user_role.change",
        extra={
            "actor_id": str(_admin.id),
            "target_id": str(target.id),
            "from_role": prev_role,
            "to_role": new_role,
            "sessions_invalidated": True,
        },
    )

    return UserResponse(id=target.id, email=target.email, role=target.role)


@router.get("/users/{user_id}/memberships")
async def admin_list_user_memberships(
    user_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_super_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> list[MembershipListItem]:
    """List the explicit school memberships for a user. Requires super-admin."""
    await _get_target_user(db, user_id)
    result = await db.execute(
        select(UserSchoolMembership.school_id, School.name)
        .join(School, School.id == UserSchoolMembership.school_id)
        .where(UserSchoolMembership.user_id == user_id)
        .order_by(School.name)
    )
    return [MembershipListItem(school_id=row[0], school_name=row[1]) for row in result.all()]


@router.post("/users/{user_id}/memberships", status_code=status.HTTP_201_CREATED)
async def admin_grant_user_membership(
    user_id: uuid.UUID,
    body: MembershipGrantRequest,
    _admin: Annotated[User, Depends(require_super_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> MembershipResponse:
    """Grant a school membership to a user. Requires super-admin.

    Rejects with 409 ``{"code": "membership_redundant_home_school"}`` when
    the school is already the user's home school, and 409
    ``{"code": "membership_exists"}`` when a membership row already exists.
    Does NOT invalidate the target's sessions (grant is purely additive).
    """
    target = await _get_target_user(db, user_id)
    school = await _get_school_or_404(db, body.school_id)

    if body.school_id == target.school_id:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail={"code": "membership_redundant_home_school"},
        )

    existing = await db.execute(
        select(UserSchoolMembership).where(
            UserSchoolMembership.user_id == user_id,
            UserSchoolMembership.school_id == body.school_id,
        )
    )
    if existing.scalar_one_or_none() is not None:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail={"code": "membership_exists"},
        )

    db.add(UserSchoolMembership(user_id=user_id, school_id=body.school_id))
    await db.commit()

    logger.info(
        "admin.user_membership.grant",
        extra={
            "actor_id": str(_admin.id),
            "target_id": str(target.id),
            "school_id": str(body.school_id),
            "sessions_invalidated": False,
        },
    )

    return MembershipResponse(
        user_id=target.id,
        school_id=school.id,
        school_name=school.name,
    )


@router.delete(
    "/users/{user_id}/memberships/{school_id}",
    status_code=status.HTTP_204_NO_CONTENT,
)
async def admin_revoke_user_membership(
    user_id: uuid.UUID,
    school_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_super_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Revoke a school membership from a user. Requires super-admin.

    Returns 404 when the target user, target school, or membership row is
    absent. On success, invalidates every session for the target so a
    cached ``session.active_school_id`` cannot outlive the revoke.
    """
    target = await _get_target_user(db, user_id)
    await _get_school_or_404(db, school_id)

    result = await db.execute(
        select(UserSchoolMembership).where(
            UserSchoolMembership.user_id == user_id,
            UserSchoolMembership.school_id == school_id,
        )
    )
    membership = result.scalar_one_or_none()
    if membership is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)

    await db.delete(membership)
    await delete_user_sessions(db, target.id)
    await db.commit()

    logger.info(
        "admin.user_membership.revoke",
        extra={
            "actor_id": str(_admin.id),
            "target_id": str(target.id),
            "school_id": str(school_id),
            "sessions_invalidated": True,
        },
    )
