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
from klassenzeit_backend.scheduling.schemas.week_scheme import (
    TimeBlockCreate,
    TimeBlockResponse,
    TimeBlockUpdate,
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


def test_time_block_create_defaults_to_lesson_kind() -> None:
    """TimeBlockCreate defaults kind to LESSON when the field is omitted."""
    body = TimeBlockCreate(
        day_of_week=0, position=1, start_time=dt.time(8, 0), end_time=dt.time(8, 45)
    )
    assert body.kind.value == "lesson"


def test_time_block_create_accepts_break_kind() -> None:
    """TimeBlockCreate accepts an explicit BREAK kind via the enum."""
    body = TimeBlockCreate(
        day_of_week=0,
        position=1,
        start_time=dt.time(9, 30),
        end_time=dt.time(9, 50),
        kind=TimeBlockKind.BREAK,
    )
    assert body.kind.value == "break"


def test_time_block_create_parses_kind_from_json_string() -> None:
    """TimeBlockCreate.model_validate parses the wire string "break" to the enum."""
    body = TimeBlockCreate.model_validate(
        {
            "day_of_week": 0,
            "position": 1,
            "start_time": "09:30:00",
            "end_time": "09:50:00",
            "kind": "break",
        }
    )
    assert body.kind is TimeBlockKind.BREAK


def test_time_block_update_omits_kind_when_absent() -> None:
    """TimeBlockUpdate does not include kind in model_fields_set when omitted."""
    body = TimeBlockUpdate(start_time=dt.time(8, 5))
    assert "kind" not in body.model_fields_set


def test_time_block_update_carries_kind_when_set() -> None:
    """TimeBlockUpdate sets kind in model_fields_set when explicitly provided."""
    body = TimeBlockUpdate(kind=TimeBlockKind.BREAK)
    assert "kind" in body.model_fields_set
    assert body.kind is not None
    assert body.kind.value == "break"


def test_time_block_response_serializes_kind_as_lowercase_value() -> None:
    """TimeBlockResponse.model_dump emits the lowercase enum value, not the name."""
    body = TimeBlockResponse(
        id=uuid.UUID("00000000-0000-0000-0000-000000000000"),
        day_of_week=0,
        position=1,
        start_time=dt.time(8, 0),
        end_time=dt.time(8, 45),
        kind=TimeBlockKind.LESSON,
    )
    dumped = body.model_dump(mode="json")
    assert dumped["kind"] == "lesson"
