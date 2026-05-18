"""Session model for cookie-based authentication.

Named ``UserSession`` to avoid confusion with SQLAlchemy's
``AsyncSession``. The table name remains ``sessions``.
"""

import uuid
from datetime import datetime
from typing import TYPE_CHECKING

from sqlalchemy import DateTime, ForeignKey, func, text
from sqlalchemy.orm import Mapped, mapped_column, relationship

from klassenzeit_backend.db.base import Base

if TYPE_CHECKING:
    from klassenzeit_backend.db.models.school import School


class UserSession(Base):
    """Cookie-based login session tied to a user with an expiry timestamp."""

    __tablename__ = "sessions"

    id: Mapped[uuid.UUID] = mapped_column(
        primary_key=True,
        server_default=text("gen_random_uuid()"),
    )
    user_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("users.id"),
        index=True,
    )
    active_school_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("schools.id", ondelete="RESTRICT"),
        nullable=False,
        index=True,
    )
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        server_default=func.now(),
    )
    expires_at: Mapped[datetime] = mapped_column(DateTime(timezone=True))

    active_school: Mapped["School"] = relationship("School", lazy="joined")
