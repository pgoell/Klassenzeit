"""Room ORM models."""

import uuid
from datetime import datetime

import sqlalchemy as sa
from sqlalchemy import Boolean, DateTime, ForeignKey, SmallInteger, String, func
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class Room(Base):
    """A physical room (classroom, lab, gym, pool, etc.)."""

    __tablename__ = "rooms"

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    name: Mapped[str] = mapped_column(String(100), unique=True)
    short_name: Mapped[str] = mapped_column(String(20), unique=True)
    capacity: Mapped[int | None] = mapped_column(SmallInteger, nullable=True)
    is_external: Mapped[bool] = mapped_column(
        Boolean, nullable=False, server_default=sa.text("false")
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )


class RoomSubjectSuitability(Base):
    """M:N join between Room and Subject for suitability rules."""

    __tablename__ = "room_subject_suitabilities"

    room_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("rooms.id", ondelete="CASCADE"), primary_key=True
    )
    subject_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("subjects.id"), primary_key=True)


class RoomAvailability(Base):
    """Whitelist of time blocks when a room is available."""

    __tablename__ = "room_availabilities"

    room_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("rooms.id", ondelete="CASCADE"), primary_key=True
    )
    time_block_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("time_blocks.id"), primary_key=True)
