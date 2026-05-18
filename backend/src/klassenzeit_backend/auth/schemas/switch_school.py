"""Schemas for the school-switch endpoint."""

import uuid

from pydantic import BaseModel


class SwitchSchoolRequest(BaseModel):
    """Request body for ``POST /auth/switch-school``."""

    school_id: uuid.UUID
