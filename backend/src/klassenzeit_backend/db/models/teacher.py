"""Teacher ORM models."""

import uuid
from datetime import datetime

from sqlalchemy import Boolean, DateTime, ForeignKey, SmallInteger, String, func
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class Teacher(Base):
    """A teacher who can be assigned to lessons."""

    __tablename__ = "teachers"

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    first_name: Mapped[str] = mapped_column(String(100))
    last_name: Mapped[str] = mapped_column(String(100))
    short_code: Mapped[str] = mapped_column(String(10), unique=True)
    max_hours_per_week: Mapped[int] = mapped_column(SmallInteger)
    reserve_hours_per_week: Mapped[int] = mapped_column(
        SmallInteger, nullable=False, server_default="0"
    )
    is_active: Mapped[bool] = mapped_column(Boolean, server_default="true")
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )


class TeacherQualification(Base):
    """M:N join between Teacher and Subject."""

    __tablename__ = "teacher_qualifications"

    teacher_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("teachers.id"), primary_key=True)
    subject_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("subjects.id"), primary_key=True)


class TeacherAvailability(Base):
    """Per-time-block availability status for a teacher."""

    __tablename__ = "teacher_availabilities"

    teacher_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("teachers.id"), primary_key=True)
    time_block_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("time_blocks.id"), primary_key=True)
    status: Mapped[str] = mapped_column(String(16))
