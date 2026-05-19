"""Super-admin audit-log read endpoint."""

from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_super_admin
from klassenzeit_backend.auth.schemas.audit_log import (
    AuditLogEntryItem,
    AuditLogListResponse,
    AuditLogQuery,
)
from klassenzeit_backend.db.models.super_admin_audit_log import SuperAdminAuditLog
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session

router = APIRouter(prefix="/auth/admin", tags=["auth-admin"])


@router.get(
    "/audit-log",
    response_model=AuditLogListResponse,
    status_code=status.HTTP_200_OK,
)
async def list_audit_log(
    query: Annotated[AuditLogQuery, Depends()],
    _admin: Annotated[User, Depends(require_super_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> AuditLogListResponse:
    """List super-admin audited writes; newest first; filterable; paginated."""
    if query.from_ts is not None and query.to_ts is not None and query.from_ts > query.to_ts:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail="from_ts must be <= to_ts",
        )

    filters = []
    if query.actor_user_id is not None:
        filters.append(SuperAdminAuditLog.actor_user_id == query.actor_user_id)
    if query.target_school_id is not None:
        filters.append(SuperAdminAuditLog.target_school_id == query.target_school_id)
    if query.from_ts is not None:
        filters.append(SuperAdminAuditLog.ts >= query.from_ts)
    if query.to_ts is not None:
        filters.append(SuperAdminAuditLog.ts <= query.to_ts)

    rows_stmt = (
        select(SuperAdminAuditLog)
        .where(*filters)
        .order_by(SuperAdminAuditLog.ts.desc())
        .offset(query.skip)
        .limit(query.limit)
    )
    total_stmt = select(func.count()).select_from(SuperAdminAuditLog).where(*filters)

    rows_result = await db.execute(rows_stmt)
    total_result = await db.execute(total_stmt)

    items = [AuditLogEntryItem.model_validate(row) for row in rows_result.scalars().all()]
    total = int(total_result.scalar_one())

    return AuditLogListResponse(items=items, total=total)
