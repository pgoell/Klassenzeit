"""CRUD routes for the SchoolClass entity."""

import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling.schemas.school_class import (
    SchoolClassCreate,
    SchoolClassResponse,
    SchoolClassUpdate,
)

router = APIRouter(prefix="/classes", tags=["classes"])


async def _get_school_class(db: AsyncSession, class_id: uuid.UUID) -> SchoolClass:
    """Load a SchoolClass by primary key or raise 404.

    Args:
        db: Active async database session.
        class_id: UUID of the school class to load.

    Returns:
        The matching SchoolClass ORM instance.

    Raises:
        HTTPException: 404 if no school class with that ID exists.
    """
    school_class = await db.get(SchoolClass, class_id)
    if school_class is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return school_class


def _to_response(school_class: SchoolClass) -> SchoolClassResponse:
    """Convert a SchoolClass ORM instance to a SchoolClassResponse.

    Args:
        school_class: The ORM instance to convert.

    Returns:
        A SchoolClassResponse populated from the ORM instance.
    """
    return SchoolClassResponse(
        id=school_class.id,
        name=school_class.name,
        grade_level=school_class.grade_level,
        stundentafel_id=school_class.stundentafel_id,
        week_scheme_id=school_class.week_scheme_id,
        home_room_id=school_class.home_room_id,
        created_at=school_class.created_at,
        updated_at=school_class.updated_at,
    )


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_school_class_route(
    body: SchoolClassCreate,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SchoolClassResponse:
    """Create a new school class.

    Args:
        body: Fields for the new school class including FK references.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The created school class as a SchoolClassResponse.

    Raises:
        HTTPException: 409 if name conflicts or FKs are invalid.
    """
    school_class = SchoolClass(
        name=body.name,
        grade_level=body.grade_level,
        stundentafel_id=body.stundentafel_id,
        week_scheme_id=body.week_scheme_id,
        home_room_id=body.home_room_id,
    )
    db.add(school_class)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=(
                "A school class with this name already exists, or a referenced"
                " stundentafel/week_scheme does not exist."
            ),
        ) from exc
    await db.refresh(school_class)
    return _to_response(school_class)


@router.get("")
async def list_school_classes(
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> list[SchoolClassResponse]:
    """Return all school classes ordered by name.

    Args:
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        List of all school classes sorted alphabetically by name.
    """
    result = await db.execute(select(SchoolClass).order_by(SchoolClass.name))
    return [_to_response(sc) for sc in result.scalars()]


@router.get("/{class_id}")
async def get_school_class(
    class_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SchoolClassResponse:
    """Fetch a single school class by ID.

    Args:
        class_id: UUID path parameter identifying the school class.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The matching school class as a SchoolClassResponse.

    Raises:
        HTTPException: 404 if no school class with that ID exists.
    """
    school_class = await _get_school_class(db, class_id)
    return _to_response(school_class)


@router.patch("/{class_id}")
async def update_school_class_route(
    class_id: uuid.UUID,
    body: SchoolClassUpdate,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SchoolClassResponse:
    """Partially update a school class.

    Args:
        class_id: UUID path parameter identifying the school class to patch.
        body: Fields to update; omitted fields remain unchanged.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The updated school class as a SchoolClassResponse.

    Raises:
        HTTPException: 404 if no school class with that ID exists.
        HTTPException: 409 if the new name conflicts or FK is invalid.
    """
    school_class = await _get_school_class(db, class_id)
    if body.name is not None:
        school_class.name = body.name
    if body.grade_level is not None:
        school_class.grade_level = body.grade_level
    if body.stundentafel_id is not None:
        school_class.stundentafel_id = body.stundentafel_id
    if body.week_scheme_id is not None:
        school_class.week_scheme_id = body.week_scheme_id
    if "home_room_id" in body.model_fields_set:
        school_class.home_room_id = body.home_room_id
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=(
                "A school class with this name already exists, or a referenced"
                " stundentafel/week_scheme does not exist."
            ),
        ) from exc
    await db.refresh(school_class)
    return _to_response(school_class)


@router.delete("/{class_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_school_class_route(
    class_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Delete a school class by ID.

    Args:
        class_id: UUID path parameter identifying the school class to delete.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Raises:
        HTTPException: 404 if no school class with that ID exists.
        HTTPException: 409 if the school class is referenced by lessons or other records.
    """
    school_class = await _get_school_class(db, class_id)
    await db.delete(school_class)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Cannot delete school class: it is still referenced by other records.",
        ) from exc
