"""add scheduled_lesson teacher_id

Revision ID: 1f5cf40b36ba
Revises: f9dc48849350
Create Date: 2026-05-08 23:12:25.114169

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "1f5cf40b36ba"
down_revision: str | Sequence[str] | None = "f9dc48849350"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema.

    Adds ``scheduled_lessons.teacher_id`` (NOT NULL FK to ``teachers``,
    ON DELETE CASCADE), backfilled from ``lessons.teacher_id``.

    Per item 65 (solver-driven teacher assignment), every persisted
    placement now records the teacher the solver picked. Backfill is safe
    because ``auto_assign_teachers_for_lessons`` runs at the route handler
    boundary today, so every Lesson with a ScheduledLesson row already has
    a non-null ``Lesson.teacher_id``.
    """
    op.add_column(
        "scheduled_lessons",
        sa.Column("teacher_id", sa.Uuid(), nullable=True),
    )
    op.create_foreign_key(
        "fk_scheduled_lessons_teacher_id_teachers",
        "scheduled_lessons",
        "teachers",
        ["teacher_id"],
        ["id"],
        ondelete="CASCADE",
    )
    op.execute(
        """
        UPDATE scheduled_lessons
        SET teacher_id = lessons.teacher_id
        FROM lessons
        WHERE lessons.id = scheduled_lessons.lesson_id
        """
    )
    null_count = (
        op.get_bind()
        .execute(sa.text("SELECT count(*) FROM scheduled_lessons WHERE teacher_id IS NULL"))
        .scalar()
    )
    if null_count and null_count > 0:
        raise RuntimeError(
            f"backfill left {null_count} scheduled_lessons rows with NULL teacher_id; "
            "investigate before NOT NULL alter"
        )
    op.alter_column("scheduled_lessons", "teacher_id", nullable=False)


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_constraint(
        "fk_scheduled_lessons_teacher_id_teachers",
        "scheduled_lessons",
        type_="foreignkey",
    )
    op.drop_column("scheduled_lessons", "teacher_id")
