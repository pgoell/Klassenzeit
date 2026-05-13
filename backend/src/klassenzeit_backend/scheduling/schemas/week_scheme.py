"""Pydantic schemas for week scheme and time block routes."""

import uuid
from datetime import datetime, time

from pydantic import BaseModel, Field

from klassenzeit_backend.db.models.week_scheme import TimeBlockKind


class TimeBlockCreate(BaseModel):
    """Request body for creating a time block."""

    day_of_week: int = Field(ge=0, le=4)
    position: int = Field(ge=1)
    start_time: time
    end_time: time
    kind: TimeBlockKind = TimeBlockKind.LESSON


class TimeBlockUpdate(BaseModel):
    """Request body for patching a time block."""

    day_of_week: int | None = Field(default=None, ge=0, le=4)
    position: int | None = Field(default=None, ge=1)
    start_time: time | None = None
    end_time: time | None = None
    kind: TimeBlockKind | None = None


class TimeBlockResponse(BaseModel):
    """Response body for a time block."""

    id: uuid.UUID
    day_of_week: int
    position: int
    start_time: time
    end_time: time
    kind: TimeBlockKind


class WeekSchemeCreate(BaseModel):
    """Request body for creating a week scheme."""

    name: str
    description: str | None = None


class WeekSchemeUpdate(BaseModel):
    """Request body for patching a week scheme."""

    name: str | None = None
    description: str | None = None


class WeekSchemeListResponse(BaseModel):
    """Response body for a week scheme in list view (no time blocks)."""

    id: uuid.UUID
    name: str
    description: str | None
    created_at: datetime
    updated_at: datetime


class WeekSchemeDetailResponse(BaseModel):
    """Response body for a week scheme detail view (with time blocks)."""

    id: uuid.UUID
    name: str
    description: str | None
    time_blocks: list[TimeBlockResponse]
    created_at: datetime
    updated_at: datetime
