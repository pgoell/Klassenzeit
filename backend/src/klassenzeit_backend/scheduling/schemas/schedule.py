"""Pydantic response schemas for the schedule endpoint.

Mirrors the `solver_core::Solution` wire format, filtered to a single school
class's lessons by the route handler.
"""

from typing import Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field

from .quality_report import QualityReportResponse


class PlacementResponse(BaseModel):
    """One placed lesson-hour: which lesson, in which time block, in which room.

    ``pinned`` reflects ``ScheduledLesson.pinned`` for persisted reads and the
    placement-mutation endpoints; defaults to ``False`` for fresh solver
    output, which carries no pinned flag in its wire format. ``teacher_id``
    mirrors ``ScheduledLesson.teacher_id`` (non-null since OPEN_THINGS item 63);
    on fresh solver output it is the solver's per-placement pick.
    """

    model_config = ConfigDict(from_attributes=True)

    lesson_id: UUID
    teacher_id: UUID
    time_block_id: UUID
    room_id: UUID
    pinned: bool = False


class ViolationResponse(BaseModel):
    """One hard-constraint violation emitted by the solver."""

    kind: Literal[
        "no_qualified_teacher",
        "teacher_over_capacity",
        "no_free_time_block",
        "no_suitable_room",
        "lesson_group_split",
        "pinned_conflict",
        "subject_daily_hour_cap_exceeded",
        "class_daily_lesson_cap_exceeded",
        "class_subject_teacher_split",
        "supervision_gap",
    ]
    lesson_id: UUID
    hour_index: int = Field(ge=0)
    reason: str | None = None


class SupervisionAssignmentResponse(BaseModel):
    """One Hofpause supervision assignment: a teacher covering a break TimeBlock.

    Mirrors the solver-core ``SupervisionAssignment`` wire format. One entry
    per break-kind TimeBlock for which the solver found an eligible
    supervisor; break slots without a feasible supervisor surface as a
    ``ViolationResponse`` with ``kind="supervision_gap"`` instead.
    """

    time_block_id: UUID
    teacher_id: UUID


class ScheduleResponse(BaseModel):
    """Per-class filtered solver output for `POST /api/classes/{id}/schedule`.

    ``was_cancelled`` is ``True`` when the originating POST was interrupted
    via ``POST /schedule/cancel`` mid-solve; the placements list then carries
    the best-so-far solution at the moment of the cancel.

    ``supervision_assignments`` carries the whole-school Hofpause rota
    emitted by the solver. The list is school-wide rather than class-scoped
    because supervision is a teacher-level duty: every break-kind TimeBlock
    on the affected WeekScheme appears, regardless of which class triggered
    the solve.
    """

    placements: list[PlacementResponse]
    violations: list[ViolationResponse]
    soft_score: int = Field(default=0, ge=0)
    quality_report: QualityReportResponse
    was_cancelled: bool = False
    supervision_assignments: list[SupervisionAssignmentResponse] = Field(default_factory=list)


class ProgressSnapshot(BaseModel):
    """Live progress snapshot for an in-flight solve.

    Emitted by ``GET /api/classes/{id}/schedule/progress`` while the LAHC
    loop runs. Merges atomics from the Rust ``ProgressBeacon`` with
    request-side fields (``total_lessons``, ``elapsed_ms``, ``deadline_ms``)
    derived from the in-flight registration entry.
    """

    iter: int
    placement_count: int
    total_lessons: int
    best_score: int
    is_feasible: bool
    cancel_requested: bool
    elapsed_ms: int
    deadline_ms: int


class ScheduleReadResponse(BaseModel):
    """Persisted placements for `GET /api/classes/{id}/schedule`.

    Deliberately omits ``violations``: they are per-solve diagnostics and are
    not persisted, so returning an empty list here would misrepresent the
    absence of storage.

    ``supervision_assignments`` carries the persisted Hofpause rota scoped to
    the resource: per-teacher GETs filter to that teacher's rows, while the
    per-class and per-room GETs leave it empty (supervision is a teacher-level
    duty, not a class- or room-level one).
    """

    placements: list[PlacementResponse]
    supervision_assignments: list[SupervisionAssignmentResponse] = Field(default_factory=list)


class ClassScheduleSummary(BaseModel):
    """Per-class outcome of `POST /api/schedule/all`."""

    class_id: UUID
    placements_count: int
    violations_count: int


class WholeSchoolScheduleResponse(BaseModel):
    """Slim response for `POST /api/schedule/all`.

    The per-class GET endpoint fetches full placements when needed; this
    response carries counts only to keep the wire size manageable when the
    schedule spans many classes.
    """

    classes: list[ClassScheduleSummary]
    total_placements: int
    total_violations: int
    quality_report: QualityReportResponse
