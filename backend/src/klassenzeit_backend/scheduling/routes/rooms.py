"""CRUD routes for the Room entity with suitability and availability sub-resources."""

import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import delete, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin
from klassenzeit_backend.db.models.room import Room, RoomAvailability, RoomSubjectSuitability
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.models.week_scheme import TimeBlock
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling.schemas.room import (
    AvailabilityReplaceRequest,
    AvailabilityResponse,
    RoomCreate,
    RoomDetailResponse,
    RoomListResponse,
    RoomUpdate,
    SuitabilityReplaceRequest,
    SuitabilitySubjectResponse,
)

router = APIRouter(prefix="/rooms", tags=["rooms"])


async def _get_room(db: AsyncSession, room_id: uuid.UUID) -> Room:
    """Load a Room by primary key or raise 404.

    Args:
        db: Active async database session.
        room_id: UUID of the room to load.

    Returns:
        The matching Room ORM instance.

    Raises:
        HTTPException: 404 if no room with that ID exists.
    """
    room = await db.get(Room, room_id)
    if room is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return room


async def _build_room_detail(db: AsyncSession, room: Room) -> RoomDetailResponse:
    """Build a RoomDetailResponse by loading suitability subjects and availability time blocks.

    Args:
        db: Active async database session.
        room: The Room ORM instance to build the response for.

    Returns:
        A fully populated RoomDetailResponse.
    """
    suit_result = await db.execute(
        select(Subject)
        .join(RoomSubjectSuitability, RoomSubjectSuitability.subject_id == Subject.id)
        .where(RoomSubjectSuitability.room_id == room.id)
        .order_by(Subject.name)
    )
    suitability_subjects = [
        SuitabilitySubjectResponse(id=s.id, name=s.name, short_name=s.short_name)
        for s in suit_result.scalars()
    ]

    avail_result = await db.execute(
        select(RoomAvailability.time_block_id, TimeBlock.day_of_week, TimeBlock.position)
        .join(TimeBlock, RoomAvailability.time_block_id == TimeBlock.id)
        .where(RoomAvailability.room_id == room.id)
        .order_by(TimeBlock.day_of_week, TimeBlock.position)
    )
    availability = [
        AvailabilityResponse(
            time_block_id=row.time_block_id,
            day_of_week=row.day_of_week,
            position=row.position,
        )
        for row in avail_result
    ]

    return RoomDetailResponse(
        id=room.id,
        name=room.name,
        short_name=room.short_name,
        capacity=room.capacity,
        is_external=room.is_external,
        suitability_subjects=suitability_subjects,
        availability=availability,
        created_at=room.created_at,
        updated_at=room.updated_at,
    )


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_room_route(
    body: RoomCreate,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> RoomListResponse:
    """Create a new room.

    Args:
        body: Name, short_name, and capacity for the new room.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The created room as a RoomListResponse.

    Raises:
        HTTPException: 409 if name or short_name conflicts with an existing room.
    """
    room = Room(
        name=body.name,
        short_name=body.short_name,
        capacity=body.capacity,
        is_external=body.is_external,
    )
    db.add(room)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A room with this name or short_name already exists.",
        ) from exc
    await db.refresh(room)
    return RoomListResponse(
        id=room.id,
        name=room.name,
        short_name=room.short_name,
        capacity=room.capacity,
        is_external=room.is_external,
        created_at=room.created_at,
        updated_at=room.updated_at,
    )


@router.get("")
async def list_rooms(
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> list[RoomListResponse]:
    """Return all rooms ordered by name.

    Args:
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        List of all rooms sorted alphabetically by name (no nested suitability or availability).
    """
    result = await db.execute(select(Room).order_by(Room.name))
    return [
        RoomListResponse(
            id=r.id,
            name=r.name,
            short_name=r.short_name,
            capacity=r.capacity,
            is_external=r.is_external,
            created_at=r.created_at,
            updated_at=r.updated_at,
        )
        for r in result.scalars()
    ]


@router.get("/{room_id}")
async def get_room(
    room_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> RoomDetailResponse:
    """Fetch a single room by ID, including suitability subjects and availability time blocks.

    Args:
        room_id: UUID path parameter identifying the room.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The matching room with nested suitability and availability as a RoomDetailResponse.

    Raises:
        HTTPException: 404 if no room with that ID exists.
    """
    room = await _get_room(db, room_id)
    return await _build_room_detail(db, room)


@router.patch("/{room_id}")
async def update_room_route(
    room_id: uuid.UUID,
    body: RoomUpdate,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> RoomListResponse:
    """Partially update a room's fields.

    Args:
        room_id: UUID path parameter identifying the room to patch.
        body: Fields to update; omitted fields remain unchanged.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The updated room as a RoomListResponse.

    Raises:
        HTTPException: 404 if no room with that ID exists.
        HTTPException: 409 if the new name or short_name conflicts.
    """
    room = await _get_room(db, room_id)
    if body.name is not None:
        room.name = body.name
    if body.short_name is not None:
        room.short_name = body.short_name
    if body.capacity is not None:
        room.capacity = body.capacity
    if body.is_external is not None:
        room.is_external = body.is_external
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A room with this name or short_name already exists.",
        ) from exc
    await db.refresh(room)
    return RoomListResponse(
        id=room.id,
        name=room.name,
        short_name=room.short_name,
        capacity=room.capacity,
        is_external=room.is_external,
        created_at=room.created_at,
        updated_at=room.updated_at,
    )


@router.delete("/{room_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_room_route(
    room_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Delete a room by ID.

    Suitability and availability rows are removed automatically by FK ondelete CASCADE.

    Args:
        room_id: UUID path parameter identifying the room to delete.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Raises:
        HTTPException: 404 if no room with that ID exists.
        HTTPException: 409 if the room is referenced by other records (FK protection).
    """
    room = await _get_room(db, room_id)
    await db.delete(room)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Cannot delete room: it is still referenced by other records.",
        ) from exc


@router.put("/{room_id}/suitability")
async def replace_room_suitability(
    room_id: uuid.UUID,
    body: SuitabilityReplaceRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> RoomDetailResponse:
    """Replace the entire suitability subject list for a room.

    Deletes all existing RoomSubjectSuitability rows for the room and inserts
    new ones from the supplied subject_ids list. Deduplicates input server-side.

    Args:
        room_id: UUID path parameter identifying the room.
        body: List of subject UUIDs that define the new suitability set.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The updated room detail including the new suitability list.

    Raises:
        HTTPException: 404 if no room with that ID exists.
        HTTPException: 400 if any subject_id does not exist; body contains
            ``missing_subject_ids`` list.
    """
    room = await _get_room(db, room_id)
    # Deduplicate while preserving order.
    seen: set[uuid.UUID] = set()
    unique_ids: list[uuid.UUID] = []
    for sid in body.subject_ids:
        if sid not in seen:
            seen.add(sid)
            unique_ids.append(sid)

    if unique_ids:
        found = await db.execute(select(Subject.id).where(Subject.id.in_(unique_ids)))
        found_ids = {row[0] for row in found}
        missing = [sid for sid in unique_ids if sid not in found_ids]
        if missing:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail={
                    "detail": "Some subjects do not exist.",
                    "missing_subject_ids": [str(m) for m in missing],
                },
            )

    await db.execute(
        delete(RoomSubjectSuitability).where(RoomSubjectSuitability.room_id == room_id)
    )
    for subject_id in unique_ids:
        db.add(RoomSubjectSuitability(room_id=room_id, subject_id=subject_id))
    await db.commit()

    await db.refresh(room)
    return await _build_room_detail(db, room)


@router.put("/{room_id}/availability")
async def replace_room_availability(
    room_id: uuid.UUID,
    body: AvailabilityReplaceRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> RoomDetailResponse:
    """Replace the entire availability time block list for a room.

    Deletes all existing RoomAvailability rows for the room and inserts new
    ones from the supplied time_block_ids list.

    Args:
        room_id: UUID path parameter identifying the room.
        body: List of time block UUIDs that define the new availability set.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The updated room detail including the new availability list.

    Raises:
        HTTPException: 404 if no room with that ID exists.
        HTTPException: 409 if any time_block_id is invalid (FK violation).
    """
    room = await _get_room(db, room_id)
    await db.execute(delete(RoomAvailability).where(RoomAvailability.room_id == room_id))
    for time_block_id in body.time_block_ids:
        db.add(RoomAvailability(room_id=room_id, time_block_id=time_block_id))
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="One or more time block IDs are invalid.",
        ) from exc

    await db.refresh(room)
    return await _build_room_detail(db, room)
