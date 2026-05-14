"""add teacher working_days

Revision ID: e10df3a8dce2
Revises: a5286c6739b4
Create Date: 2026-05-14 13:10:37.491264

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "e10df3a8dce2"
down_revision: str | Sequence[str] | None = "a5286c6739b4"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "teachers",
        sa.Column(
            "working_days",
            sa.ARRAY(sa.SmallInteger()),
            nullable=True,
        ),
    )
    op.create_check_constraint(
        "ck_teachers_working_days_range",
        "teachers",
        (
            "working_days IS NULL OR ("
            "array_length(working_days, 1) BETWEEN 1 AND 5 "
            "AND working_days <@ ARRAY[0, 1, 2, 3, 4]::smallint[]"
            ")"
        ),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint("ck_teachers_working_days_range", "teachers", type_="check")
    op.drop_column("teachers", "working_days")
