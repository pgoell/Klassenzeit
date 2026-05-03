"""Structural assertions on the dreizuegige Grundschule seed.

Mirrors test_demo_grundschule_zweizuegig_shape.py but adds the
multi-class Religion trio assertions: each Jahrgang produces one
``lesson_group_id`` group of three lessons, and every Religion lesson
spans the three classes of its Jahrgang via ``LessonSchoolClass``.
"""

import pytest
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import SchoolType, Stundentafel
from klassenzeit_backend.seed.demo_grundschule_dreizuegig import (
    seed_demo_grundschule_dreizuegig,
)

pytestmark = pytest.mark.anyio


async def test_dreizuegig_seed_creates_twelve_school_classes(
    db_session: AsyncSession,
) -> None:
    await seed_demo_grundschule_dreizuegig(db_session)
    count = (await db_session.execute(select(func.count(SchoolClass.id)))).scalar_one()
    assert count == 12


async def test_dreizuegig_seed_emits_three_religion_lessons_per_jahrgang(
    db_session: AsyncSession,
) -> None:
    await seed_demo_grundschule_dreizuegig(db_session)
    grouped = (
        await db_session.execute(
            select(Lesson.lesson_group_id, func.count())
            .where(Lesson.lesson_group_id.is_not(None))
            .group_by(Lesson.lesson_group_id)
        )
    ).all()
    # Four Jahrgaenge each contribute one lesson_group_id with three lessons.
    assert len(grouped) == 4
    assert all(count == 3 for _, count in grouped)


async def test_dreizuegig_religion_lessons_are_multi_class(
    db_session: AsyncSession,
) -> None:
    await seed_demo_grundschule_dreizuegig(db_session)
    lessons_with_groups = (
        (await db_session.execute(select(Lesson.id).where(Lesson.lesson_group_id.is_not(None))))
        .scalars()
        .all()
    )
    for lesson_id in lessons_with_groups:
        members = (
            await db_session.execute(
                select(func.count())
                .select_from(LessonSchoolClass)
                .where(LessonSchoolClass.lesson_id == lesson_id)
            )
        ).scalar_one()
        assert members == 3, "each Religion lesson must span three classes"


async def test_dreizuegig_stundentafeln_are_grundschule_school_type(
    db_session: AsyncSession,
) -> None:
    await seed_demo_grundschule_dreizuegig(db_session)
    tafeln = (await db_session.execute(select(Stundentafel))).scalars().all()
    assert len(tafeln) == 4
    for tafel in tafeln:
        assert tafel.school_type is SchoolType.GRUNDSCHULE
