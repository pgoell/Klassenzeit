"""add teacher reserve_hours_per_week

Revision ID: c9ddbe9b2fa0
Revises: 1f5cf40b36ba
Create Date: 2026-05-13 20:32:57.466098

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "c9ddbe9b2fa0"
down_revision: str | Sequence[str] | None = "1f5cf40b36ba"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "teachers",
        sa.Column(
            "reserve_hours_per_week",
            sa.SmallInteger(),
            nullable=False,
            server_default="0",
        ),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_column("teachers", "reserve_hours_per_week")
