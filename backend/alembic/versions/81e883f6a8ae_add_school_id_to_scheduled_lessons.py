"""add school_id to scheduled_lessons

Revision ID: 81e883f6a8ae
Revises: 4b43b36defee
Create Date: 2026-05-17 17:48:59.309398

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects import postgresql

revision: str = "81e883f6a8ae"
down_revision: str | Sequence[str] | None = "4b43b36defee"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "scheduled_lessons",
        sa.Column(
            "school_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("schools.id"),
            nullable=False,
            server_default=sa.text("'00000000-0000-0000-0000-000000000001'::uuid"),
        ),
    )
    op.create_index(
        "ix_scheduled_lessons_school_id",
        "scheduled_lessons",
        ["school_id"],
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_index(
        "ix_scheduled_lessons_school_id",
        table_name="scheduled_lessons",
    )
    op.drop_column("scheduled_lessons", "school_id")
