"""add daily caps

Revision ID: e8ff05bec987
Revises: 33b0f181d900
Create Date: 2026-05-05 16:05:52.769766

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "e8ff05bec987"
down_revision: str | Sequence[str] | None = "33b0f181d900"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "subjects",
        sa.Column(
            "max_hours_per_day",
            sa.Integer(),
            nullable=False,
            server_default="2",
        ),
    )
    op.add_column(
        "school_classes",
        sa.Column("max_lessons_per_day", sa.Integer(), nullable=True),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_column("school_classes", "max_lessons_per_day")
    op.drop_column("subjects", "max_hours_per_day")
