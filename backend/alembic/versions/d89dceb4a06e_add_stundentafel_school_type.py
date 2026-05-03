"""add stundentafel school_type

Revision ID: d89dceb4a06e
Revises: 09dab109a11c
Create Date: 2026-05-03 09:52:37.143830

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects import postgresql

# revision identifiers, used by Alembic.
revision: str = "d89dceb4a06e"
down_revision: str | Sequence[str] | None = "09dab109a11c"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


school_type_enum = postgresql.ENUM(
    "Grundschule",
    "Hauptschule",
    "Realschule",
    "Gymnasium",
    "Gesamtschule",
    name="school_type",
    create_type=False,
)


def upgrade() -> None:
    """Upgrade schema."""
    school_type_enum.create(op.get_bind(), checkfirst=True)
    op.add_column(
        "stundentafeln",
        sa.Column(
            "school_type",
            school_type_enum,
            server_default="Grundschule",
            nullable=False,
        ),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_column("stundentafeln", "school_type")
    school_type_enum.drop(op.get_bind(), checkfirst=True)
