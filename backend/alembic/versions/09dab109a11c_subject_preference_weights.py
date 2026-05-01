"""subject preference weights

Revision ID: 09dab109a11c
Revises: 8ec1bca8bc5a
Create Date: 2026-05-01 17:28:56.878190

"""

from collections.abc import Sequence

from alembic import op

# revision identifiers, used by Alembic.
revision: str = "09dab109a11c"
down_revision: str | Sequence[str] | None = "8ec1bca8bc5a"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.alter_column(
        "subjects",
        "prefer_early_periods",
        new_column_name="prefer_early_period",
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN prefer_early_period DROP DEFAULT, "
        "ALTER COLUMN prefer_early_period TYPE INTEGER "
        "USING (CASE WHEN prefer_early_period THEN 1 ELSE 0 END), "
        "ALTER COLUMN prefer_early_period SET DEFAULT 0, "
        "ALTER COLUMN prefer_early_period SET NOT NULL"
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN avoid_first_period DROP DEFAULT, "
        "ALTER COLUMN avoid_first_period TYPE INTEGER "
        "USING (CASE WHEN avoid_first_period THEN 1 ELSE 0 END), "
        "ALTER COLUMN avoid_first_period SET DEFAULT 0, "
        "ALTER COLUMN avoid_first_period SET NOT NULL"
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN avoid_last_period DROP DEFAULT, "
        "ALTER COLUMN avoid_last_period TYPE INTEGER "
        "USING (CASE WHEN avoid_last_period THEN 1 ELSE 0 END), "
        "ALTER COLUMN avoid_last_period SET DEFAULT 0, "
        "ALTER COLUMN avoid_last_period SET NOT NULL"
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN avoid_last_period DROP DEFAULT, "
        "ALTER COLUMN avoid_last_period TYPE BOOLEAN "
        "USING (avoid_last_period <> 0), "
        "ALTER COLUMN avoid_last_period SET DEFAULT FALSE, "
        "ALTER COLUMN avoid_last_period SET NOT NULL"
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN avoid_first_period DROP DEFAULT, "
        "ALTER COLUMN avoid_first_period TYPE BOOLEAN "
        "USING (avoid_first_period <> 0), "
        "ALTER COLUMN avoid_first_period SET DEFAULT FALSE, "
        "ALTER COLUMN avoid_first_period SET NOT NULL"
    )
    op.execute(
        "ALTER TABLE subjects "
        "ALTER COLUMN prefer_early_period DROP DEFAULT, "
        "ALTER COLUMN prefer_early_period TYPE BOOLEAN "
        "USING (prefer_early_period <> 0), "
        "ALTER COLUMN prefer_early_period SET DEFAULT FALSE, "
        "ALTER COLUMN prefer_early_period SET NOT NULL"
    )
    op.alter_column(
        "subjects",
        "prefer_early_period",
        new_column_name="prefer_early_periods",
    )
