"""Pydantic schemas for the placement-mutation endpoints (Sprint C)."""

import uuid

from pydantic import BaseModel

from klassenzeit_backend.scheduling.schemas.schedule import PlacementResponse

__all__ = [
    "MovePlacementRequest",
    "PinPlacementRequest",
    "PlacementKey",
    "PlacementResponse",
    "SwapPlacementsRequest",
    "SwapPlacementsResponse",
]


class MovePlacementRequest(BaseModel):
    """Body for `PATCH /api/placements/{lesson_id}/{time_block_id}`."""

    time_block_id: uuid.UUID
    room_id: uuid.UUID


class PinPlacementRequest(BaseModel):
    """Body for `PATCH /api/placements/{lesson_id}/{time_block_id}/pin`."""

    pinned: bool


class PlacementKey(BaseModel):
    """Composite key used inside `SwapPlacementsRequest`."""

    lesson_id: uuid.UUID
    time_block_id: uuid.UUID


class SwapPlacementsRequest(BaseModel):
    """Body for `POST /api/placements/swap`."""

    a: PlacementKey
    b: PlacementKey


class SwapPlacementsResponse(BaseModel):
    """Two `PlacementResponse`s after the swap completes."""

    a: PlacementResponse
    b: PlacementResponse
