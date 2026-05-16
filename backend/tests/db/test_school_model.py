"""Tests for the School ORM model and per-school uniqueness."""

import uuid

import pytest
from sqlalchemy import text
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School


async def test_default_school_row_exists(db_session: AsyncSession) -> None:
    """The Alembic migration must seed a row with the canonical DEFAULT_SCHOOL_ID."""
    school = await db_session.get(School, DEFAULT_SCHOOL_ID)
    assert school is not None
    assert school.name == "Default Schule"


async def test_room_school_id_is_not_null(db_session: AsyncSession) -> None:
    """Forcing NULL into rooms.school_id raises IntegrityError (NOT NULL enforced).

    The column carries a transitional server_default during the tracer-bullet
    PR (so the CRUD route stays untouched until Task 2). The SQL-level NOT NULL
    is still load-bearing; this test sends a raw INSERT with an explicit NULL
    to bypass the server_default and prove the constraint is in force.
    """
    sp = await db_session.begin_nested()
    try:
        with pytest.raises(IntegrityError):
            await db_session.execute(
                text(
                    "INSERT INTO rooms (id, name, short_name, school_id) "
                    "VALUES (gen_random_uuid(), 'Test 101', 'T101', NULL)"
                )
            )
    finally:
        if sp.is_active:
            await sp.rollback()


async def test_room_name_is_unique_per_school(db_session: AsyncSession) -> None:
    """Two schools can each have a room named '101'; same school cannot."""
    other = School(name="Andere Schule", short_name="AS")
    db_session.add(other)
    await db_session.flush()

    db_session.add(Room(name="101", short_name="101", school_id=DEFAULT_SCHOOL_ID))
    db_session.add(Room(name="101", short_name="101-b", school_id=other.id))
    await db_session.flush()

    async with db_session.begin_nested():
        db_session.add(Room(name="101", short_name="101-dup", school_id=DEFAULT_SCHOOL_ID))
        with pytest.raises(IntegrityError):
            await db_session.flush()


async def test_room_short_name_is_unique_per_school(db_session: AsyncSession) -> None:
    """short_name uniqueness is scoped per school."""
    other = School(name="Andere Schule 2", short_name="AS2")
    db_session.add(other)
    await db_session.flush()

    db_session.add(Room(name="Zimmer A", short_name="ZA", school_id=DEFAULT_SCHOOL_ID))
    db_session.add(Room(name="Zimmer A 2", short_name="ZA", school_id=other.id))
    await db_session.flush()

    async with db_session.begin_nested():
        db_session.add(
            Room(name="Zimmer A duplicate", short_name="ZA", school_id=DEFAULT_SCHOOL_ID)
        )
        with pytest.raises(IntegrityError):
            await db_session.flush()


async def test_room_school_id_fk_enforced(db_session: AsyncSession) -> None:
    """Inserting a Room with an unknown school_id must raise IntegrityError."""
    async with db_session.begin_nested():
        db_session.add(Room(name="Geist", short_name="G", school_id=uuid.uuid4()))
        with pytest.raises(IntegrityError):
            await db_session.flush()
