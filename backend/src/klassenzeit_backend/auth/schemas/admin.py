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


class MembershipGrantRequest(BaseModel):
    """Request body for granting a school membership to a user."""

    school_id: uuid.UUID


class MembershipResponse(BaseModel):
    """Response body for grant: echoes user_id so the caller can correlate."""

    user_id: uuid.UUID
    school_id: uuid.UUID
    school_name: str


class MembershipListItem(BaseModel):
    """Single entry in the per-user memberships listing.

    Omits ``user_id`` because the list URL already carries it; mirroring
    the same id on every row would be duplicate metadata.
    """

    school_id: uuid.UUID
    school_name: str
