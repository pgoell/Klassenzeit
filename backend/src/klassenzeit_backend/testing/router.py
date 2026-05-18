"""Test-only HTTP endpoints.

These endpoints exist to let Playwright (or other black-box test drivers)
control backend state without going through the real API. The module must
only be mounted when ``settings.env == "test"``. See
``klassenzeit_backend.testing.mount``.
"""

import contextlib
import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, Response, status
from pydantic import BaseModel
from sqlalchemy import select, text
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.cli import (
    E2E_ADMIN_B_EMAIL,
    E2E_ADMIN_B_PASSWORD,
    E2E_SUPER_ADMIN_EMAIL,
    E2E_SUPER_ADMIN_PASSWORD,
    DuplicateEmailError,
    create_admin_in_db,
)
from klassenzeit_backend.db.base import Base
from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule

testing_router = APIRouter(prefix="/__test__", tags=["testing"])

# Tables that must survive a reset. ``users`` and ``sessions`` stay so the
# Playwright storageState cookie remains valid between tests. ``schools`` stays
# because ``users.school_id`` FK-references it; truncating with CASCADE would
# wipe users transitively.
# ``alembic_version`` is managed outside ``Base.metadata`` and will never
# appear in ``sorted_tables``; it is listed here as explicit documentation
# of intent and as a guard should it ever be registered as a mapped table.
PRESERVED_TABLES: frozenset[str] = frozenset({"users", "sessions", "schools", "alembic_version"})


@testing_router.get("/health")
async def testing_health() -> dict[str, str]:
    """Trivial readiness probe used by the Playwright webServer."""
    return {"status": "ok"}


@testing_router.post("/reset", status_code=status.HTTP_204_NO_CONTENT)
async def testing_reset(session: Annotated[AsyncSession, Depends(get_session)]) -> Response:
    """Truncate all entity tables, preserving users, sessions, and alembic_version.

    Returns 204 with no body.
    """
    tables = [t for t in Base.metadata.sorted_tables if t.name not in PRESERVED_TABLES]
    if tables:
        names = ", ".join(f'"{t.name}"' for t in tables)
        await session.execute(text(f"TRUNCATE {names} RESTART IDENTITY CASCADE"))
        await session.commit()
    return Response(status_code=status.HTTP_204_NO_CONTENT)


@testing_router.post("/seed-grundschule", status_code=status.HTTP_204_NO_CONTENT)
async def testing_seed_grundschule(
    session: Annotated[AsyncSession, Depends(get_session)],
) -> Response:
    """Seed a Hessen Grundschule into the current session and commit.

    Returns 204 with no body. The caller (Playwright fixture) is expected
    to truncate first via ``/__test__/reset``; calling this endpoint
    twice without a reset in between will raise ``IntegrityError``.
    """
    await seed_demo_grundschule(session)
    await session.commit()
    return Response(status_code=status.HTTP_204_NO_CONTENT)


class SeedSchoolBResponse(BaseModel):
    """Row ids returned by the ``/seed-school-b`` endpoint."""

    school_b_id: uuid.UUID
    room_b1_id: uuid.UUID
    room_b2_id: uuid.UUID


@testing_router.post("/seed-school-b", response_model=SeedSchoolBResponse)
async def testing_seed_school_b(
    session: Annotated[AsyncSession, Depends(get_session)],
) -> SeedSchoolBResponse:
    """Idempotently seed Schule B with two rooms, an admin-B user, and a super-admin user.

    Returns the seeded school id and the two room ids so Playwright tests
    can deep-link without re-querying the API.
    """
    school_b = await _upsert_school(session, name="Schule B", short_name="SB")
    room_b1 = await _upsert_room(
        session, school_id=school_b.id, name="SB Raum 1", short_name="SB-R1"
    )
    room_b2 = await _upsert_room(
        session, school_id=school_b.id, name="SB Raum 2", short_name="SB-R2"
    )

    with contextlib.suppress(DuplicateEmailError):
        await create_admin_in_db(
            session,
            E2E_ADMIN_B_EMAIL,
            E2E_ADMIN_B_PASSWORD,
            min_password_length=12,
            role="admin",
            school_id=school_b.id,
        )

    with contextlib.suppress(DuplicateEmailError):
        await create_admin_in_db(
            session,
            E2E_SUPER_ADMIN_EMAIL,
            E2E_SUPER_ADMIN_PASSWORD,
            min_password_length=12,
            role="super_admin",
            school_id=DEFAULT_SCHOOL_ID,
        )

    await session.commit()
    await session.refresh(school_b)
    await session.refresh(room_b1)
    await session.refresh(room_b2)
    return SeedSchoolBResponse(
        school_b_id=school_b.id,
        room_b1_id=room_b1.id,
        room_b2_id=room_b2.id,
    )


async def _upsert_school(session: AsyncSession, *, name: str, short_name: str) -> School:
    existing = await session.execute(select(School).where(School.name == name))
    found = existing.scalar_one_or_none()
    if found is not None:
        return found
    school = School(name=name, short_name=short_name)
    session.add(school)
    try:
        await session.flush()
    except IntegrityError:
        await session.rollback()
        retry = await session.execute(select(School).where(School.name == name))
        return retry.scalar_one()
    return school


async def _upsert_room(
    session: AsyncSession, *, school_id: uuid.UUID, name: str, short_name: str
) -> Room:
    existing = await session.execute(
        select(Room).where(Room.school_id == school_id, Room.name == name)
    )
    found = existing.scalar_one_or_none()
    if found is not None:
        return found
    room = Room(school_id=school_id, name=name, short_name=short_name)
    session.add(room)
    try:
        await session.flush()
    except IntegrityError:
        await session.rollback()
        retry = await session.execute(
            select(Room).where(Room.school_id == school_id, Room.name == name)
        )
        return retry.scalar_one()
    return room
