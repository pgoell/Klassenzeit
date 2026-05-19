"""Super-admin audit-log read endpoint."""

import uuid
from typing import Annotated, Any

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_super_admin
from klassenzeit_backend.auth.schemas.audit_log import (
    AuditLogEntryDetail,
    AuditLogEntryItem,
    AuditLogListResponse,
    AuditLogQuery,
)
from klassenzeit_backend.db.models.super_admin_audit_log import SuperAdminAuditLog
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session

router = APIRouter(prefix="/auth/admin", tags=["auth-admin"])

_REDACTED_KEYS: frozenset[str] = frozenset(
    {"password", "password_hash", "token", "secret", "pin", "pin_code", "api_key"}
)
_REDACTED_VALUE = "[REDACTED]"


def _redact_sensitive(value: Any) -> Any:
    """Walk the captured request body and redact sensitive values.

    Lowercase the candidate key for the lookup; no separator
    normalization (the codebase's schemas are snake_case).
    """
    if isinstance(value, dict):
        return {
            k: (_REDACTED_VALUE if k.lower() in _REDACTED_KEYS else _redact_sensitive(v))
            for k, v in value.items()
        }
    if isinstance(value, list):
        return [_redact_sensitive(item) for item in value]
    return value


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


@router.get(
    "/audit-log/{audit_log_id}",
    response_model=AuditLogEntryDetail,
    status_code=status.HTTP_200_OK,
)
async def read_audit_log_detail(
    audit_log_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_super_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> AuditLogEntryDetail:
    """Return one audited write with ``path_params`` and ``request_body``.

    Sensitive keys in ``request_body`` are redacted server-side.
    """
    row = await db.get(SuperAdminAuditLog, audit_log_id)
    if row is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="audit log row not found")
    return AuditLogEntryDetail(
        id=row.id,
        ts=row.ts,
        actor_user_id=row.actor_user_id,
        actor_user_email=row.actor_user_email,
        target_school_id=row.target_school_id,
        target_school_name=row.target_school_name,
        request_id=row.request_id,
        method=row.method,
        route_template=row.route_template,
        response_status=row.response_status,
        path_params=dict(row.path_params or {}),
        request_body=_redact_sensitive(row.request_body),
        request_body_truncated=row.request_body_truncated,
    )
