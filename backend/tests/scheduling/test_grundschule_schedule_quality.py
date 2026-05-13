"""Integration test: demo Grundschule schedule must clear the quality bar.

Seeds the demo Grundschule, drives lesson generation and the per-class
solve through the production HTTP routes, then asserts every quality
predicate (room hops, daily-load balance, home-room ratio, interior
gaps, day length) returns no issues for the persisted schedule.

This guards against future solver / weight / seed changes producing
visually bad schedules without a hard-violation gate to catch them.
The test opts into the production 5000 ms LAHC pass (the rest of the
backend test suite stays greedy-only via the per-backend zero entries
in ``backend/.env.test``, per ADR 0038) because the soft costs the
new constraints rely on are LAHC-driven;
greedy alone produces a lopsided baseline that cannot pass the bar.
"""

from collections.abc import Awaitable, Callable
from uuid import UUID

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.models.week_scheme import TimeBlock
from klassenzeit_backend.main import app
from klassenzeit_backend.scheduling.quality_checks import (
    Placement,
    QualityIssue,
    check_class_day_balance,
    check_class_teacher_subject_share,
    check_day_length,
    check_home_room_ratio,
    check_interior_gaps,
    check_room_hop,
)
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule

CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
LoginFn = Callable[[str, str], Awaitable[None]]

MIN_KLASSENLEHRER_SHARE: float = 0.5


async def _load_placements(db: AsyncSession) -> list[Placement]:
    """Project persisted ScheduledLesson rows into Placement records.

    A lesson can serve multiple classes via lesson_school_classes; each
    membership produces its own Placement so per-class predicates see
    every class the lesson lands in.
    """
    rows = (
        await db.execute(
            select(
                ScheduledLesson.lesson_id,
                ScheduledLesson.time_block_id,
                ScheduledLesson.room_id,
                Lesson.subject_id,
                LessonSchoolClass.school_class_id,
                TimeBlock.day_of_week,
                TimeBlock.position,
            )
            .join(Lesson, Lesson.id == ScheduledLesson.lesson_id)
            .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
            .join(TimeBlock, TimeBlock.id == ScheduledLesson.time_block_id)
        )
    ).all()
    return [
        Placement(
            class_id=row.school_class_id,
            day=row.day_of_week,
            subject_id=row.subject_id,
            room_id=row.room_id,
            lesson_id=row.lesson_id,
            time_block_id=row.time_block_id,
            position=row.position,
        )
        for row in rows
    ]


async def _load_class_teacher_lookup_grundschule(
    db: AsyncSession,
) -> dict[UUID, UUID | None]:
    """Return `{school_class_id: class_teacher_id_or_none}` over all classes."""
    rows = (await db.execute(select(SchoolClass.id, SchoolClass.class_teacher_id))).all()
    return {row.id: row.class_teacher_id for row in rows}


async def _load_placement_teacher_lookup_grundschule(
    db: AsyncSession,
) -> dict[UUID, UUID]:
    """Return `{lesson_id: teacher_id}` over every persisted ScheduledLesson.

    Per item 65 `ScheduledLesson.teacher_id` is non-null on solver output;
    the dict-comprehension below relies on that and would silently drop a
    row with `teacher_id=None`, which is the intent (a None slot would
    indicate a regression worth surfacing elsewhere).
    """
    rows = (await db.execute(select(ScheduledLesson.lesson_id, ScheduledLesson.teacher_id))).all()
    return {row.lesson_id: row.teacher_id for row in rows if row.teacher_id is not None}


def _counts_per_class(placements: list[Placement]) -> dict[UUID, list[int]]:
    """Build per-class daily-count lists indexed by day 0..4."""
    counts: dict[UUID, list[int]] = {}
    for placement in placements:
        per_day = counts.setdefault(placement.class_id, [0, 0, 0, 0, 0])
        if 0 <= placement.day <= 4:
            per_day[placement.day] += 1
    return counts


def _positions_per_class_day(
    placements: list[Placement],
) -> dict[tuple[UUID, int], list[int]]:
    """Group placement positions by `(class_id, day_of_week)`."""
    positions: dict[tuple[UUID, int], list[int]] = {}
    for placement in placements:
        positions.setdefault((placement.class_id, placement.day), []).append(placement.position)
    return positions


async def test_grundschule_schedule_meets_quality_bar(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setitem(app.state.settings.solve_deadline_ms_by_backend, "lahc_rr", 5000)
    await seed_demo_grundschule(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-quality@example.com",
        password="quality-test-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    classes = (
        (await db_session.execute(select(SchoolClass).order_by(SchoolClass.grade_level)))
        .scalars()
        .all()
    )
    assert [c.name for c in classes] == ["1a", "2a", "3a", "4a"]

    for school_class in classes:
        gen_resp = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
        assert gen_resp.status_code == 201, gen_resp.text

    for school_class in classes:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])

    placements = await _load_placements(db_session)
    assert placements, "expected persisted placements after solving every class"

    home_rooms: dict[UUID, UUID] = {
        c.id: c.home_room_id for c in classes if c.home_room_id is not None
    }

    exempt_short_names = {"SP", "KU", "MU"}
    exempt_subjects: set[UUID] = set(
        (
            await db_session.execute(
                select(Subject.id).where(Subject.short_name.in_(exempt_short_names))
            )
        ).scalars()
    )
    assert len(exempt_subjects) == len(exempt_short_names), (
        f"expected {exempt_short_names} subjects in seed, found {len(exempt_subjects)}"
    )

    counts_per_class = _counts_per_class(placements)
    positions_per_class_day = _positions_per_class_day(placements)
    class_teacher_lookup = await _load_class_teacher_lookup_grundschule(db_session)
    placement_teacher_lookup = await _load_placement_teacher_lookup_grundschule(db_session)

    issues: list[QualityIssue] = []
    issues.extend(check_room_hop(placements))
    issues.extend(check_class_day_balance(counts_per_class, max_spread=2))
    issues.extend(
        check_home_room_ratio(
            placements,
            home_rooms=home_rooms,
            min_ratio=0.6,
            exempt_subjects=exempt_subjects,
        )
    )
    issues.extend(check_interior_gaps(positions_per_class_day, max_gaps_per_class=2))
    issues.extend(check_day_length(placements, max_position=7))
    issues.extend(
        check_class_teacher_subject_share(
            placements,
            class_teacher_lookup=class_teacher_lookup,
            placement_teacher_lookup=placement_teacher_lookup,
        )
    )

    assert issues == [], "demo Grundschule schedule failed quality checks:\n" + "\n".join(
        f"  - {issue}" for issue in issues
    )
