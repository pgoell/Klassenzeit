"""drop lesson school id default

Revision ID: ac71888219a3
Revises: 86e08db3b58a
Create Date: 2026-05-17 14:38:42.312410

"""

from collections.abc import Sequence

from alembic import op

revision: str = "ac71888219a3"
down_revision: str | Sequence[str] | None = "86e08db3b58a"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema: drop the transitional school_id server_default."""
    op.alter_column("lessons", "school_id", server_default=None)


def downgrade() -> None:
    """Downgrade schema: restore the transitional school_id server_default."""
    op.alter_column("lessons", "school_id", server_default="00000000-0000-0000-0000-000000000001")
