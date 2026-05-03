"""Sprint C placement-mutation endpoints addressed by composite key."""

import uuid
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.auth.dependencies import require_admin
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.models.week_scheme import TimeBlock
from klassenzeit_backend.db.session import get_session
from klassenzeit_backend.scheduling.schemas.placement import (
    MovePlacementRequest,
    PinPlacementRequest,
    PlacementResponse,
    SwapPlacementsRequest,
    SwapPlacementsResponse,
)

router = APIRouter(prefix="/placements", tags=["placements"])


async def _load_placement_or_404(
    db: AsyncSession,
    lesson_id: uuid.UUID,
    time_block_id: uuid.UUID,
) -> ScheduledLesson:
    """Fetch a ScheduledLesson by composite key or raise 404."""
    stmt = select(ScheduledLesson).where(
        ScheduledLesson.lesson_id == lesson_id,
        ScheduledLesson.time_block_id == time_block_id,
    )
    row = (await db.execute(stmt)).scalar_one_or_none()
    if row is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"placement {lesson_id}/{time_block_id} not found",
        )
    return row


async def _load_time_block_or_404(db: AsyncSession, time_block_id: uuid.UUID) -> TimeBlock:
    """Fetch a TimeBlock by id or raise 404."""
    tb = (
        await db.execute(select(TimeBlock).where(TimeBlock.id == time_block_id))
    ).scalar_one_or_none()
    if tb is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"time block {time_block_id} not found",
        )
    return tb


async def _load_room_or_404(db: AsyncSession, room_id: uuid.UUID) -> Room:
    """Fetch a Room by id or raise 404."""
    room = (await db.execute(select(Room).where(Room.id == room_id))).scalar_one_or_none()
    if room is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"room {room_id} not found",
        )
    return room


async def _assert_lesson_week_scheme_matches(
    db: AsyncSession,
    lesson_id: uuid.UUID,
    target_time_block: TimeBlock,
) -> None:
    """Reject moves that cross week schemes.

    A lesson is reachable from a class via the LessonSchoolClass association;
    the class carries the week_scheme_id. The target time_block must live in
    that same week_scheme.
    """
    rows = (
        (
            await db.execute(
                select(SchoolClass.week_scheme_id)
                .join(LessonSchoolClass, LessonSchoolClass.school_class_id == SchoolClass.id)
                .where(LessonSchoolClass.lesson_id == lesson_id)
            )
        )
        .scalars()
        .all()
    )
    if not rows:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail=f"lesson {lesson_id} has no class membership",
        )
    if any(row != target_time_block.week_scheme_id for row in rows):
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_CONTENT,
            detail=(
                f"target time block {target_time_block.id} belongs to a "
                "different week scheme than the lesson's class"
            ),
        )


@router.patch("/{lesson_id}/{time_block_id}")
async def move_placement_route(
    lesson_id: uuid.UUID,
    time_block_id: uuid.UUID,
    body: MovePlacementRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> PlacementResponse:
    """Move a placement to a new time block (and possibly room) and pin it."""
    placement = await _load_placement_or_404(db, lesson_id, time_block_id)
    target_tb = await _load_time_block_or_404(db, body.time_block_id)
    await _load_room_or_404(db, body.room_id)
    await _assert_lesson_week_scheme_matches(db, lesson_id, target_tb)
    if body.time_block_id != time_block_id:
        # Drop the old composite-PK row before inserting the new one to avoid
        # a transient duplicate during the same transaction.
        await db.delete(placement)
        await db.flush()
        placement = ScheduledLesson(
            lesson_id=lesson_id,
            time_block_id=body.time_block_id,
            room_id=body.room_id,
            pinned=True,
        )
        db.add(placement)
    else:
        placement.room_id = body.room_id
        placement.pinned = True
    await db.commit()
    await db.refresh(placement)
    return PlacementResponse.model_validate(placement)


@router.patch("/{lesson_id}/{time_block_id}/pin")
async def pin_placement_route(
    lesson_id: uuid.UUID,
    time_block_id: uuid.UUID,
    body: PinPlacementRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> PlacementResponse:
    """Toggle the ``pinned`` flag on an existing placement."""
    placement = await _load_placement_or_404(db, lesson_id, time_block_id)
    placement.pinned = body.pinned
    await db.commit()
    await db.refresh(placement)
    return PlacementResponse.model_validate(placement)


@router.post("/swap")
async def swap_placements_route(
    body: SwapPlacementsRequest,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> SwapPlacementsResponse:
    """Swap two placements' time blocks (and rooms) and pin both."""
    placement_a = await _load_placement_or_404(db, body.a.lesson_id, body.a.time_block_id)
    placement_b = await _load_placement_or_404(db, body.b.lesson_id, body.b.time_block_id)
    a_target_tb = await _load_time_block_or_404(db, body.b.time_block_id)
    b_target_tb = await _load_time_block_or_404(db, body.a.time_block_id)
    await _assert_lesson_week_scheme_matches(db, body.a.lesson_id, a_target_tb)
    await _assert_lesson_week_scheme_matches(db, body.b.lesson_id, b_target_tb)
    a_room = placement_a.room_id
    b_room = placement_b.room_id
    await db.delete(placement_a)
    await db.delete(placement_b)
    await db.flush()
    new_a = ScheduledLesson(
        lesson_id=body.a.lesson_id,
        time_block_id=body.b.time_block_id,
        room_id=b_room,
        pinned=True,
    )
    new_b = ScheduledLesson(
        lesson_id=body.b.lesson_id,
        time_block_id=body.a.time_block_id,
        room_id=a_room,
        pinned=True,
    )
    db.add(new_a)
    db.add(new_b)
    await db.commit()
    await db.refresh(new_a)
    await db.refresh(new_b)
    return SwapPlacementsResponse(
        a=PlacementResponse.model_validate(new_a),
        b=PlacementResponse.model_validate(new_b),
    )
