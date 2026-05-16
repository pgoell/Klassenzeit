"""add school id to school classes

Revision ID: 10cb7ba350a2
Revises: 4fa6fbc15625
Create Date: 2026-05-16 19:16:30.718342

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "10cb7ba350a2"
down_revision: str | Sequence[str] | None = "4fa6fbc15625"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

DEFAULT_SCHOOL_ID = "00000000-0000-0000-0000-000000000001"


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "school_classes",
        sa.Column(
            "school_id",
            sa.UUID(),
            nullable=True,
            server_default=sa.text(f"'{DEFAULT_SCHOOL_ID}'::uuid"),
        ),
    )
    op.execute(f"UPDATE school_classes SET school_id = '{DEFAULT_SCHOOL_ID}'")  # noqa: S608
    # Drop the transitional server_default once existing rows have been
    # backfilled; future inserts must supply ``school_id`` explicitly so a
    # forgotten assignment fails loudly instead of silently landing in the
    # default tenant.
    op.alter_column("school_classes", "school_id", nullable=False, server_default=None)
    op.create_foreign_key(
        "fk_school_classes_school_id_schools",
        "school_classes",
        "schools",
        ["school_id"],
        ["id"],
    )
    op.create_index("ix_school_classes_school_id", "school_classes", ["school_id"])

    op.drop_constraint("uq_school_classes_name", "school_classes", type_="unique")
    op.create_unique_constraint(
        "uq_school_classes_school_id_name", "school_classes", ["school_id", "name"]
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint("uq_school_classes_school_id_name", "school_classes", type_="unique")
    op.create_unique_constraint("uq_school_classes_name", "school_classes", ["name"])

    op.drop_index("ix_school_classes_school_id", table_name="school_classes")
    op.drop_constraint("fk_school_classes_school_id_schools", "school_classes", type_="foreignkey")
    op.drop_column("school_classes", "school_id")
