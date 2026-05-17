"""Lesson ORM model."""

import uuid
from datetime import datetime

import sqlalchemy as sa
from sqlalchemy import DateTime, ForeignKey, SmallInteger, func
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class Lesson(Base):
    """A concrete lesson assignment: subject + teacher + hours, served to one or more classes."""

    __tablename__ = "lessons"

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    school_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("schools.id"),
        nullable=False,
    )
    subject_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("subjects.id"))
    teacher_id: Mapped[uuid.UUID | None] = mapped_column(ForeignKey("teachers.id"), nullable=True)
    hours_per_week: Mapped[int] = mapped_column(SmallInteger)
    preferred_block_size: Mapped[int] = mapped_column(SmallInteger, server_default="1")
    pre_buffer_minutes: Mapped[int] = mapped_column(
        SmallInteger, nullable=False, server_default=sa.text("0")
    )
    post_buffer_minutes: Mapped[int] = mapped_column(
        SmallInteger, nullable=False, server_default=sa.text("0")
    )
    lesson_group_id: Mapped[uuid.UUID | None] = mapped_column(nullable=True, index=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )
