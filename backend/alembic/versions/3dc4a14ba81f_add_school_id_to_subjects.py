"""add school id to subjects

Revision ID: 3dc4a14ba81f
Revises: 0992660a3118
Create Date: 2026-05-17 00:00:00.000000

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "3dc4a14ba81f"
down_revision: str | Sequence[str] | None = "0992660a3118"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

DEFAULT_SCHOOL_ID = "00000000-0000-0000-0000-000000000001"


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "subjects",
        sa.Column(
            "school_id",
            sa.UUID(),
            nullable=True,
            server_default=sa.text(f"'{DEFAULT_SCHOOL_ID}'::uuid"),
        ),
    )
    op.execute(f"UPDATE subjects SET school_id = '{DEFAULT_SCHOOL_ID}'")  # noqa: S608
    # Drop the transitional server_default once existing rows have been
    # backfilled; future inserts must supply ``school_id`` explicitly so a
    # forgotten assignment fails loudly instead of silently landing in the
    # default tenant.
    op.alter_column("subjects", "school_id", nullable=False, server_default=None)
    op.create_foreign_key(
        "fk_subjects_school_id_schools",
        "subjects",
        "schools",
        ["school_id"],
        ["id"],
    )
    op.create_index("ix_subjects_school_id", "subjects", ["school_id"])

    op.drop_constraint("uq_subjects_name", "subjects", type_="unique")
    op.drop_constraint("uq_subjects_short_name", "subjects", type_="unique")
    op.create_unique_constraint("uq_subjects_school_id_name", "subjects", ["school_id", "name"])
    op.create_unique_constraint(
        "uq_subjects_school_id_short_name", "subjects", ["school_id", "short_name"]
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint("uq_subjects_school_id_short_name", "subjects", type_="unique")
    op.drop_constraint("uq_subjects_school_id_name", "subjects", type_="unique")
    op.create_unique_constraint("uq_subjects_short_name", "subjects", ["short_name"])
    op.create_unique_constraint("uq_subjects_name", "subjects", ["name"])

    op.drop_index("ix_subjects_school_id", table_name="subjects")
    op.drop_constraint("fk_subjects_school_id_schools", "subjects", type_="foreignkey")
    op.drop_column("subjects", "school_id")
