"""Schemas for admin user management routes."""

import uuid
from datetime import datetime
from typing import Literal

from pydantic import BaseModel, EmailStr


class CreateUserRequest(BaseModel):
    """Request body for admin user creation."""

    email: EmailStr
    password: str
    role: str = "user"


class UserResponse(BaseModel):
    """Response body after creating a user."""

    id: uuid.UUID
    email: str
    role: str


class UserListItem(BaseModel):
    """Single entry in the admin user listing."""

    id: uuid.UUID
    email: str
    role: str
    is_active: bool
    last_login_at: datetime | None


class ResetPasswordRequest(BaseModel):
    """Request body for admin password reset."""

    new_password: str


class SetRoleRequest(BaseModel):
    """Request body for changing a user's role.

    The `role` field is constrained to the three in-tree role strings.
    """

    role: Literal["user", "admin", "super_admin"]
