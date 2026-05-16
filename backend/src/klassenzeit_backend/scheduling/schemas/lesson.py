"""Pydantic schemas for lesson routes."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field, model_validator


class LessonCreate(BaseModel):
    """Request body for creating a lesson."""

    school_class_ids: list[uuid.UUID] = Field(min_length=1)
    subject_id: uuid.UUID
    teacher_id: uuid.UUID | None = None
    hours_per_week: int = Field(ge=1)
    preferred_block_size: int = Field(default=1, ge=1, le=2)
    pre_buffer_minutes: int = Field(default=0, ge=0, le=60)
    post_buffer_minutes: int = Field(default=0, ge=0, le=60)
    lesson_group_id: uuid.UUID | None = None

    @model_validator(mode="after")
    def _lesson_create_school_class_ids_unique(self) -> "LessonCreate":
        if len(set(self.school_class_ids)) != len(self.school_class_ids):
            raise ValueError("school_class_ids must not contain duplicates")
        return self

    @model_validator(mode="after")
    def _lesson_create_hours_divisible_by_block_size(self) -> "LessonCreate":
        if self.hours_per_week % self.preferred_block_size != 0:
            raise ValueError("hours_per_week must be divisible by preferred_block_size")
        return self


class LessonUpdate(BaseModel):
    """Request body for patching a lesson."""

    school_class_ids: list[uuid.UUID] | None = Field(default=None, min_length=1)
    teacher_id: uuid.UUID | None = None
    hours_per_week: int | None = Field(default=None, ge=1)
    preferred_block_size: int | None = Field(default=None, ge=1, le=2)
    pre_buffer_minutes: int | None = Field(default=None, ge=0, le=60)
    post_buffer_minutes: int | None = Field(default=None, ge=0, le=60)
    lesson_group_id: uuid.UUID | None = None

    @model_validator(mode="after")
    def _lesson_update_school_class_ids_unique(self) -> "LessonUpdate":
        if self.school_class_ids is not None and (
            len(set(self.school_class_ids)) != len(self.school_class_ids)
        ):
            raise ValueError("school_class_ids must not contain duplicates")
        return self


class LessonClassResponse(BaseModel):
    """Embedded school class in a lesson response."""

    id: uuid.UUID
    name: str


class LessonSubjectResponse(BaseModel):
    """Embedded subject in a lesson response."""

    id: uuid.UUID
    name: str
    short_name: str


class LessonTeacherResponse(BaseModel):
    """Embedded teacher in a lesson response."""

    id: uuid.UUID
    first_name: str
    last_name: str
    short_code: str


class LessonResponse(BaseModel):
    """Response body for a lesson."""

    id: uuid.UUID
    school_classes: list[LessonClassResponse]
    subject: LessonSubjectResponse
    teacher: LessonTeacherResponse | None
    hours_per_week: int
    preferred_block_size: int
    pre_buffer_minutes: int
    post_buffer_minutes: int
    lesson_group_id: uuid.UUID | None
    created_at: datetime
    updated_at: datetime
