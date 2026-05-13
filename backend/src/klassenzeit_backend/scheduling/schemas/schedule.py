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
    ]
    lesson_id: UUID
    hour_index: int = Field(ge=0)
    reason: str | None = None


class ScheduleResponse(BaseModel):
    """Per-class filtered solver output for `POST /api/classes/{id}/schedule`."""

    placements: list[PlacementResponse]
    violations: list[ViolationResponse]
    soft_score: int = Field(default=0, ge=0)
    quality_report: QualityReportResponse


class ScheduleReadResponse(BaseModel):
    """Persisted placements for `GET /api/classes/{id}/schedule`.

    Deliberately omits ``violations``: they are per-solve diagnostics and are
    not persisted, so returning an empty list here would misrepresent the
    absence of storage.
    """

    placements: list[PlacementResponse]


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
