"""Pydantic schemas for teacher routes."""

import uuid
from datetime import datetime
from typing import Literal

from pydantic import BaseModel, Field


class TeacherCreate(BaseModel):
    """Request body for creating a teacher."""

    first_name: str
    last_name: str
    short_code: str
    max_hours_per_week: int = Field(ge=1)


class TeacherUpdate(BaseModel):
    """Request body for patching a teacher."""

    first_name: str | None = None
    last_name: str | None = None
    short_code: str | None = None
    max_hours_per_week: int | None = Field(default=None, ge=1)


class QualificationResponse(BaseModel):
    """Subject in a teacher's qualification list."""

    id: uuid.UUID
    name: str
    short_name: str


class TeacherAvailabilityEntry(BaseModel):
    """Single availability entry in responses."""

    time_block_id: uuid.UUID
    day_of_week: int
    position: int
    status: str


class TeacherListResponse(BaseModel):
    """Response body for a teacher in list view."""

    id: uuid.UUID
    first_name: str
    last_name: str
    short_code: str
    max_hours_per_week: int
    is_active: bool
    subject_ids: list[uuid.UUID] = Field(default_factory=list)
    created_at: datetime
    updated_at: datetime


class TeacherDetailResponse(BaseModel):
    """Response body for a teacher detail view."""

    id: uuid.UUID
    first_name: str
    last_name: str
    short_code: str
    max_hours_per_week: int
    is_active: bool
    qualifications: list[QualificationResponse]
    availability: list[TeacherAvailabilityEntry]
    created_at: datetime
    updated_at: datetime


class QualificationsReplaceRequest(BaseModel):
    """Request body for replacing a teacher's qualifications."""

    subject_ids: list[uuid.UUID]


class AvailabilityEntryInput(BaseModel):
    """Single availability entry in request."""

    time_block_id: uuid.UUID
    status: Literal["available", "preferred", "unavailable"]


class AvailabilityReplaceRequest(BaseModel):
    """Request body for replacing a teacher's availability."""

    entries: list[AvailabilityEntryInput]
