"""Schemas for school CRUD routes (item 10b)."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field, model_validator


class SchoolCreate(BaseModel):
    """Request body for creating a school."""

    name: str = Field(min_length=1, max_length=120)
    short_name: str | None = Field(default=None, min_length=1, max_length=20)


class SchoolUpdate(BaseModel):
    """Request body for patching a school.

    At least one field must be present in the payload; an empty body
    (``{}``) is rejected with 422 by the post-init validator below.
    Explicit ``short_name=null`` is a valid "clear" operation.
    """

    name: str | None = Field(default=None, min_length=1, max_length=120)
    short_name: str | None = Field(default=None, min_length=1, max_length=20)

    @model_validator(mode="after")
    def _require_at_least_one_field(self) -> "SchoolUpdate":
        if not self.model_fields_set:
            raise ValueError("At least one field must be provided.")
        return self


class SchoolResponse(BaseModel):
    """Detail response returned by create, get, and update."""

    id: uuid.UUID
    name: str
    short_name: str | None
    created_at: datetime
    updated_at: datetime


class SchoolListItem(BaseModel):
    """Lightweight entry returned by list."""

    id: uuid.UUID
    name: str
    short_name: str | None
