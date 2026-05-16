"""Test-only HTTP endpoints.

These endpoints exist to let Playwright (or other black-box test drivers)
control backend state without going through the real API. The module must
only be mounted when ``settings.env == "test"``. See
``klassenzeit_backend.testing.mount``.
"""

from typing import Annotated

from fastapi import APIRouter, Depends, Response, status
from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.base import Base
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
