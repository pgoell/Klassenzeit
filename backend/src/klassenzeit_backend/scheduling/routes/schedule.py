"""POST /api/classes/{class_id}/schedule and POST /api/schedule/all."""

import logging
import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, Request
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling import solver_io
from klassenzeit_backend.scheduling.schemas.schedule import (
    ScheduleReadResponse,
    ScheduleResponse,
    WholeSchoolScheduleResponse,
)

router = APIRouter(tags=["schedule"])
logger = logging.getLogger(__name__)


@router.post("/classes/{class_id}/schedule")
async def generate_schedule_for_class(
    class_id: uuid.UUID,
    request: Request,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> ScheduleResponse:
    """Run the solver for the given class, persist the placements, and return them.

    Args:
        class_id: UUID path parameter identifying the school class.
        request: The FastAPI request, used to read ``solve_deadline_ms_by_backend`` from
            ``app.state.settings``.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        ``ScheduleResponse`` with placements and violations scoped to this class.

    Raises:
        HTTPException: 404 if the class doesn't exist; 422 if the class's
            week_scheme has no time_blocks, if other classes in the solve use a
            different week_scheme, or if the rooms table is empty.
    """
    sibling_pins = await solver_io.collect_pinned_placements(db, {class_id})
    own_pins = await solver_io.collect_own_class_pins(db, class_id)
    all_pins = sibling_pins + own_pins
    problem_json, class_lesson_ids, input_counts = await solver_io.build_problem_json(
        db, class_id, pinned_placements=all_pins
    )
    settings = request.app.state.settings
    deadline_ms = settings.solve_deadline_ms_by_backend[settings.solver_backend]
    solver_backend = settings.solver_backend
    solution = await solver_io.run_solve(
        problem_json,
        scope_id=class_id,
        input_counts=input_counts,
        deadline_ms=deadline_ms,
        solver_backend=solver_backend,
    )
    filtered = solver_io.filter_solution_for_class(solution, class_lesson_ids)
    logger.info(
        "solver.solve.filtered",
        extra={
            "school_class_id": str(class_id),
            "placements_for_class": len(filtered["placements"]),
            "violations_for_class": len(filtered["violations"]),
        },
    )
    own_pinned_keys = {(uuid.UUID(p["lesson_id"]), uuid.UUID(p["time_block_id"])) for p in own_pins}
    await solver_io.persist_solution_for_class(db, class_id, filtered, pinned_keys=own_pinned_keys)
    await db.commit()
    return ScheduleResponse.model_validate(filtered)


@router.post("/schedule/all")
async def generate_schedule_for_all_classes(
    request: Request,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
    respect_pins: bool = True,
) -> WholeSchoolScheduleResponse:
    """Run the solver for every class in one transaction and persist atomically.

    When ``respect_pins`` is true (default), every ``ScheduledLesson`` with
    ``pinned=true`` is fed into the solver as a hard pin and the persist
    helper carries the flag onto the resulting row. When ``respect_pins`` is
    false, pins are ignored for this run; the persist helper still preserves
    the database flag for any row that re-emerges in the solver output (per
    the spec contract: "Pin state in the database is unchanged").

    Args:
        request: The FastAPI request, used to read ``solve_deadline_ms_by_backend``.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.
        respect_pins: When true, pinned rows are threaded as solver input
            pins. Defaults to true.

    Returns:
        :class:`WholeSchoolScheduleResponse` with per-class summaries plus
        school-wide placement and violation totals.

    Raises:
        HTTPException: 422 on a pre-solve data invariant (no school
            classes, no rooms, heterogeneous week_schemes across classes,
            no time_blocks for the anchor class's week_scheme).
    """
    pins = await solver_io.collect_all_pins(db) if respect_pins else []
    problem_json, _, input_counts = await solver_io.build_problem_json(
        db, class_id=None, pinned_placements=pins
    )
    settings = request.app.state.settings
    deadline_ms = settings.solve_deadline_ms_by_backend[settings.solver_backend]
    solver_backend = settings.solver_backend
    solution = await solver_io.run_solve(
        problem_json,
        scope_id=None,
        input_counts=input_counts,
        deadline_ms=deadline_ms,
        solver_backend=solver_backend,
    )
    pinned_keys = {(uuid.UUID(p["lesson_id"]), uuid.UUID(p["time_block_id"])) for p in pins}
    summaries = await solver_io.persist_solution_for_all_classes(
        db, solution, pinned_keys=pinned_keys
    )
    await db.commit()
    return WholeSchoolScheduleResponse(
        classes=summaries,
        total_placements=sum(s.placements_count for s in summaries),
        total_violations=sum(s.violations_count for s in summaries),
        quality_report=solution["quality_report"],
    )


@router.get("/classes/{class_id}/schedule")
async def read_schedule_for_class_route(
    class_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> ScheduleReadResponse:
    """Return the persisted placements for this class.

    Args:
        class_id: UUID path parameter identifying the school class.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        ``ScheduleReadResponse`` with the class's persisted placements. Empty
        ``placements`` means the class exists but has never been scheduled.

    Raises:
        HTTPException: 404 if the class doesn't exist.
    """
    placements = await solver_io.read_schedule_for_class(db, class_id)
    return ScheduleReadResponse(placements=placements)


@router.get("/teachers/{teacher_id}/schedule")
async def read_schedule_for_teacher_route(
    teacher_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> ScheduleReadResponse:
    """Return the persisted placements for every lesson where Lesson.teacher_id matches.

    Args:
        teacher_id: UUID path parameter identifying the teacher.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        ``ScheduleReadResponse`` with the teacher's persisted placements. Empty
        ``placements`` means the teacher exists but has no scheduled lessons yet.

    Raises:
        HTTPException: 404 if the teacher doesn't exist.
    """
    placements = await solver_io.read_schedule_for_teacher(db, teacher_id)
    return ScheduleReadResponse(placements=placements)


@router.get("/rooms/{room_id}/schedule")
async def read_schedule_for_room_route(
    room_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> ScheduleReadResponse:
    """Return the persisted placements where ``ScheduledLesson.room_id`` matches.

    Args:
        room_id: UUID path parameter identifying the room.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        ``ScheduleReadResponse`` with the room's persisted placements. Empty
        ``placements`` means the room exists but has no scheduled lessons yet.

    Raises:
        HTTPException: 404 if the room doesn't exist.
    """
    placements = await solver_io.read_schedule_for_room(db, room_id)
    return ScheduleReadResponse(placements=placements)
