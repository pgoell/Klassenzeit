"""Verify the daily-caps migration columns exist on the live test schema.

The repo has no Alembic round-trip fixture; the conftest's ``apply_migrations``
already runs ``alembic upgrade head`` against the per-worker DB before any
test executes. Querying ``information_schema.columns`` over the same session
exercises the migration's effect end to end.
"""

from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.base import Base


def test_subject_metadata_has_max_hours_per_day() -> None:
    """``Subject.max_hours_per_day`` is registered on the SQLAlchemy metadata."""
    table = Base.metadata.tables["subjects"]
    col = table.c["max_hours_per_day"]
    assert col.nullable is False
    assert col.server_default is not None


def test_school_class_metadata_has_max_lessons_per_day() -> None:
    """``SchoolClass.max_lessons_per_day`` is registered on the SQLAlchemy metadata."""
    table = Base.metadata.tables["school_classes"]
    col = table.c["max_lessons_per_day"]
    assert col.nullable is True


async def test_subject_max_hours_per_day_column_exists_on_live_schema(
    db_session: AsyncSession,
) -> None:
    """The live test DB (post-migration) carries the new ``subjects.max_hours_per_day`` column."""
    rows = (
        await db_session.execute(
            text(
                "SELECT column_name, is_nullable, column_default "
                "FROM information_schema.columns "
                "WHERE table_name = 'subjects' AND column_name = 'max_hours_per_day'"
            )
        )
    ).all()
    assert len(rows) == 1
    column_name, is_nullable, column_default = rows[0]
    assert column_name == "max_hours_per_day"
    assert is_nullable == "NO"
    assert column_default is not None and "2" in str(column_default)


async def test_school_class_max_lessons_per_day_column_exists_on_live_schema(
    db_session: AsyncSession,
) -> None:
    """The live test DB carries the new nullable ``school_classes.max_lessons_per_day`` column."""
    rows = (
        await db_session.execute(
            text(
                "SELECT column_name, is_nullable "
                "FROM information_schema.columns "
                "WHERE table_name = 'school_classes' "
                "AND column_name = 'max_lessons_per_day'"
            )
        )
    ).all()
    assert len(rows) == 1
    column_name, is_nullable = rows[0]
    assert column_name == "max_lessons_per_day"
    assert is_nullable == "YES"
