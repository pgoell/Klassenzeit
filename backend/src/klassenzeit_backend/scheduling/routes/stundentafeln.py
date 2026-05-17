"""CRUD routes for the Stundentafel and StundentafelEntry entities."""

import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin
from klassenzeit_backend.db.models.stundentafel import Stundentafel, StundentafelEntry
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling.schemas.stundentafel import (
    EntryCreate,
    EntrySubjectResponse,
    EntryUpdate,
    StundentafelCreate,
    StundentafelDetailResponse,
    StundentafelEntryResponse,
    StundentafelListResponse,
    StundentafelUpdate,
)

router = APIRouter(prefix="/stundentafeln", tags=["stundentafeln"])


async def _get_stundentafel(
    db: AsyncSession, tafel_id: uuid.UUID, school_id: uuid.UUID
) -> Stundentafel:
    """Load a Stundentafel by primary key scoped to a school, or raise 404.

    Returns 404 both for unknown Stundentafel IDs and for Stundentafeln
    belonging to a different school. This avoids leaking the existence of
    other-school rows via a 403.

    Args:
        db: Active async database session.
        tafel_id: UUID of the Stundentafel to load.
        school_id: Tenant school to scope the lookup to.

    Returns:
        The matching Stundentafel ORM instance.

    Raises:
        HTTPException: 404 if no Stundentafel with that ID exists in the school.
    """
    result = await db.execute(
        select(Stundentafel).where(Stundentafel.id == tafel_id, Stundentafel.school_id == school_id)
    )
    tafel = result.scalar_one_or_none()
    if tafel is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return tafel


async def _get_entry(
    db: AsyncSession, tafel_id: uuid.UUID, entry_id: uuid.UUID
) -> StundentafelEntry:
    """Load a StundentafelEntry by primary key and verify it belongs to the given Stundentafel.

    Args:
        db: Active async database session.
        tafel_id: UUID of the parent Stundentafel.
        entry_id: UUID of the entry to load.

    Returns:
        The matching StundentafelEntry ORM instance.

    Raises:
        HTTPException: 404 if the entry does not exist or belongs to a different Stundentafel.
    """
    entry = await db.get(StundentafelEntry, entry_id)
    if entry is None or entry.stundentafel_id != tafel_id:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return entry


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_stundentafel_route(
    body: StundentafelCreate,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> StundentafelListResponse:
    """Create a new Stundentafel in the requesting user's school.

    Args:
        body: Name and grade_level for the new Stundentafel.
        current_user: Injected admin user; supplies the tenant school_id stamp.
        db: Injected async database session.

    Returns:
        The created Stundentafel as a StundentafelListResponse.

    Raises:
        HTTPException: 409 if a Stundentafel with this name already exists
            in the user's school.
    """
    tafel = Stundentafel(
        name=body.name,
        grade_level=body.grade_level,
        school_type=body.school_type,
        school_id=current_user.school_id,
    )
    db.add(tafel)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A Stundentafel with this name already exists.",
        ) from exc
    await db.refresh(tafel)
    return StundentafelListResponse(
        id=tafel.id,
        name=tafel.name,
        grade_level=tafel.grade_level,
        school_type=tafel.school_type,
        created_at=tafel.created_at,
        updated_at=tafel.updated_at,
    )


@router.get("")
async def list_stundentafeln(
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> list[StundentafelListResponse]:
    """Return all Stundentafeln in the user's school ordered by name.

    Args:
        current_user: Injected admin user; scopes the query to their school.
        db: Injected async database session.

    Returns:
        List of all Stundentafeln in the user's school sorted alphabetically by name.
    """
    result = await db.execute(
        select(Stundentafel)
        .where(Stundentafel.school_id == current_user.school_id)
        .order_by(Stundentafel.name)
    )
    return [
        StundentafelListResponse(
            id=t.id,
            name=t.name,
            grade_level=t.grade_level,
            school_type=t.school_type,
            created_at=t.created_at,
            updated_at=t.updated_at,
        )
        for t in result.scalars()
    ]


@router.get("/{tafel_id}")
async def get_stundentafel(
    tafel_id: uuid.UUID,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> StundentafelDetailResponse:
    """Fetch a single Stundentafel by ID, including its entries with subject info.

    Args:
        tafel_id: UUID path parameter identifying the Stundentafel.
        current_user: Injected admin user; scopes the entry Subject join to their school.
        db: Injected async database session.

    Returns:
        The matching Stundentafel with nested entries as a StundentafelDetailResponse.

    Raises:
        HTTPException: 404 if no Stundentafel with that ID exists.
    """
    tafel = await _get_stundentafel(db, tafel_id, current_user.school_id)
    entries_result = await db.execute(
        select(StundentafelEntry, Subject)
        .join(Subject, StundentafelEntry.subject_id == Subject.id)
        .where(
            StundentafelEntry.stundentafel_id == tafel.id,
            Subject.school_id == current_user.school_id,
        )
    )
    entries = [
        StundentafelEntryResponse(
            id=entry.id,
            subject=EntrySubjectResponse(id=subj.id, name=subj.name, short_name=subj.short_name),
            hours_per_week=entry.hours_per_week,
            preferred_block_size=entry.preferred_block_size,
        )
        for entry, subj in entries_result.all()
    ]
    return StundentafelDetailResponse(
        id=tafel.id,
        name=tafel.name,
        grade_level=tafel.grade_level,
        school_type=tafel.school_type,
        entries=entries,
        created_at=tafel.created_at,
        updated_at=tafel.updated_at,
    )


@router.patch("/{tafel_id}")
async def update_stundentafel_route(
    tafel_id: uuid.UUID,
    body: StundentafelUpdate,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> StundentafelListResponse:
    """Partially update a Stundentafel's name or grade_level.

    Args:
        tafel_id: UUID path parameter identifying the Stundentafel to patch.
        body: Fields to update; omitted fields remain unchanged.
        current_user: Injected admin user; scopes the lookup to their school.
        db: Injected async database session.

    Returns:
        The updated Stundentafel as a StundentafelListResponse.

    Raises:
        HTTPException: 404 if no Stundentafel with that ID exists in the user's school.
        HTTPException: 409 if the new name conflicts with an existing Stundentafel.
    """
    tafel = await _get_stundentafel(db, tafel_id, current_user.school_id)
    if body.name is not None:
        tafel.name = body.name
    if body.grade_level is not None:
        tafel.grade_level = body.grade_level
    if body.school_type is not None:
        tafel.school_type = body.school_type
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A Stundentafel with this name already exists.",
        ) from exc
    await db.refresh(tafel)
    return StundentafelListResponse(
        id=tafel.id,
        name=tafel.name,
        grade_level=tafel.grade_level,
        school_type=tafel.school_type,
        created_at=tafel.created_at,
        updated_at=tafel.updated_at,
    )


@router.delete("/{tafel_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_stundentafel_route(
    tafel_id: uuid.UUID,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Delete a Stundentafel by ID scoped to the user's school.

    Args:
        tafel_id: UUID path parameter identifying the Stundentafel to delete.
        current_user: Injected admin user; scopes the lookup to their school.
        db: Injected async database session.

    Raises:
        HTTPException: 404 if no Stundentafel with that ID exists in the user's school.
        HTTPException: 409 if the Stundentafel is referenced by classes (FK protection).
    """
    tafel = await _get_stundentafel(db, tafel_id, current_user.school_id)
    await db.delete(tafel)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Cannot delete Stundentafel: it is still referenced by classes.",
        ) from exc


@router.post("/{tafel_id}/entries", status_code=status.HTTP_201_CREATED)
async def create_stundentafel_entry_route(
    tafel_id: uuid.UUID,
    body: EntryCreate,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> StundentafelEntryResponse:
    """Add a subject entry to a Stundentafel.

    Args:
        tafel_id: UUID path parameter identifying the parent Stundentafel.
        body: subject_id, hours_per_week, and preferred_block_size for the new entry.
        current_user: Injected admin user; scopes the Subject lookup to their school.
        db: Injected async database session.

    Returns:
        The created entry as a StundentafelEntryResponse.

    Raises:
        HTTPException: 404 if the Stundentafel or Subject does not exist.
        HTTPException: 409 if this subject is already in the Stundentafel.
    """
    await _get_stundentafel(db, tafel_id, current_user.school_id)
    entry = StundentafelEntry(
        stundentafel_id=tafel_id,
        subject_id=body.subject_id,
        hours_per_week=body.hours_per_week,
        preferred_block_size=body.preferred_block_size,
    )
    db.add(entry)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="This subject is already in the Stundentafel.",
        ) from exc
    await db.refresh(entry)
    subj = (
        await db.execute(
            select(Subject).where(
                Subject.id == entry.subject_id, Subject.school_id == current_user.school_id
            )
        )
    ).scalar_one_or_none()
    if subj is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Subject not found.")
    return StundentafelEntryResponse(
        id=entry.id,
        subject=EntrySubjectResponse(id=subj.id, name=subj.name, short_name=subj.short_name),
        hours_per_week=entry.hours_per_week,
        preferred_block_size=entry.preferred_block_size,
    )


@router.patch("/{tafel_id}/entries/{entry_id}")
async def update_stundentafel_entry(
    tafel_id: uuid.UUID,
    entry_id: uuid.UUID,
    body: EntryUpdate,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> StundentafelEntryResponse:
    """Partially update a Stundentafel entry's hours or block size.

    Args:
        tafel_id: UUID path parameter identifying the parent Stundentafel.
        entry_id: UUID path parameter identifying the entry to patch.
        body: Fields to update; omitted fields remain unchanged.
        current_user: Injected admin user; scopes the Subject lookup to their school.
        db: Injected async database session.

    Returns:
        The updated entry as a StundentafelEntryResponse.

    Raises:
        HTTPException: 404 if the entry does not exist or belongs to a different Stundentafel.
    """
    await _get_stundentafel(db, tafel_id, current_user.school_id)
    entry = await _get_entry(db, tafel_id, entry_id)
    if body.hours_per_week is not None:
        entry.hours_per_week = body.hours_per_week
    if body.preferred_block_size is not None:
        entry.preferred_block_size = body.preferred_block_size
    if entry.hours_per_week % entry.preferred_block_size != 0:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail="hours_per_week must be divisible by preferred_block_size",
        )
    await db.commit()
    await db.refresh(entry)
    subj = (
        await db.execute(
            select(Subject).where(
                Subject.id == entry.subject_id, Subject.school_id == current_user.school_id
            )
        )
    ).scalar_one_or_none()
    if subj is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Subject not found.")
    return StundentafelEntryResponse(
        id=entry.id,
        subject=EntrySubjectResponse(id=subj.id, name=subj.name, short_name=subj.short_name),
        hours_per_week=entry.hours_per_week,
        preferred_block_size=entry.preferred_block_size,
    )


@router.delete("/{tafel_id}/entries/{entry_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_stundentafel_entry(
    tafel_id: uuid.UUID,
    entry_id: uuid.UUID,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Delete an entry from a Stundentafel.

    Args:
        tafel_id: UUID path parameter identifying the parent Stundentafel.
        entry_id: UUID path parameter identifying the entry to delete.
        current_user: Injected admin user; scopes the parent Stundentafel lookup to their school.
        db: Injected async database session.

    Raises:
        HTTPException: 404 if the entry does not exist or belongs to a different Stundentafel.
    """
    await _get_stundentafel(db, tafel_id, current_user.school_id)
    entry = await _get_entry(db, tafel_id, entry_id)
    await db.delete(entry)
    await db.commit()
