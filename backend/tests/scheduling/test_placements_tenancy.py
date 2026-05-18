"""Cross-school isolation tests for the placements aggregate."""

from uuid import uuid4

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.pin_kind import PinKind
from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.scheduling import quality_checks, solver_io
from tests.scheduling.conftest import SeededMovablePlacement, SeededTwoPlacements

pytestmark = pytest.mark.anyio


@pytest.fixture
async def school_b_for_placements(db_session: AsyncSession) -> School:
    """A second school distinct from DEFAULT_SCHOOL_ID."""
    school = School(name="Schule B Placements", short_name="SBP")
    db_session.add(school)
    await db_session.flush()
    return school


async def test_move_placement_with_cross_school_room_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_for_placements: School,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    """PATCH /placements/{lesson_id}/{tb_id} with a cross-school room returns 404."""
    await create_test_user(
        email="admin-place-cross@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    room_in_b = Room(
        name="Cross-school room",
        short_name="CSX",
        school_id=school_b_for_placements.id,
    )
    db_session.add(room_in_b)
    await db_session.flush()
    await login_as("admin-place-cross@test.com", "testpassword123")
    fixture = seeded_movable_placement

    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}",
        json={
            "time_block_id": str(fixture.target_time_block_id),
            "room_id": str(room_in_b.id),
        },
    )
    assert response.status_code == 404


async def test_move_placement_stamps_school_id_on_replacement_row(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    """PATCH /placements that crosses time blocks must stamp the new row's school_id."""
    await create_test_user(
        email="admin-stamp-move@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    await login_as("admin-stamp-move@test.com", "testpassword123")
    fixture = seeded_movable_placement
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}",
        json={
            "time_block_id": str(fixture.target_time_block_id),
            "room_id": str(fixture.target_room_id),
        },
    )
    assert response.status_code == 200
    new_row = (
        await db_session.execute(
            select(ScheduledLesson).where(
                ScheduledLesson.lesson_id == fixture.lesson_id,
                ScheduledLesson.time_block_id == fixture.target_time_block_id,
            )
        )
    ).scalar_one()
    assert new_row.school_id == DEFAULT_SCHOOL_ID


async def test_swap_placements_stamps_school_id_on_both_new_rows(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    seeded_two_placements_for_swap: SeededTwoPlacements,
) -> None:
    """POST /placements/swap stamps school_id on both resulting rows."""
    await create_test_user(
        email="admin-stamp-swap@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    await login_as("admin-stamp-swap@test.com", "testpassword123")
    fixture = seeded_two_placements_for_swap
    response = await client.post(
        "/api/placements/swap",
        json={
            "a": {
                "lesson_id": str(fixture.lesson_a_id),
                "time_block_id": str(fixture.time_block_a_id),
            },
            "b": {
                "lesson_id": str(fixture.lesson_b_id),
                "time_block_id": str(fixture.time_block_b_id),
            },
        },
    )
    assert response.status_code == 200
    rows = (await db_session.execute(select(ScheduledLesson))).scalars().all()
    assert len(rows) == 2
    for row in rows:
        assert row.school_id == DEFAULT_SCHOOL_ID


async def test_swap_with_cross_school_other_placement_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_for_placements: School,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    """POST /placements/swap with one placement from school B returns 404."""
    await create_test_user(
        email="admin-swap-cross@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    await login_as("admin-swap-cross@test.com", "testpassword123")
    fixture = seeded_movable_placement
    bogus_lesson_id = uuid4()
    bogus_tb_id = uuid4()
    response = await client.post(
        "/api/placements/swap",
        json={
            "a": {
                "lesson_id": str(fixture.lesson_id),
                "time_block_id": str(fixture.source_time_block_id),
            },
            "b": {
                "lesson_id": str(bogus_lesson_id),
                "time_block_id": str(bogus_tb_id),
            },
        },
    )
    assert response.status_code == 404


async def test_move_with_cross_school_time_block_returns_404(
    client: AsyncClient,
    db_session: AsyncSession,
    school_b_for_placements: School,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
    create_week_scheme,
    create_time_block,
) -> None:
    """PATCH /placements with body.time_block_id in school B's WeekScheme returns 404."""
    await create_test_user(
        email="admin-tb-cross@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    foreign_scheme = await create_week_scheme(
        name="Foreign Scheme", school_id=school_b_for_placements.id
    )
    foreign_tb = await create_time_block(
        week_scheme_id=foreign_scheme.id,
        day_of_week=0,
        position=5,
    )
    await login_as("admin-tb-cross@test.com", "testpassword123")
    fixture = seeded_movable_placement
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}",
        json={
            "time_block_id": str(foreign_tb.id),
            "room_id": str(fixture.target_room_id),
        },
    )
    assert response.status_code == 404


async def test_collect_all_pins_filters_by_school_id(
    db_session: AsyncSession,
    seeded_movable_placement: SeededMovablePlacement,
    school_b_for_placements: School,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    """collect_all_pins returns only same-school pins."""
    # Promote the seeded school-A placement to a HARD pin.
    own_row = (
        await db_session.execute(
            select(ScheduledLesson).where(
                ScheduledLesson.lesson_id == seeded_movable_placement.lesson_id,
                ScheduledLesson.time_block_id == seeded_movable_placement.source_time_block_id,
            )
        )
    ).scalar_one()
    own_row.pin_kind = PinKind.HARD
    await db_session.flush()

    # Seed a parallel pinned placement in school B.
    b_id = school_b_for_placements.id
    foreign_subject = await create_subject(school_id=b_id, short_name="FBS")
    foreign_scheme = await create_week_scheme(name="WS-B", school_id=b_id)
    foreign_tb = await create_time_block(
        week_scheme_id=foreign_scheme.id, day_of_week=0, position=1
    )
    foreign_room = await create_room(school_id=b_id, short_name="FBR")
    foreign_teacher = await create_teacher(school_id=b_id, short_code="FBT")
    foreign_tafel = await create_stundentafel(school_id=b_id, name="Tafel-B")
    foreign_class = await create_school_class(
        stundentafel_id=foreign_tafel.id, week_scheme_id=foreign_scheme.id, school_id=b_id
    )
    foreign_lesson = Lesson(
        school_id=b_id,
        subject_id=foreign_subject.id,
        teacher_id=foreign_teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(foreign_lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=foreign_lesson.id, school_class_id=foreign_class.id))
    db_session.add(
        ScheduledLesson(
            lesson_id=foreign_lesson.id,
            time_block_id=foreign_tb.id,
            room_id=foreign_room.id,
            teacher_id=foreign_teacher.id,
            school_id=b_id,
            pin_kind=PinKind.HARD,
        )
    )
    await db_session.flush()

    pins = await solver_io.collect_all_pins(db_session, school_id=DEFAULT_SCHOOL_ID)
    pin_lesson_ids = {p["lesson_id"] for p in pins}
    assert str(seeded_movable_placement.lesson_id) in pin_lesson_ids
    assert str(foreign_lesson.id) not in pin_lesson_ids


async def test_persist_solution_for_class_stamps_school_id(
    db_session: AsyncSession,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    """persist_solution_for_class stamps school_id on every inserted row."""
    fixture = seeded_movable_placement
    existing = (
        await db_session.execute(
            select(ScheduledLesson).where(
                ScheduledLesson.lesson_id == fixture.lesson_id,
                ScheduledLesson.time_block_id == fixture.source_time_block_id,
            )
        )
    ).scalar_one()
    teacher_id = existing.teacher_id
    placement = {
        "lesson_id": str(fixture.lesson_id),
        "time_block_id": str(fixture.target_time_block_id),
        "room_id": str(fixture.target_room_id),
        "teacher_id": str(teacher_id),
    }
    class_id = (
        await db_session.execute(
            select(LessonSchoolClass.school_class_id).where(
                LessonSchoolClass.lesson_id == fixture.lesson_id
            )
        )
    ).scalar_one()
    filtered = {"placements": [placement], "violations": []}
    await solver_io.persist_solution_for_class(
        db_session, class_id, filtered, school_id=DEFAULT_SCHOOL_ID
    )
    await db_session.flush()
    row = (
        await db_session.execute(
            select(ScheduledLesson).where(
                ScheduledLesson.lesson_id == fixture.lesson_id,
                ScheduledLesson.time_block_id == fixture.target_time_block_id,
            )
        )
    ).scalar_one()
    assert row.school_id == DEFAULT_SCHOOL_ID


async def test_super_admin_with_other_school_param_can_pin_in_other_school(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
) -> None:
    """Super-admin scoped via ?school_id=<home> can pin placements in that school.

    Uses the home-school fixture but routes the call through ?school_id=<home>
    to prove the dependency resolves the override path. The DB row's school_id
    after the pin must equal the resolved scope school (i.e. home).
    """
    sa, password = await create_test_user(
        email="sa-place-pin@test.com", role="super_admin", school_id=DEFAULT_SCHOOL_ID
    )
    await login_as(sa.email, password)
    fixture = seeded_movable_placement
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin"
        f"?school_id={DEFAULT_SCHOOL_ID}",
        json={"pin_kind": PinKind.HARD.value},
    )
    assert response.status_code == 200
    row = (
        await db_session.execute(
            select(ScheduledLesson).where(
                ScheduledLesson.lesson_id == fixture.lesson_id,
                ScheduledLesson.time_block_id == fixture.source_time_block_id,
            )
        )
    ).scalar_one()
    assert row.pin_kind == PinKind.HARD
    assert row.school_id == DEFAULT_SCHOOL_ID


async def test_admin_pinning_with_other_school_param_is_ignored(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user,
    login_as,
    seeded_movable_placement: SeededMovablePlacement,
    school_b_for_placements: School,
) -> None:
    """Plain admin's ?school_id query parameter is ignored: pin still lands in home."""
    await create_test_user(
        email="admin-pin-ignore@test.com", role="admin", school_id=DEFAULT_SCHOOL_ID
    )
    await login_as("admin-pin-ignore@test.com", "testpassword123")
    fixture = seeded_movable_placement
    # Admin passes school_id=B; should be ignored, pin lands in home (school A).
    response = await client.patch(
        f"/api/placements/{fixture.lesson_id}/{fixture.source_time_block_id}/pin"
        f"?school_id={school_b_for_placements.id}",
        json={"pin_kind": PinKind.HARD.value},
    )
    assert response.status_code == 200
    row = (
        await db_session.execute(
            select(ScheduledLesson).where(
                ScheduledLesson.lesson_id == fixture.lesson_id,
                ScheduledLesson.time_block_id == fixture.source_time_block_id,
            )
        )
    ).scalar_one()
    assert row.pin_kind == PinKind.HARD
    assert row.school_id == DEFAULT_SCHOOL_ID


async def test_load_placements_filters_by_school_id(
    db_session: AsyncSession,
    seeded_movable_placement: SeededMovablePlacement,
    school_b_for_placements: School,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> None:
    """quality_checks.load_placements returns only same-school placements."""
    b_id = school_b_for_placements.id
    foreign_subject = await create_subject(school_id=b_id, short_name="QBS")
    foreign_scheme = await create_week_scheme(name="WS-Q", school_id=b_id)
    foreign_tb = await create_time_block(
        week_scheme_id=foreign_scheme.id, day_of_week=0, position=1
    )
    foreign_room = await create_room(school_id=b_id, short_name="QBR")
    foreign_teacher = await create_teacher(school_id=b_id, short_code="QBT")
    foreign_tafel = await create_stundentafel(school_id=b_id, name="Tafel-Q")
    foreign_class = await create_school_class(
        stundentafel_id=foreign_tafel.id, week_scheme_id=foreign_scheme.id, school_id=b_id
    )
    foreign_lesson = Lesson(
        school_id=b_id,
        subject_id=foreign_subject.id,
        teacher_id=foreign_teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(foreign_lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=foreign_lesson.id, school_class_id=foreign_class.id))
    db_session.add(
        ScheduledLesson(
            lesson_id=foreign_lesson.id,
            time_block_id=foreign_tb.id,
            room_id=foreign_room.id,
            teacher_id=foreign_teacher.id,
            school_id=b_id,
        )
    )
    await db_session.flush()
    placements = await quality_checks.load_placements(db_session, school_id=DEFAULT_SCHOOL_ID)
    placement_lesson_ids = {p.lesson_id for p in placements}
    assert seeded_movable_placement.lesson_id in placement_lesson_ids
    assert foreign_lesson.id not in placement_lesson_ids
