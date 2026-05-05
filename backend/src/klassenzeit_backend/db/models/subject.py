"""Subject ORM model."""

import uuid
from datetime import datetime

from sqlalchemy import DateTime, Integer, String, func, text
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class Subject(Base):
    """A school subject (e.g. Mathematik, Deutsch, Sport)."""

    __tablename__ = "subjects"

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    name: Mapped[str] = mapped_column(String(100), unique=True)
    short_name: Mapped[str] = mapped_column(String(10), unique=True)
    color: Mapped[str] = mapped_column(String(16))
    prefer_early_period: Mapped[int] = mapped_column(
        Integer, nullable=False, default=0, server_default=text("0")
    )
    prefer_late_period: Mapped[int] = mapped_column(
        Integer, nullable=False, default=0, server_default=text("0")
    )
    avoid_first_period: Mapped[int] = mapped_column(
        Integer, nullable=False, default=0, server_default=text("0")
    )
    avoid_last_period: Mapped[int] = mapped_column(
        Integer, nullable=False, default=0, server_default=text("0")
    )
    max_hours_per_day: Mapped[int] = mapped_column(
        Integer, nullable=False, default=2, server_default=text("2")
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )
