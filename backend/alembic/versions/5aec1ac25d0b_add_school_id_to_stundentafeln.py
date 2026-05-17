"""add school id to stundentafeln

Revision ID: 5aec1ac25d0b
Revises: 3dc4a14ba81f
Create Date: 2026-05-17 00:00:00.000000

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "5aec1ac25d0b"
down_revision: str | Sequence[str] | None = "3dc4a14ba81f"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

DEFAULT_SCHOOL_ID = "00000000-0000-0000-0000-000000000001"


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "stundentafeln",
        sa.Column(
            "school_id",
            sa.UUID(),
            nullable=True,
            server_default=sa.text(f"'{DEFAULT_SCHOOL_ID}'::uuid"),
        ),
    )
    op.execute(f"UPDATE stundentafeln SET school_id = '{DEFAULT_SCHOOL_ID}'")  # noqa: S608
    # Drop the transitional server_default once existing rows have been
    # backfilled; future inserts must supply ``school_id`` explicitly so a
    # forgotten assignment fails loudly instead of silently landing in the
    # default tenant.
    op.alter_column("stundentafeln", "school_id", nullable=False, server_default=None)
    op.create_foreign_key(
        "fk_stundentafeln_school_id_schools",
        "stundentafeln",
        "schools",
        ["school_id"],
        ["id"],
    )
    op.create_index("ix_stundentafeln_school_id", "stundentafeln", ["school_id"])

    op.drop_constraint("uq_stundentafeln_name", "stundentafeln", type_="unique")
    op.create_unique_constraint(
        "uq_stundentafeln_school_id_name", "stundentafeln", ["school_id", "name"]
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint("uq_stundentafeln_school_id_name", "stundentafeln", type_="unique")
    op.create_unique_constraint("uq_stundentafeln_name", "stundentafeln", ["name"])

    op.drop_index("ix_stundentafeln_school_id", table_name="stundentafeln")
    op.drop_constraint("fk_stundentafeln_school_id_schools", "stundentafeln", type_="foreignkey")
    op.drop_column("stundentafeln", "school_id")
