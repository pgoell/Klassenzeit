"""Pydantic schemas for school class routes."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field


class SchoolClassCreate(BaseModel):
    """Request body for creating a school class."""

    name: str
    grade_level: int = Field(ge=1, le=13)
    stundentafel_id: uuid.UUID
    week_scheme_id: uuid.UUID
    home_room_id: uuid.UUID | None = None


class SchoolClassUpdate(BaseModel):
    """Request body for patching a school class."""

    name: str | None = None
    grade_level: int | None = Field(default=None, ge=1, le=13)
    stundentafel_id: uuid.UUID | None = None
    week_scheme_id: uuid.UUID | None = None
    home_room_id: uuid.UUID | None = None


class SchoolClassResponse(BaseModel):
    """Response body for a school class."""

    id: uuid.UUID
    name: str
    grade_level: int
    stundentafel_id: uuid.UUID
    week_scheme_id: uuid.UUID
    home_room_id: uuid.UUID | None = None
    created_at: datetime
    updated_at: datetime
