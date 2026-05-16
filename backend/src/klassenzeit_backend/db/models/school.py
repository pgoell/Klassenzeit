"""School ORM model for multi-tenant scoping."""

import uuid
from datetime import datetime

from sqlalchemy import DateTime, String, func, text
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base

DEFAULT_SCHOOL_ID = uuid.UUID("00000000-0000-0000-0000-000000000001")


class School(Base):
    """A tenant school. Every user and aggregate root belongs to exactly one school."""

    __tablename__ = "schools"

    id: Mapped[uuid.UUID] = mapped_column(
        primary_key=True,
        server_default=text("gen_random_uuid()"),
    )
    name: Mapped[str] = mapped_column(String(120), unique=True)
    short_name: Mapped[str | None] = mapped_column(String(20), nullable=True, unique=True)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        server_default=func.now(),
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        server_default=func.now(),
        onupdate=func.now(),
    )
