"""drop supervision_assignment school_id default

Revision ID: 4b43b36defee
Revises: 0a06cf0ccc53
Create Date: 2026-05-17 15:34:10.938160

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "4b43b36defee"
down_revision: str | Sequence[str] | None = "0a06cf0ccc53"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.alter_column("supervision_assignments", "school_id", server_default=None)


def downgrade() -> None:
    """Downgrade schema."""
    op.alter_column(
        "supervision_assignments",
        "school_id",
        server_default=sa.text("'00000000-0000-0000-0000-000000000001'::uuid"),
    )
