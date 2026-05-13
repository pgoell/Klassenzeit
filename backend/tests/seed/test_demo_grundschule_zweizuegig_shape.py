"""Shape + FK-integrity assertions for seed_demo_grundschule_zweizuegig.

Mirrors the structure of test_demo_grundschule_shape.py but asserts the
zweizuegige counts: 8 classes, 12 teachers, 12 rooms (8 Klassenraeume +
Turnhalle + Sportplatz + Musikraum + Kunstraum), 4 reused Stundentafeln.
Pre-assigned teacher_ids on the lessons-equivalent are verified via
test_demo_grundschule_zweizuegig_solvability.py; this file covers the
entity layer.
"""

from datetime import time

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
from klassenzeit_backend.seed.demo_grundschule_zweizuegig import (
    seed_demo_grundschule_zweizuegig,
)


async def _count_zw(session: AsyncSession, model: type) -> int:
    result = await session.execute(select(func.count()).select_from(model))
    return int(result.scalar_one())


@pytest.fixture
async def seeded_zweizuegig(db_session: AsyncSession) -> AsyncSession:
    await seed_demo_grundschule_zweizuegig(db_session)
    await db_session.flush()
    return db_session


async def test_zweizuegig_creates_expected_entity_counts(
    seeded_zweizuegig: AsyncSession,
) -> None:
    assert await _count_zw(seeded_zweizuegig, Subject) == 11
    assert await _count_zw(seeded_zweizuegig, WeekScheme) == 1
    # 5 days x 8 ordinal positions (6 lesson + 2 Hofpause) = 40 TimeBlocks.
    assert await _count_zw(seeded_zweizuegig, TimeBlock) == 5 * 8
    assert await _count_zw(seeded_zweizuegig, Stundentafel) == 4
    assert await _count_zw(seeded_zweizuegig, SchoolClass) == 8
    assert await _count_zw(seeded_zweizuegig, Teacher) == 12
    assert await _count_zw(seeded_zweizuegig, Room) == 12


async def test_zweizuegig_klassenlehrer_qualifications_cover_their_classes(
    seeded_zweizuegig: AsyncSession,
) -> None:
    # Each Klassenlehrer (MUE, SCH, WEB, FIS, KAI, LAN, NEU, OTT) must be
    # qualified for D + M + SU at minimum; if not, the seed mis-aligned
    # teacher.qualified_subject_short_names with _TEACHER_ASSIGNMENTS.
    klassenlehrer_codes = ("MUE", "SCH", "WEB", "FIS", "KAI", "LAN", "NEU", "OTT")
    teachers = (
        (
            await seeded_zweizuegig.execute(
                select(Teacher).where(Teacher.short_code.in_(klassenlehrer_codes))
            )
        )
        .scalars()
        .all()
    )
    assert len(teachers) == 8
    subjects_d_m_su = (
        (
            await seeded_zweizuegig.execute(
                select(Subject).where(Subject.short_name.in_(("D", "M", "SU")))
            )
        )
        .scalars()
        .all()
    )
    assert len(subjects_d_m_su) == 3
    for teacher in teachers:
        quals = (
            (
                await seeded_zweizuegig.execute(
                    select(TeacherQualification.subject_id).where(
                        TeacherQualification.teacher_id == teacher.id
                    )
                )
            )
            .scalars()
            .all()
        )
        for subject in subjects_d_m_su:
            assert subject.id in quals, (
                f"Klassenlehrer {teacher.short_code} missing qualification for {subject.short_name}"
            )


async def test_zweizuegig_klassenraeume_suit_general_subjects(
    seeded_zweizuegig: AsyncSession,
) -> None:
    # Each Klassenraum (1a..4b, eight rooms) suits exactly the subjects in
    # _KLASSENRAUM_SUITABLE_SUBJECTS reused from demo_grundschule.
    klassenraum_short_names = ("1a", "1b", "2a", "2b", "3a", "3b", "4a", "4b")
    rooms = (
        (
            await seeded_zweizuegig.execute(
                select(Room).where(Room.short_name.in_(klassenraum_short_names))
            )
        )
        .scalars()
        .all()
    )
    assert len(rooms) == 8
    expected_subject_count = 8  # D, M, SU, RK, RE, ETH, E, FOE
    for room in rooms:
        suit_count = await seeded_zweizuegig.scalar(
            select(func.count())
            .select_from(RoomSubjectSuitability)
            .where(RoomSubjectSuitability.room_id == room.id)
        )
        assert suit_count == expected_subject_count, (
            f"Klassenraum {room.short_name} has {suit_count} suitabilities, "
            f"expected {expected_subject_count}"
        )


async def test_zweizuegig_time_blocks_form_full_week_grid(
    seeded_zweizuegig: AsyncSession,
) -> None:
    blocks = (
        (
            await seeded_zweizuegig.execute(
                select(TimeBlock).order_by(TimeBlock.day_of_week, TimeBlock.position)
            )
        )
        .scalars()
        .all()
    )
    assert len(blocks) == 40
    for day in range(5):
        day_blocks = [b for b in blocks if b.day_of_week == day]
        assert [b.position for b in day_blocks] == list(range(1, 9))
        for b in day_blocks:
            assert isinstance(b.start_time, time)
            assert b.start_time < b.end_time


# Stundentafel-entry totals match einzügig (4 grades x same hours table).
# But there are 4 Stundentafeln (one per grade), and each is reused by both Züge.
async def test_zweizuegig_stundentafel_entries_match_einzuegig_total(
    seeded_zweizuegig: AsyncSession,
) -> None:
    # Einzuegig produces 34 StundentafelEntry rows (8 + 8 + 9 + 9). Zweizuegig
    # reuses the same Stundentafeln (1 per grade), so the count is identical.
    assert await _count_zw(seeded_zweizuegig, StundentafelEntry) == 34


async def test_zweizuegig_stundentafeln_are_grundschule_school_type(
    seeded_zweizuegig: AsyncSession,
) -> None:
    tafeln = (await seeded_zweizuegig.execute(select(Stundentafel))).scalars().all()
    assert len(tafeln) == 4
    for tafel in tafeln:
        assert tafel.school_type is SchoolType.GRUNDSCHULE
