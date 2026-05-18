"""CRUD routes for the Subject entity."""

import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import get_scope_school_id
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling.schemas.subject import (
    SubjectCreate,
    SubjectResponse,
    SubjectUpdate,
)

router = APIRouter(prefix="/subjects", tags=["subjects"])


async def _get_subject(db: AsyncSession, subject_id: uuid.UUID, school_id: uuid.UUID) -> Subject:
    """Load a Subject by primary key scoped to a school, or raise 404.

    Returns 404 both for unknown subject IDs and for subjects belonging to a
    different school. This avoids leaking the existence of other-school rows
    via a 403.

    Args:
        db: Active async database session.
        subject_id: UUID of the subject to load.
        school_id: Tenant school to scope the lookup to.

    Returns:
        The matching Subject ORM instance.

    Raises:
        HTTPException: 404 if no subject with that ID exists in the school.
    """
    result = await db.execute(
        select(Subject).where(Subject.id == subject_id, Subject.school_id == school_id)
    )
    subject = result.scalar_one_or_none()
    if subject is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return subject


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_subject_route(
    body: SubjectCreate,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> SubjectResponse:
    """Create a new subject in the current operating school.

    Args:
        body: Name, short_name, and color for the new subject.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``; stamped on the new row.

    Returns:
        The created subject as a SubjectResponse.

    Raises:
        HTTPException: 409 if name or short_name conflicts with an existing
            subject in the operating school.
    """
    subject = Subject(
        name=body.name,
        short_name=body.short_name,
        color=body.color,
        prefer_early_period=body.prefer_early_period,
        prefer_late_period=body.prefer_late_period,
        avoid_first_period=body.avoid_first_period,
        avoid_last_period=body.avoid_last_period,
        max_hours_per_day=body.max_hours_per_day,
        school_id=scope_school_id,
    )
    db.add(subject)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A subject with this name or short_name already exists.",
        ) from exc
    await db.refresh(subject)
    return SubjectResponse(
        id=subject.id,
        name=subject.name,
        short_name=subject.short_name,
        color=subject.color,
        prefer_early_period=subject.prefer_early_period,
        prefer_late_period=subject.prefer_late_period,
        avoid_first_period=subject.avoid_first_period,
        avoid_last_period=subject.avoid_last_period,
        max_hours_per_day=subject.max_hours_per_day,
        created_at=subject.created_at,
        updated_at=subject.updated_at,
    )


@router.get("")
async def list_subjects(
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> list[SubjectResponse]:
    """Return all subjects in the current operating school, ordered by name.

    Args:
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``; scopes the query.

    Returns:
        List of subjects in the operating school sorted alphabetically by name.
    """
    result = await db.execute(
        select(Subject).where(Subject.school_id == scope_school_id).order_by(Subject.name)
    )
    return [
        SubjectResponse(
            id=s.id,
            name=s.name,
            short_name=s.short_name,
            color=s.color,
            prefer_early_period=s.prefer_early_period,
            prefer_late_period=s.prefer_late_period,
            avoid_first_period=s.avoid_first_period,
            avoid_last_period=s.avoid_last_period,
            max_hours_per_day=s.max_hours_per_day,
            created_at=s.created_at,
            updated_at=s.updated_at,
        )
        for s in result.scalars()
    ]


@router.get("/{subject_id}")
async def get_subject(
    subject_id: uuid.UUID,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> SubjectResponse:
    """Fetch a single subject by ID, scoped to the operating school.

    Args:
        subject_id: UUID path parameter identifying the subject.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Returns:
        The matching subject as a SubjectResponse.

    Raises:
        HTTPException: 404 if no subject with that ID exists in the operating school.
    """
    subject = await _get_subject(db, subject_id, scope_school_id)
    return SubjectResponse(
        id=subject.id,
        name=subject.name,
        short_name=subject.short_name,
        color=subject.color,
        prefer_early_period=subject.prefer_early_period,
        prefer_late_period=subject.prefer_late_period,
        avoid_first_period=subject.avoid_first_period,
        avoid_last_period=subject.avoid_last_period,
        max_hours_per_day=subject.max_hours_per_day,
        created_at=subject.created_at,
        updated_at=subject.updated_at,
    )


@router.patch("/{subject_id}")
async def update_subject(
    subject_id: uuid.UUID,
    body: SubjectUpdate,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> SubjectResponse:
    """Partially update a subject's name, short_name, or color.

    Args:
        subject_id: UUID path parameter identifying the subject to patch.
        body: Fields to update; omitted fields remain unchanged.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Returns:
        The updated subject as a SubjectResponse.

    Raises:
        HTTPException: 404 if no subject with that ID exists in the operating school.
        HTTPException: 409 if the new name or short_name conflicts.
    """
    subject = await _get_subject(db, subject_id, scope_school_id)
    if body.name is not None:
        subject.name = body.name
    if body.short_name is not None:
        subject.short_name = body.short_name
    if body.color is not None:
        subject.color = body.color
    if body.prefer_early_period is not None:
        subject.prefer_early_period = body.prefer_early_period
    if body.prefer_late_period is not None:
        subject.prefer_late_period = body.prefer_late_period
    if body.avoid_first_period is not None:
        subject.avoid_first_period = body.avoid_first_period
    if body.avoid_last_period is not None:
        subject.avoid_last_period = body.avoid_last_period
    if body.max_hours_per_day is not None:
        subject.max_hours_per_day = body.max_hours_per_day
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A subject with this name or short_name already exists.",
        ) from exc
    await db.refresh(subject)
    return SubjectResponse(
        id=subject.id,
        name=subject.name,
        short_name=subject.short_name,
        color=subject.color,
        prefer_early_period=subject.prefer_early_period,
        prefer_late_period=subject.prefer_late_period,
        avoid_first_period=subject.avoid_first_period,
        avoid_last_period=subject.avoid_last_period,
        max_hours_per_day=subject.max_hours_per_day,
        created_at=subject.created_at,
        updated_at=subject.updated_at,
    )


@router.delete("/{subject_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_subject(
    subject_id: uuid.UUID,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> None:
    """Delete a subject by ID, scoped to the operating school.

    Args:
        subject_id: UUID path parameter identifying the subject to delete.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Raises:
        HTTPException: 404 if no subject with that ID exists in the operating school.
        HTTPException: 409 if the subject is referenced by other records (FK protection).
    """
    subject = await _get_subject(db, subject_id, scope_school_id)
    await db.delete(subject)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Cannot delete subject: it is still referenced by other records.",
        ) from exc
