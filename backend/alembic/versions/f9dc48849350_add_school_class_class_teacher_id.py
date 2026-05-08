"""add school_class class_teacher_id

Revision ID: f9dc48849350
Revises: e8ff05bec987
Create Date: 2026-05-08 21:57:03.321712

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "f9dc48849350"
down_revision: str | Sequence[str] | None = "e8ff05bec987"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "school_classes",
        sa.Column("class_teacher_id", sa.Uuid(), nullable=True),
    )
    op.create_foreign_key(
        "fk_school_classes_class_teacher_id_teachers",
        "school_classes",
        "teachers",
        ["class_teacher_id"],
        ["id"],
        ondelete="SET NULL",
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint(
        "fk_school_classes_class_teacher_id_teachers",
        "school_classes",
        type_="foreignkey",
    )
    op.drop_column("school_classes", "class_teacher_id")
