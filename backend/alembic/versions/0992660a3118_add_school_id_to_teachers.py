"""add school id to teachers

Revision ID: 0992660a3118
Revises: 10cb7ba350a2
Create Date: 2026-05-16 12:00:00.000000

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "0992660a3118"
down_revision: str | Sequence[str] | None = "10cb7ba350a2"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

DEFAULT_SCHOOL_ID = "00000000-0000-0000-0000-000000000001"


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "teachers",
        sa.Column(
            "school_id",
            sa.UUID(),
            nullable=True,
            server_default=sa.text(f"'{DEFAULT_SCHOOL_ID}'::uuid"),
        ),
    )
    op.execute(f"UPDATE teachers SET school_id = '{DEFAULT_SCHOOL_ID}'")  # noqa: S608
    # Drop the transitional server_default once existing rows have been
    # backfilled; future inserts must supply ``school_id`` explicitly so a
    # forgotten assignment fails loudly instead of silently landing in the
    # default tenant.
    op.alter_column("teachers", "school_id", nullable=False, server_default=None)
    op.create_foreign_key(
        "fk_teachers_school_id_schools",
        "teachers",
        "schools",
        ["school_id"],
        ["id"],
    )
    op.create_index("ix_teachers_school_id", "teachers", ["school_id"])

    op.drop_constraint("uq_teachers_short_code", "teachers", type_="unique")
    op.create_unique_constraint(
        "uq_teachers_school_id_short_code", "teachers", ["school_id", "short_code"]
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint("uq_teachers_school_id_short_code", "teachers", type_="unique")
    op.create_unique_constraint("uq_teachers_short_code", "teachers", ["short_code"])

    op.drop_index("ix_teachers_school_id", table_name="teachers")
    op.drop_constraint("fk_teachers_school_id_schools", "teachers", type_="foreignkey")
    op.drop_column("teachers", "school_id")
