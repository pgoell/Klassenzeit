"""add school_class home_room_id

Revision ID: eb74171c5dec
Revises: 17f73c6e1a91
Create Date: 2026-04-30 20:17:28.863867

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "eb74171c5dec"
down_revision: str | Sequence[str] | None = "17f73c6e1a91"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "school_classes",
        sa.Column("home_room_id", sa.Uuid(), nullable=True),
    )
    op.create_foreign_key(
        "fk_school_classes_home_room_id_rooms",
        "school_classes",
        "rooms",
        ["home_room_id"],
        ["id"],
        ondelete="SET NULL",
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint("fk_school_classes_home_room_id_rooms", "school_classes", type_="foreignkey")
    op.drop_column("school_classes", "home_room_id")
