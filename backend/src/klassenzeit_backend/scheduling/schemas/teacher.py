"""Pydantic schemas for teacher routes."""

import uuid
from datetime import datetime
from typing import Literal

from pydantic import BaseModel, Field, field_validator

_VALID_WEEKDAYS: frozenset[int] = frozenset({0, 1, 2, 3, 4})


def _validate_working_days(value: set[int] | None) -> list[int] | None:
    if value is None:
        return None
    if not value:
        raise ValueError("working_days must contain at least one day")
    if not value <= _VALID_WEEKDAYS:
        raise ValueError("working_days entries must be in {0, 1, 2, 3, 4}")
    return sorted(value)


class TeacherCreate(BaseModel):
    """Request body for creating a teacher."""

    first_name: str
    last_name: str
    short_code: str
    max_hours_per_week: int = Field(ge=1)
    reserve_hours_per_week: int = Field(default=0, ge=0)
    working_days: set[int] | None = None

    @field_validator("working_days")
    @classmethod
    def _normalize_working_days_create(cls, value: set[int] | None) -> list[int] | None:
        return _validate_working_days(value)


class TeacherUpdate(BaseModel):
    """Request body for patching a teacher."""

    first_name: str | None = None
    last_name: str | None = None
    short_code: str | None = None
    max_hours_per_week: int | None = Field(default=None, ge=1)
    reserve_hours_per_week: int | None = Field(default=None, ge=0)
    working_days: set[int] | None = None

    @field_validator("working_days")
    @classmethod
    def _normalize_working_days_update(cls, value: set[int] | None) -> list[int] | None:
        return _validate_working_days(value)


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
    reserve_hours_per_week: int
    is_active: bool
    subject_ids: list[uuid.UUID] = Field(default_factory=list)
    working_days: list[int] | None = None
    created_at: datetime
    updated_at: datetime


class TeacherDetailResponse(BaseModel):
    """Response body for a teacher detail view."""

    id: uuid.UUID
    first_name: str
    last_name: str
    short_code: str
    max_hours_per_week: int
    reserve_hours_per_week: int
    is_active: bool
    qualifications: list[QualificationResponse]
    availability: list[TeacherAvailabilityEntry]
    working_days: list[int] | None = None
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
