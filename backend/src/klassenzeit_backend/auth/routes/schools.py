"""School CRUD routes (item 10b)."""

import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin
from klassenzeit_backend.auth.schemas.school import (
    SchoolCreate,
    SchoolListItem,
    SchoolResponse,
    SchoolUpdate,
)
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session

router = APIRouter(prefix="/schools", tags=["schools"])


def _school_to_response(school: School) -> SchoolResponse:
    return SchoolResponse(
        id=school.id,
        name=school.name,
        short_name=school.short_name,
        created_at=school.created_at,
        updated_at=school.updated_at,
    )


async def _get_school(db: AsyncSession, school_id: uuid.UUID) -> School:
    school = await db.get(School, school_id)
    if school is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return school


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_school(
    body: SchoolCreate,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SchoolResponse:
    """Create a new school."""
    school = School(name=body.name, short_name=body.short_name)
    db.add(school)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A school with this name or short_name already exists.",
        ) from exc
    await db.refresh(school)
    return _school_to_response(school)


@router.get("")
async def list_schools(
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> list[SchoolListItem]:
    """List every school in the system."""
    result = await db.execute(select(School).order_by(School.name))
    return [SchoolListItem(id=s.id, name=s.name, short_name=s.short_name) for s in result.scalars()]


@router.get("/{school_id}")
async def get_school(
    school_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SchoolResponse:
    """Fetch one school by id."""
    return _school_to_response(await _get_school(db, school_id))


@router.patch("/{school_id}")
async def update_school(
    school_id: uuid.UUID,
    body: SchoolUpdate,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SchoolResponse:
    """Patch a school's name and/or short_name."""
    school = await _get_school(db, school_id)
    fields = body.model_dump(exclude_unset=True)
    for key, value in fields.items():
        setattr(school, key, value)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A school with this name or short_name already exists.",
        ) from exc
    await db.refresh(school)
    return _school_to_response(school)


@router.delete("/{school_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_school(
    school_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Delete a school. Refuses the default school and any school with FK dependents."""
    if school_id == DEFAULT_SCHOOL_ID:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Cannot delete the default school.",
        )
    school = await _get_school(db, school_id)
    await db.delete(school)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Cannot delete school: it is still referenced by other records.",
        ) from exc
