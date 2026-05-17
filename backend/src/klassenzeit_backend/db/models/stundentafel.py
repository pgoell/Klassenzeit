"""Stundentafel (curriculum template) ORM models."""

import enum
import uuid
from datetime import datetime

from sqlalchemy import DateTime, ForeignKey, SmallInteger, String, UniqueConstraint, func
from sqlalchemy.dialects.postgresql import ENUM as PG_ENUM
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class SchoolType(enum.StrEnum):
    """Hessen Schulform classification on a curriculum (Stundentafel)."""

    GRUNDSCHULE = "Grundschule"
    HAUPTSCHULE = "Hauptschule"
    REALSCHULE = "Realschule"
    GYMNASIUM = "Gymnasium"
    GESAMTSCHULE = "Gesamtschule"


class Stundentafel(Base):
    """A reusable curriculum template (e.g. 'Gymnasium Klasse 5 Latein')."""

    __tablename__ = "stundentafeln"
    __table_args__ = (
        UniqueConstraint("school_id", "name", name="uq_stundentafeln_school_id_name"),
    )

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    name: Mapped[str] = mapped_column(String(100))
    grade_level: Mapped[int] = mapped_column(SmallInteger)
    school_type: Mapped[SchoolType] = mapped_column(
        PG_ENUM(
            SchoolType,
            name="school_type",
            create_type=False,
            native_enum=True,
            values_callable=lambda enum_cls: [member.value for member in enum_cls],
        ),
        nullable=False,
        server_default=SchoolType.GRUNDSCHULE.value,
        default=SchoolType.GRUNDSCHULE,
    )
    school_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("schools.id"),
        nullable=False,
        index=True,
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )


class StundentafelEntry(Base):
    """One subject-hours pair within a Stundentafel."""

    __tablename__ = "stundentafel_entries"
    __table_args__ = (UniqueConstraint("stundentafel_id", "subject_id"),)

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    stundentafel_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("stundentafeln.id"), index=True)
    subject_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("subjects.id"))
    hours_per_week: Mapped[int] = mapped_column(SmallInteger)
    preferred_block_size: Mapped[int] = mapped_column(SmallInteger, server_default="1")
