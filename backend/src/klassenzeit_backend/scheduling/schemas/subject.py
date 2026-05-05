"""Pydantic schemas for subject routes."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field

COLOR_PATTERN = r"^(chart-(1[0-2]|[1-9])|#[0-9a-fA-F]{6})$"


class SubjectCreate(BaseModel):
    """Request body for creating a subject."""

    name: str
    short_name: str
    color: str = Field(pattern=COLOR_PATTERN)
    prefer_early_period: int = Field(0, ge=0, le=10)
    prefer_late_period: int = Field(0, ge=0, le=10)
    avoid_first_period: int = Field(0, ge=0, le=10)
    avoid_last_period: int = Field(0, ge=0, le=10)
    max_hours_per_day: int = Field(2, ge=1, le=20)


class SubjectUpdate(BaseModel):
    """Request body for patching a subject."""

    name: str | None = None
    short_name: str | None = None
    color: str | None = Field(default=None, pattern=COLOR_PATTERN)
    prefer_early_period: int | None = Field(default=None, ge=0, le=10)
    prefer_late_period: int | None = Field(default=None, ge=0, le=10)
    avoid_first_period: int | None = Field(default=None, ge=0, le=10)
    avoid_last_period: int | None = Field(default=None, ge=0, le=10)
    max_hours_per_day: int | None = Field(default=None, ge=1, le=20)


class SubjectResponse(BaseModel):
    """Response body for a subject."""

    id: uuid.UUID
    name: str
    short_name: str
    color: str
    prefer_early_period: int
    prefer_late_period: int
    avoid_first_period: int
    avoid_last_period: int
    max_hours_per_day: int
    created_at: datetime
    updated_at: datetime
