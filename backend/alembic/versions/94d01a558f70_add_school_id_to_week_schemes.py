"""add school id to week schemes

Revision ID: 94d01a558f70
Revises: 5aec1ac25d0b
Create Date: 2026-05-17 12:26:49.868619

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "94d01a558f70"
down_revision: str | Sequence[str] | None = "5aec1ac25d0b"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

DEFAULT_SCHOOL_ID = "00000000-0000-0000-0000-000000000001"


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "week_schemes",
        sa.Column(
            "school_id",
            sa.UUID(),
            nullable=True,
            server_default=sa.text(f"'{DEFAULT_SCHOOL_ID}'::uuid"),
        ),
    )
    op.execute(f"UPDATE week_schemes SET school_id = '{DEFAULT_SCHOOL_ID}'")  # noqa: S608
    op.alter_column("week_schemes", "school_id", nullable=False, server_default=None)
    op.create_foreign_key(
        "fk_week_schemes_school_id_schools",
        "week_schemes",
        "schools",
        ["school_id"],
        ["id"],
    )
    op.create_index("ix_week_schemes_school_id", "week_schemes", ["school_id"])

    op.drop_constraint("uq_week_schemes_name", "week_schemes", type_="unique")
    op.create_unique_constraint(
        "uq_week_schemes_school_id_name", "week_schemes", ["school_id", "name"]
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint("uq_week_schemes_school_id_name", "week_schemes", type_="unique")
    op.create_unique_constraint("uq_week_schemes_name", "week_schemes", ["name"])

    op.drop_index("ix_week_schemes_school_id", table_name="week_schemes")
    op.drop_constraint("fk_week_schemes_school_id_schools", "week_schemes", type_="foreignkey")
    op.drop_column("week_schemes", "school_id")
