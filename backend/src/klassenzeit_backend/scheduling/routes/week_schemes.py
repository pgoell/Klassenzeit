"""CRUD routes for the WeekScheme and TimeBlock entities."""

import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import get_scope_school_id
from klassenzeit_backend.db.models.week_scheme import TimeBlock, WeekScheme
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling.schemas.week_scheme import (
    TimeBlockCreate,
    TimeBlockResponse,
    TimeBlockUpdate,
    WeekSchemeCreate,
    WeekSchemeDetailResponse,
    WeekSchemeListResponse,
    WeekSchemeUpdate,
)

router = APIRouter(prefix="/week-schemes", tags=["week-schemes"])


async def _get_week_scheme(
    db: AsyncSession, scheme_id: uuid.UUID, school_id: uuid.UUID
) -> WeekScheme:
    """Load a WeekScheme by primary key scoped to a school or raise 404.

    Args:
        db: Active async database session.
        scheme_id: UUID of the week scheme to load.
        school_id: Tenant filter; rows in other schools surface as 404.

    Returns:
        The matching WeekScheme ORM instance.

    Raises:
        HTTPException: 404 if no scheme with that ID exists in the given school.
    """
    result = await db.execute(
        select(WeekScheme).where(WeekScheme.id == scheme_id, WeekScheme.school_id == school_id)
    )
    scheme = result.scalar_one_or_none()
    if scheme is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return scheme


async def _get_time_block(db: AsyncSession, scheme_id: uuid.UUID, block_id: uuid.UUID) -> TimeBlock:
    """Load a TimeBlock by primary key and verify it belongs to the given scheme.

    Args:
        db: Active async database session.
        scheme_id: UUID of the parent week scheme.
        block_id: UUID of the time block to load.

    Returns:
        The matching TimeBlock ORM instance.

    Raises:
        HTTPException: 404 if the time block does not exist or belongs to a different scheme.
    """
    block = await db.get(TimeBlock, block_id)
    if block is None or block.week_scheme_id != scheme_id:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return block


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_week_scheme_route(
    body: WeekSchemeCreate,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> WeekSchemeListResponse:
    """Create a new week scheme in the current operating school.

    Args:
        body: Name and optional description for the new week scheme.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``; stamped on the new row.

    Returns:
        The created week scheme as a WeekSchemeListResponse.

    Raises:
        HTTPException: 409 if a scheme with this name already exists in the school.
    """
    scheme = WeekScheme(
        name=body.name,
        description=body.description,
        school_id=scope_school_id,
    )
    db.add(scheme)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A week scheme with this name already exists.",
        ) from exc
    await db.refresh(scheme)
    return WeekSchemeListResponse(
        id=scheme.id,
        name=scheme.name,
        description=scheme.description,
        created_at=scheme.created_at,
        updated_at=scheme.updated_at,
    )


@router.get("")
async def list_week_schemes(
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> list[WeekSchemeListResponse]:
    """Return all week schemes in the current operating school ordered by name.

    Args:
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``; scopes the query.

    Returns:
        List of week schemes in the operating school sorted alphabetically by name.
    """
    result = await db.execute(
        select(WeekScheme).where(WeekScheme.school_id == scope_school_id).order_by(WeekScheme.name)
    )
    return [
        WeekSchemeListResponse(
            id=s.id,
            name=s.name,
            description=s.description,
            created_at=s.created_at,
            updated_at=s.updated_at,
        )
        for s in result.scalars()
    ]


@router.get("/{scheme_id}")
async def get_week_scheme_route(
    scheme_id: uuid.UUID,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> WeekSchemeDetailResponse:
    """Fetch a single week scheme by ID, including its time blocks.

    Args:
        scheme_id: UUID path parameter identifying the week scheme.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Returns:
        The matching week scheme with nested time blocks as a WeekSchemeDetailResponse.

    Raises:
        HTTPException: 404 if no week scheme with that ID exists in the operating school.
    """
    scheme = await _get_week_scheme(db, scheme_id, scope_school_id)
    blocks_result = await db.execute(
        select(TimeBlock)
        .where(TimeBlock.week_scheme_id == scheme_id)
        .order_by(TimeBlock.day_of_week, TimeBlock.position)
    )
    time_blocks = [
        TimeBlockResponse(
            id=b.id,
            day_of_week=b.day_of_week,
            position=b.position,
            start_time=b.start_time,
            end_time=b.end_time,
            kind=b.kind,
        )
        for b in blocks_result.scalars()
    ]
    return WeekSchemeDetailResponse(
        id=scheme.id,
        name=scheme.name,
        description=scheme.description,
        time_blocks=time_blocks,
        created_at=scheme.created_at,
        updated_at=scheme.updated_at,
    )


@router.patch("/{scheme_id}")
async def update_week_scheme_route(
    scheme_id: uuid.UUID,
    body: WeekSchemeUpdate,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> WeekSchemeListResponse:
    """Partially update a week scheme's name or description.

    Args:
        scheme_id: UUID path parameter identifying the week scheme to patch.
        body: Fields to update; omitted fields remain unchanged.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Returns:
        The updated week scheme as a WeekSchemeListResponse.

    Raises:
        HTTPException: 404 if no week scheme with that ID exists in the operating school.
        HTTPException: 409 if the new name conflicts with an existing scheme.
    """
    scheme = await _get_week_scheme(db, scheme_id, scope_school_id)
    if body.name is not None:
        scheme.name = body.name
    if body.description is not None:
        scheme.description = body.description
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A week scheme with this name already exists.",
        ) from exc
    await db.refresh(scheme)
    return WeekSchemeListResponse(
        id=scheme.id,
        name=scheme.name,
        description=scheme.description,
        created_at=scheme.created_at,
        updated_at=scheme.updated_at,
    )


@router.delete("/{scheme_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_week_scheme_route(
    scheme_id: uuid.UUID,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> None:
    """Delete a week scheme by ID.

    Args:
        scheme_id: UUID path parameter identifying the week scheme to delete.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Raises:
        HTTPException: 404 if no week scheme with that ID exists in the operating school.
        HTTPException: 409 if the scheme is referenced by classes (FK protection).
    """
    scheme = await _get_week_scheme(db, scheme_id, scope_school_id)
    await db.delete(scheme)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Cannot delete week scheme: it is still referenced by classes.",
        ) from exc


@router.post("/{scheme_id}/time-blocks", status_code=status.HTTP_201_CREATED)
async def create_time_block_route(
    scheme_id: uuid.UUID,
    body: TimeBlockCreate,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> TimeBlockResponse:
    """Create a new time block within a week scheme.

    Args:
        scheme_id: UUID path parameter identifying the parent week scheme.
        body: day_of_week, position, start_time, and end_time for the new block.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``; the parent scheme is gated by this scope.

    Returns:
        The created time block as a TimeBlockResponse.

    Raises:
        HTTPException: 404 if the week scheme does not exist in the operating school.
        HTTPException: 409 if a block with the same day_of_week+position already exists.
    """
    await _get_week_scheme(db, scheme_id, scope_school_id)
    block = TimeBlock(
        week_scheme_id=scheme_id,
        day_of_week=body.day_of_week,
        position=body.position,
        start_time=body.start_time,
        end_time=body.end_time,
        kind=body.kind,
    )
    db.add(block)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A time block with this day and position already exists in this scheme.",
        ) from exc
    await db.refresh(block)
    return TimeBlockResponse(
        id=block.id,
        day_of_week=block.day_of_week,
        position=block.position,
        start_time=block.start_time,
        end_time=block.end_time,
        kind=block.kind,
    )


@router.patch("/{scheme_id}/time-blocks/{block_id}")
async def update_time_block_route(
    scheme_id: uuid.UUID,
    block_id: uuid.UUID,
    body: TimeBlockUpdate,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> TimeBlockResponse:
    """Partially update a time block's fields.

    Args:
        scheme_id: UUID path parameter identifying the parent week scheme.
        block_id: UUID path parameter identifying the time block to patch.
        body: Fields to update; omitted fields remain unchanged.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``; the parent scheme is gated by this scope.

    Returns:
        The updated time block as a TimeBlockResponse.

    Raises:
        HTTPException: 404 if the parent scheme is in a different school, or if
            the time block does not exist or belongs to a different scheme.
        HTTPException: 409 if the new day+position conflicts with an existing block.
    """
    await _get_week_scheme(db, scheme_id, scope_school_id)
    block = await _get_time_block(db, scheme_id, block_id)
    if body.day_of_week is not None:
        block.day_of_week = body.day_of_week
    if body.position is not None:
        block.position = body.position
    if body.start_time is not None:
        block.start_time = body.start_time
    if body.end_time is not None:
        block.end_time = body.end_time
    if "kind" in body.model_fields_set and body.kind is not None:
        block.kind = body.kind
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A time block with this day and position already exists in this scheme.",
        ) from exc
    await db.refresh(block)
    return TimeBlockResponse(
        id=block.id,
        day_of_week=block.day_of_week,
        position=block.position,
        start_time=block.start_time,
        end_time=block.end_time,
        kind=block.kind,
    )


@router.delete("/{scheme_id}/time-blocks/{block_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_time_block_route(
    scheme_id: uuid.UUID,
    block_id: uuid.UUID,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> None:
    """Delete a time block from a week scheme.

    Args:
        scheme_id: UUID path parameter identifying the parent week scheme.
        block_id: UUID path parameter identifying the time block to delete.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``; the parent scheme is gated by this scope.

    Raises:
        HTTPException: 404 if the parent scheme is in a different school, or if
            the time block does not exist or belongs to a different scheme.
        HTTPException: 409 if the block is referenced by availabilities (FK protection).
    """
    await _get_week_scheme(db, scheme_id, scope_school_id)
    block = await _get_time_block(db, scheme_id, block_id)
    await db.delete(block)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Cannot delete time block: it is still referenced by other records.",
        ) from exc
