"""drop scheduled_lesson school_id default

Revision ID: 791b768c7167
Revises: 81e883f6a8ae
Create Date: 2026-05-17 18:07:51.830837

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "791b768c7167"
down_revision: str | Sequence[str] | None = "81e883f6a8ae"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.alter_column("scheduled_lessons", "school_id", server_default=None)


def downgrade() -> None:
    """Downgrade schema."""
    op.alter_column(
        "scheduled_lessons",
        "school_id",
        server_default=sa.text("'00000000-0000-0000-0000-000000000001'::uuid"),
    )
