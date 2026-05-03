"""ScheduledLesson ORM model: one persisted placement of a lesson-hour."""

import uuid
from datetime import datetime

from sqlalchemy import Boolean, DateTime, ForeignKey, func, text
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class ScheduledLesson(Base):
    """A lesson-hour pinned to a (time_block, room) slot by the solver.

    Composite PK is ``(lesson_id, time_block_id)``: a given lesson cannot
    legitimately occupy the same time block twice. ``room_id`` is a dependent
    attribute of the pairing, not part of the key.

    All three FKs use ``ON DELETE CASCADE`` because an orphan placement has no
    user-facing meaning. The next solve rebuilds whatever placements are still
    consistent with the updated schema.

    ``pinned`` flags placements that the user has manually fixed. The solver
    treats pinned placements as immovable on subsequent runs (Sprint C).
    """

    __tablename__ = "scheduled_lessons"

    lesson_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("lessons.id", ondelete="CASCADE"), primary_key=True
    )
    time_block_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("time_blocks.id", ondelete="CASCADE"), primary_key=True
    )
    room_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("rooms.id", ondelete="CASCADE"))
    pinned: Mapped[bool] = mapped_column(Boolean, nullable=False, server_default=text("false"))
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
