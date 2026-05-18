"""Schemas for current-user routes."""

import uuid

from pydantic import BaseModel


class AccessibleSchool(BaseModel):
    """A school the current user can operate in."""

    id: uuid.UUID
    name: str


class MeResponse(BaseModel):
    """Response body for the current user profile."""

    id: uuid.UUID
    email: str
    role: str
    force_password_change: bool
    school_id: uuid.UUID
    school_name: str
    active_school_id: uuid.UUID
    active_school_name: str
    accessible_schools: list[AccessibleSchool]


class ChangePasswordRequest(BaseModel):
    """Request body for changing the current user's password."""

    current_password: str
    new_password: str
