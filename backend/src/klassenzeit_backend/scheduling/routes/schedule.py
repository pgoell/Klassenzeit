"""POST /api/classes/{class_id}/schedule and POST /api/schedule/all."""

import json
import logging
import time
import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, Request, Response, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling import solver_io
from klassenzeit_backend.scheduling.progress import register_progress
from klassenzeit_backend.scheduling.quality_checks import (
    QualityIssue,
    compute_quality_issues,
)
from klassenzeit_backend.scheduling.schemas.schedule import (
    CellCoord,
    ProgressSnapshot,
    QualityIssueResponse,
    ScheduleReadResponse,
    ScheduleResponse,
    WholeSchoolScheduleResponse,
)

router = APIRouter(tags=["schedule"])
logger = logging.getLogger(__name__)


def _quality_issues_to_response(
    issues: list[QualityIssue],
) -> list[QualityIssueResponse]:
    """Map orchestrator output (tuple-of-tuples cells) to the wire format."""
    return [
        QualityIssueResponse(
            kind=issue.kind,
            school_class_id=issue.school_class_id,
            day_of_week=issue.day_of_week,
            subject_id=issue.subject_id,
            detail=dict(issue.detail),
            cells=[CellCoord(day_of_week=d, position=p) for d, p in issue.cells],
        )
        for issue in issues
    ]


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
    # Sum of hours_per_week for this class's lessons. The progress endpoint
    # surfaces this as the placement target; it's the denominator behind the
    # "K / N lessons placed" frontend badge.
    problem = json.loads(problem_json)
    total_lessons = sum(
        lesson["hours_per_week"]
        for lesson in problem["lessons"]
        if str(class_id) in lesson["school_class_ids"]
    )
    with register_progress(
        request.app.state.solver_progress,
        class_id=class_id,
        deadline_ms=deadline_ms or 0,
        total_lessons=total_lessons,
    ) as entry:
        solution = await solver_io.run_solve(
            problem_json,
            scope_id=class_id,
            input_counts=input_counts,
            deadline_ms=deadline_ms,
            solver_backend=solver_backend,
            progress_handle=entry.handle,
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
    # build_problem_json has already verified the class exists; the scalar
    # below resolves its WeekScheme so the supervision rota can be scoped
    # to the affected scheme on a delete-and-rewrite basis.
    week_scheme_id = (
        await db.execute(select(SchoolClass.week_scheme_id).where(SchoolClass.id == class_id))
    ).scalar_one()
    await solver_io.persist_supervision_assignments(db, week_scheme_id, solution)
    await db.commit()
    quality_issues = await compute_quality_issues(db, class_id)
    return ScheduleResponse.model_validate(
        {**filtered, "quality_issues": _quality_issues_to_response(quality_issues)}
    )


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
    quality_issues = await compute_quality_issues(db, class_id)
    return ScheduleReadResponse(
        placements=placements,
        quality_issues=_quality_issues_to_response(quality_issues),
    )


@router.get("/classes/{class_id}/quality-issues")
async def read_quality_issues_for_class_route(
    class_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> list[QualityIssueResponse]:
    """Return the soft-quality issues for the given class.

    Issues are computed on demand from the persisted ScheduledLesson rows;
    no per-solve snapshot is stored. Returns an empty list when the class
    exists but has never been scheduled.

    Args:
        class_id: UUID path parameter identifying the school class.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        ``list[QualityIssueResponse]``; empty when no issues apply.

    Raises:
        HTTPException: 404 if the class doesn't exist.
    """
    cls = await db.get(SchoolClass, class_id)
    if cls is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Class not found")
    quality_issues = await compute_quality_issues(db, class_id)
    return _quality_issues_to_response(quality_issues)


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
        ``ScheduleReadResponse`` with the teacher's persisted placements and the
        subset of supervision_assignments rows attributed to this teacher.
        Empty ``placements`` means the teacher exists but has no scheduled
        lessons yet; empty ``supervision_assignments`` means the teacher has
        no break-supervision attributions in the current rota.

    Raises:
        HTTPException: 404 if the teacher doesn't exist.
    """
    placements = await solver_io.read_schedule_for_teacher(db, teacher_id)
    supervision_assignments = await solver_io.read_supervision_assignments_for_teacher(
        db, teacher_id
    )
    return ScheduleReadResponse(
        placements=placements,
        supervision_assignments=supervision_assignments,
    )


@router.get("/classes/{class_id}/schedule/progress", response_model=ProgressSnapshot)
async def get_schedule_progress(
    class_id: uuid.UUID,
    request: Request,
    _admin: Annotated[User, Depends(require_admin)],
) -> ProgressSnapshot:
    """Return the live progress snapshot for an in-flight solve.

    Reads the ``ProgressHandle`` registered by the schedule POST in
    ``app.state.solver_progress`` and merges its atomics with the
    request-side ``elapsed_ms`` / ``deadline_ms`` / ``total_lessons``
    fields. Returns 404 when no solve is in flight for this class.
    """
    entry = request.app.state.solver_progress.get(class_id)
    if entry is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="No solve in flight for this class",
        )
    snap = entry.handle.snapshot()
    elapsed_ms = int((time.monotonic() - entry.started_at) * 1000)
    return ProgressSnapshot(
        iter=snap["iter"],
        placement_count=snap["placement_count"],
        total_lessons=entry.total_lessons,
        best_score=snap["best_score"],
        is_feasible=snap["is_feasible"],
        cancel_requested=snap["cancel_requested"],
        elapsed_ms=elapsed_ms,
        deadline_ms=entry.deadline_ms,
    )


@router.post("/classes/{class_id}/schedule/cancel", status_code=status.HTTP_204_NO_CONTENT)
async def cancel_schedule(
    class_id: uuid.UUID,
    request: Request,
    _admin: Annotated[User, Depends(require_admin)],
) -> Response:
    """Soft-cancel an in-flight solve.

    Flips the ``ProgressBeacon``'s ``cancel_requested`` flag; the LAHC inner
    loop exits at the next iteration boundary and the originating POST
    returns with ``was_cancelled=true`` and the best-so-far placements.
    Returns 404 when no solve is in flight for this class.
    """
    entry = request.app.state.solver_progress.get(class_id)
    if entry is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="No solve in flight for this class",
        )
    entry.handle.cancel()
    return Response(status_code=status.HTTP_204_NO_CONTENT)


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
