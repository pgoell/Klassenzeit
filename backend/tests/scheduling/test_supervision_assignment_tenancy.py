"""Cross-school tenancy isolation tests for SupervisionAssignment (item 10a)."""

import uuid
from datetime import time

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.supervision_assignment import SupervisionAssignment
from klassenzeit_backend.scheduling.solver_io import (
    persist_supervision_assignments,
    read_supervision_assignments_for_teacher,
)

pytestmark = pytest.mark.anyio


@pytest.fixture
async def school_b_supervision_assignments(db_session: AsyncSession) -> School:
    """Second school distinct from DEFAULT_SCHOOL_ID, scoped to this test file."""
    school = School(name="Schule B (supervision)", short_name="SBSA")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_insert_with_null_school_id_raises_integrity_error(
    db_session: AsyncSession,
    create_week_scheme,
    create_time_block,
    create_teacher,
) -> None:
    """Omitting school_id must IntegrityError once Task 3 drops the default."""
    scheme = await create_week_scheme()
    block = await create_time_block(
        week_scheme_id=scheme.id,
        day_of_week=0,
        position=3,
        start_time=time(9, 30),
        end_time=time(9, 45),
    )
    teacher = await create_teacher(short_code="SVN")
    with pytest.raises(IntegrityError):
        async with db_session.begin_nested():
            db_session.add(SupervisionAssignment(time_block_id=block.id, teacher_id=teacher.id))
            await db_session.flush()


async def test_insert_with_unknown_school_id_raises_integrity_error(
    db_session: AsyncSession,
    create_week_scheme,
    create_time_block,
    create_teacher,
) -> None:
    """Bogus school_id must FK-violate on flush."""
    scheme = await create_week_scheme()
    block = await create_time_block(
        week_scheme_id=scheme.id,
        day_of_week=0,
        position=3,
        start_time=time(9, 30),
        end_time=time(9, 45),
    )
    teacher = await create_teacher(short_code="SVU")
    with pytest.raises(IntegrityError):
        async with db_session.begin_nested():
            db_session.add(
                SupervisionAssignment(
                    time_block_id=block.id,
                    teacher_id=teacher.id,
                    school_id=uuid.uuid4(),
                )
            )
            await db_session.flush()


async def test_read_filters_by_school_id(
    db_session: AsyncSession,
    school_b_supervision_assignments: School,
    create_week_scheme,
    create_time_block,
    create_teacher,
) -> None:
    """Helper returns only same-school rows."""
    own_scheme = await create_week_scheme(name="WS-own")
    own_block = await create_time_block(
        week_scheme_id=own_scheme.id,
        day_of_week=0,
        position=3,
        start_time=time(9, 30),
        end_time=time(9, 45),
    )
    own_teacher = await create_teacher(short_code="OWN")
    db_session.add(
        SupervisionAssignment(
            time_block_id=own_block.id,
            teacher_id=own_teacher.id,
            school_id=DEFAULT_SCHOOL_ID,
        )
    )

    foreign_scheme = await create_week_scheme(
        name="WS-foreign", school_id=school_b_supervision_assignments.id
    )
    foreign_block = await create_time_block(
        week_scheme_id=foreign_scheme.id,
        day_of_week=0,
        position=3,
        start_time=time(9, 30),
        end_time=time(9, 45),
    )
    foreign_teacher = await create_teacher(
        short_code="FRG", school_id=school_b_supervision_assignments.id
    )
    db_session.add(
        SupervisionAssignment(
            time_block_id=foreign_block.id,
            teacher_id=foreign_teacher.id,
            school_id=school_b_supervision_assignments.id,
        )
    )
    await db_session.flush()

    own_view = await read_supervision_assignments_for_teacher(
        db_session, own_teacher.id, school_id=DEFAULT_SCHOOL_ID
    )
    assert len(own_view) == 1
    assert own_view[0].time_block_id == own_block.id

    cross_view = await read_supervision_assignments_for_teacher(
        db_session, foreign_teacher.id, school_id=DEFAULT_SCHOOL_ID
    )
    assert cross_view == []


async def test_persist_stamps_school_id_on_new_rows(
    db_session: AsyncSession,
    create_week_scheme,
    create_time_block,
    create_teacher,
) -> None:
    """persist_supervision_assignments stamps school_id on every inserted row."""
    scheme = await create_week_scheme()
    block = await create_time_block(
        week_scheme_id=scheme.id,
        day_of_week=0,
        position=3,
        start_time=time(9, 30),
        end_time=time(9, 45),
    )
    teacher = await create_teacher(short_code="STP")
    solution = {
        "supervision_assignments": [{"time_block_id": str(block.id), "teacher_id": str(teacher.id)}]
    }

    await persist_supervision_assignments(
        db_session, scheme.id, solution, school_id=DEFAULT_SCHOOL_ID
    )
    await db_session.flush()

    rows = (await db_session.execute(select(SupervisionAssignment))).scalars().all()
    assert len(rows) == 1
    assert rows[0].school_id == DEFAULT_SCHOOL_ID
    assert rows[0].time_block_id == block.id


async def test_get_teacher_schedule_does_not_leak_cross_school_sas(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_supervision_assignments: School,
    create_test_user,
    login_as,
    create_week_scheme,
    create_time_block,
    create_teacher,
) -> None:
    """GET /api/teachers/{id}/schedule never returns cross-school SAs."""
    admin, password = await create_test_user(
        email="admin-sa-schedule@test.com",
        role="admin",
        school_id=DEFAULT_SCHOOL_ID,
    )
    own_scheme = await create_week_scheme(name="WS-own-route")
    own_block = await create_time_block(
        week_scheme_id=own_scheme.id,
        day_of_week=0,
        position=3,
        start_time=time(9, 30),
        end_time=time(9, 45),
    )
    own_teacher = await create_teacher(short_code="OWR")
    db_session.add(
        SupervisionAssignment(
            time_block_id=own_block.id,
            teacher_id=own_teacher.id,
            school_id=DEFAULT_SCHOOL_ID,
        )
    )

    foreign_scheme = await create_week_scheme(
        name="WS-foreign-route", school_id=school_b_supervision_assignments.id
    )
    foreign_block = await create_time_block(
        week_scheme_id=foreign_scheme.id,
        day_of_week=0,
        position=3,
        start_time=time(9, 30),
        end_time=time(9, 45),
    )
    db_session.add(
        SupervisionAssignment(
            time_block_id=foreign_block.id,
            teacher_id=own_teacher.id,  # same teacher_id, different school
            school_id=school_b_supervision_assignments.id,
        )
    )
    await db_session.commit()

    await login_as(admin.email, password)
    response = await client.get(f"/api/teachers/{own_teacher.id}/schedule")
    assert response.status_code == 200
    payload = response.json()
    returned_time_block_ids = {sa["time_block_id"] for sa in payload["supervision_assignments"]}
    assert str(own_block.id) in returned_time_block_ids
    assert str(foreign_block.id) not in returned_time_block_ids
