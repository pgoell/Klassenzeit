"""Rollback test: seed_demo_grundschule must fail atomically on duplicate name.

Pre-inserts a Subject named ``Deutsch`` (which the seed also inserts), then
invokes ``seed_demo_grundschule`` inside a nested savepoint. The unique-name
IntegrityError must bubble up, and the DB must retain exactly the one
pre-existing Subject after the rollback.
"""

import pytest
from sqlalchemy import func, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule


async def test_seed_rolls_back_on_duplicate_subject_name(
    db_session: AsyncSession,
) -> None:
    pre_existing = Subject(
        name="Deutsch", short_name="PRE", color="chart-1", school_id=DEFAULT_SCHOOL_ID
    )
    db_session.add(pre_existing)
    await db_session.flush()

    with pytest.raises(IntegrityError):
        async with db_session.begin_nested():
            await seed_demo_grundschule(db_session)

    subject_count = int(
        (await db_session.execute(select(func.count()).select_from(Subject))).scalar_one()
    )
    assert subject_count == 1

    remaining = (await db_session.execute(select(Subject))).scalar_one()
    assert remaining.name == "Deutsch"
    assert remaining.short_name == "PRE"
