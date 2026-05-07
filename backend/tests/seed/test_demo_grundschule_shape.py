"""Shape + FK-integrity assertions for seed_demo_grundschule.

Runs against the per-test db_session fixture (nested savepoint, rolled back
at teardown). The seed coroutine is called without a commit; tests read via
the same session.
"""

from datetime import datetime, time, timedelta

import pytest
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.room import Room, RoomSubjectSuitability
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import (
    SchoolType,
    Stundentafel,
    StundentafelEntry,
)
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.db.models.week_scheme import TimeBlock, WeekScheme
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule


async def _count(session: AsyncSession, model: type) -> int:
    """Helper: SELECT count(*) for ``model``."""
    result = await session.execute(select(func.count()).select_from(model))
    return int(result.scalar_one())


@pytest.fixture
async def seeded_session(db_session: AsyncSession) -> AsyncSession:
    """Pre-seed the db_session and yield it for per-test assertions."""
    await seed_demo_grundschule(db_session)
    await db_session.flush()
    return db_session


async def test_seed_creates_expected_entity_counts(
    seeded_session: AsyncSession,
) -> None:
    assert await _count(seeded_session, Subject) == 11
    assert await _count(seeded_session, WeekScheme) == 1
    assert await _count(seeded_session, TimeBlock) == 30
    assert await _count(seeded_session, Stundentafel) == 4
    assert await _count(seeded_session, StundentafelEntry) == 34
    assert await _count(seeded_session, SchoolClass) == 4
    assert await _count(seeded_session, Teacher) == 6
    assert await _count(seeded_session, TeacherQualification) == 24
    assert await _count(seeded_session, Room) == 7
    assert await _count(seeded_session, RoomSubjectSuitability) == 35


async def test_time_blocks_span_five_days_six_periods_forty_five_minutes(
    seeded_session: AsyncSession,
) -> None:
    result = await seeded_session.execute(select(TimeBlock))
    blocks = list(result.scalars().all())
    assert len(blocks) == 30

    days = {b.day_of_week for b in blocks}
    assert days == {0, 1, 2, 3, 4}, days

    positions_per_day: dict[int, set[int]] = {}
    for b in blocks:
        positions_per_day.setdefault(b.day_of_week, set()).add(b.position)
    for day, positions in positions_per_day.items():
        assert positions == {1, 2, 3, 4, 5, 6}, (day, positions)

    forty_five = timedelta(minutes=45)
    for b in blocks:
        delta = datetime.combine(datetime.min, b.end_time) - datetime.combine(
            datetime.min, b.start_time
        )
        assert delta == forty_five, b


async def test_school_class_grade_matches_stundentafel_grade(
    seeded_session: AsyncSession,
) -> None:
    rows = (
        await seeded_session.execute(
            select(SchoolClass.name, SchoolClass.grade_level, Stundentafel.grade_level)
            .join(Stundentafel, SchoolClass.stundentafel_id == Stundentafel.id)
            .order_by(SchoolClass.grade_level)
        )
    ).all()
    assert [(r[0], r[1], r[2]) for r in rows] == [
        ("1a", 1, 1),
        ("2a", 2, 2),
        ("3a", 3, 3),
        ("4a", 4, 4),
    ]


async def test_stundentafeln_are_grundschule_school_type(
    seeded_session: AsyncSession,
) -> None:
    tafeln = (await seeded_session.execute(select(Stundentafel))).scalars().all()
    assert len(tafeln) == 4
    for tafel in tafeln:
        assert tafel.school_type is SchoolType.GRUNDSCHULE


async def test_stundentafel_hour_sums_match_hessen_reference(
    seeded_session: AsyncSession,
) -> None:
    rows = (
        await seeded_session.execute(
            select(
                Stundentafel.grade_level,
                func.sum(StundentafelEntry.hours_per_week),
            )
            .join(StundentafelEntry, StundentafelEntry.stundentafel_id == Stundentafel.id)
            .group_by(Stundentafel.grade_level)
            .order_by(Stundentafel.grade_level)
        )
    ).all()
    assert [(r[0], int(r[1])) for r in rows] == [
        (1, 23),
        (2, 23),
        (3, 26),
        (4, 26),
    ]


async def test_teacher_qualifications_reference_existing_rows(
    seeded_session: AsyncSession,
) -> None:
    rows = (
        await seeded_session.execute(
            select(TeacherQualification.teacher_id, TeacherQualification.subject_id)
        )
    ).all()
    teacher_ids = {row[0] for row in (await seeded_session.execute(select(Teacher.id))).all()}
    subject_ids = {row[0] for row in (await seeded_session.execute(select(Subject.id))).all()}
    for tq_teacher, tq_subject in rows:
        assert tq_teacher in teacher_ids
        assert tq_subject in subject_ids


async def test_room_suitabilities_encode_specialty_split(
    seeded_session: AsyncSession,
) -> None:
    rows = (
        await seeded_session.execute(
            select(Room.short_name, Subject.short_name)
            .join(RoomSubjectSuitability, RoomSubjectSuitability.room_id == Room.id)
            .join(Subject, Subject.id == RoomSubjectSuitability.subject_id)
        )
    ).all()
    pairs = {(r[0], r[1]) for r in rows}

    for klassenraum in ("1a", "2a", "3a", "4a"):
        for subject in ("D", "M", "SU", "RK", "RE", "ETH", "E", "FÖ"):
            assert (klassenraum, subject) in pairs, (klassenraum, subject)

    assert ("TH", "SP") in pairs
    assert ("MU-R", "MU") in pairs
    assert ("KU-R", "KU") in pairs

    specialty_subjects = {"SP", "MU", "KU"}
    for klassenraum in ("1a", "2a", "3a", "4a"):
        for subject in specialty_subjects:
            assert (klassenraum, subject) not in pairs, (klassenraum, subject)


async def test_week_scheme_has_expected_period_times(
    seeded_session: AsyncSession,
) -> None:
    scheme = (
        await seeded_session.execute(
            select(WeekScheme).where(WeekScheme.name == "Grundschule Zeitraster")
        )
    ).scalar_one()
    rows = (
        await seeded_session.execute(
            select(TimeBlock.position, TimeBlock.start_time, TimeBlock.end_time)
            .where(TimeBlock.week_scheme_id == scheme.id, TimeBlock.day_of_week == 0)
            .order_by(TimeBlock.position)
        )
    ).all()
    assert [(r[0], r[1], r[2]) for r in rows] == [
        (1, time(8, 0), time(8, 45)),
        (2, time(8, 45), time(9, 30)),
        (3, time(9, 50), time(10, 35)),
        (4, time(10, 35), time(11, 20)),
        (5, time(11, 35), time(12, 20)),
        (6, time(12, 20), time(13, 5)),
    ]
