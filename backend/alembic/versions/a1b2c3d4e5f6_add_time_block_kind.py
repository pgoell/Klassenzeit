"""add time_block_kind

Revision ID: a1b2c3d4e5f6
Revises: c9ddbe9b2fa0
Create Date: 2026-05-13 12:00:00.000000

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import ENUM as PG_ENUM

# revision identifiers, used by Alembic.
revision: str = "a1b2c3d4e5f6"
down_revision: str | Sequence[str] | None = "c9ddbe9b2fa0"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


time_block_kind = PG_ENUM(
    "lesson",
    "break",
    name="time_block_kind",
    create_type=False,
)


def upgrade() -> None:
    """Upgrade schema."""
    time_block_kind.create(op.get_bind(), checkfirst=True)
    op.add_column(
        "time_blocks",
        sa.Column(
            "kind",
            time_block_kind,
            nullable=False,
            server_default=sa.text("'lesson'"),
        ),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_column("time_blocks", "kind")
    time_block_kind.drop(op.get_bind(), checkfirst=True)
