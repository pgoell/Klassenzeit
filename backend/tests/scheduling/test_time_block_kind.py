"""Tests for TimeBlock.kind (lesson | break)."""

import datetime as dt
import uuid

import pytest
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.week_scheme import (
    TimeBlock,
    TimeBlockKind,
    WeekScheme,
)


@pytest.mark.asyncio
async def test_time_block_kind_defaults_to_lesson(db_session: AsyncSession) -> None:
    """A TimeBlock with no explicit kind defaults to LESSON after flush+refresh."""
    ws = WeekScheme(name=f"orm-default-{uuid.uuid4().hex[:8]}", description=None)
    db_session.add(ws)
    await db_session.flush()
    tb = TimeBlock(
        week_scheme_id=ws.id,
        day_of_week=0,
        position=1,
        start_time=dt.time(8, 0),
        end_time=dt.time(8, 45),
    )
    db_session.add(tb)
    await db_session.flush()
    await db_session.refresh(tb)
    assert tb.kind is TimeBlockKind.LESSON


@pytest.mark.asyncio
async def test_time_block_kind_persists_break(db_session: AsyncSession) -> None:
    """An explicit kind=BREAK round-trips through the native Postgres enum."""
    ws = WeekScheme(name=f"orm-break-{uuid.uuid4().hex[:8]}", description=None)
    db_session.add(ws)
    await db_session.flush()
    tb = TimeBlock(
        week_scheme_id=ws.id,
        day_of_week=0,
        position=1,
        start_time=dt.time(9, 30),
        end_time=dt.time(9, 50),
        kind=TimeBlockKind.BREAK,
    )
    db_session.add(tb)
    await db_session.flush()
    fetched = (
        await db_session.execute(select(TimeBlock).where(TimeBlock.id == tb.id))
    ).scalar_one()
    assert fetched.kind is TimeBlockKind.BREAK
