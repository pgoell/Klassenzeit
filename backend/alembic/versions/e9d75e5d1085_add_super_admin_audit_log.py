"""add super admin audit log

Revision ID: e9d75e5d1085
Revises: e3260c214eeb
Create Date: 2026-05-18 22:32:51.061967

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects import postgresql

# revision identifiers, used by Alembic.
revision: str = "e9d75e5d1085"
down_revision: str | Sequence[str] | None = "e3260c214eeb"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.create_table(
        "super_admin_audit_log",
        sa.Column("id", sa.UUID(), server_default=sa.text("gen_random_uuid()"), nullable=False),
        sa.Column(
            "ts",
            postgresql.TIMESTAMP(timezone=True),
            server_default=sa.text("now()"),
            nullable=False,
        ),
        sa.Column("actor_user_id", sa.UUID(), nullable=True),
        sa.Column("actor_user_email", sa.Text(), nullable=False),
        sa.Column("target_school_id", sa.UUID(), nullable=True),
        sa.Column("target_school_name", sa.Text(), nullable=True),
        sa.Column("request_id", sa.Text(), nullable=True),
        sa.Column("method", sa.Text(), nullable=False),
        sa.Column("route_template", sa.Text(), nullable=False),
        sa.Column(
            "path_params",
            postgresql.JSONB(astext_type=sa.Text()),
            server_default=sa.text("'{}'::jsonb"),
            nullable=False,
        ),
        sa.Column("request_body", postgresql.JSONB(astext_type=sa.Text()), nullable=True),
        sa.Column(
            "request_body_truncated", sa.Boolean(), server_default=sa.text("false"), nullable=False
        ),
        sa.Column("response_status", sa.SmallInteger(), nullable=False),
        sa.ForeignKeyConstraint(
            ["actor_user_id"],
            ["users.id"],
            name=op.f("fk_super_admin_audit_log_actor_user_id_users"),
            ondelete="SET NULL",
        ),
        sa.ForeignKeyConstraint(
            ["target_school_id"],
            ["schools.id"],
            name=op.f("fk_super_admin_audit_log_target_school_id_schools"),
            ondelete="SET NULL",
        ),
        sa.PrimaryKeyConstraint("id", name=op.f("pk_super_admin_audit_log")),
    )
    op.create_index(
        "idx_audit_log_actor",
        "super_admin_audit_log",
        ["actor_user_id", sa.literal_column("ts DESC")],
        unique=False,
    )
    op.create_index(
        "idx_audit_log_target",
        "super_admin_audit_log",
        ["target_school_id", sa.literal_column("ts DESC")],
        unique=False,
    )
    op.create_index(
        "idx_audit_log_ts", "super_admin_audit_log", [sa.literal_column("ts DESC")], unique=False
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_index("idx_audit_log_ts", table_name="super_admin_audit_log")
    op.drop_index("idx_audit_log_target", table_name="super_admin_audit_log")
    op.drop_index("idx_audit_log_actor", table_name="super_admin_audit_log")
    op.drop_table("super_admin_audit_log")
