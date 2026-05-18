"""Audit log of super-admin cross-school writes.

One row is inserted by ``SuperAdminAuditMiddleware`` for every write where
the actor's authorization derives from ``is_super_admin`` elevation
(target school is not in the user's home + memberships) or where the route
is under ``/schools``. Snapshot columns (``actor_user_email``,
``target_school_name``) survive deletion of the referenced user / school
via ``ON DELETE SET NULL``.
"""

import uuid
from datetime import datetime
from typing import Any

from sqlalchemy import ForeignKey, Index, SmallInteger, Text, func, text
from sqlalchemy.dialects.postgresql import JSONB, TIMESTAMP, UUID
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class SuperAdminAuditLog(Base):
    """One row per super-admin write where elevation was actually used."""

    __tablename__ = "super_admin_audit_log"
    __table_args__ = (
        Index("idx_audit_log_ts", text("ts DESC")),
        Index("idx_audit_log_actor", "actor_user_id", text("ts DESC")),
        Index("idx_audit_log_target", "target_school_id", text("ts DESC")),
    )

    id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        primary_key=True,
        server_default=func.gen_random_uuid(),
    )
    ts: Mapped[datetime] = mapped_column(
        TIMESTAMP(timezone=True),
        nullable=False,
        server_default=func.now(),
    )
    actor_user_id: Mapped[uuid.UUID | None] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("users.id", ondelete="SET NULL"),
        nullable=True,
    )
    actor_user_email: Mapped[str] = mapped_column(Text, nullable=False)
    target_school_id: Mapped[uuid.UUID | None] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("schools.id", ondelete="SET NULL"),
        nullable=True,
    )
    target_school_name: Mapped[str | None] = mapped_column(Text, nullable=True)
    request_id: Mapped[str | None] = mapped_column(Text, nullable=True)
    method: Mapped[str] = mapped_column(Text, nullable=False)
    route_template: Mapped[str] = mapped_column(Text, nullable=False)
    path_params: Mapped[dict[str, Any]] = mapped_column(
        JSONB,
        nullable=False,
        server_default=text("'{}'::jsonb"),
    )
    request_body: Mapped[dict[str, Any] | list[Any] | None] = mapped_column(JSONB, nullable=True)
    request_body_truncated: Mapped[bool] = mapped_column(
        nullable=False, server_default=text("false")
    )
    response_status: Mapped[int] = mapped_column(SmallInteger, nullable=False)
