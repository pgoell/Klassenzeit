"""Solver IO: problem building, solve runner, per-class response filter.

Sits between the route handler and the PyO3 binding. Route handlers use the
three exported helpers (`build_problem_json`, `run_solve`, `filter_solution_for_class`).
"""

from __future__ import annotations

import asyncio
import json
import logging
import time
from typing import TYPE_CHECKING, Literal, get_args
from uuid import UUID

from fastapi import HTTPException, status
from sqlalchemy import delete, select

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.pin_kind import PinKind
from klassenzeit_backend.db.models.room import (
    Room,
    RoomAvailability,
    RoomSubjectSuitability,
)
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.supervision_assignment import SupervisionAssignment
from klassenzeit_backend.db.models.teacher import (
    Teacher,
    TeacherAvailability,
    TeacherQualification,
)
from klassenzeit_backend.db.models.week_scheme import TimeBlock
from klassenzeit_backend.scheduling.schemas.schedule import (
    ClassScheduleSummary,
    PlacementResponse,
    SupervisionAssignmentResponse,
    ViolationResponse,
)
from klassenzeit_solver import (
    solve_cpsat_json as _solve_cpsat_json,
)
from klassenzeit_solver import (
    solve_json_with_config as _solve_json_with_config,
)
from klassenzeit_solver import (
    solve_json_with_progress as _solve_json_with_progress,
)

if TYPE_CHECKING:
    from collections.abc import Sequence

    from sqlalchemy.ext.asyncio import AsyncSession

    from klassenzeit_solver import ProgressHandle

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

    The school-wide ``soft_score`` and ``quality_report`` are passed through
    unchanged so the per-class route response carries the solver's overall
    quality signal even though the placement list is class-scoped. PR-9c
    originally noted re-scoring as a follow-up; the same applies to
    re-computing ``quality_report`` against the filtered subset. For now,
    both fields reflect the whole-school solve.

    ``was_cancelled`` is passed through verbatim: cancellation is a
    whole-solve event, not a per-class one, so the field reflects the
    originating POST regardless of which class is being filtered.
    """
    placements = [p for p in solution["placements"] if UUID(p["lesson_id"]) in class_lesson_ids]
    violations = [v for v in solution["violations"] if UUID(v["lesson_id"]) in class_lesson_ids]
    return {
        "placements": placements,
        "violations": violations,
        "soft_score": solution.get("soft_score", 0),
        "quality_report": solution["quality_report"],
        "was_cancelled": bool(solution.get("was_cancelled", False)),
        "supervision_assignments": solution.get("supervision_assignments", []),
    }


async def _resolve_anchor_class(db: AsyncSession, class_id: UUID | None) -> SchoolClass:
    """Return the anchor class used to scope the solver input.

    For a per-class solve this is the requested class. For a whole-school
    solve (``class_id is None``) it is any one existing class, used solely
    to anchor the ``week_scheme`` / ``time_blocks`` lookup; the
    heterogeneous-week_scheme check downstream still rejects mixed schemes.

    Raises:
        HTTPException: 404 if ``class_id`` is provided and missing; 422 if
            ``class_id`` is None and no school classes exist.
    """
    if class_id is not None:
        existing = await db.get(SchoolClass, class_id)
        if existing is None:
            raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Class not found")
        return existing
    first = (
        (await db.execute(select(SchoolClass).order_by(SchoolClass.name).limit(1)))
        .scalars()
        .first()
    )
    if first is None:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail="no school classes configured; cannot solve",
        )
    return first


def _build_room_blocked_times(
    rooms: Sequence[Room],
    time_blocks: Sequence[TimeBlock],
    room_availabilities: Sequence[RoomAvailability],
) -> list[dict[str, str]]:
    """Return one ``(room_id, time_block_id)`` entry per non-whitelisted TimeBlock.

    A room with zero ``RoomAvailability`` rows is universally available; any
    explicit row flips the room into the must-whitelist regime.
    """
    whitelist_by_room: dict[UUID, set[UUID]] = {}
    for ra in room_availabilities:
        whitelist_by_room.setdefault(ra.room_id, set()).add(ra.time_block_id)
    blocked: list[dict[str, str]] = []
    for room in rooms:
        whitelist = whitelist_by_room.get(room.id)
        if whitelist is None:
            continue
        for tb in time_blocks:
            if tb.id not in whitelist:
                blocked.append({"room_id": str(room.id), "time_block_id": str(tb.id)})
    return blocked


def _extend_blocked_times_for_off_days(
    teacher_blocked_times: list[dict[str, str]],
    teachers: Sequence[Teacher],
    time_blocks: Sequence[TimeBlock],
) -> None:
    """Append off-day ``(teacher_id, time_block_id)`` entries for Teilzeit teachers.

    Teachers with ``working_days is None`` are full-time and contribute nothing.
    Existing explicit-unavailable entries are deduped against so no
    ``(teacher_id, time_block_id)`` pair is emitted twice. Mutates the list in
    place to keep the call site (``build_problem_json``) flat.
    """
    existing_pairs: set[tuple[str, str]] = {
        (entry["teacher_id"], entry["time_block_id"]) for entry in teacher_blocked_times
    }
    for t in teachers:
        if t.working_days is None:
            continue
        working_set = set(t.working_days)
        for tb in time_blocks:
            if tb.day_of_week in working_set:
                continue
            pair = (str(t.id), str(tb.id))
            if pair in existing_pairs:
                continue
            existing_pairs.add(pair)
            teacher_blocked_times.append({"teacher_id": pair[0], "time_block_id": pair[1]})


def _candidates_for_lesson(
    lesson: Lesson,
    teacher_qualifications: Sequence[TeacherQualification],
    teacher_availabilities: Sequence[TeacherAvailability],
    class_tb_ids: set[UUID],
    working_days_by_teacher: dict[UUID, list[int] | None],
    tb_day_by_id: dict[UUID, int],
) -> list[str]:
    """Compute the per-Lesson candidate teacher set (item 64).

    A teacher is a candidate iff they are qualified for the lesson's subject
    AND their availability overlaps at least one of the WeekScheme's time
    blocks. A teacher with NO ``TeacherAvailability`` row at all is treated as
    universally available (matching the no-blocked-times convention used to
    build ``teacher_blocked_times``); any explicit row, available or blocked,
    flips the teacher into the must-overlap-explicitly regime.

    Teilzeit teachers (``working_days`` set) have the candidate-tb set
    restricted to TimeBlocks whose ``day_of_week`` is in ``working_days``
    before either rule is applied.

    Output is sorted by teacher uuid ascending; the pin (if set and
    qualifying) is moved to the front so the algorithm-phase PR (item 68) can
    iterate the list in determined order.
    """
    qualified: set[UUID] = {
        q.teacher_id for q in teacher_qualifications if q.subject_id == lesson.subject_id
    }
    teachers_with_any_row: set[UUID] = set()
    available_by_teacher: dict[UUID, set[UUID]] = {}
    for a in teacher_availabilities:
        teachers_with_any_row.add(a.teacher_id)
        if a.status == "available":
            available_by_teacher.setdefault(a.teacher_id, set()).add(a.time_block_id)

    def _allowed_tb_ids_for(tid: UUID) -> set[UUID]:
        wd = working_days_by_teacher.get(tid)
        if wd is None:
            return class_tb_ids
        wd_set = set(wd)
        return {tb_id for tb_id in class_tb_ids if tb_day_by_id.get(tb_id) in wd_set}

    def _has_overlap(tid: UUID) -> bool:
        allowed = _allowed_tb_ids_for(tid)
        if not allowed:
            return False
        if tid not in teachers_with_any_row:
            # No availability rows = universally available (production convention).
            return True
        return bool(available_by_teacher.get(tid, set()) & allowed)

    candidates: list[UUID] = sorted(tid for tid in qualified if _has_overlap(tid))
    pin = lesson.teacher_id
    if pin is not None and pin in candidates:
        candidates.remove(pin)
        candidates.insert(0, pin)
    return [str(t) for t in candidates]


async def build_problem_json(
    db: AsyncSession,
    class_id: UUID | None = None,
    *,
    pinned_placements: list[dict[str, str]] | None = None,
) -> tuple[str, set[UUID], dict[str, int]]:
    """Load the school-wide solver input and serialize it to JSON.

    Returns ``(problem_json, class_lesson_ids, input_counts)``.

    When ``class_id`` is a UUID, ``class_lesson_ids`` is the set of Lesson
    UUIDs belonging to that class (used by the per-class response filter).
    When ``class_id`` is ``None`` (whole-school solve), ``class_lesson_ids``
    is empty: the whole-school caller persists every placement and never
    needs the per-class filter.

    The optional ``pinned_placements`` is forwarded verbatim into the wire
    format under the same-named key. Default-empty mirrors the solver-core
    ``#[serde(default)]`` behavior so callers omitting the field work
    unchanged.

    Raises:
        HTTPException: 404 if ``class_id`` is provided and the class doesn't
            exist; 422 on a pre-solve data invariant (no time_blocks for the
            class's week_scheme, empty rooms table, classes referencing
            different week_schemes). For the whole-school path the 422 fires
            on missing rooms and on heterogeneous week_schemes across
            existing classes; the time-blocks check is anchored on the
            first class found.
    """
    requested_class = await _resolve_anchor_class(db, class_id)

    time_blocks = (
        (
            await db.execute(
                select(TimeBlock).where(
                    TimeBlock.week_scheme_id == requested_class.week_scheme_id,
                )
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

    lessons = (await db.execute(select(Lesson))).scalars().all()

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

    pinned_teacher_ids = {lesson.teacher_id for lesson in lessons if lesson.teacher_id is not None}
    subject_ids = {lesson.subject_id for lesson in lessons}
    # Sentinel UUID used when a filter set is empty: SQLAlchemy's
    # ``in_(empty_set)`` raises on some driver combinations, so we pass a set
    # containing a UUID that cannot match any real row to keep the query valid
    # while ensuring no spurious matches.
    sentinel: set[UUID] = {UUID(int=0)}

    subjects = (
        ((await db.execute(select(Subject).where(Subject.id.in_(subject_ids)))).scalars().all())
        if subject_ids
        else []
    )

    time_block_ids = {tb.id for tb in time_blocks}
    room_ids = {r.id for r in rooms}

    # Load every TeacherQualification for the lessons' subjects so the per-Lesson
    # candidate set (item 64) considers all qualified teachers, not just the pin.
    teacher_qualifications = (
        (
            await db.execute(
                select(TeacherQualification).where(
                    TeacherQualification.subject_id.in_(subject_ids or sentinel),
                )
            )
        )
        .scalars()
        .all()
    )

    qualified_teacher_ids = {q.teacher_id for q in teacher_qualifications}
    teacher_ids = pinned_teacher_ids | qualified_teacher_ids

    teachers = (
        ((await db.execute(select(Teacher).where(Teacher.id.in_(teacher_ids)))).scalars().all())
        if teacher_ids
        else []
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

    working_days_by_teacher: dict[UUID, list[int] | None] = {t.id: t.working_days for t in teachers}
    tb_day_by_id: dict[UUID, int] = {tb.id: tb.day_of_week for tb in time_blocks}

    _extend_blocked_times_for_off_days(teacher_blocked_times, teachers, time_blocks)

    room_blocked_times = _build_room_blocked_times(rooms, time_blocks, room_availabilities)

    problem = {
        "time_blocks": [
            {
                "id": str(tb.id),
                "day_of_week": tb.day_of_week,
                "position": tb.position,
                "kind": tb.kind.value,
            }
            for tb in time_blocks
        ],
        "teachers": [
            {
                "id": str(t.id),
                "max_hours_per_week": t.max_hours_per_week,
                "reserve_hours_per_week": t.reserve_hours_per_week,
            }
            for t in teachers
        ],
        "rooms": [{"id": str(r.id)} for r in rooms],
        "subjects": [
            {
                "id": str(s.id),
                "prefer_early_period": s.prefer_early_period,
                "prefer_late_period": s.prefer_late_period,
                "avoid_first_period": s.avoid_first_period,
                "avoid_last_period": s.avoid_last_period,
                "max_hours_per_day": s.max_hours_per_day,
            }
            for s in subjects
        ],
        "school_classes": [
            {
                "id": str(c.id),
                "home_room_id": str(c.home_room_id) if c.home_room_id else None,
                "max_lessons_per_day": c.max_lessons_per_day,
                "class_teacher_id": (str(c.class_teacher_id) if c.class_teacher_id else None),
            }
            for c in involved_classes
        ],
        "lessons": [
            {
                "id": str(lesson.id),
                "school_class_ids": [str(cid) for cid in classes_by_lesson.get(lesson.id, [])],
                "subject_id": str(lesson.subject_id),
                "teacher_candidates": _candidates_for_lesson(
                    lesson,
                    teacher_qualifications,
                    teacher_availabilities,
                    time_block_ids,
                    working_days_by_teacher,
                    tb_day_by_id,
                ),
                "teacher_pin": str(lesson.teacher_id) if lesson.teacher_id else None,
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
        "pinned_placements": pinned_placements or [],
    }

    class_lesson_ids: set[UUID]
    if class_id is None:
        # Whole-school solve: caller persists every placement, no filter needed.
        class_lesson_ids = set()
    else:
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
    scope_id: UUID | None,
    input_counts: dict[str, int],
    *,
    deadline_ms: int | None,
    solver_backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"] = "lahc",
    progress_handle: ProgressHandle | None = None,
) -> dict:
    """Run the solver off the event loop, emit structured log events, return the Solution dict.

    ``scope_id`` is the per-class UUID for a single-class solve, or ``None``
    for a whole-school solve. It is logged under ``school_class_id`` for
    continuity with existing log shape; ``None`` is logged verbatim so the
    whole-school path is identifiable in structured-log queries.

    ``solver_backend`` selects which solver runs. The default ``lahc`` matches
    pre-Sprint-4 behaviour; ``lahc_rr`` and ``lahc_rr_kempe`` thread the
    corresponding period kwargs into ``solve_json_with_config``; ``cpsat``
    dispatches to the CP-SAT seed (ADR 0030).

    ``progress_handle`` is the PyO3 ``ProgressHandle`` whose underlying
    ``ProgressBeacon`` the LAHC loop writes to each iteration. When set, the
    LAHC backends dispatch to ``solve_json_with_progress`` so the beacon is
    threaded into the inner loop and ``cancel()`` is honored. The CP-SAT
    backend is not yet wired to the beacon; we log a warning and fall through
    to the existing CP-SAT path (the progress endpoint returns zero counters
    in that case but the solve still works).
    """
    scope_str = str(scope_id) if scope_id is not None else None
    logger.info(
        "solver.solve.start",
        extra={"school_class_id": scope_str, "backend": solver_backend, **input_counts},
    )
    started = time.monotonic()
    try:
        match solver_backend:
            case "lahc":
                if progress_handle is not None:
                    solution_json = await asyncio.to_thread(
                        _solve_json_with_progress,
                        problem_json,
                        deadline_ms,
                        progress_handle,
                    )
                else:
                    solution_json = await asyncio.to_thread(
                        _solve_json_with_config, problem_json, deadline_ms
                    )
            case "lahc_rr":
                if progress_handle is not None:
                    solution_json = await asyncio.to_thread(
                        _solve_json_with_progress,
                        problem_json,
                        deadline_ms,
                        progress_handle,
                        25,
                    )
                else:
                    solution_json = await asyncio.to_thread(
                        _solve_json_with_config,
                        problem_json,
                        deadline_ms,
                        lahc_rr_period=25,
                    )
            case "lahc_rr_kempe":
                if progress_handle is not None:
                    solution_json = await asyncio.to_thread(
                        _solve_json_with_progress,
                        problem_json,
                        deadline_ms,
                        progress_handle,
                        25,
                        23,
                    )
                else:
                    solution_json = await asyncio.to_thread(
                        _solve_json_with_config,
                        problem_json,
                        deadline_ms,
                        lahc_rr_period=25,
                        lahc_kempe_period=23,
                    )
            case "cpsat":
                if progress_handle is not None:
                    logger.warning(
                        "solver.solve.progress_unsupported",
                        extra={"school_class_id": scope_str, "backend": solver_backend},
                    )
                solution_json = await asyncio.to_thread(
                    _solve_cpsat_json, problem_json, deadline_ms
                )
    except (ValueError, RuntimeError) as exc:
        duration_ms = (time.monotonic() - started) * 1000.0
        logger.error(
            "solver.solve.error",
            extra={
                "school_class_id": scope_str,
                "backend": solver_backend,
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
            "school_class_id": scope_str,
            "backend": solver_backend,
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

    Returns one ``{"lesson_id", "time_block_id", "room_id", "teacher_id"}``
    dict per ScheduledLesson row whose Lesson has at least one class
    membership OUTSIDE ``exclude_class_ids``. Single-class lessons in
    excluded classes are dropped (the focus class re-places them);
    cross-class lessons are pinned whenever any sibling class would
    otherwise see drift on a per-class re-solve. Lessons whose membership
    lies entirely inside ``exclude_class_ids`` are dropped.

    ``teacher_id`` (item 77) carries the picker's chosen teacher from
    the prior solve so the seed Placement reflects the real teacher
    rather than falling back to ``teacher_candidates[0]`` (which would
    false-positive ``validate_no_double_booking`` when two pins share
    the static fallback under unpinned mode).

    Output ordered by ``(lesson_id, time_block_id)`` for determinism.
    """
    pinned_lessons_subq = (
        select(LessonSchoolClass.lesson_id)
        .where(LessonSchoolClass.school_class_id.notin_(exclude_class_ids))
        .scalar_subquery()
    )
    stmt = (
        select(ScheduledLesson)
        .where(ScheduledLesson.lesson_id.in_(pinned_lessons_subq))
        .order_by(ScheduledLesson.lesson_id, ScheduledLesson.time_block_id)
    )
    rows = (await db.execute(stmt)).scalars().all()
    return [
        {
            "lesson_id": str(row.lesson_id),
            "time_block_id": str(row.time_block_id),
            "room_id": str(row.room_id),
            "teacher_id": str(row.teacher_id),
            # Sibling-class placements are immovable on a per-class re-solve
            # regardless of the row's user-facing pin_kind; emit them as hard
            # so the solver treats them as fixed.
            "kind": PinKind.HARD.value,
        }
        for row in rows
    ]


async def collect_own_class_pins(
    db: AsyncSession,
    class_id: UUID,
) -> list[dict[str, str]]:
    """Return wire-format pin dicts for the requested class's pinned rows.

    Pulls every ``ScheduledLesson`` whose ``Lesson`` is a member of
    ``class_id`` AND whose ``pinned`` flag is true. Output is ordered by
    ``(lesson_id, time_block_id)`` for determinism, matching
    ``collect_pinned_placements``. Carries ``teacher_id`` per item 77.
    """
    own_lessons_subq = (
        select(LessonSchoolClass.lesson_id)
        .where(LessonSchoolClass.school_class_id == class_id)
        .scalar_subquery()
    )
    stmt = (
        select(ScheduledLesson)
        .where(ScheduledLesson.lesson_id.in_(own_lessons_subq))
        .where(ScheduledLesson.pin_kind.is_not(None))
        .order_by(ScheduledLesson.lesson_id, ScheduledLesson.time_block_id)
    )
    rows = (await db.execute(stmt)).scalars().all()
    return [
        {
            "lesson_id": str(row.lesson_id),
            "time_block_id": str(row.time_block_id),
            "room_id": str(row.room_id),
            "teacher_id": str(row.teacher_id),
            # pin_kind is not None by the WHERE filter above.
            "kind": row.pin_kind.value if row.pin_kind is not None else PinKind.HARD.value,
        }
        for row in rows
    ]


async def collect_all_pins(
    db: AsyncSession,
) -> list[dict[str, str]]:
    """Return wire-format pin dicts for every ScheduledLesson with a pin set.

    Carries ``teacher_id`` per item 77.
    """
    stmt = (
        select(ScheduledLesson)
        .where(ScheduledLesson.pin_kind.is_not(None))
        .order_by(ScheduledLesson.lesson_id, ScheduledLesson.time_block_id)
    )
    rows = (await db.execute(stmt)).scalars().all()
    return [
        {
            "lesson_id": str(row.lesson_id),
            "time_block_id": str(row.time_block_id),
            "room_id": str(row.room_id),
            "teacher_id": str(row.teacher_id),
            # pin_kind is not None by the WHERE filter above.
            "kind": row.pin_kind.value if row.pin_kind is not None else PinKind.HARD.value,
        }
        for row in rows
    ]


async def persist_solution_for_class(
    db: AsyncSession,
    class_id: UUID,
    filtered: dict,
    *,
    pinned_keys: set[tuple[UUID, UUID]] | None = None,
) -> None:
    """Replace this class's persisted placements with the filtered solver output.

    Deletes every ``scheduled_lessons`` row whose ``lesson_id`` belongs to the
    class, then inserts one row per placement in ``filtered["placements"]``.
    Runs inside the caller's transaction; does not commit.

    Pin-flag preservation: an output placement is persisted with
    ``pinned=True`` if either (a) its ``(lesson_id, time_block_id)`` is in the
    caller-supplied ``pinned_keys`` set OR (b) the row already had
    ``pinned=True`` in the DB at the matching key before the delete. This
    keeps the spec contract "Pin state in the database is unchanged" valid
    on a per-class re-solve regardless of whether the caller respects pins
    on this run.

    Args:
        db: The ambient async session (committed by the route handler on
            successful exit).
        class_id: UUID of the class whose placements are being replaced.
        filtered: The solver output already narrowed to this class via
            :func:`filter_solution_for_class`. Only ``filtered["placements"]``
            is read; violations are ignored.
        pinned_keys: Set of ``(lesson_id, time_block_id)`` pairs that should
            be marked ``pinned=True`` regardless of prior DB state.
    """
    lesson_ids_subquery = (
        select(Lesson.id)
        .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
        .where(LessonSchoolClass.school_class_id == class_id)
    )
    existing_pin_keys = await _existing_pin_keys_for_class(db, class_id)
    pin_lookup = (pinned_keys or set()) | existing_pin_keys
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
            teacher_id=UUID(p["teacher_id"]),
            pin_kind=(
                PinKind.HARD
                if (UUID(p["lesson_id"]), UUID(p["time_block_id"])) in pin_lookup
                else None
            ),
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


async def persist_supervision_assignments(
    db: AsyncSession,
    week_scheme_id: UUID,
    solution: dict,
) -> None:
    """Replace the WeekScheme's supervision rota with the solver output.

    Deletes every ``supervision_assignments`` row whose ``time_block_id``
    belongs to ``week_scheme_id``, then inserts one row per entry in
    ``solution["supervision_assignments"]``. Scoped to the WeekScheme
    rather than the class because Hofpause supervision is a school-wide
    duty: the supervision pass emits one entry per break-kind TimeBlock
    on the affected scheme, and a per-class re-solve overwrites the
    whole rota.

    Runs inside the caller's transaction; does not commit.
    """
    tb_id_rows = (
        (await db.execute(select(TimeBlock.id).where(TimeBlock.week_scheme_id == week_scheme_id)))
        .scalars()
        .all()
    )
    tb_ids = set(tb_id_rows)
    if tb_ids:
        await db.execute(
            delete(SupervisionAssignment).where(SupervisionAssignment.time_block_id.in_(tb_ids))
        )
    for a in solution.get("supervision_assignments", []):
        db.add(
            SupervisionAssignment(
                time_block_id=UUID(a["time_block_id"]),
                teacher_id=UUID(a["teacher_id"]),
            )
        )


async def _existing_pin_keys_for_class(db: AsyncSession, class_id: UUID) -> set[tuple[UUID, UUID]]:
    """Return ``(lesson_id, time_block_id)`` pairs that are pinned for this class."""
    stmt = (
        select(ScheduledLesson.lesson_id, ScheduledLesson.time_block_id)
        .join(LessonSchoolClass, LessonSchoolClass.lesson_id == ScheduledLesson.lesson_id)
        .where(LessonSchoolClass.school_class_id == class_id)
        .where(ScheduledLesson.pin_kind.is_not(None))
    )
    rows = (await db.execute(stmt)).all()
    return {(lesson_id, time_block_id) for lesson_id, time_block_id in rows}


async def persist_solution_for_all_classes(
    db: AsyncSession,
    solution: dict,
    *,
    pinned_keys: set[tuple[UUID, UUID]] | None = None,
) -> list[ClassScheduleSummary]:
    """Persist the solution's placements for every class in one transaction.

    Returns per-class summaries. A placement is attributed to every class its
    lesson belongs to via ``LessonSchoolClass``. Violations are attributed
    similarly: a violation on a cross-class lesson counts once per affected
    class.

    Existing ``ScheduledLesson`` rows for any lesson in the new placements are
    deleted before the new placements are inserted (delete-then-insert). Runs
    inside the caller's transaction; does not commit.

    Pin-flag preservation: an output placement is persisted with
    ``pinned=True`` if either (a) its ``(lesson_id, time_block_id)`` is in the
    caller-supplied ``pinned_keys`` set OR (b) the row already had
    ``pinned=True`` in the DB at the matching key before the delete. This
    keeps the spec contract "Pin state in the database is unchanged" valid
    when ``respect_pins=false`` ignores pins on the solver input.
    """
    placements = solution["placements"]
    violations = solution["violations"]

    placement_lesson_ids = {UUID(p["lesson_id"]) for p in placements}
    violation_lesson_ids = {UUID(v["lesson_id"]) for v in violations}
    touched_lesson_ids = placement_lesson_ids | violation_lesson_ids

    lesson_to_classes: dict[UUID, set[UUID]] = {}
    if touched_lesson_ids:
        rows = (
            await db.execute(
                select(LessonSchoolClass.lesson_id, LessonSchoolClass.school_class_id).where(
                    LessonSchoolClass.lesson_id.in_(touched_lesson_ids)
                )
            )
        ).all()
        for lesson_id, class_id in rows:
            lesson_to_classes.setdefault(lesson_id, set()).add(class_id)

    existing_pin_keys: set[tuple[UUID, UUID]] = set()
    if placement_lesson_ids:
        existing_pin_rows = (
            await db.execute(
                select(ScheduledLesson.lesson_id, ScheduledLesson.time_block_id).where(
                    ScheduledLesson.lesson_id.in_(placement_lesson_ids),
                    ScheduledLesson.pin_kind.is_not(None),
                )
            )
        ).all()
        existing_pin_keys = {
            (lesson_id, time_block_id) for lesson_id, time_block_id in existing_pin_rows
        }
    pin_lookup = (pinned_keys or set()) | existing_pin_keys

    if placement_lesson_ids:
        await db.execute(
            delete(ScheduledLesson).where(ScheduledLesson.lesson_id.in_(placement_lesson_ids))
        )

    for p in placements:
        lesson_uuid = UUID(p["lesson_id"])
        time_block_uuid = UUID(p["time_block_id"])
        db.add(
            ScheduledLesson(
                lesson_id=lesson_uuid,
                time_block_id=time_block_uuid,
                room_id=UUID(p["room_id"]),
                teacher_id=UUID(p["teacher_id"]),
                pin_kind=(PinKind.HARD if (lesson_uuid, time_block_uuid) in pin_lookup else None),
            )
        )
    await db.flush()

    class_to_placements: dict[UUID, int] = {}
    for p in placements:
        for class_id in lesson_to_classes.get(UUID(p["lesson_id"]), set()):
            class_to_placements[class_id] = class_to_placements.get(class_id, 0) + 1

    class_to_violations: dict[UUID, int] = {}
    for v in violations:
        v_lesson = UUID(v["lesson_id"])
        for class_id in lesson_to_classes.get(v_lesson, set()):
            class_to_violations[class_id] = class_to_violations.get(class_id, 0) + 1

    all_class_ids = sorted(set(class_to_placements) | set(class_to_violations))
    return [
        ClassScheduleSummary(
            class_id=class_id,
            placements_count=class_to_placements.get(class_id, 0),
            violations_count=class_to_violations.get(class_id, 0),
        )
        for class_id in all_class_ids
    ]


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
            teacher_id=row.teacher_id,
            time_block_id=row.time_block_id,
            room_id=row.room_id,
            pin_kind=row.pin_kind,
        )
        for row in rows
    ]


async def read_schedule_for_teacher(
    db: AsyncSession,
    teacher_id: UUID,
) -> list[PlacementResponse]:
    """Return persisted placements where the lesson's teacher matches.

    Args:
        db: The ambient async session.
        teacher_id: UUID of the teacher to read.

    Returns:
        A list of :class:`PlacementResponse` values; empty if the teacher has no
        scheduled lessons yet.

    Raises:
        HTTPException: 404 if the teacher doesn't exist. The empty-schedule
            case is distinguished by returning an empty list.
    """
    teacher = await db.get(Teacher, teacher_id)
    if teacher is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Teacher not found")

    rows = (
        (
            await db.execute(
                select(ScheduledLesson)
                .join(Lesson, Lesson.id == ScheduledLesson.lesson_id)
                .where(Lesson.teacher_id == teacher_id)
            )
        )
        .scalars()
        .all()
    )

    return [
        PlacementResponse(
            lesson_id=row.lesson_id,
            teacher_id=row.teacher_id,
            time_block_id=row.time_block_id,
            room_id=row.room_id,
            pin_kind=row.pin_kind,
        )
        for row in rows
    ]


async def read_supervision_assignments_for_teacher(
    db: AsyncSession,
    teacher_id: UUID,
) -> list[SupervisionAssignmentResponse]:
    """Return persisted Hofpause supervision rows assigned to this teacher.

    Args:
        db: The ambient async session.
        teacher_id: UUID of the teacher to read.

    Returns:
        A list of :class:`SupervisionAssignmentResponse` values; empty if the
        teacher has no supervision attributions yet. Does not raise on a
        missing teacher; the sibling :func:`read_schedule_for_teacher` call
        in the GET handler already enforces the 404.
    """
    rows = (
        (
            await db.execute(
                select(SupervisionAssignment).where(SupervisionAssignment.teacher_id == teacher_id)
            )
        )
        .scalars()
        .all()
    )
    return [
        SupervisionAssignmentResponse(
            time_block_id=row.time_block_id,
            teacher_id=row.teacher_id,
        )
        for row in rows
    ]


async def read_schedule_for_room(
    db: AsyncSession,
    room_id: UUID,
) -> list[PlacementResponse]:
    """Return persisted placements where ``ScheduledLesson.room_id`` matches.

    Args:
        db: The ambient async session.
        room_id: UUID of the room to read.

    Returns:
        A list of :class:`PlacementResponse` values; empty if the room has no
        scheduled lessons yet.

    Raises:
        HTTPException: 404 if the room doesn't exist. The empty-schedule case
            is distinguished by returning an empty list.
    """
    room = await db.get(Room, room_id)
    if room is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Room not found")

    rows = (
        (await db.execute(select(ScheduledLesson).where(ScheduledLesson.room_id == room_id)))
        .scalars()
        .all()
    )

    return [
        PlacementResponse(
            lesson_id=row.lesson_id,
            teacher_id=row.teacher_id,
            time_block_id=row.time_block_id,
            room_id=row.room_id,
            pin_kind=row.pin_kind,
        )
        for row in rows
    ]
