"""add school id to lessons

Revision ID: 86e08db3b58a
Revises: 94d01a558f70
Create Date: 2026-05-17 14:15:06.687480

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "86e08db3b58a"
down_revision: str | Sequence[str] | None = "94d01a558f70"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

DEFAULT_SCHOOL_ID = "00000000-0000-0000-0000-000000000001"


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "lessons",
        sa.Column(
            "school_id",
            sa.UUID(),
            nullable=True,
            server_default=sa.text(f"'{DEFAULT_SCHOOL_ID}'::uuid"),
        ),
    )
    op.execute(f"UPDATE lessons SET school_id = '{DEFAULT_SCHOOL_ID}'")  # noqa: S608
    op.alter_column("lessons", "school_id", nullable=False)
    op.create_foreign_key(
        "fk_lessons_school_id_schools",
        "lessons",
        "schools",
        ["school_id"],
        ["id"],
    )
    op.create_index("ix_lessons_school_id", "lessons", ["school_id"])


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_index("ix_lessons_school_id", table_name="lessons")
    op.drop_constraint("fk_lessons_school_id_schools", "lessons", type_="foreignkey")
    op.drop_column("lessons", "school_id")
