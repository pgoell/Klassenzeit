"""CRUD routes for the Lesson entity, plus the generate-lessons endpoint."""

import logging
import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import delete, func, select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin
from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import StundentafelEntry
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling.schemas.lesson import (
    LessonClassResponse,
    LessonCreate,
    LessonResponse,
    LessonSubjectResponse,
    LessonTeacherResponse,
    LessonUpdate,
)

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/lessons", tags=["lessons"])
generate_router = APIRouter(tags=["lessons"])


async def _get_lesson(db: AsyncSession, lesson_id: uuid.UUID) -> Lesson:
    """Load a Lesson by primary key or raise 404.

    Args:
        db: Active async database session.
        lesson_id: UUID of the lesson to load.

    Returns:
        The matching Lesson ORM instance.

    Raises:
        HTTPException: 404 if no lesson with that ID exists.
    """
    lesson = await db.get(Lesson, lesson_id)
    if lesson is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND)
    return lesson


async def _build_lesson_response(db: AsyncSession, lesson: Lesson) -> LessonResponse:
    """Construct a LessonResponse with eager-loaded class memberships.

    Args:
        db: Active async database session.
        lesson: The Lesson ORM instance to build a response for.

    Returns:
        A fully populated LessonResponse including nested entities. School
        classes are sorted by name for stable response ordering.
    """
    membership_rows = (
        (
            await db.execute(
                select(LessonSchoolClass).where(LessonSchoolClass.lesson_id == lesson.id)
            )
        )
        .scalars()
        .all()
    )
    class_ids = [row.school_class_id for row in membership_rows]
    classes: list[SchoolClass] = []
    if class_ids:
        classes = list(
            (
                await db.execute(
                    select(SchoolClass)
                    .where(SchoolClass.id.in_(class_ids))
                    .order_by(SchoolClass.name)
                )
            )
            .scalars()
            .all()
        )

    subj_result = await db.execute(select(Subject).where(Subject.id == lesson.subject_id))
    subject = subj_result.scalar_one()

    teacher_resp = None
    if lesson.teacher_id:
        teacher_result = await db.execute(select(Teacher).where(Teacher.id == lesson.teacher_id))
        teacher = teacher_result.scalar_one()
        teacher_resp = LessonTeacherResponse(
            id=teacher.id,
            first_name=teacher.first_name,
            last_name=teacher.last_name,
            short_code=teacher.short_code,
        )

    return LessonResponse(
        id=lesson.id,
        school_classes=[LessonClassResponse(id=c.id, name=c.name) for c in classes],
        subject=LessonSubjectResponse(
            id=subject.id,
            name=subject.name,
            short_name=subject.short_name,
        ),
        teacher=teacher_resp,
        hours_per_week=lesson.hours_per_week,
        preferred_block_size=lesson.preferred_block_size,
        pre_buffer_minutes=lesson.pre_buffer_minutes,
        post_buffer_minutes=lesson.post_buffer_minutes,
        lesson_group_id=lesson.lesson_group_id,
        created_at=lesson.created_at,
        updated_at=lesson.updated_at,
    )


async def _check_subject_class_collision(
    db: AsyncSession,
    subject_id: uuid.UUID,
    school_class_ids: list[uuid.UUID],
    *,
    excluding_lesson_id: uuid.UUID | None = None,
) -> None:
    """Raise 409 if any existing Lesson teaches the same subject for any of the given classes.

    Replaces the dropped ``(school_class_id, subject_id)`` UNIQUE constraint.
    A class can host at most one lesson per subject; a lesson with multiple
    memberships still cannot collide with a single-class lesson on the same
    ``(class, subject)`` pair.
    """
    stmt = (
        select(Lesson.id)
        .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
        .where(
            Lesson.subject_id == subject_id,
            LessonSchoolClass.school_class_id.in_(school_class_ids),
        )
    )
    if excluding_lesson_id is not None:
        stmt = stmt.where(Lesson.id != excluding_lesson_id)
    if (await db.execute(stmt)).first() is not None:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="A lesson for one of these classes and subject already exists.",
        )


@router.post("", status_code=status.HTTP_201_CREATED)
async def create_lesson(
    body: LessonCreate,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> LessonResponse:
    """Create a new lesson with one or more class memberships.

    Args:
        body: Fields for the new lesson.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The created lesson as a LessonResponse.

    Raises:
        HTTPException: 409 if a lesson for any (class, subject) pair in the
            request already exists.
    """
    await _check_subject_class_collision(db, body.subject_id, body.school_class_ids)
    lesson = Lesson(
        subject_id=body.subject_id,
        teacher_id=body.teacher_id,
        hours_per_week=body.hours_per_week,
        preferred_block_size=body.preferred_block_size,
        pre_buffer_minutes=body.pre_buffer_minutes,
        post_buffer_minutes=body.post_buffer_minutes,
        lesson_group_id=body.lesson_group_id,
    )
    db.add(lesson)
    await db.flush()
    db.add_all(
        [
            LessonSchoolClass(lesson_id=lesson.id, school_class_id=class_id)
            for class_id in body.school_class_ids
        ]
    )
    await db.commit()
    await db.refresh(lesson)
    return await _build_lesson_response(db, lesson)


@router.get("")
async def list_lessons(
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
    class_id: uuid.UUID | None = None,
    teacher_id: uuid.UUID | None = None,
    subject_id: uuid.UUID | None = None,
) -> list[LessonResponse]:
    """Return all lessons, with optional filters by class, teacher or subject.

    Args:
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.
        class_id: Optional filter; only lessons that include this school
            class in their memberships.
        teacher_id: Optional filter; only lessons assigned to this teacher.
        subject_id: Optional filter; only lessons for this subject.

    Returns:
        List of lessons matching the applied filters.
    """
    stmt = select(Lesson)
    if class_id is not None:
        stmt = stmt.join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id).where(
            LessonSchoolClass.school_class_id == class_id
        )
    if teacher_id is not None:
        stmt = stmt.where(Lesson.teacher_id == teacher_id)
    if subject_id is not None:
        stmt = stmt.where(Lesson.subject_id == subject_id)
    result = await db.execute(stmt)
    lessons = result.scalars().all()
    return [await _build_lesson_response(db, lesson) for lesson in lessons]


@router.get("/{lesson_id}")
async def get_lesson(
    lesson_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> LessonResponse:
    """Fetch a single lesson by ID with joined class, subject and teacher data.

    Args:
        lesson_id: UUID path parameter identifying the lesson.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The matching lesson as a LessonResponse.

    Raises:
        HTTPException: 404 if no lesson with that ID exists.
    """
    lesson = await _get_lesson(db, lesson_id)
    return await _build_lesson_response(db, lesson)


@router.patch("/{lesson_id}")
async def update_lesson(
    lesson_id: uuid.UUID,
    body: LessonUpdate,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> LessonResponse:
    """Partially update a lesson's memberships, teacher, hours or block size.

    Args:
        lesson_id: UUID path parameter identifying the lesson to patch.
        body: Fields to update; omitted fields remain unchanged.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Returns:
        The updated lesson as a LessonResponse.

    Raises:
        HTTPException: 404 if no lesson with that ID exists; 409 if the new
            membership set collides with another lesson on the same subject.
    """
    lesson = await _get_lesson(db, lesson_id)
    if body.school_class_ids is not None:
        await _check_subject_class_collision(
            db,
            lesson.subject_id,
            body.school_class_ids,
            excluding_lesson_id=lesson.id,
        )
        await db.execute(delete(LessonSchoolClass).where(LessonSchoolClass.lesson_id == lesson.id))
        db.add_all(
            [
                LessonSchoolClass(lesson_id=lesson.id, school_class_id=class_id)
                for class_id in body.school_class_ids
            ]
        )
    if body.teacher_id is not None:
        lesson.teacher_id = body.teacher_id
    if body.hours_per_week is not None:
        lesson.hours_per_week = body.hours_per_week
    if body.preferred_block_size is not None:
        lesson.preferred_block_size = body.preferred_block_size
    if body.pre_buffer_minutes is not None:
        lesson.pre_buffer_minutes = body.pre_buffer_minutes
    if body.post_buffer_minutes is not None:
        lesson.post_buffer_minutes = body.post_buffer_minutes
    if body.lesson_group_id is not None:
        lesson.lesson_group_id = body.lesson_group_id
    if lesson.hours_per_week % lesson.preferred_block_size != 0:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail="hours_per_week must be divisible by preferred_block_size",
        )
    await db.commit()
    await db.refresh(lesson)
    return await _build_lesson_response(db, lesson)


@router.delete("/{lesson_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_lesson(
    lesson_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> None:
    """Delete a lesson by ID.

    Args:
        lesson_id: UUID path parameter identifying the lesson to delete.
        _admin: Injected admin user (enforces authentication).
        db: Injected async database session.

    Raises:
        HTTPException: 404 if no lesson with that ID exists.
    """
    lesson = await _get_lesson(db, lesson_id)
    await db.delete(lesson)
    await db.commit()


async def _validate_qualified_teacher_coverage(
    db: AsyncSession,
    subject_ids: list[uuid.UUID],
    *,
    school_id: uuid.UUID,
) -> None:
    """Raise 422 if any subject in the list has zero qualified teachers in the school.

    Aggregates every offender into a single error so the admin can fix all
    data gaps in one batch. The 422 ``detail`` is a structured dict with a
    stable ``code`` plus the offending ``subject_ids`` and ``subject_short_names``
    for frontend display.

    The double ``outerjoin`` chain (Subject -> TeacherQualification -> Teacher)
    enforces both the FK link AND the school filter at JOIN time. Without the
    second outerjoin, an unconditional ``WHERE Teacher.school_id = ...`` would
    drop subjects that have no qualifications at all.

    Args:
        db: Active async database session.
        subject_ids: Curriculum subject UUIDs (typically one per StundentafelEntry
            for the class).
        school_id: Tenant school whose teachers are eligible to cover the subject.

    Raises:
        HTTPException: 422 with detail
            ``{"code": "missing_qualified_teacher", "subject_ids": [...],
            "subject_short_names": [...]}`` if one or more subjects have no
            qualified teacher in the school (active or not).
    """
    if not subject_ids:
        return
    result = await db.execute(
        select(Subject.id, Subject.short_name)
        .outerjoin(
            TeacherQualification,
            TeacherQualification.subject_id == Subject.id,
        )
        .outerjoin(
            Teacher,
            (Teacher.id == TeacherQualification.teacher_id) & (Teacher.school_id == school_id),
        )
        .where(Subject.id.in_(subject_ids))
        .group_by(Subject.id, Subject.short_name)
        .having(func.count(Teacher.id) == 0)
    )
    rows = result.all()
    if not rows:
        return
    raise HTTPException(
        status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
        detail={
            "code": "missing_qualified_teacher",
            "subject_ids": [str(row[0]) for row in rows],
            "subject_short_names": [row[1] for row in rows],
        },
    )


@generate_router.post("/classes/{class_id}/generate-lessons", status_code=status.HTTP_201_CREATED)
async def generate_lessons_from_stundentafel(
    class_id: uuid.UUID,
    current_user: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> list[LessonResponse]:
    """Bulk-create lessons for a class from its associated Stundentafel.

    Only creates lessons for subjects not already assigned to the class.
    Subjects that already have a lesson (single- or multi-class) which
    includes this class as a member are silently skipped.

    Args:
        class_id: UUID path parameter identifying the school class.
        current_user: Injected admin user; scopes the class lookup and
            qualified-teacher coverage check to their school.
        db: Injected async database session.

    Returns:
        List of newly created LessonResponse objects (may be empty if all exist).

    Raises:
        HTTPException: 404 if no school class with that ID exists in the user's school.
    """
    result = await db.execute(select(SchoolClass).where(SchoolClass.id == class_id))
    school_class = result.scalar_one_or_none()
    if school_class is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Class not found")

    entries_result = await db.execute(
        select(StundentafelEntry)
        .where(StundentafelEntry.stundentafel_id == school_class.stundentafel_id)
        .order_by(StundentafelEntry.subject_id)
    )
    entries = entries_result.scalars().all()

    curriculum_subject_ids = [entry.subject_id for entry in entries]
    await _validate_qualified_teacher_coverage(
        db, curriculum_subject_ids, school_id=current_user.school_id
    )

    existing_result = await db.execute(
        select(Lesson.subject_id)
        .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
        .where(LessonSchoolClass.school_class_id == class_id)
    )
    existing_subject_ids = {row[0] for row in existing_result.all()}

    created: list[Lesson] = []
    for entry in entries:
        if entry.subject_id in existing_subject_ids:
            continue
        lesson = Lesson(
            subject_id=entry.subject_id,
            teacher_id=None,
            hours_per_week=entry.hours_per_week,
            preferred_block_size=entry.preferred_block_size,
        )
        db.add(lesson)
        created.append(lesson)

    await db.flush()

    db.add_all(
        [LessonSchoolClass(lesson_id=lesson.id, school_class_id=class_id) for lesson in created]
    )
    await db.flush()

    await db.commit()

    logger.info(
        "generate_lessons.done",
        extra={
            "school_class_id": str(class_id),
            "lessons_created": len(created),
        },
    )

    responses = []
    for lesson in created:
        await db.refresh(lesson)
        responses.append(await _build_lesson_response(db, lesson))
    return responses
