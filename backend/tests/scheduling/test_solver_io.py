"""Tests for solver_io.py — Problem building, solve runner, per-class filter."""

import json
import logging
import uuid
from collections.abc import Awaitable, Callable
from datetime import time
from typing import NamedTuple
from uuid import UUID, uuid4

import pytest
from fastapi import HTTPException
from sqlalchemy import delete, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.room import Room, RoomAvailability
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import Stundentafel
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import (
    Teacher,
    TeacherAvailability,
    TeacherQualification,
)
from klassenzeit_backend.db.models.week_scheme import TimeBlock, WeekScheme
from klassenzeit_backend.scheduling import solver_io
from klassenzeit_backend.scheduling.solver_io import (
    _VIOLATION_KINDS,
    _count_violations_by_kind,
    build_problem_json,
    collect_pinned_placements,
    filter_solution_for_class,
    persist_solution_for_all_classes,
    read_schedule_for_room,
    read_schedule_for_teacher,
    run_solve,
)
from tests.scheduling.conftest import SeededClassWithPlacements

# Type aliases matching the factory fixtures defined in conftest.py
type CreateSubjectFn = Callable[..., Awaitable[Subject]]
type CreateWeekSchemeFn = Callable[..., Awaitable[WeekScheme]]
type CreateTimeBlockFn = Callable[..., Awaitable[TimeBlock]]
type CreateRoomFn = Callable[..., Awaitable[Room]]
type CreateTeacherFn = Callable[..., Awaitable[Teacher]]
type CreateStundentafelFn = Callable[..., Awaitable[Stundentafel]]
type CreateSchoolClassFn = Callable[..., Awaitable[SchoolClass]]


_EMPTY_QUALITY_REPORT: dict[str, int] = {
    "hard_violations": 0,
    "unplaced_hours": 0,
    "class_gap_hours": 0,
    "teacher_gap_hours": 0,
    "class_day_balance_cost": 0,
    "home_room_misses": 0,
    "prefer_early_units": 0,
    "avoid_first_units": 0,
    "avoid_last_units": 0,
    "prefer_late_units": 0,
    "prefer_class_teacher_misses": 0,
    "weighted_score": 0,
    "worst_per_class_spread": 0,
    "worst_per_class_interior_gaps": 0,
}


def test_filter_solution_for_class_keeps_only_class_lessons() -> None:
    class_lesson = uuid4()
    other_lesson = uuid4()
    solution = {
        "placements": [
            {
                "lesson_id": str(class_lesson),
                "time_block_id": str(uuid4()),
                "room_id": str(uuid4()),
            },
            {
                "lesson_id": str(other_lesson),
                "time_block_id": str(uuid4()),
                "room_id": str(uuid4()),
            },
        ],
        "violations": [],
        "quality_report": _EMPTY_QUALITY_REPORT,
    }
    filtered = filter_solution_for_class(solution, {class_lesson})
    assert len(filtered["placements"]) == 1
    assert UUID(filtered["placements"][0]["lesson_id"]) == class_lesson


def test_filter_solution_for_class_drops_violations_for_other_classes() -> None:
    class_lesson = uuid4()
    other_lesson = uuid4()
    solution = {
        "placements": [],
        "violations": [
            {
                "kind": "teacher_over_capacity",
                "lesson_id": str(class_lesson),
                "hour_index": 0,
            },
            {
                "kind": "no_suitable_room",
                "lesson_id": str(other_lesson),
                "hour_index": 0,
            },
        ],
        "quality_report": _EMPTY_QUALITY_REPORT,
    }
    filtered = filter_solution_for_class(solution, {class_lesson})
    assert len(filtered["violations"]) == 1
    assert UUID(filtered["violations"][0]["lesson_id"]) == class_lesson


def test_filter_solution_for_class_empty_input() -> None:
    filtered = filter_solution_for_class(
        {"placements": [], "violations": [], "quality_report": _EMPTY_QUALITY_REPORT},
        set(),
    )
    assert filtered == {
        "placements": [],
        "violations": [],
        "soft_score": 0,
        "quality_report": _EMPTY_QUALITY_REPORT,
        "was_cancelled": False,
    }


# ─── build_problem_json tests ──────────────────────────────────────────────


class _SeededSchool(NamedTuple):
    """Bundle of ORM instances returned by ``_seed_minimal_school``."""

    subject: Subject
    scheme: WeekScheme
    block: TimeBlock
    room: Room
    teacher: Teacher
    tafel: Stundentafel
    cls: SchoolClass
    lesson: Lesson


async def _seed_minimal_school(
    db_session: AsyncSession,
    *,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> _SeededSchool:
    """Seed one of each solver entity, wire up a single lesson + qualification."""
    subject = await create_subject()
    scheme = await create_week_scheme()
    block = await create_time_block(week_scheme_id=scheme.id, position=1)
    room = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    qualification = TeacherQualification(teacher_id=teacher.id, subject_id=subject.id)
    db_session.add(qualification)
    await db_session.flush()
    return _SeededSchool(
        subject=subject,
        scheme=scheme,
        block=block,
        room=room,
        teacher=teacher,
        tafel=tafel,
        cls=cls,
        lesson=lesson,
    )


async def test_build_problem_json_returns_populated_shape(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    cls = seeded.cls
    lesson = seeded.lesson

    problem_json, class_lesson_ids, counts = await build_problem_json(db_session, cls.id)

    problem = json.loads(problem_json)
    expected_keys = {
        "time_blocks",
        "teachers",
        "rooms",
        "subjects",
        "school_classes",
        "lessons",
        "teacher_qualifications",
        "teacher_blocked_times",
        "room_blocked_times",
        "room_subject_suitabilities",
        "pinned_placements",
    }
    assert set(problem.keys()) == expected_keys

    assert len(problem["time_blocks"]) == 1
    assert len(problem["teachers"]) == 1
    assert len(problem["rooms"]) == 1
    assert len(problem["subjects"]) == 1
    assert len(problem["school_classes"]) == 1
    assert len(problem["lessons"]) == 1
    assert len(problem["teacher_qualifications"]) == 1
    assert problem["teacher_blocked_times"] == []
    assert problem["room_blocked_times"] == []
    assert problem["room_subject_suitabilities"] == []

    assert class_lesson_ids == {lesson.id}

    assert counts == {
        "time_blocks": 1,
        "teachers": 1,
        "rooms": 1,
        "subjects": 1,
        "school_classes": 1,
        "lessons": 1,
        "teacher_qualifications": 1,
        "teacher_blocked_times": 0,
        "room_blocked_times": 0,
        "room_subject_suitabilities": 0,
    }


async def test_build_problem_json_raises_404_for_unknown_class(
    db_session: AsyncSession,
) -> None:
    with pytest.raises(HTTPException) as excinfo:
        await build_problem_json(db_session, uuid.uuid4())
    assert excinfo.value.status_code == 404


async def test_build_problem_json_raises_422_when_week_scheme_has_no_time_blocks(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    cls = seeded.cls
    scheme = seeded.scheme

    # Drop the only TimeBlock for this scheme.
    await db_session.execute(delete(TimeBlock).where(TimeBlock.week_scheme_id == scheme.id))
    await db_session.flush()

    with pytest.raises(HTTPException) as excinfo:
        await build_problem_json(db_session, cls.id)
    assert excinfo.value.status_code == 422
    assert "time_blocks" in excinfo.value.detail.lower()


async def test_build_problem_json_raises_422_when_rooms_table_empty(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    cls = seeded.cls

    # Clear Room table (cascade handles RoomSubjectSuitability / RoomAvailability).
    await db_session.execute(delete(Room))
    await db_session.flush()

    with pytest.raises(HTTPException) as excinfo:
        await build_problem_json(db_session, cls.id)
    assert excinfo.value.status_code == 422
    assert "rooms" in excinfo.value.detail.lower()


async def test_build_problem_json_raises_422_on_mixed_week_schemes(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    # Base seed gives class A on scheme X with a lesson.
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    cls_a = seeded.cls
    teacher = seeded.teacher
    tafel = seeded.tafel

    # Seed class B on a different week-scheme Y, with its own subject + lesson.
    scheme_y = await create_week_scheme()
    await create_time_block(week_scheme_id=scheme_y.id, position=1)
    cls_b = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme_y.id)
    subject_b = await create_subject()
    lesson_b = Lesson(
        subject_id=subject_b.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson_b)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson_b.id, school_class_id=cls_b.id))
    await db_session.flush()

    with pytest.raises(HTTPException) as excinfo:
        await build_problem_json(db_session, cls_a.id)
    assert excinfo.value.status_code == 422
    assert "week_scheme" in excinfo.value.detail


async def test_build_problem_json_transforms_teacher_availability_status_blocked(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    cls = seeded.cls
    teacher = seeded.teacher
    block = seeded.block

    db_session.add(
        TeacherAvailability(teacher_id=teacher.id, time_block_id=block.id, status="blocked")
    )
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    problem = json.loads(problem_json)

    assert len(problem["teacher_blocked_times"]) == 1
    entry = problem["teacher_blocked_times"][0]
    assert entry == {
        "teacher_id": str(teacher.id),
        "time_block_id": str(block.id),
    }


async def test_build_problem_json_teacher_availability_available_is_not_blocked(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    cls = seeded.cls
    teacher = seeded.teacher
    block = seeded.block

    db_session.add(
        TeacherAvailability(teacher_id=teacher.id, time_block_id=block.id, status="available")
    )
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    problem = json.loads(problem_json)

    assert problem["teacher_blocked_times"] == []


async def test_build_problem_json_transforms_room_availability_whitelist(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    # Base seed: one TimeBlock at position 1 already. Add two more at positions 2 and 3.
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    cls = seeded.cls
    room = seeded.room
    scheme = seeded.scheme
    block_p1 = seeded.block

    block_p2 = await create_time_block(
        week_scheme_id=scheme.id,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    block_p3 = await create_time_block(
        week_scheme_id=scheme.id,
        position=3,
        start_time=time(9, 30),
        end_time=time(10, 15),
    )

    # Whitelist the room for position 1 only.
    db_session.add(RoomAvailability(room_id=room.id, time_block_id=block_p1.id))
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    problem = json.loads(problem_json)

    blocked = problem["room_blocked_times"]
    assert len(blocked) == 2
    blocked_time_block_ids = {entry["time_block_id"] for entry in blocked}
    assert blocked_time_block_ids == {str(block_p2.id), str(block_p3.id)}
    for entry in blocked:
        assert entry["room_id"] == str(room.id)


async def test_build_problem_json_room_without_whitelist_is_unblocked(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    cls = seeded.cls

    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    problem = json.loads(problem_json)

    assert problem["room_blocked_times"] == []


# ─── run_solve tests ───────────────────────────────────────────────────────


def _minimal_runnable_problem() -> dict:
    teacher = str(uuid4())
    tb = str(uuid4())
    subject = str(uuid4())
    room = str(uuid4())
    klass = str(uuid4())
    lesson = str(uuid4())
    return {
        "time_blocks": [{"id": tb, "day_of_week": 0, "position": 0}],
        "teachers": [{"id": teacher, "max_hours_per_week": 5}],
        "rooms": [{"id": room}],
        "subjects": [
            {
                "id": subject,
                "prefer_early_period": 0,
                "avoid_first_period": 0,
                "avoid_last_period": 0,
            }
        ],
        "school_classes": [{"id": klass}],
        "lessons": [
            {
                "id": lesson,
                "school_class_ids": [klass],
                "subject_id": subject,
                "teacher_candidates": [teacher],
                "teacher_pin": teacher,
                "hours_per_week": 1,
            }
        ],
        "teacher_qualifications": [{"teacher_id": teacher, "subject_id": subject}],
        "teacher_blocked_times": [],
        "room_blocked_times": [],
        "room_subject_suitabilities": [],
    }


async def test_build_problem_json_emits_subject_preference_flags(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """solver_io must emit all three subject preference flags so Rust sees them."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    # Flip the flags on the seeded subject.
    seeded.subject.prefer_early_period = 1
    seeded.subject.avoid_first_period = 0
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id)
    problem = json.loads(problem_json)

    matched = next(s for s in problem["subjects"] if s["id"] == str(seeded.subject.id))
    assert matched["prefer_early_period"] == 1
    assert matched["avoid_first_period"] == 0
    assert matched["avoid_last_period"] == 0


async def test_run_solve_round_trips_and_logs(caplog: pytest.LogCaptureFixture) -> None:
    caplog.set_level(logging.INFO, logger="klassenzeit_backend.scheduling.solver_io")
    class_id = uuid4()
    solution = await run_solve(
        json.dumps(_minimal_runnable_problem()),
        class_id,
        {"lessons": 1, "time_blocks": 1},
        deadline_ms=200,
    )
    assert len(solution["placements"]) == 1
    assert solution["violations"] == []

    messages = [r.message for r in caplog.records]
    assert "solver.solve.start" in messages
    assert "solver.solve.done" in messages

    done = next(r for r in caplog.records if r.message == "solver.solve.done")
    assert done.__dict__["violations_by_kind"] == dict.fromkeys(_VIOLATION_KINDS, 0)


async def test_run_solve_logs_error_and_reraises(caplog: pytest.LogCaptureFixture) -> None:
    caplog.set_level(logging.ERROR, logger="klassenzeit_backend.scheduling.solver_io")
    class_id = uuid4()
    with pytest.raises(ValueError):
        await run_solve("not json", class_id, {"lessons": 0}, deadline_ms=200)
    messages = [r.message for r in caplog.records]
    assert "solver.solve.error" in messages


async def test_run_solve_passes_deadline_ms_to_binding(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    seen: dict[str, object] = {}

    def fake_solve_json_with_config(problem_json: str, deadline_ms: int | None) -> str:
        seen["deadline_ms"] = deadline_ms
        return '{"placements": [], "violations": [], "soft_score": 0}'

    monkeypatch.setattr(
        "klassenzeit_backend.scheduling.solver_io._solve_json_with_config",
        fake_solve_json_with_config,
    )

    await run_solve(
        '{"teachers":[]}',
        uuid.uuid4(),
        {
            "lessons": 0,
            "teachers": 0,
            "rooms": 0,
            "subjects": 0,
            "school_classes": 0,
            "teacher_qualifications": 0,
            "teacher_blocked_times": 0,
            "room_blocked_times": 0,
            "room_subject_suitabilities": 0,
        },
        deadline_ms=0,
    )
    assert seen["deadline_ms"] == 0


def test_count_violations_by_kind_clean_solve_returns_zeros() -> None:
    counts = _count_violations_by_kind([])
    assert counts == dict.fromkeys(_VIOLATION_KINDS, 0)
    assert set(counts) == {
        "no_qualified_teacher",
        "teacher_over_capacity",
        "no_free_time_block",
        "no_suitable_room",
        "lesson_group_split",
        "pinned_conflict",
        "subject_daily_hour_cap_exceeded",
        "class_daily_lesson_cap_exceeded",
        "class_subject_teacher_split",
    }


async def test_build_problem_json_includes_preferred_block_size(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """``preferred_block_size`` is forwarded to the solver per lesson."""
    subject = await create_subject()
    scheme = await create_week_scheme()
    await create_time_block(week_scheme_id=scheme.id, position=1)
    await create_time_block(
        week_scheme_id=scheme.id,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=2,
        preferred_block_size=2,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    problem = json.loads(problem_json)

    assert len(problem["lessons"]) == 1
    assert problem["lessons"][0]["preferred_block_size"] == 2


def test_count_violations_by_kind_aggregates_mixed_kinds() -> None:
    violations: list[dict] = [
        {"kind": "no_free_time_block", "lesson_id": "x", "hour_index": 0},
        {"kind": "no_qualified_teacher", "lesson_id": "y", "hour_index": 0},
        {"kind": "no_free_time_block", "lesson_id": "z", "hour_index": 0},
    ]
    counts = _count_violations_by_kind(violations)
    assert counts == {
        "no_qualified_teacher": 1,
        "teacher_over_capacity": 0,
        "no_free_time_block": 2,
        "no_suitable_room": 0,
        "lesson_group_split": 0,
        "pinned_conflict": 0,
        "subject_daily_hour_cap_exceeded": 0,
        "class_daily_lesson_cap_exceeded": 0,
        "class_subject_teacher_split": 0,
    }


async def test_build_problem_json_emits_school_class_ids_array_for_multi_class_lesson(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """A lesson with two memberships round-trips as a two-element school_class_ids array."""
    subject = await create_subject()
    scheme = await create_week_scheme()
    await create_time_block(week_scheme_id=scheme.id, position=1)
    await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls_a = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    cls_b = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    group_id = uuid4()
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
        lesson_group_id=group_id,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls_a.id))
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls_b.id))
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    await db_session.flush()

    problem_json, class_lesson_ids, _ = await build_problem_json(db_session, cls_a.id)
    problem = json.loads(problem_json)

    assert len(problem["lessons"]) == 1
    serialized = problem["lessons"][0]
    assert set(serialized["school_class_ids"]) == {str(cls_a.id), str(cls_b.id)}
    assert serialized["lesson_group_id"] == str(group_id)
    # The lesson belongs to the requested class via membership.
    assert lesson.id in class_lesson_ids


async def test_build_problem_json_emits_null_lesson_group_id_when_unset(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """A lesson with no lesson_group_id serialises ``lesson_group_id: null``."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id)
    problem = json.loads(problem_json)
    assert problem["lessons"][0]["lesson_group_id"] is None


async def test_build_problem_json_emits_home_room_id_per_school_class(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """When a school class has a home_room_id set, build_problem_json echoes it."""
    home_room = await create_room()
    scheme = await create_week_scheme()
    await create_time_block(week_scheme_id=scheme.id, position=1)
    subject = await create_subject()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        home_room_id=home_room.id,
    )
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    payload = json.loads(problem_json)
    sc_entry = next(c for c in payload["school_classes"] if c["id"] == str(cls.id))
    assert sc_entry["home_room_id"] == str(home_room.id)


async def test_build_problem_json_emits_subject_max_hours_per_day(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """``Subject.max_hours_per_day`` is forwarded to the solver per subject row."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    seeded.subject.max_hours_per_day = 3
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id)
    problem = json.loads(problem_json)

    matched = next(s for s in problem["subjects"] if s["id"] == str(seeded.subject.id))
    assert matched["max_hours_per_day"] == 3


async def test_build_problem_json_emits_school_class_max_lessons_per_day_when_set(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """When a class has max_lessons_per_day set, build_problem_json echoes it."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    seeded.cls.max_lessons_per_day = 4
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id)
    payload = json.loads(problem_json)
    sc_entry = next(c for c in payload["school_classes"] if c["id"] == str(seeded.cls.id))
    assert sc_entry["max_lessons_per_day"] == 4


async def test_build_problem_json_emits_null_max_lessons_per_day_when_class_has_no_cap(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """When a class has no max_lessons_per_day, build_problem_json emits null."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id)
    payload = json.loads(problem_json)
    sc_entry = next(c for c in payload["school_classes"] if c["id"] == str(seeded.cls.id))
    assert sc_entry["max_lessons_per_day"] is None


async def test_run_solve_honors_subject_daily_hour_cap(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """A 4-hour-per-week subject with cap=2 distributes across multiple days."""
    subject = await create_subject()
    subject.max_hours_per_day = 2
    scheme = await create_week_scheme()
    blocks = []
    for d in range(5):
        for p in range(5):
            blocks.append(
                await create_time_block(
                    week_scheme_id=scheme.id,
                    day_of_week=d,
                    position=p,
                    start_time=time(8 + p, 0),
                    end_time=time(9 + p, 0),
                )
            )
    await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
    )
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=4,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    await db_session.flush()

    problem_json, _, counts = await build_problem_json(db_session, cls.id)
    solution = await run_solve(problem_json, cls.id, counts, deadline_ms=0)

    placements = solution["placements"]
    assert len(placements) == 4
    block_lookup = {str(b.id): b for b in blocks}
    by_day: dict[int, int] = {}
    for p in placements:
        day = block_lookup[p["time_block_id"]].day_of_week
        by_day[day] = by_day.get(day, 0) + 1
    for day, count in by_day.items():
        assert count <= 2, f"day {day} has {count} hours of subject; cap is 2"


async def test_build_problem_json_emits_null_home_room_id_when_class_has_no_home_room(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """When a school class has no home_room_id, build_problem_json emits null."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id)
    payload = json.loads(problem_json)
    sc_entry = next(c for c in payload["school_classes"] if c["id"] == str(seeded.cls.id))
    assert sc_entry["home_room_id"] is None


async def test_build_problem_json_emits_class_teacher_id_when_set(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """When a class has class_teacher_id set, build_problem_json echoes it."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    seeded.cls.class_teacher_id = seeded.teacher.id
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id)
    payload = json.loads(problem_json)
    sc_entry = next(c for c in payload["school_classes"] if c["id"] == str(seeded.cls.id))
    assert sc_entry["class_teacher_id"] == str(seeded.teacher.id)


async def test_build_problem_json_emits_null_class_teacher_id_when_unset(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """When a class has no class_teacher_id, build_problem_json emits null."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id)
    payload = json.loads(problem_json)
    sc_entry = next(c for c in payload["school_classes"] if c["id"] == str(seeded.cls.id))
    assert sc_entry["class_teacher_id"] is None


# ─── collect_pinned_placements tests ──────────────────────────────────────


async def test_collect_pinned_placements_excludes_target_class(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """When exclude_class_ids contains class A, ScheduledLesson rows for class A
    are excluded; class B's persisted placement is returned as a wire-format pin.
    """
    subject = await create_subject()
    scheme = await create_week_scheme()
    tb_a = await create_time_block(week_scheme_id=scheme.id, position=1)
    tb_b = await create_time_block(
        week_scheme_id=scheme.id,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    room_a = await create_room()
    room_b = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    class_a = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    class_b = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)

    lesson_a = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    lesson_b = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add_all([lesson_a, lesson_b])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=lesson_a.id, school_class_id=class_a.id),
            LessonSchoolClass(lesson_id=lesson_b.id, school_class_id=class_b.id),
            ScheduledLesson(
                lesson_id=lesson_a.id,
                time_block_id=tb_a.id,
                room_id=room_a.id,
                teacher_id=teacher.id,
            ),
            ScheduledLesson(
                lesson_id=lesson_b.id,
                time_block_id=tb_b.id,
                room_id=room_b.id,
                teacher_id=teacher.id,
            ),
        ]
    )
    await db_session.flush()

    pins = await collect_pinned_placements(db_session, {class_a.id})

    assert len(pins) == 1
    assert pins[0]["lesson_id"] == str(lesson_b.id)
    assert pins[0]["time_block_id"] == str(tb_b.id)
    assert pins[0]["room_id"] == str(room_b.id)


async def test_build_problem_json_threads_pinned_placements_into_wire_format(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """build_problem_json embeds pinned_placements verbatim into the returned wire JSON."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    pins = [
        {
            "lesson_id": "00000000-0000-0000-0000-000000000001",
            "time_block_id": "00000000-0000-0000-0000-000000000002",
            "room_id": "00000000-0000-0000-0000-000000000003",
        }
    ]

    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id, pinned_placements=pins)
    parsed = json.loads(problem_json)

    assert parsed["pinned_placements"] == pins


async def test_build_problem_json_omits_pinned_placements_defaults_to_empty_list(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """Callers omitting pinned_placements get an empty list in the wire format."""
    seeded = await _seed_minimal_school(
        db_session,
        create_subject=create_subject,
        create_week_scheme=create_week_scheme,
        create_time_block=create_time_block,
        create_room=create_room,
        create_teacher=create_teacher,
        create_stundentafel=create_stundentafel,
        create_school_class=create_school_class,
    )
    problem_json, _, _ = await build_problem_json(db_session, seeded.cls.id)
    parsed = json.loads(problem_json)
    assert parsed["pinned_placements"] == []


async def test_build_problem_json_includes_null_teacher_lessons(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """build_problem_json includes Lessons with teacher_id=None.

    The Lesson is emitted with teacher_pin=None and a non-empty
    teacher_candidates list (qualified active teachers with availability
    that overlaps the class's slots). Per OPEN_THINGS item 63, null on the
    Lesson means "let the solver decide", not "exclude from the problem."
    """
    subject = await create_subject()
    scheme = await create_week_scheme()
    tb = await create_time_block(week_scheme_id=scheme.id, position=1)
    await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=None,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    db_session.add(
        TeacherAvailability(teacher_id=teacher.id, time_block_id=tb.id, status="available")
    )
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    problem = json.loads(problem_json)
    [emitted_lesson] = [item for item in problem["lessons"] if item["id"] == str(lesson.id)]
    assert emitted_lesson["teacher_pin"] is None
    assert len(emitted_lesson["teacher_candidates"]) >= 1
    assert str(teacher.id) in emitted_lesson["teacher_candidates"]


async def test_build_problem_json_emits_teacher_candidates_and_pin(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """``build_problem_json`` emits per-Lesson ``teacher_candidates`` + ``teacher_pin``.

    Candidates: qualified teachers whose availability overlaps the class's
    time blocks; sorted by teacher uuid ascending; the pin appears first.
    The legacy ``teacher_id`` key is gone.
    """
    subject = await create_subject()
    scheme = await create_week_scheme()
    tb = await create_time_block(week_scheme_id=scheme.id, position=1)
    await create_room()
    teacher_a = await create_teacher()
    teacher_b = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher_a.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    db_session.add(TeacherQualification(teacher_id=teacher_a.id, subject_id=subject.id))
    db_session.add(TeacherQualification(teacher_id=teacher_b.id, subject_id=subject.id))
    db_session.add(
        TeacherAvailability(teacher_id=teacher_a.id, time_block_id=tb.id, status="available")
    )
    db_session.add(
        TeacherAvailability(teacher_id=teacher_b.id, time_block_id=tb.id, status="available")
    )
    await db_session.flush()

    problem_json, _, _ = await build_problem_json(db_session, cls.id)
    problem = json.loads(problem_json)
    [emitted_lesson] = [item for item in problem["lessons"] if item["id"] == str(lesson.id)]
    assert "teacher_candidates" in emitted_lesson
    assert "teacher_pin" in emitted_lesson
    assert emitted_lesson["teacher_pin"] == str(teacher_a.id)
    assert str(teacher_a.id) in emitted_lesson["teacher_candidates"]
    assert str(teacher_b.id) in emitted_lesson["teacher_candidates"]
    # Pin appears first in the candidate list.
    assert emitted_lesson["teacher_candidates"][0] == str(teacher_a.id)
    # The legacy teacher_id key is gone.
    assert "teacher_id" not in emitted_lesson


async def test_collect_pinned_placements_returns_empty_when_all_excluded(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """exclude_class_ids covering every class returns an empty list."""
    subject = await create_subject()
    scheme = await create_week_scheme()
    tb = await create_time_block(week_scheme_id=scheme.id, position=1)
    room = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    class_a = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)

    lesson_a = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson_a)
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=lesson_a.id, school_class_id=class_a.id),
            ScheduledLesson(
                lesson_id=lesson_a.id,
                time_block_id=tb.id,
                room_id=room.id,
                teacher_id=teacher.id,
            ),
        ]
    )
    await db_session.flush()

    pins = await collect_pinned_placements(db_session, {class_a.id})

    assert pins == []


# ─── persist_solution_for_all_classes tests ───────────────────────────────


async def test_persist_solution_for_all_classes_writes_every_class(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    """Persisting a multi-class solution writes ScheduledLesson rows for every
    class and returns per-class summaries with correct counts."""
    subject = await create_subject()
    scheme = await create_week_scheme()
    tb_a = await create_time_block(week_scheme_id=scheme.id, position=1)
    tb_b = await create_time_block(
        week_scheme_id=scheme.id,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    room = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    class_a = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    class_b = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)

    lesson_a = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    lesson_b = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add_all([lesson_a, lesson_b])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=lesson_a.id, school_class_id=class_a.id),
            LessonSchoolClass(lesson_id=lesson_b.id, school_class_id=class_b.id),
            TeacherQualification(teacher_id=teacher.id, subject_id=subject.id),
        ]
    )
    await db_session.flush()

    solution = {
        "placements": [
            {
                "lesson_id": str(lesson_a.id),
                "time_block_id": str(tb_a.id),
                "room_id": str(room.id),
                "teacher_id": str(teacher.id),
            },
            {
                "lesson_id": str(lesson_b.id),
                "time_block_id": str(tb_b.id),
                "room_id": str(room.id),
                "teacher_id": str(teacher.id),
            },
        ],
        "violations": [],
    }

    summaries = await persist_solution_for_all_classes(db_session, solution)

    persisted = (await db_session.execute(select(ScheduledLesson))).scalars().all()
    assert len(persisted) == 2

    expected_class_ids = sorted([class_a.id, class_b.id])
    assert [s.class_id for s in summaries] == expected_class_ids
    for summary in summaries:
        assert summary.placements_count == 1
        assert summary.violations_count == 0


async def test_read_schedule_for_teacher_404_when_missing(
    db_session: AsyncSession,
) -> None:
    with pytest.raises(HTTPException) as excinfo:
        await read_schedule_for_teacher(db_session, uuid.uuid4())
    assert excinfo.value.status_code == 404


async def test_read_schedule_for_teacher_returns_empty_when_no_placements(
    db_session: AsyncSession,
    create_teacher: CreateTeacherFn,
) -> None:
    teacher = await create_teacher()
    rows = await read_schedule_for_teacher(db_session, teacher.id)
    assert rows == []


async def test_read_schedule_for_teacher_returns_placements_for_teachers_lessons(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    subject = await create_subject()
    scheme = await create_week_scheme()
    block = await create_time_block(
        week_scheme_id=scheme.id,
        position=0,
        start_time=time(8, 0),
        end_time=time(8, 45),
    )
    room = await create_room()
    teacher = await create_teacher()
    other_teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
    )
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson.id,
            time_block_id=block.id,
            room_id=room.id,
            teacher_id=teacher.id,
        )
    )
    await db_session.flush()

    rows = await read_schedule_for_teacher(db_session, teacher.id)
    assert len(rows) == 1
    assert rows[0].lesson_id == lesson.id
    assert rows[0].time_block_id == block.id
    assert rows[0].room_id == room.id

    # Other teacher sees nothing.
    other_rows = await read_schedule_for_teacher(db_session, other_teacher.id)
    assert other_rows == []


async def test_read_schedule_for_room_404_when_missing(
    db_session: AsyncSession,
) -> None:
    with pytest.raises(HTTPException) as excinfo:
        await read_schedule_for_room(db_session, uuid.uuid4())
    assert excinfo.value.status_code == 404


async def test_read_schedule_for_room_returns_empty_when_no_placements(
    db_session: AsyncSession,
    create_room: CreateRoomFn,
) -> None:
    room = await create_room()
    rows = await read_schedule_for_room(db_session, room.id)
    assert rows == []


async def test_collect_own_class_pins_returns_only_pinned_rows_for_class(
    db_session: AsyncSession,
    seeded_class_with_two_placements: SeededClassWithPlacements,
) -> None:
    """Pinned rows in the class are returned; unpinned rows are not."""
    pins = await solver_io.collect_own_class_pins(
        db_session, seeded_class_with_two_placements.class_id
    )
    pinned_ids = {pin["lesson_id"] for pin in pins}
    assert seeded_class_with_two_placements.pinned_lesson_id_str in pinned_ids
    assert seeded_class_with_two_placements.unpinned_lesson_id_str not in pinned_ids


async def test_collect_own_class_pins_empty_when_class_has_none(
    db_session: AsyncSession,
    seeded_class_without_pins: uuid.UUID,
) -> None:
    pins = await solver_io.collect_own_class_pins(db_session, seeded_class_without_pins)
    assert pins == []


async def test_read_schedule_for_room_returns_placements_for_rooms_lessons(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> None:
    subject = await create_subject()
    scheme = await create_week_scheme()
    block = await create_time_block(
        week_scheme_id=scheme.id,
        position=0,
        start_time=time(8, 0),
        end_time=time(8, 45),
    )
    room = await create_room()
    other_room = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
    )
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson.id,
            time_block_id=block.id,
            room_id=room.id,
            teacher_id=teacher.id,
        )
    )
    await db_session.flush()

    rows = await read_schedule_for_room(db_session, room.id)
    assert len(rows) == 1
    assert rows[0].lesson_id == lesson.id
    assert rows[0].time_block_id == block.id
    assert rows[0].room_id == room.id

    # Other room sees nothing.
    other_rows = await read_schedule_for_room(db_session, other_room.id)
    assert other_rows == []
