"""add subject avoid last period

Revision ID: 8ec1bca8bc5a
Revises: eb74171c5dec
Create Date: 2026-05-01 16:40:19.652689

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "8ec1bca8bc5a"
down_revision: str | Sequence[str] | None = "eb74171c5dec"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "subjects",
        sa.Column(
            "avoid_last_period",
            sa.Boolean(),
            nullable=False,
            server_default=sa.text("false"),
        ),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_column("subjects", "avoid_last_period")
