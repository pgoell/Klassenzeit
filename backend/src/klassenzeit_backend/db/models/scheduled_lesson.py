"""ScheduledLesson ORM model: one persisted placement of a lesson-hour."""

import uuid
from datetime import datetime

from sqlalchemy import DateTime, ForeignKey, func, text
from sqlalchemy.dialects.postgresql import ENUM as PG_ENUM
from sqlalchemy.dialects.postgresql import UUID
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base
from klassenzeit_backend.db.models.pin_kind import PinKind


class ScheduledLesson(Base):
    """A lesson-hour pinned to a (time_block, room) slot by the solver.

    Composite PK is ``(lesson_id, time_block_id)``: a given lesson cannot
    legitimately occupy the same time block twice. ``room_id`` is a dependent
    attribute of the pairing, not part of the key.

    All FKs use ``ON DELETE CASCADE`` because an orphan placement has no
    user-facing meaning. The next solve rebuilds whatever placements are still
    consistent with the updated schema.

    ``pin_kind`` flags placements the user has manually fixed. ``HARD`` pins
    survive re-solves verbatim; ``SOFT`` pins enter the LAHC objective as a
    per-placement penalty axis (Task 2). ``None`` means unpinned. See ADR 0042.

    School-scoped via ``school_id`` for multi-tenant isolation (ADR 0045).

    ``teacher_id`` records the teacher the solver picked for this placement
    (item 65). For lessons with ``Lesson.teacher_id`` pinned (pin-only
    semantics per item 63), the solver pick equals the pin; otherwise the
    solver picks among the per-Lesson ``teacher_candidates`` set per ADR 0036.
    """

    __tablename__ = "scheduled_lessons"

    lesson_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("lessons.id", ondelete="CASCADE"), primary_key=True
    )
    time_block_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("time_blocks.id", ondelete="CASCADE"), primary_key=True
    )
    room_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("rooms.id", ondelete="CASCADE"))
    teacher_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("teachers.id", ondelete="CASCADE"), nullable=False
    )
    school_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("schools.id"),
        nullable=False,
        index=True,
        server_default=text("'00000000-0000-0000-0000-000000000001'::uuid"),
    )
    pin_kind: Mapped[PinKind | None] = mapped_column(
        PG_ENUM(
            PinKind,
            name="pin_kind",
            create_type=False,
            native_enum=True,
            values_callable=lambda enum_cls: [member.value for member in enum_cls],
        ),
        nullable=True,
        server_default=None,
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
