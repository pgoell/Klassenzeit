"""Pydantic schemas for the super-admin audit-log read endpoint."""

import uuid
from datetime import datetime
from typing import Any

from pydantic import BaseModel, ConfigDict, Field


class AuditLogEntryItem(BaseModel):
    """One audited write, list-view shape (no JSONB blobs)."""

    model_config = ConfigDict(from_attributes=True)

    id: uuid.UUID
    ts: datetime
    actor_user_id: uuid.UUID | None
    actor_user_email: str
    target_school_id: uuid.UUID | None
    target_school_name: str | None
    request_id: str | None
    method: str
    route_template: str
    response_status: int


class AuditLogEntryDetail(AuditLogEntryItem):
    """Full audited write including JSONB payloads and truncation flag.

    ``request_body`` mirrors the ORM ``Mapped[dict | list | None]`` column;
    a top-level JSON array body must validate through. Sensitive keys are
    redacted server-side before serialization (see route module).
    """

    path_params: dict[str, Any]
    request_body: dict[str, Any] | list[Any] | None
    request_body_truncated: bool


class AuditLogListResponse(BaseModel):
    """Paginated audit-log list response."""

    items: list[AuditLogEntryItem]
    total: int


class AuditLogQuery(BaseModel):
    """Query params for the audit-log list endpoint.

    The ``from_ts`` / ``to_ts`` range is validated in the route handler so the
    422 response surfaces as a FastAPI request-validation error rather than a
    Pydantic ``ValidationError`` escaping the dependency-resolution layer.
    """

    model_config = ConfigDict(extra="forbid")

    skip: int = Field(default=0, ge=0)
    limit: int = Field(default=50, ge=1, le=200)
    actor_user_id: uuid.UUID | None = None
    target_school_id: uuid.UUID | None = None
    from_ts: datetime | None = None
    to_ts: datetime | None = None
