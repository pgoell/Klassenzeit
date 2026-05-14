"""add supervision_assignments

Revision ID: a5286c6739b4
Revises: a1b2c3d4e5f6
Create Date: 2026-05-14 10:04:20.555306

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "a5286c6739b4"
down_revision: str | Sequence[str] | None = "a1b2c3d4e5f6"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.create_table(
        "supervision_assignments",
        sa.Column("id", sa.UUID(), nullable=False),
        sa.Column("time_block_id", sa.UUID(), nullable=False),
        sa.Column("teacher_id", sa.UUID(), nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            server_default=sa.text("now()"),
            nullable=False,
        ),
        sa.ForeignKeyConstraint(
            ["teacher_id"],
            ["teachers.id"],
            name=op.f("fk_supervision_assignments_teacher_id_teachers"),
            ondelete="CASCADE",
        ),
        sa.ForeignKeyConstraint(
            ["time_block_id"],
            ["time_blocks.id"],
            name=op.f("fk_supervision_assignments_time_block_id_time_blocks"),
            ondelete="CASCADE",
        ),
        sa.PrimaryKeyConstraint("id", name=op.f("pk_supervision_assignments")),
        sa.UniqueConstraint("time_block_id", name="uq_supervision_assignments_time_block_id"),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_table("supervision_assignments")
