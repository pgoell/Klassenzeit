"""WeekScheme and TimeBlock ORM models."""

import enum
import uuid
from datetime import datetime, time

from sqlalchemy import (
    DateTime,
    ForeignKey,
    SmallInteger,
    String,
    Text,
    Time,
    UniqueConstraint,
    func,
)
from sqlalchemy.dialects.postgresql import ENUM as PG_ENUM
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class TimeBlockKind(enum.StrEnum):
    """Kind of a time block: bookable lesson slot vs non-bookable break."""

    LESSON = "lesson"
    BREAK = "break"


class WeekScheme(Base):
    """An admin-defined weekly time grid."""

    __tablename__ = "week_schemes"
    __table_args__ = (UniqueConstraint("school_id", "name", name="uq_week_schemes_school_id_name"),)

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    school_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("schools.id"), nullable=False, index=True
    )
    name: Mapped[str] = mapped_column(String(100))
    description: Mapped[str | None] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )


class TimeBlock(Base):
    """A single period within a WeekScheme (e.g. Monday period 1, 08:00-08:45)."""

    __tablename__ = "time_blocks"
    __table_args__ = (UniqueConstraint("week_scheme_id", "day_of_week", "position"),)

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    week_scheme_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("week_schemes.id"), index=True)
    day_of_week: Mapped[int] = mapped_column(SmallInteger)
    position: Mapped[int] = mapped_column(SmallInteger)
    start_time: Mapped[time] = mapped_column(Time)
    end_time: Mapped[time] = mapped_column(Time)
    kind: Mapped[TimeBlockKind] = mapped_column(
        PG_ENUM(
            TimeBlockKind,
            name="time_block_kind",
            create_type=False,
            native_enum=True,
            values_callable=lambda enum_cls: [member.value for member in enum_cls],
        ),
        nullable=False,
        server_default=TimeBlockKind.LESSON.value,
        default=TimeBlockKind.LESSON,
    )
