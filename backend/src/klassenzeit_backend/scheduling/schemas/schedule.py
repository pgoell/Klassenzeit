"""Pydantic response schemas for the schedule endpoint.

Mirrors the `solver_core::Solution` wire format, filtered to a single school
class's lessons by the route handler.
"""

from typing import Literal
from uuid import UUID

from pydantic import BaseModel, Field


class PlacementResponse(BaseModel):
    """One placed lesson-hour: which lesson, in which time block, in which room."""

    lesson_id: UUID
    time_block_id: UUID
    room_id: UUID


class ViolationResponse(BaseModel):
    """One hard-constraint violation emitted by the solver."""

    kind: Literal[
        "no_qualified_teacher",
        "teacher_over_capacity",
        "no_free_time_block",
        "no_suitable_room",
        "lesson_group_split",
        "pinned_conflict",
    ]
    lesson_id: UUID
    hour_index: int = Field(ge=0)


class ScheduleResponse(BaseModel):
    """Per-class filtered solver output for `POST /api/classes/{id}/schedule`."""

    placements: list[PlacementResponse]
    violations: list[ViolationResponse]
    soft_score: int = Field(default=0, ge=0)


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
