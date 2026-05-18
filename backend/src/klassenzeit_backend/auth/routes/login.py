"""Login and logout routes."""

import uuid
from datetime import UTC, datetime
from typing import TYPE_CHECKING, Annotated

from fastapi import APIRouter, Cookie, Depends, HTTPException, Request, Response, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import get_current_user
from klassenzeit_backend.auth.passwords import verify_password
from klassenzeit_backend.auth.schemas.login import LoginRequest
from klassenzeit_backend.auth.sessions import create_session, delete_session
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session

if TYPE_CHECKING:
    from klassenzeit_backend.auth.rate_limit import LoginRateLimiter
    from klassenzeit_backend.core.settings import Settings

router = APIRouter(prefix="/auth", tags=["auth"])


@router.post("/login", status_code=status.HTTP_204_NO_CONTENT)
async def login(
    body: LoginRequest,
    request: Request,
    response: Response,
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Authenticate with email/password and set a session cookie."""
    settings: Settings = request.app.state.settings
    rate_limiter: LoginRateLimiter = request.app.state.rate_limiter
    email = body.email.lower()

    if rate_limiter.is_locked(email):
        retry_after = rate_limiter.seconds_until_unlock(email)
        raise HTTPException(
            status_code=status.HTTP_429_TOO_MANY_REQUESTS,
            headers={"Retry-After": str(retry_after)},
        )

    result = await db.execute(select(User).where(User.email == email))
    user = result.scalar_one_or_none()

    if user is None or not verify_password(body.password, user.password_hash):
        rate_limiter.record_failure(email)
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)

    if not user.is_active:
        rate_limiter.record_failure(email)
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)

    rate_limiter.reset(email)

    user.last_login_at = datetime.now(UTC)
    session = await create_session(
        db,
        user.id,
        active_school_id=user.school_id,
        ttl_days=settings.session_ttl_days,
    )
    await db.commit()

    response.set_cookie(
        key="kz_session",
        value=str(session.id),
        httponly=True,
        samesite="lax",
        secure=settings.cookie_secure,
        domain=settings.cookie_domain,
        path="/",
        max_age=settings.session_ttl_days * 86400,
    )


@router.post("/logout", status_code=status.HTTP_204_NO_CONTENT)
async def logout(
    request: Request,
    response: Response,
    db: Annotated[AsyncSession, Depends(get_session)],
    _user: Annotated[User, Depends(get_current_user)],
    kz_session: Annotated[str | None, Cookie()] = None,
) -> None:
    """Delete the current session and clear the session cookie."""
    settings: Settings = request.app.state.settings

    if kz_session:
        await delete_session(db, uuid.UUID(kz_session))
        await db.commit()

    response.delete_cookie(
        key="kz_session",
        httponly=True,
        samesite="lax",
        secure=settings.cookie_secure,
        domain=settings.cookie_domain,
        path="/",
    )
