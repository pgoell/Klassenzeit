"""User-to-school membership (M:N) for multi-school access."""

import uuid
from datetime import datetime

from sqlalchemy import DateTime, ForeignKey, UniqueConstraint, func, text
from sqlalchemy.orm import Mapped, mapped_column

from klassenzeit_backend.db.base import Base


class UserSchoolMembership(Base):
    """Grants a user access to a school beyond their home school."""

    __tablename__ = "user_school_memberships"
    __table_args__ = (
        UniqueConstraint(
            "user_id", "school_id", name="uq_user_school_memberships_user_id_school_id"
        ),
    )

    id: Mapped[uuid.UUID] = mapped_column(
        primary_key=True,
        server_default=text("gen_random_uuid()"),
    )
    user_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("users.id", ondelete="CASCADE"),
        nullable=False,
        index=True,
    )
    school_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("schools.id", ondelete="CASCADE"),
        nullable=False,
        index=True,
    )
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        server_default=func.now(),
        nullable=False,
    )
