"""add prefer_late_period to subjects

Revision ID: 33b0f181d900
Revises: e48412c5a858
Create Date: 2026-05-03 23:20:24.639770

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "33b0f181d900"
down_revision: str | Sequence[str] | None = "e48412c5a858"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "subjects",
        sa.Column(
            "prefer_late_period",
            sa.Integer(),
            nullable=False,
            server_default="0",
        ),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_column("subjects", "prefer_late_period")
