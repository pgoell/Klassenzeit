"""SchoolClass ORM model."""

import uuid
from datetime import datetime

from sqlalchemy import DateTime, ForeignKey, Integer, SmallInteger, String, func
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class SchoolClass(Base):
    """A class/group of students (e.g. '5a', '10b')."""

    __tablename__ = "school_classes"

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    name: Mapped[str] = mapped_column(String(20), unique=True)
    grade_level: Mapped[int] = mapped_column(SmallInteger)
    stundentafel_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("stundentafeln.id"))
    week_scheme_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("week_schemes.id"))
    home_room_id: Mapped[uuid.UUID | None] = mapped_column(
        ForeignKey("rooms.id", ondelete="SET NULL"), nullable=True
    )
    max_lessons_per_day: Mapped[int | None] = mapped_column(Integer, nullable=True, default=None)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )
