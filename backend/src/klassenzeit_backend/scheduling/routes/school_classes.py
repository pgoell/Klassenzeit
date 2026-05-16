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


async def _get_school_class(
    db: AsyncSession, class_id: uuid.UUID, school_id: uuid.UUID
) -> SchoolClass:
    """Load a SchoolClass by primary key scoped to a school, or raise 404.

    Returns 404 both for unknown class IDs and for classes belonging to a
    different school. This avoids leaking the existence of other-school
    rows via a 403.

    Args:
        db: Active async database session.
        class_id: UUID of the school class to load.
        school_id: Tenant school to scope the lookup to.

    Returns:
        The matching SchoolClass ORM instance.

    Raises:
        HTTPException: 404 if no school class with that ID exists within the school.
    """
    result = await db.execute(
        select(SchoolClass).where(SchoolClass.id == class_id, SchoolClass.school_id == school_id)
    )
    school_class = result.scalar_one_or_none()
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
        class_teacher_id=school_class.class_teacher_id,
        max_lessons_per_day=school_class.max_lessons_per_day,
        created_at=school_class.created_at,
        updated_at=school_class.updated_at,
    )


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_school_class_route(
    body: SchoolClassCreate,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SchoolClassResponse:
    """Create a new school class scoped to the current user's school.

    Args:
        body: Fields for the new school class including FK references.
        current_user: Injected admin user; the new class's ``school_id`` is
            stamped from ``current_user.school_id``.
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
        class_teacher_id=body.class_teacher_id,
        max_lessons_per_day=body.max_lessons_per_day,
        school_id=current_user.school_id,
    )
    db.add(school_class)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=(
                "A school class with this name already exists, or a referenced"
                " stundentafel/week_scheme/class_teacher does not exist."
            ),
        ) from exc
    await db.refresh(school_class)
    return _to_response(school_class)


@router.get("")
async def list_school_classes(
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> list[SchoolClassResponse]:
    """Return all school classes in the user's school ordered by name.

    Args:
        current_user: Injected admin user; scopes the query to their school.
        db: Injected async database session.

    Returns:
        List of school classes in the user's school sorted alphabetically by name.
    """
    result = await db.execute(
        select(SchoolClass)
        .where(SchoolClass.school_id == current_user.school_id)
        .order_by(SchoolClass.name)
    )
    return [_to_response(sc) for sc in result.scalars()]


@router.get("/{class_id}")
async def get_school_class(
    class_id: uuid.UUID,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SchoolClassResponse:
    """Fetch a single school class by ID scoped to the user's school.

    Args:
        class_id: UUID path parameter identifying the school class.
        current_user: Injected admin user; scopes the lookup to their school.
        db: Injected async database session.

    Returns:
        The matching school class as a SchoolClassResponse.

    Raises:
        HTTPException: 404 if no school class with that ID exists in the user's school.
    """
    school_class = await _get_school_class(db, class_id, current_user.school_id)
    return _to_response(school_class)


@router.patch("/{class_id}")
async def update_school_class_route(
    class_id: uuid.UUID,
    body: SchoolClassUpdate,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SchoolClassResponse:
    """Partially update a school class scoped to the user's school.

    Args:
        class_id: UUID path parameter identifying the school class to patch.
        body: Fields to update; omitted fields remain unchanged.
        current_user: Injected admin user; scopes the lookup to their school.
        db: Injected async database session.

    Returns:
        The updated school class as a SchoolClassResponse.

    Raises:
        HTTPException: 404 if no school class with that ID exists in the user's school.
        HTTPException: 409 if the new name conflicts or FK is invalid.
    """
    school_class = await _get_school_class(db, class_id, current_user.school_id)
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
    if "class_teacher_id" in body.model_fields_set:
        school_class.class_teacher_id = body.class_teacher_id
    if "max_lessons_per_day" in body.model_fields_set:
        school_class.max_lessons_per_day = body.max_lessons_per_day
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=(
                "A school class with this name already exists, or a referenced"
                " stundentafel/week_scheme/class_teacher does not exist."
            ),
        ) from exc
    await db.refresh(school_class)
    return _to_response(school_class)


@router.delete("/{class_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_school_class_route(
    class_id: uuid.UUID,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Delete a school class by ID scoped to the user's school.

    Args:
        class_id: UUID path parameter identifying the school class to delete.
        current_user: Injected admin user; scopes the lookup to their school.
        db: Injected async database session.

    Raises:
        HTTPException: 404 if no school class with that ID exists in the user's school.
        HTTPException: 409 if the school class is referenced by lessons or other records.
    """
    school_class = await _get_school_class(db, class_id, current_user.school_id)
    await db.delete(school_class)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Cannot delete school class: it is still referenced by other records.",
        ) from exc
