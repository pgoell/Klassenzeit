"""add scheduled_lesson pinned

Revision ID: e48412c5a858
Revises: d89dceb4a06e
Create Date: 2026-05-03 15:17:55.307840

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "e48412c5a858"
down_revision: str | Sequence[str] | None = "d89dceb4a06e"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    op.add_column(
        "scheduled_lessons",
        sa.Column("pinned", sa.Boolean(), server_default=sa.text("false"), nullable=False),
    )


def downgrade() -> None:
    op.drop_column("scheduled_lessons", "pinned")
