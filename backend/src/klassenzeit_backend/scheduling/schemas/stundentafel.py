"""Pydantic schemas for Stundentafel routes."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field, model_validator

from klassenzeit_backend.db.models.stundentafel import SchoolType


class StundentafelCreate(BaseModel):
    """Request body for creating a Stundentafel."""

    name: str
    grade_level: int = Field(ge=1, le=13)
    school_type: SchoolType = SchoolType.GRUNDSCHULE


class StundentafelUpdate(BaseModel):
    """Request body for patching a Stundentafel."""

    name: str | None = None
    grade_level: int | None = Field(default=None, ge=1, le=13)
    school_type: SchoolType | None = None


class EntrySubjectResponse(BaseModel):
    """Embedded subject in a Stundentafel entry."""

    id: uuid.UUID
    name: str
    short_name: str


class StundentafelEntryResponse(BaseModel):
    """Response body for a Stundentafel entry."""

    id: uuid.UUID
    subject: EntrySubjectResponse
    hours_per_week: int
    preferred_block_size: int


class StundentafelListResponse(BaseModel):
    """Response body for a Stundentafel in list view."""

    id: uuid.UUID
    name: str
    grade_level: int
    school_type: SchoolType
    created_at: datetime
    updated_at: datetime


class StundentafelDetailResponse(BaseModel):
    """Response body for a Stundentafel detail view."""

    id: uuid.UUID
    name: str
    grade_level: int
    school_type: SchoolType
    entries: list[StundentafelEntryResponse]
    created_at: datetime
    updated_at: datetime


class EntryCreate(BaseModel):
    """Request body for adding an entry to a Stundentafel."""

    subject_id: uuid.UUID
    hours_per_week: int = Field(ge=1)
    preferred_block_size: int = Field(default=1, ge=1, le=2)

    @model_validator(mode="after")
    def _entry_hours_divisible_by_block_size(self) -> "EntryCreate":
        if self.hours_per_week % self.preferred_block_size != 0:
            raise ValueError("hours_per_week must be divisible by preferred_block_size")
        return self


class EntryUpdate(BaseModel):
    """Request body for patching a Stundentafel entry."""

    hours_per_week: int | None = Field(default=None, ge=1)
    preferred_block_size: int | None = Field(default=None, ge=1, le=2)
