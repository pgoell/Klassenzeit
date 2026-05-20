"""add week_scheme pre_first_slot_grace_minutes

Revision ID: 036bed89c879
Revises: e9d75e5d1085
Create Date: 2026-05-20 08:41:23.120089

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "036bed89c879"
down_revision: str | Sequence[str] | None = "e9d75e5d1085"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "week_schemes",
        sa.Column(
            "pre_first_slot_grace_minutes",
            sa.SmallInteger(),
            nullable=False,
            server_default=sa.text("0"),
        ),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_column("week_schemes", "pre_first_slot_grace_minutes")
