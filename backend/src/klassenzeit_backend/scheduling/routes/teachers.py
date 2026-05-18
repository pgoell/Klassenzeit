"""CRUD routes for the Teacher entity with qualifications and availability sub-resources."""

import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import delete, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import get_scope_school_id
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherAvailability, TeacherQualification
from klassenzeit_backend.db.models.week_scheme import TimeBlock
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling.schemas.teacher import (
    AvailabilityReplaceRequest,
    QualificationResponse,
    QualificationsReplaceRequest,
    TeacherAvailabilityEntry,
    TeacherCreate,
    TeacherDetailResponse,
    TeacherListResponse,
    TeacherUpdate,
)

router = APIRouter(prefix="/teachers", tags=["teachers"])


async def _get_teacher(db: AsyncSession, teacher_id: uuid.UUID, school_id: uuid.UUID) -> Teacher:
    """Load a Teacher by primary key scoped to a school, or raise 404.

    Returns 404 both for unknown teacher IDs and for teachers belonging to a
    different school. This avoids leaking the existence of other-school rows
    via a 403.

    Args:
        db: Active async database session.
        teacher_id: UUID of the teacher to load.
        school_id: Tenant school to scope the lookup to.

    Returns:
        The matching Teacher ORM instance.

    Raises:
        HTTPException: 404 if no teacher with that ID exists in the school.
    """
    result = await db.execute(
        select(Teacher).where(Teacher.id == teacher_id, Teacher.school_id == school_id)
    )
    teacher = result.scalar_one_or_none()
    if teacher is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return teacher


async def _build_teacher_detail(db: AsyncSession, teacher: Teacher) -> TeacherDetailResponse:
    """Build a TeacherDetailResponse by loading qualifications and availability.

    Args:
        db: Active async database session.
        teacher: The Teacher ORM instance to build the response for.

    Returns:
        A fully populated TeacherDetailResponse.
    """
    qual_result = await db.execute(
        select(Subject)
        .join(TeacherQualification, TeacherQualification.subject_id == Subject.id)
        .where(
            TeacherQualification.teacher_id == teacher.id,
            Subject.school_id == teacher.school_id,
        )
        .order_by(Subject.name)
    )
    qualifications = [
        QualificationResponse(id=s.id, name=s.name, short_name=s.short_name)
        for s in qual_result.scalars()
    ]

    avail_result = await db.execute(
        select(
            TeacherAvailability.time_block_id,
            TeacherAvailability.status,
            TimeBlock.day_of_week,
            TimeBlock.position,
        )
        .join(TimeBlock, TeacherAvailability.time_block_id == TimeBlock.id)
        .where(TeacherAvailability.teacher_id == teacher.id)
        .order_by(TimeBlock.day_of_week, TimeBlock.position)
    )
    availability = [
        TeacherAvailabilityEntry(
            time_block_id=row.time_block_id,
            day_of_week=row.day_of_week,
            position=row.position,
            status=row.status,
        )
        for row in avail_result
    ]

    return TeacherDetailResponse(
        id=teacher.id,
        first_name=teacher.first_name,
        last_name=teacher.last_name,
        short_code=teacher.short_code,
        max_hours_per_week=teacher.max_hours_per_week,
        reserve_hours_per_week=teacher.reserve_hours_per_week,
        is_active=teacher.is_active,
        qualifications=qualifications,
        availability=availability,
        working_days=teacher.working_days,
        created_at=teacher.created_at,
        updated_at=teacher.updated_at,
    )


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_teacher_route(
    body: TeacherCreate,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> TeacherListResponse:
    """Create a new teacher in the current operating school.

    Args:
        body: First name, last name, short_code, and max_hours_per_week for the new teacher.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``; stamped on the new row.

    Returns:
        The created teacher as a TeacherListResponse.

    Raises:
        HTTPException: 409 if short_code conflicts with an existing teacher in
            the operating school.
    """
    teacher = Teacher(
        first_name=body.first_name,
        last_name=body.last_name,
        short_code=body.short_code,
        max_hours_per_week=body.max_hours_per_week,
        reserve_hours_per_week=body.reserve_hours_per_week,
        working_days=body.working_days,
        school_id=scope_school_id,
    )
    db.add(teacher)
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A teacher with this short_code already exists.",
        ) from exc
    await db.refresh(teacher)
    return TeacherListResponse(
        id=teacher.id,
        first_name=teacher.first_name,
        last_name=teacher.last_name,
        short_code=teacher.short_code,
        max_hours_per_week=teacher.max_hours_per_week,
        reserve_hours_per_week=teacher.reserve_hours_per_week,
        is_active=teacher.is_active,
        subject_ids=[],
        working_days=teacher.working_days,
        created_at=teacher.created_at,
        updated_at=teacher.updated_at,
    )


@router.get("")
async def list_teachers(
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
    active: bool | None = None,
) -> list[TeacherListResponse]:
    """Return teachers in the current operating school ordered by last name.

    Args:
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``; scopes the query.
        active: Optional filter; if True returns only active teachers, if False only inactive.

    Returns:
        List of teachers sorted alphabetically by last name (no nested availability;
        qualified-subject UUIDs are returned as ``subject_ids``).
    """
    query = select(Teacher).where(Teacher.school_id == scope_school_id).order_by(Teacher.last_name)
    if active is not None:
        query = query.where(Teacher.is_active == active)
    teachers = list((await db.execute(query)).scalars())

    teacher_ids = [t.id for t in teachers]
    subject_ids_by_teacher: dict[uuid.UUID, list[uuid.UUID]] = {tid: [] for tid in teacher_ids}
    if teacher_ids:
        qual_rows = await db.execute(
            select(TeacherQualification.teacher_id, TeacherQualification.subject_id).where(
                TeacherQualification.teacher_id.in_(teacher_ids)
            )
        )
        for teacher_id, subject_id in qual_rows:
            subject_ids_by_teacher[teacher_id].append(subject_id)

    return [
        TeacherListResponse(
            id=t.id,
            first_name=t.first_name,
            last_name=t.last_name,
            short_code=t.short_code,
            max_hours_per_week=t.max_hours_per_week,
            reserve_hours_per_week=t.reserve_hours_per_week,
            is_active=t.is_active,
            subject_ids=subject_ids_by_teacher[t.id],
            working_days=t.working_days,
            created_at=t.created_at,
            updated_at=t.updated_at,
        )
        for t in teachers
    ]


@router.get("/{teacher_id}")
async def get_teacher(
    teacher_id: uuid.UUID,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> TeacherDetailResponse:
    """Fetch a single teacher by ID, including qualifications and availability.

    Args:
        teacher_id: UUID path parameter identifying the teacher.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Returns:
        The matching teacher with nested qualifications and availability as a TeacherDetailResponse.

    Raises:
        HTTPException: 404 if no teacher with that ID exists in the operating school.
    """
    teacher = await _get_teacher(db, teacher_id, scope_school_id)
    return await _build_teacher_detail(db, teacher)


@router.patch("/{teacher_id}")
async def update_teacher_route(
    teacher_id: uuid.UUID,
    body: TeacherUpdate,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> TeacherListResponse:
    """Partially update a teacher's fields.

    Args:
        teacher_id: UUID path parameter identifying the teacher to patch.
        body: Fields to update; omitted fields remain unchanged.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Returns:
        The updated teacher as a TeacherListResponse.

    Raises:
        HTTPException: 404 if no teacher with that ID exists in the operating school.
        HTTPException: 409 if the new short_code conflicts with an existing teacher.
    """
    teacher = await _get_teacher(db, teacher_id, scope_school_id)
    if body.first_name is not None:
        teacher.first_name = body.first_name
    if body.last_name is not None:
        teacher.last_name = body.last_name
    if body.short_code is not None:
        teacher.short_code = body.short_code
    if body.max_hours_per_week is not None:
        teacher.max_hours_per_week = body.max_hours_per_week
    if body.reserve_hours_per_week is not None:
        teacher.reserve_hours_per_week = body.reserve_hours_per_week
    if "working_days" in body.model_fields_set:
        teacher.working_days = sorted(body.working_days) if body.working_days is not None else None
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A teacher with this short_code already exists.",
        ) from exc
    await db.refresh(teacher)
    subject_id_rows = await db.execute(
        select(TeacherQualification.subject_id).where(TeacherQualification.teacher_id == teacher.id)
    )
    subject_ids = [row[0] for row in subject_id_rows]
    return TeacherListResponse(
        id=teacher.id,
        first_name=teacher.first_name,
        last_name=teacher.last_name,
        short_code=teacher.short_code,
        max_hours_per_week=teacher.max_hours_per_week,
        reserve_hours_per_week=teacher.reserve_hours_per_week,
        is_active=teacher.is_active,
        subject_ids=subject_ids,
        working_days=teacher.working_days,
        created_at=teacher.created_at,
        updated_at=teacher.updated_at,
    )


@router.delete("/{teacher_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_teacher_route(
    teacher_id: uuid.UUID,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> None:
    """Soft-delete a teacher by setting is_active to False.

    The teacher record is retained in the database and remains accessible via GET.

    Args:
        teacher_id: UUID path parameter identifying the teacher to deactivate.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Raises:
        HTTPException: 404 if no teacher with that ID exists in the operating school.
    """
    teacher = await _get_teacher(db, teacher_id, scope_school_id)
    teacher.is_active = False
    await db.commit()


@router.put("/{teacher_id}/qualifications")
async def replace_teacher_qualifications(
    teacher_id: uuid.UUID,
    body: QualificationsReplaceRequest,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> TeacherDetailResponse:
    """Replace the entire qualification subject list for a teacher.

    Deletes all existing TeacherQualification rows for the teacher and inserts
    new ones from the supplied subject_ids list.

    Args:
        teacher_id: UUID path parameter identifying the teacher.
        body: List of subject UUIDs that define the new qualification set.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Returns:
        The updated teacher detail including the new qualifications list.

    Raises:
        HTTPException: 404 if no teacher with that ID exists in the operating school.
        HTTPException: 409 if any subject_id is invalid (FK violation).
    """
    teacher = await _get_teacher(db, teacher_id, scope_school_id)
    await db.execute(
        delete(TeacherQualification).where(TeacherQualification.teacher_id == teacher_id)
    )
    for subject_id in body.subject_ids:
        db.add(TeacherQualification(teacher_id=teacher_id, subject_id=subject_id))
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="One or more subject IDs are invalid.",
        ) from exc

    await db.refresh(teacher)
    return await _build_teacher_detail(db, teacher)


@router.put("/{teacher_id}/availability")
async def replace_teacher_availability(
    teacher_id: uuid.UUID,
    body: AvailabilityReplaceRequest,
    db: Annotated[AsyncSession, Depends(get_session)],
    scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)],
) -> TeacherDetailResponse:
    """Replace the entire availability list for a teacher.

    Deletes all existing TeacherAvailability rows for the teacher and inserts
    new ones from the supplied entries list.

    Args:
        teacher_id: UUID path parameter identifying the teacher.
        body: List of availability entries with time_block_id and status.
        db: Injected async database session.
        scope_school_id: Per-request operating school resolved by
            ``get_scope_school_id``.

    Returns:
        The updated teacher detail including the new availability list.

    Raises:
        HTTPException: 404 if no teacher with that ID exists in the operating school.
        HTTPException: 409 if any time_block_id is invalid (FK violation).
    """
    teacher = await _get_teacher(db, teacher_id, scope_school_id)

    await db.execute(
        delete(TeacherAvailability).where(TeacherAvailability.teacher_id == teacher_id)
    )
    for entry in body.entries:
        db.add(
            TeacherAvailability(
                teacher_id=teacher_id,
                time_block_id=entry.time_block_id,
                status=entry.status,
            )
        )
    try:
        await db.commit()
    except IntegrityError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="One or more time block IDs are invalid.",
        ) from exc

    await db.refresh(teacher)
    return await _build_teacher_detail(db, teacher)
