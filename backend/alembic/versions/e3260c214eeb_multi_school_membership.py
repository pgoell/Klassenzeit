"""multi-school membership: add user_school_memberships and sessions.active_school_id

Revision ID: e3260c214eeb
Revises: 791b768c7167
Create Date: 2026-05-18 00:00:00.000000
"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "e3260c214eeb"
down_revision: str | None = "791b768c7167"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    # 1) New M:N join table.
    op.create_table(
        "user_school_memberships",
        sa.Column(
            "id",
            sa.Uuid(),
            primary_key=True,
            server_default=sa.text("gen_random_uuid()"),
        ),
        sa.Column(
            "user_id",
            sa.Uuid(),
            sa.ForeignKey("users.id", ondelete="CASCADE"),
            nullable=False,
            index=True,
        ),
        sa.Column(
            "school_id",
            sa.Uuid(),
            sa.ForeignKey("schools.id", ondelete="CASCADE"),
            nullable=False,
            index=True,
        ),
        sa.Column(
            "created_at",
            sa.DateTime(timezone=True),
            server_default=sa.func.now(),
            nullable=False,
        ),
        sa.UniqueConstraint(
            "user_id", "school_id", name="uq_user_school_memberships_user_id_school_id"
        ),
    )

    # 2) Add active_school_id to sessions, nullable for backfill.
    op.add_column(
        "sessions",
        sa.Column("active_school_id", sa.Uuid(), nullable=True),
    )

    # 3) Backfill from users.school_id.
    op.execute(
        """
        UPDATE sessions
        SET active_school_id = users.school_id
        FROM users
        WHERE sessions.user_id = users.id
        """
    )

    # 4) Assert zero NULLs remain, then add FK + NOT NULL.
    bind = op.get_bind()
    null_count = bind.execute(
        sa.text("SELECT COUNT(*) FROM sessions WHERE active_school_id IS NULL")
    ).scalar_one()
    if null_count != 0:
        raise RuntimeError(
            f"Backfill missed {null_count} session rows; aborting before NOT NULL alter"
        )

    op.alter_column("sessions", "active_school_id", nullable=False)
    op.create_foreign_key(
        "fk_sessions_active_school_id_schools",
        "sessions",
        "schools",
        ["active_school_id"],
        ["id"],
        ondelete="RESTRICT",
    )
    op.create_index(
        "ix_sessions_active_school_id",
        "sessions",
        ["active_school_id"],
    )


def downgrade() -> None:
    op.drop_index("ix_sessions_active_school_id", table_name="sessions")
    op.drop_constraint("fk_sessions_active_school_id_schools", "sessions", type_="foreignkey")
    op.drop_column("sessions", "active_school_id")
    op.drop_table("user_school_memberships")
