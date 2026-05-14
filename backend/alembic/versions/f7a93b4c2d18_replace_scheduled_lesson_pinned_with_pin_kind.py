"""replace scheduled_lesson pinned with pin_kind

Revision ID: f7a93b4c2d18
Revises: e10df3a8dce2
Create Date: 2026-05-14 14:00:00.000000

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import ENUM as PG_ENUM

# revision identifiers, used by Alembic.
revision: str = "f7a93b4c2d18"
down_revision: str | Sequence[str] | None = "e10df3a8dce2"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


pin_kind_enum = PG_ENUM("hard", "soft", name="pin_kind", create_type=False)


def upgrade() -> None:
    """Upgrade schema."""
    pin_kind_enum.create(op.get_bind(), checkfirst=True)
    op.add_column(
        "scheduled_lessons",
        sa.Column("pin_kind", pin_kind_enum, nullable=True),
    )
    op.execute("UPDATE scheduled_lessons SET pin_kind = 'hard' WHERE pinned = true")
    op.drop_column("scheduled_lessons", "pinned")


def downgrade() -> None:
    """Downgrade schema."""
    op.add_column(
        "scheduled_lessons",
        sa.Column("pinned", sa.Boolean(), nullable=False, server_default=sa.false()),
    )
    op.execute("UPDATE scheduled_lessons SET pinned = (pin_kind = 'hard')")
    op.drop_column("scheduled_lessons", "pin_kind")
    pin_kind_enum.drop(op.get_bind(), checkfirst=True)
