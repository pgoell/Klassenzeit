"""add school_id to supervision_assignments

Revision ID: 0a06cf0ccc53
Revises: ac71888219a3
Create Date: 2026-05-17 15:28:23.416357

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects import postgresql

revision: str = "0a06cf0ccc53"
down_revision: str | Sequence[str] | None = "ac71888219a3"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "supervision_assignments",
        sa.Column(
            "school_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("schools.id"),
            nullable=False,
            server_default=sa.text("'00000000-0000-0000-0000-000000000001'::uuid"),
        ),
    )
    op.create_index(
        "ix_supervision_assignments_school_id",
        "supervision_assignments",
        ["school_id"],
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_index(
        "ix_supervision_assignments_school_id",
        table_name="supervision_assignments",
    )
    op.drop_column("supervision_assignments", "school_id")
