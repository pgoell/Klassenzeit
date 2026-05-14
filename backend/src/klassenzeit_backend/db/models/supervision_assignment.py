"""SupervisionAssignment: per-Hofpause supervisor assignment."""

import uuid
from datetime import datetime

from sqlalchemy import DateTime, ForeignKey, UniqueConstraint, func
from sqlalchemy.dialects.postgresql import UUID
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class SupervisionAssignment(Base):
    """Pairs a break-kind TimeBlock with the teacher supervising it.

    The solver writes one row per Hofpause per solve; the UNIQUE constraint
    on ``time_block_id`` enforces at most one supervisor per break.
    """

    __tablename__ = "supervision_assignments"
    __table_args__ = (
        UniqueConstraint("time_block_id", name="uq_supervision_assignments_time_block_id"),
    )

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=uuid.uuid4)
    time_block_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("time_blocks.id", ondelete="CASCADE"),
        nullable=False,
    )
    teacher_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("teachers.id", ondelete="CASCADE"),
        nullable=False,
    )
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), nullable=False
    )
