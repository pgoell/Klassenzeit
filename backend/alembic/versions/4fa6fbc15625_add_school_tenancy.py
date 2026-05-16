"""add school tenancy

Revision ID: 4fa6fbc15625
Revises: c2a5a324d8e0
Create Date: 2026-05-16 15:46:43.853915

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "4fa6fbc15625"
down_revision: str | Sequence[str] | None = "c2a5a324d8e0"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

DEFAULT_SCHOOL_ID = "00000000-0000-0000-0000-000000000001"


def upgrade() -> None:
    """Upgrade schema."""
    op.create_table(
        "schools",
        sa.Column(
            "id",
            sa.UUID(),
            primary_key=True,
            server_default=sa.text("gen_random_uuid()"),
        ),
        sa.Column("name", sa.String(length=120), nullable=False),
        sa.Column("short_name", sa.String(length=20), nullable=True),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            server_default=sa.text("now()"),
            nullable=False,
        ),
        sa.Column(
            "updated_at",
            sa.DateTime(timezone=True),
            server_default=sa.text("now()"),
            nullable=False,
        ),
        sa.PrimaryKeyConstraint("id", name="pk_schools"),
        sa.UniqueConstraint("name", name="uq_schools_name"),
        sa.UniqueConstraint("short_name", name="uq_schools_short_name"),
    )

    op.execute(
        f"INSERT INTO schools (id, name, short_name) "  # noqa: S608
        f"VALUES ('{DEFAULT_SCHOOL_ID}', 'Default Schule', 'DS')"
    )

    op.add_column(
        "users",
        sa.Column(
            "school_id",
            sa.UUID(),
            nullable=True,
            server_default=sa.text(f"'{DEFAULT_SCHOOL_ID}'::uuid"),
        ),
    )
    op.execute(f"UPDATE users SET school_id = '{DEFAULT_SCHOOL_ID}'")  # noqa: S608
    # Drop the transitional server_default once existing rows have been
    # backfilled; future inserts must supply ``school_id`` explicitly so a
    # forgotten assignment fails loudly instead of silently landing in the
    # default tenant.
    op.alter_column("users", "school_id", nullable=False, server_default=None)
    op.create_foreign_key("fk_users_school_id_schools", "users", "schools", ["school_id"], ["id"])
    op.create_index("ix_users_school_id", "users", ["school_id"])

    op.add_column(
        "rooms",
        sa.Column(
            "school_id",
            sa.UUID(),
            nullable=True,
            server_default=sa.text(f"'{DEFAULT_SCHOOL_ID}'::uuid"),
        ),
    )
    op.execute(f"UPDATE rooms SET school_id = '{DEFAULT_SCHOOL_ID}'")  # noqa: S608
    # Drop the transitional server_default (see users.school_id above).
    op.alter_column("rooms", "school_id", nullable=False, server_default=None)
    op.create_foreign_key("fk_rooms_school_id_schools", "rooms", "schools", ["school_id"], ["id"])
    op.create_index("ix_rooms_school_id", "rooms", ["school_id"])

    op.drop_constraint("uq_rooms_name", "rooms", type_="unique")
    op.drop_constraint("uq_rooms_short_name", "rooms", type_="unique")
    op.create_unique_constraint("uq_rooms_school_id_name", "rooms", ["school_id", "name"])
    op.create_unique_constraint(
        "uq_rooms_school_id_short_name", "rooms", ["school_id", "short_name"]
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint("uq_rooms_school_id_short_name", "rooms", type_="unique")
    op.drop_constraint("uq_rooms_school_id_name", "rooms", type_="unique")
    op.create_unique_constraint("uq_rooms_short_name", "rooms", ["short_name"])
    op.create_unique_constraint("uq_rooms_name", "rooms", ["name"])

    op.drop_index("ix_rooms_school_id", table_name="rooms")
    op.drop_constraint("fk_rooms_school_id_schools", "rooms", type_="foreignkey")
    op.drop_column("rooms", "school_id")

    op.drop_index("ix_users_school_id", table_name="users")
    op.drop_constraint("fk_users_school_id_schools", "users", type_="foreignkey")
    op.drop_column("users", "school_id")

    op.drop_table("schools")
