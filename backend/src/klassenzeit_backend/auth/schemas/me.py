"""Schemas for current-user routes."""

import uuid

from pydantic import BaseModel


class MeResponse(BaseModel):
    """Response body for the current user profile."""

    id: uuid.UUID
    email: str
    role: str
    force_password_change: bool
    school_id: uuid.UUID
    school_name: str


class ChangePasswordRequest(BaseModel):
    """Request body for changing the current user's password."""

    current_password: str
    new_password: str
