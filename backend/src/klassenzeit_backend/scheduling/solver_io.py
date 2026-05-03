"""Solver IO: problem building, solve runner, per-class response filter.

Sits between the route handler and the PyO3 binding. Route handlers use the
three exported helpers (`build_problem_json`, `run_solve`, `filter_solution_for_class`).
"""

from __future__ import annotations

import asyncio
import json
import logging
import time
from typing import TYPE_CHECKING, get_args
from uuid import UUID

from fastapi import HTTPException, status
from sqlalchemy import delete, select

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.room import (
    Room,
    RoomAvailability,
    RoomSubjectSuitability,
)
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import (
    Teacher,
    TeacherAvailability,
    TeacherQualification,
)
from klassenzeit_backend.db.models.week_scheme import TimeBlock
from klassenzeit_backend.scheduling.schemas.schedule import (
    PlacementResponse,
    ViolationResponse,
)
from klassenzeit_solver import solve_json_with_config as _solve_json_with_config

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession

logger = logging.getLogger(__name__)

_VIOLATION_KINDS: tuple[str, ...] = get_args(ViolationResponse.model_fields["kind"].annotation)


def _count_violations_by_kind(violations: list[dict]) -> dict[str, int]:
    """Aggregate a solver-output violation list into per-kind counts.

    Always returns one entry per known ``ViolationKind``. Defensively drops
    unknown kinds so a Rust-only addition cannot crash the log path; an
    unknown kind would already be rejected at the API boundary by Pydantic
    Literal validation, so this guard exists only to keep ``logger.info``
    from raising ``KeyError`` in a hypothetical desync.
    """
    counts = dict.fromkeys(_VIOLATION_KINDS, 0)
    for violation in violations:
        kind = violation["kind"]
        if kind in counts:
            counts[kind] += 1
    return counts


def filter_solution_for_class(solution: dict, class_lesson_ids: set[UUID]) -> dict:
    """Keep only placements and violations whose lesson belongs to this class.

    The school-wide ``soft_score`` is passed through unchanged so the route
    response carries the solver's overall quality signal even though the
    placement list is class-scoped. PR-9c will decide whether to re-score on
    the filtered subset.
    """
    placements = [p for p in solution["placements"] if UUID(p["lesson_id"]) in class_lesson_ids]
    violations = [v for v in solution["violations"] if UUID(v["lesson_id"]) in class_lesson_ids]
    return {
        "placements": placements,
        "violations": violations,
        "soft_score": solution.get("soft_score", 0),
    }


async def build_problem_json(
    db: AsyncSession, class_id: UUID
) -> tuple[str, set[UUID], dict[str, int]]:
    """Load the school-wide solver input for the class and serialize it to JSON.

    Returns ``(problem_json, class_lesson_ids, input_counts)``.

    Raises:
        HTTPException: 404 if the class doesn't exist, 422 on a pre-solve data
            invariant (no time_blocks for the class's week_scheme, empty rooms
            table, classes referencing different week_schemes).
    """
    requested_class = await db.get(SchoolClass, class_id)
    if requested_class is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Class not found")

    time_blocks = (
        (
            await db.execute(
                select(TimeBlock).where(TimeBlock.week_scheme_id == requested_class.week_scheme_id)
            )
        )
        .scalars()
        .all()
    )
    if not time_blocks:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail="class's week_scheme has no time_blocks configured",
        )

    lessons = (
        (await db.execute(select(Lesson).where(Lesson.teacher_id.is_not(None)))).scalars().all()
    )

    lesson_ids = [lesson.id for lesson in lessons]
    memberships: list[LessonSchoolClass] = []
    if lesson_ids:
        membership_result = await db.execute(
            select(LessonSchoolClass).where(LessonSchoolClass.lesson_id.in_(lesson_ids))
        )
        memberships = list(membership_result.scalars().all())
    classes_by_lesson: dict[UUID, list[UUID]] = {}
    for row in memberships:
        classes_by_lesson.setdefault(row.lesson_id, []).append(row.school_class_id)

    involved_class_ids = {cid for cids in classes_by_lesson.values() for cid in cids} | {
        requested_class.id
    }
    involved_classes = (
        (await db.execute(select(SchoolClass).where(SchoolClass.id.in_(involved_class_ids))))
        .scalars()
        .all()
    )
    mismatched = [c for c in involved_classes if c.week_scheme_id != requested_class.week_scheme_id]
    if mismatched:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail=(
                "classes referenced in this solve use different week_schemes: "
                + ", ".join(str(c.id) for c in mismatched)
            ),
        )

    rooms = (await db.execute(select(Room))).scalars().all()
    if not rooms:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail="no rooms configured; cannot solve",
        )

    teacher_ids = {lesson.teacher_id for lesson in lessons}
    subject_ids = {lesson.subject_id for lesson in lessons}
    # Sentinel UUID used when a filter set is empty: SQLAlchemy's
    # ``in_(empty_set)`` raises on some driver combinations, so we pass a set
    # containing a UUID that cannot match any real row to keep the query valid
    # while ensuring no spurious matches.
    sentinel: set[UUID] = {UUID(int=0)}

    teachers = (
        ((await db.execute(select(Teacher).where(Teacher.id.in_(teacher_ids)))).scalars().all())
        if teacher_ids
        else []
    )
    subjects = (
        ((await db.execute(select(Subject).where(Subject.id.in_(subject_ids)))).scalars().all())
        if subject_ids
        else []
    )

    time_block_ids = {tb.id for tb in time_blocks}
    room_ids = {r.id for r in rooms}

    teacher_qualifications = (
        (
            await db.execute(
                select(TeacherQualification).where(
                    TeacherQualification.teacher_id.in_(teacher_ids or sentinel),
                    TeacherQualification.subject_id.in_(subject_ids or sentinel),
                )
            )
        )
        .scalars()
        .all()
    )

    teacher_availabilities = (
        (
            await db.execute(
                select(TeacherAvailability).where(
                    TeacherAvailability.teacher_id.in_(teacher_ids or sentinel),
                    TeacherAvailability.time_block_id.in_(time_block_ids),
                )
            )
        )
        .scalars()
        .all()
    )

    room_availabilities = (
        (
            await db.execute(
                select(RoomAvailability).where(
                    RoomAvailability.room_id.in_(room_ids),
                    RoomAvailability.time_block_id.in_(time_block_ids),
                )
            )
        )
        .scalars()
        .all()
    )

    room_subject_suitabilities = (
        (
            await db.execute(
                select(RoomSubjectSuitability).where(
                    RoomSubjectSuitability.room_id.in_(room_ids),
                    RoomSubjectSuitability.subject_id.in_(subject_ids or sentinel),
                )
            )
        )
        .scalars()
        .all()
    )

    teacher_blocked_times = [
        {"teacher_id": str(a.teacher_id), "time_block_id": str(a.time_block_id)}
        for a in teacher_availabilities
        if a.status != "available"
    ]

    whitelist_by_room: dict[UUID, set[UUID]] = {}
    for ra in room_availabilities:
        whitelist_by_room.setdefault(ra.room_id, set()).add(ra.time_block_id)
    room_blocked_times: list[dict[str, str]] = []
    for room in rooms:
        whitelist = whitelist_by_room.get(room.id)
        if whitelist is None:
            # Zero entries means the room is universally available.
            continue
        for tb in time_blocks:
            if tb.id not in whitelist:
                room_blocked_times.append({"room_id": str(room.id), "time_block_id": str(tb.id)})

    problem = {
        "time_blocks": [
            {"id": str(tb.id), "day_of_week": tb.day_of_week, "position": tb.position}
            for tb in time_blocks
        ],
        "teachers": [
            {"id": str(t.id), "max_hours_per_week": t.max_hours_per_week} for t in teachers
        ],
        "rooms": [{"id": str(r.id)} for r in rooms],
        "subjects": [
            {
                "id": str(s.id),
                "prefer_early_period": s.prefer_early_period,
                "avoid_first_period": s.avoid_first_period,
                "avoid_last_period": s.avoid_last_period,
            }
            for s in subjects
        ],
        "school_classes": [
            {
                "id": str(c.id),
                "home_room_id": str(c.home_room_id) if c.home_room_id else None,
            }
            for c in involved_classes
        ],
        "lessons": [
            {
                "id": str(lesson.id),
                "school_class_ids": [str(cid) for cid in classes_by_lesson.get(lesson.id, [])],
                "subject_id": str(lesson.subject_id),
                "teacher_id": str(lesson.teacher_id),
                "hours_per_week": lesson.hours_per_week,
                "preferred_block_size": lesson.preferred_block_size,
                "lesson_group_id": (
                    str(lesson.lesson_group_id) if lesson.lesson_group_id else None
                ),
            }
            for lesson in lessons
        ],
        "teacher_qualifications": [
            {"teacher_id": str(q.teacher_id), "subject_id": str(q.subject_id)}
            for q in teacher_qualifications
        ],
        "teacher_blocked_times": teacher_blocked_times,
        "room_blocked_times": room_blocked_times,
        "room_subject_suitabilities": [
            {"room_id": str(s.room_id), "subject_id": str(s.subject_id)}
            for s in room_subject_suitabilities
        ],
    }

    class_lesson_ids = {
        lesson.id
        for lesson in lessons
        if requested_class.id in classes_by_lesson.get(lesson.id, [])
    }

    counts = {
        "time_blocks": len(problem["time_blocks"]),
        "teachers": len(problem["teachers"]),
        "rooms": len(problem["rooms"]),
        "subjects": len(problem["subjects"]),
        "school_classes": len(problem["school_classes"]),
        "lessons": len(problem["lessons"]),
        "teacher_qualifications": len(problem["teacher_qualifications"]),
        "teacher_blocked_times": len(problem["teacher_blocked_times"]),
        "room_blocked_times": len(problem["room_blocked_times"]),
        "room_subject_suitabilities": len(problem["room_subject_suitabilities"]),
    }

    return json.dumps(problem), class_lesson_ids, counts


async def run_solve(
    problem_json: str,
    school_class_id: UUID,
    input_counts: dict[str, int],
    *,
    deadline_ms: int | None,
) -> dict:
    """Run the solver off the event loop, emit structured log events, return the Solution dict."""
    logger.info(
        "solver.solve.start",
        extra={"school_class_id": str(school_class_id), **input_counts},
    )
    started = time.monotonic()
    try:
        solution_json = await asyncio.to_thread(_solve_json_with_config, problem_json, deadline_ms)
    except (ValueError, RuntimeError) as exc:
        duration_ms = (time.monotonic() - started) * 1000.0
        logger.error(
            "solver.solve.error",
            extra={
                "school_class_id": str(school_class_id),
                "duration_ms": duration_ms,
                "exc_class": type(exc).__name__,
            },
            exc_info=exc,
        )
        raise
    duration_ms = (time.monotonic() - started) * 1000.0
    solution = json.loads(solution_json)
    logger.info(
        "solver.solve.done",
        extra={
            "school_class_id": str(school_class_id),
            "duration_ms": duration_ms,
            "placements_total": len(solution["placements"]),
            "violations_total": len(solution["violations"]),
            "violations_by_kind": _count_violations_by_kind(solution["violations"]),
            "soft_score": solution.get("soft_score", 0),
        },
    )
    return solution


async def collect_pinned_placements(
    db: AsyncSession,
    exclude_class_ids: set[UUID],
) -> list[dict[str, str]]:
    """Return persisted ScheduledLesson rows as solver wire-format pin entries.

    Returns one ``{"lesson_id", "time_block_id", "room_id"}`` dict per
    ScheduledLesson row whose Lesson does NOT belong to any class in
    ``exclude_class_ids``. Cross-class lessons (membership in >= 2 classes)
    are pinned only if NONE of their classes are excluded; if at least one
    is excluded, the placement is dropped because it spans the requested
    class scope.

    Output ordered by ``(lesson_id, time_block_id)`` for determinism.
    """
    excluded_lessons_subq = (
        select(LessonSchoolClass.lesson_id)
        .where(LessonSchoolClass.school_class_id.in_(exclude_class_ids))
        .scalar_subquery()
    )
    stmt = (
        select(ScheduledLesson)
        .where(ScheduledLesson.lesson_id.notin_(excluded_lessons_subq))
        .order_by(ScheduledLesson.lesson_id, ScheduledLesson.time_block_id)
    )
    rows = (await db.execute(stmt)).scalars().all()
    return [
        {
            "lesson_id": str(row.lesson_id),
            "time_block_id": str(row.time_block_id),
            "room_id": str(row.room_id),
        }
        for row in rows
    ]


async def persist_solution_for_class(
    db: AsyncSession,
    class_id: UUID,
    filtered: dict,
) -> None:
    """Replace this class's persisted placements with the filtered solver output.

    Deletes every ``scheduled_lessons`` row whose ``lesson_id`` belongs to the
    class, then inserts one row per placement in ``filtered["placements"]``.
    Runs inside the caller's transaction; does not commit.

    Args:
        db: The ambient async session (committed by the route handler on
            successful exit).
        class_id: UUID of the class whose placements are being replaced.
        filtered: The solver output already narrowed to this class via
            :func:`filter_solution_for_class`. Only ``filtered["placements"]``
            is read; violations are ignored.
    """
    lesson_ids_subquery = (
        select(Lesson.id)
        .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
        .where(LessonSchoolClass.school_class_id == class_id)
    )
    delete_result = await db.execute(
        delete(ScheduledLesson).where(ScheduledLesson.lesson_id.in_(lesson_ids_subquery))
    )
    # rowcount is available on CursorResult returned by DML statements;
    # ty sees Result[Any] (the base class), so we access it via getattr.
    deleted_count = int(getattr(delete_result, "rowcount", 0) or 0)

    new_rows = [
        ScheduledLesson(
            lesson_id=UUID(p["lesson_id"]),
            time_block_id=UUID(p["time_block_id"]),
            room_id=UUID(p["room_id"]),
        )
        for p in filtered["placements"]
    ]
    if new_rows:
        db.add_all(new_rows)

    logger.info(
        "schedule.persist.done",
        extra={
            "school_class_id": str(class_id),
            "rows_deleted": deleted_count,
            "rows_inserted": len(new_rows),
        },
    )


async def read_schedule_for_class(
    db: AsyncSession,
    class_id: UUID,
) -> list[PlacementResponse]:
    """Return the class's persisted placements, raising 404 if the class is missing.

    Args:
        db: The ambient async session.
        class_id: UUID of the class to read.

    Returns:
        A list of :class:`PlacementResponse` values; empty if the class has no
        persisted schedule yet.

    Raises:
        HTTPException: 404 if the class doesn't exist. The empty-schedule case
            is distinguished by returning an empty list.
    """
    cls = await db.get(SchoolClass, class_id)
    if cls is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Class not found")

    rows = (
        (
            await db.execute(
                select(ScheduledLesson)
                .join(Lesson, Lesson.id == ScheduledLesson.lesson_id)
                .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
                .where(LessonSchoolClass.school_class_id == class_id)
            )
        )
        .scalars()
        .all()
    )

    return [
        PlacementResponse(
            lesson_id=row.lesson_id,
            time_block_id=row.time_block_id,
            room_id=row.room_id,
        )
        for row in rows
    ]
