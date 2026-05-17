"""Shared factory fixtures for scheduling tests.

Each factory follows the ``_counter`` pattern for unique default names
and flushes (but never commits) within the per-test transaction so that
foreign-key constraints resolve before the next statement.

The ``create_test_user`` and ``login_as`` fixtures are provided by the root
``backend/tests/conftest.py`` and are available here automatically.
"""

import uuid
from collections.abc import Awaitable, Callable
from datetime import time
from itertools import count
from typing import NamedTuple

import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.pin_kind import PinKind
from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import Stundentafel, StundentafelEntry
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.db.models.week_scheme import TimeBlock, WeekScheme


class SeededClassWithPlacements(NamedTuple):
    """Bundle returned by ``seeded_class_with_two_placements``."""

    class_id: uuid.UUID
    pinned_lesson_id_str: str
    unpinned_lesson_id_str: str


class SeededMovablePlacement(NamedTuple):
    """Bundle returned by ``seeded_movable_placement``."""

    lesson_id: uuid.UUID
    source_time_block_id: uuid.UUID
    target_time_block_id: uuid.UUID
    target_room_id: uuid.UUID


class SeededCrossWeekFixture(NamedTuple):
    """Bundle returned by ``seeded_movable_placement_cross_week``."""

    lesson_id: uuid.UUID
    source_time_block_id: uuid.UUID
    foreign_time_block_id: uuid.UUID
    target_room_id: uuid.UUID


class SeededTwoPlacements(NamedTuple):
    """Bundle returned by ``seeded_two_placements_for_swap``."""

    lesson_a_id: uuid.UUID
    time_block_a_id: uuid.UUID
    lesson_b_id: uuid.UUID
    time_block_b_id: uuid.UUID
    room_id: uuid.UUID


class SeededDreizuegigWithPin(NamedTuple):
    """Bundle returned by ``seeded_dreizuegig_with_one_pin``.

    Tiny two-class school with one ScheduledLesson row pre-pinned. Cheaper
    than running the real dreizuegige seed: just enough for the solver to
    produce one feasible placement per class.
    """

    pinned_lesson_id: uuid.UUID
    pinned_time_block_id: uuid.UUID


# Type aliases for the factory callables
type CreateSubjectFn = Callable[..., Awaitable[Subject]]
type CreateWeekSchemeFn = Callable[..., Awaitable[WeekScheme]]
type CreateTimeBlockFn = Callable[..., Awaitable[TimeBlock]]
type CreateRoomFn = Callable[..., Awaitable[Room]]
type CreateTeacherFn = Callable[..., Awaitable[Teacher]]
type CreateStundentafelFn = Callable[..., Awaitable[Stundentafel]]
type CreateStundentafelEntryFn = Callable[..., Awaitable[StundentafelEntry]]
type CreateSchoolClassFn = Callable[..., Awaitable[SchoolClass]]

_subject_counter = count(1)
_week_scheme_counter = count(1)
_room_counter = count(1)
_teacher_counter = count(1)
_stundentafel_counter = count(1)
_school_class_counter = count(1)


@pytest.fixture
def create_subject(db_session: AsyncSession) -> CreateSubjectFn:
    """Factory fixture: ``await create_subject(name=..., short_name=...)``.

    Args:
        db_session: The per-test async DB session (injected by pytest).

    Returns:
        An async callable that inserts a Subject row and flushes.
    """

    async def _make_subject(
        *,
        name: str | None = None,
        short_name: str | None = None,
        color: str = "chart-1",
        school_id: uuid.UUID = DEFAULT_SCHOOL_ID,
    ) -> Subject:
        """Create and flush a Subject with auto-generated unique defaults.

        Args:
            name: Subject name; auto-generated if omitted.
            short_name: Short abbreviation; auto-generated if omitted.
            color: Palette token or hex color; defaults to ``"chart-1"``.
            school_id: Tenant school FK; defaults to the canonical default school.

        Returns:
            The newly created Subject ORM instance.
        """
        n = next(_subject_counter)
        subject = Subject(
            name=name if name is not None else f"Subject {n}",
            short_name=short_name if short_name is not None else f"S{n}",
            color=color,
            school_id=school_id,
        )
        db_session.add(subject)
        await db_session.flush()
        return subject

    return _make_subject


@pytest.fixture
def create_week_scheme(db_session: AsyncSession) -> CreateWeekSchemeFn:
    """Factory fixture: ``await create_week_scheme(name=..., description=...)``.

    Args:
        db_session: The per-test async DB session (injected by pytest).

    Returns:
        An async callable that inserts a WeekScheme row and flushes.
    """

    async def _make_week_scheme(
        *,
        name: str | None = None,
        description: str | None = None,
        school_id: uuid.UUID = DEFAULT_SCHOOL_ID,
    ) -> WeekScheme:
        """Create and flush a WeekScheme with auto-generated unique defaults.

        Args:
            name: Scheme name; auto-generated if omitted.
            description: Optional free-text description.
            school_id: Tenant school FK; defaults to the canonical default school.

        Returns:
            The newly created WeekScheme ORM instance.
        """
        n = next(_week_scheme_counter)
        scheme = WeekScheme(
            name=name if name is not None else f"Week Scheme {n}",
            description=description,
            school_id=school_id,
        )
        db_session.add(scheme)
        await db_session.flush()
        return scheme

    return _make_week_scheme


@pytest.fixture
def create_time_block(db_session: AsyncSession) -> CreateTimeBlockFn:
    """Factory fixture: ``await create_time_block(week_scheme_id=..., ...)``.

    Args:
        db_session: The per-test async DB session (injected by pytest).

    Returns:
        An async callable that inserts a TimeBlock row and flushes.
    """

    async def _make_time_block(
        *,
        week_scheme_id: uuid.UUID,
        day_of_week: int = 0,
        position: int = 1,
        start_time: time = time(8, 0),
        end_time: time = time(8, 45),
    ) -> TimeBlock:
        """Create and flush a TimeBlock within a given WeekScheme.

        Args:
            week_scheme_id: FK to an existing WeekScheme.
            day_of_week: 0=Monday … 6=Sunday.
            position: Ordinal period position within the day.
            start_time: When the block starts.
            end_time: When the block ends.

        Returns:
            The newly created TimeBlock ORM instance.
        """
        block = TimeBlock(
            week_scheme_id=week_scheme_id,
            day_of_week=day_of_week,
            position=position,
            start_time=start_time,
            end_time=end_time,
        )
        db_session.add(block)
        await db_session.flush()
        return block

    return _make_time_block


@pytest.fixture
def create_room(db_session: AsyncSession) -> CreateRoomFn:
    """Factory fixture: ``await create_room(name=..., short_name=..., ...)``.

    Args:
        db_session: The per-test async DB session (injected by pytest).

    Returns:
        An async callable that inserts a Room row and flushes.
    """

    async def _make_room(
        *,
        name: str | None = None,
        short_name: str | None = None,
        capacity: int | None = None,
        school_id: uuid.UUID = DEFAULT_SCHOOL_ID,
    ) -> Room:
        """Create and flush a Room with auto-generated unique defaults.

        Args:
            name: Room name; auto-generated if omitted.
            short_name: Short label; auto-generated if omitted.
            capacity: Optional seating capacity.
            school_id: Tenant school FK; defaults to the canonical default school.

        Returns:
            The newly created Room ORM instance.
        """
        n = next(_room_counter)
        room = Room(
            name=name if name is not None else f"Room {n}",
            short_name=short_name if short_name is not None else f"R{n}",
            capacity=capacity,
            school_id=school_id,
        )
        db_session.add(room)
        await db_session.flush()
        return room

    return _make_room


@pytest.fixture
def create_teacher(db_session: AsyncSession) -> CreateTeacherFn:
    """Factory fixture: ``await create_teacher(first_name=..., last_name=..., ...)``.

    Args:
        db_session: The per-test async DB session (injected by pytest).

    Returns:
        An async callable that inserts a Teacher row and flushes.
    """

    async def _make_teacher(
        *,
        first_name: str = "Test",
        last_name: str | None = None,
        short_code: str | None = None,
        max_hours_per_week: int = 24,
        reserve_hours_per_week: int = 0,
        working_days: list[int] | None = None,
        school_id: uuid.UUID = DEFAULT_SCHOOL_ID,
    ) -> Teacher:
        """Create and flush a Teacher with auto-generated unique defaults.

        Args:
            first_name: Given name of the teacher.
            last_name: Family name; auto-generated if omitted.
            short_code: Unique abbreviation; auto-generated if omitted.
            max_hours_per_week: Maximum teaching hours per week.
            reserve_hours_per_week: Vertretungsreserve subtracted from
                ``max_hours_per_week`` by the solver's effective-capacity rule.
            working_days: Optional Teilzeit weekday restriction; ``None`` means
                full-time (Mo-Fr).
            school_id: Tenant school FK; defaults to the canonical default school.

        Returns:
            The newly created Teacher ORM instance.
        """
        n = next(_teacher_counter)
        teacher = Teacher(
            first_name=first_name,
            last_name=last_name if last_name is not None else f"Teacher{n}",
            short_code=short_code if short_code is not None else f"TC{n}",
            max_hours_per_week=max_hours_per_week,
            reserve_hours_per_week=reserve_hours_per_week,
            working_days=working_days,
            school_id=school_id,
        )
        db_session.add(teacher)
        await db_session.flush()
        return teacher

    return _make_teacher


@pytest.fixture
def create_stundentafel(db_session: AsyncSession) -> CreateStundentafelFn:
    """Factory fixture: ``await create_stundentafel(name=..., grade_level=...)``.

    Args:
        db_session: The per-test async DB session (injected by pytest).

    Returns:
        An async callable that inserts a Stundentafel row and flushes.
    """

    async def _make_stundentafel(
        *,
        name: str | None = None,
        grade_level: int = 5,
        school_id: uuid.UUID = DEFAULT_SCHOOL_ID,
    ) -> Stundentafel:
        """Create and flush a Stundentafel with auto-generated unique defaults.

        Args:
            name: Curriculum template name; auto-generated if omitted.
            grade_level: School year level (e.g. 5 for year 5).
            school_id: Tenant school FK; defaults to the canonical default school.

        Returns:
            The newly created Stundentafel ORM instance.
        """
        n = next(_stundentafel_counter)
        tafel = Stundentafel(
            name=name if name is not None else f"Stundentafel {n}",
            grade_level=grade_level,
            school_id=school_id,
        )
        db_session.add(tafel)
        await db_session.flush()
        return tafel

    return _make_stundentafel


@pytest.fixture
def create_stundentafel_entry(db_session: AsyncSession) -> CreateStundentafelEntryFn:
    """Factory fixture: ``await create_stundentafel_entry(stundentafel_id=..., subject_id=...)``.

    Args:
        db_session: The per-test async DB session (injected by pytest).

    Returns:
        An async callable that inserts a StundentafelEntry row and flushes.
    """

    async def _make_stundentafel_entry(
        *,
        stundentafel_id: uuid.UUID,
        subject_id: uuid.UUID,
        hours_per_week: int = 4,
        preferred_block_size: int = 1,
    ) -> StundentafelEntry:
        """Create and flush a StundentafelEntry linking a subject to a curriculum.

        Args:
            stundentafel_id: FK to an existing Stundentafel.
            subject_id: FK to an existing Subject.
            hours_per_week: How many periods per week this subject occupies.
            preferred_block_size: Preferred consecutive-period block length.

        Returns:
            The newly created StundentafelEntry ORM instance.
        """
        entry = StundentafelEntry(
            stundentafel_id=stundentafel_id,
            subject_id=subject_id,
            hours_per_week=hours_per_week,
            preferred_block_size=preferred_block_size,
        )
        db_session.add(entry)
        await db_session.flush()
        return entry

    return _make_stundentafel_entry


@pytest.fixture
def create_school_class(db_session: AsyncSession) -> CreateSchoolClassFn:
    """Factory fixture for creating a SchoolClass in the DB.

    Example: ``await create_school_class(stundentafel_id=..., week_scheme_id=...)``.

    Args:
        db_session: The per-test async DB session (injected by pytest).

    Returns:
        An async callable that inserts a SchoolClass row and flushes.
    """

    async def _make_school_class(
        *,
        name: str | None = None,
        grade_level: int = 5,
        stundentafel_id: uuid.UUID,
        week_scheme_id: uuid.UUID,
        home_room_id: uuid.UUID | None = None,
        school_id: uuid.UUID = DEFAULT_SCHOOL_ID,
    ) -> SchoolClass:
        """Create and flush a SchoolClass with auto-generated unique defaults.

        Args:
            name: Class identifier such as ``"5a"``; auto-generated if omitted.
            grade_level: School year level.
            stundentafel_id: FK to an existing Stundentafel.
            week_scheme_id: FK to an existing WeekScheme.
            home_room_id: Optional FK to a preferred home Room.
            school_id: Tenant school; defaults to ``DEFAULT_SCHOOL_ID``.

        Returns:
            The newly created SchoolClass ORM instance.
        """
        n = next(_school_class_counter)
        school_class = SchoolClass(
            name=name if name is not None else f"Class{n}",
            grade_level=grade_level,
            stundentafel_id=stundentafel_id,
            week_scheme_id=week_scheme_id,
            home_room_id=home_room_id,
            school_id=school_id,
        )
        db_session.add(school_class)
        await db_session.flush()
        return school_class

    return _make_school_class


@pytest.fixture
async def seeded_lesson_for_pinning(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_school_class: CreateSchoolClassFn,
) -> tuple[uuid.UUID, uuid.UUID, uuid.UUID, uuid.UUID]:
    """Seed one Lesson + one TimeBlock + one Room for ScheduledLesson.pinned tests.

    Returns ``(lesson_id, time_block_id, room_id, teacher_id)``.
    """
    subject = await create_subject()
    week_scheme = await create_week_scheme()
    time_block = await create_time_block(week_scheme_id=week_scheme.id)
    room = await create_room()
    teacher = await create_teacher()
    lesson = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    return lesson.id, time_block.id, room.id, teacher.id


@pytest.fixture
async def seeded_class_with_two_placements(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> SeededClassWithPlacements:
    """Seed one class with two ScheduledLesson rows; one pinned, one not.

    Used by ``test_collect_own_class_pins_returns_only_pinned_rows_for_class``
    to assert the helper filters by ``pinned=True``.
    """
    subject = await create_subject()
    scheme = await create_week_scheme()
    tb_pinned = await create_time_block(week_scheme_id=scheme.id, position=1)
    tb_unpinned = await create_time_block(
        week_scheme_id=scheme.id,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    room = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)

    pinned_lesson = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    unpinned_lesson = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add_all([pinned_lesson, unpinned_lesson])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=pinned_lesson.id, school_class_id=cls.id),
            LessonSchoolClass(lesson_id=unpinned_lesson.id, school_class_id=cls.id),
            ScheduledLesson(
                lesson_id=pinned_lesson.id,
                time_block_id=tb_pinned.id,
                room_id=room.id,
                teacher_id=teacher.id,
                pin_kind=PinKind.HARD,
            ),
            ScheduledLesson(
                lesson_id=unpinned_lesson.id,
                time_block_id=tb_unpinned.id,
                room_id=room.id,
                teacher_id=teacher.id,
                pin_kind=None,
            ),
        ]
    )
    await db_session.flush()
    return SeededClassWithPlacements(
        class_id=cls.id,
        pinned_lesson_id_str=str(pinned_lesson.id),
        unpinned_lesson_id_str=str(unpinned_lesson.id),
    )


@pytest.fixture
async def seeded_class_without_pins(
    create_week_scheme: CreateWeekSchemeFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> uuid.UUID:
    """Seed a class with no ScheduledLesson rows; returns its UUID."""
    scheme = await create_week_scheme()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    return cls.id


@pytest.fixture
async def seeded_movable_placement(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> SeededMovablePlacement:
    """Seed one Lesson with a single placement and a vacant target slot.

    Layout: one WeekScheme, two TimeBlocks (source + target), one Room,
    one Lesson belonging to one SchoolClass, one ScheduledLesson at the
    source TimeBlock with ``pinned=False``.
    """
    subject = await create_subject()
    scheme = await create_week_scheme()
    source_tb = await create_time_block(week_scheme_id=scheme.id, position=1)
    target_tb = await create_time_block(
        week_scheme_id=scheme.id,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    room = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    lesson = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson.id,
            time_block_id=source_tb.id,
            room_id=room.id,
            teacher_id=teacher.id,
            pin_kind=None,
        )
    )
    await db_session.flush()
    return SeededMovablePlacement(
        lesson_id=lesson.id,
        source_time_block_id=source_tb.id,
        target_time_block_id=target_tb.id,
        target_room_id=room.id,
    )


@pytest.fixture
async def seeded_movable_placement_cross_week(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> SeededCrossWeekFixture:
    """Seed two SchoolClasses with their own WeekSchemes; lesson belongs to A only.

    The placement lives at A's time_block. The ``foreign_time_block_id`` lives
    in B's WeekScheme, so an attempted move there must trip the cross-week
    validator.
    """
    subject = await create_subject()
    scheme_a = await create_week_scheme()
    scheme_b = await create_week_scheme()
    source_tb = await create_time_block(week_scheme_id=scheme_a.id, position=1)
    foreign_tb = await create_time_block(week_scheme_id=scheme_b.id, position=1)
    room = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls_a = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme_a.id)
    await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme_b.id)
    lesson = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls_a.id))
    db_session.add(
        ScheduledLesson(
            lesson_id=lesson.id,
            time_block_id=source_tb.id,
            room_id=room.id,
            teacher_id=teacher.id,
            pin_kind=None,
        )
    )
    await db_session.flush()
    return SeededCrossWeekFixture(
        lesson_id=lesson.id,
        source_time_block_id=source_tb.id,
        foreign_time_block_id=foreign_tb.id,
        target_room_id=room.id,
    )


@pytest.fixture
async def seeded_two_placements_for_swap(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> SeededTwoPlacements:
    """Seed two Lessons in the same class + week scheme, each with one placement.

    Both placements start with ``pinned=False`` so the swap test can assert the
    handler flips both flags to ``True``.
    """
    subject = await create_subject()
    scheme = await create_week_scheme()
    tb_a = await create_time_block(week_scheme_id=scheme.id, position=1)
    tb_b = await create_time_block(
        week_scheme_id=scheme.id,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    room = await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    lesson_a = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    lesson_b = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add_all([lesson_a, lesson_b])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=lesson_a.id, school_class_id=cls.id),
            LessonSchoolClass(lesson_id=lesson_b.id, school_class_id=cls.id),
            ScheduledLesson(
                lesson_id=lesson_a.id,
                time_block_id=tb_a.id,
                room_id=room.id,
                teacher_id=teacher.id,
                pin_kind=None,
            ),
            ScheduledLesson(
                lesson_id=lesson_b.id,
                time_block_id=tb_b.id,
                room_id=room.id,
                teacher_id=teacher.id,
                pin_kind=None,
            ),
        ]
    )
    await db_session.flush()
    return SeededTwoPlacements(
        lesson_a_id=lesson_a.id,
        time_block_a_id=tb_a.id,
        lesson_b_id=lesson_b.id,
        time_block_b_id=tb_b.id,
        room_id=room.id,
    )


@pytest.fixture
def seed_placements_for_attribution(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> Callable[..., Awaitable[tuple[uuid.UUID, uuid.UUID]]]:
    """Seed a SchoolClass + placements that exercise gap_hours and home_room_misses.

    Returns an async callable:
    ``(*, gap_positions_for_a_day, place_one_outside_home_room) -> tuple[UUID, UUID]``
    yielding ``(class_id, teacher_id)`` so callers can target either axis.

    Seeds:
    - one SchoolClass with ``home_room_id`` set to one of two created rooms;
    - one WeekScheme with TimeBlocks at positions 1, 2, 3 on day_of_week=1 (all
      ``kind=LESSON``);
    - two Subjects (both non-exempt from the home-room ratio);
    - two Lessons + matching ``LessonSchoolClass`` rows;
    - two ScheduledLesson rows placed at the two requested positions, with the
      second placement's ``room_id`` toggled between home / non-home per the
      flag.
    """

    async def _seed(
        *,
        gap_positions_for_a_day: tuple[int, int],
        place_one_outside_home_room: bool,
    ) -> tuple[uuid.UUID, uuid.UUID]:
        ws = await create_week_scheme()
        # Three lesson-kind time blocks at positions 1, 2, 3 on day_of_week=1.
        tbs = [
            await create_time_block(
                week_scheme_id=ws.id,
                day_of_week=1,
                position=p,
                start_time=time(8 + p, 0),
                end_time=time(8 + p, 45),
            )
            for p in (1, 2, 3)
        ]
        home_room = await create_room()
        other_room = await create_room()
        tafel = await create_stundentafel()
        cls = await create_school_class(
            stundentafel_id=tafel.id,
            week_scheme_id=ws.id,
            home_room_id=home_room.id,
        )
        teacher = await create_teacher()
        subj_a = await create_subject()
        subj_b = await create_subject()
        lesson_a = Lesson(
            school_id=DEFAULT_SCHOOL_ID,
            subject_id=subj_a.id,
            teacher_id=teacher.id,
            hours_per_week=1,
            preferred_block_size=1,
        )
        lesson_b = Lesson(
            school_id=DEFAULT_SCHOOL_ID,
            subject_id=subj_b.id,
            teacher_id=teacher.id,
            hours_per_week=1,
            preferred_block_size=1,
        )
        db_session.add_all([lesson_a, lesson_b])
        await db_session.flush()
        db_session.add_all(
            [
                LessonSchoolClass(lesson_id=lesson_a.id, school_class_id=cls.id),
                LessonSchoolClass(lesson_id=lesson_b.id, school_class_id=cls.id),
            ]
        )
        await db_session.flush()
        pos_first, pos_second = gap_positions_for_a_day
        tb_by_pos = {tb.position: tb for tb in tbs}
        db_session.add(
            ScheduledLesson(
                lesson_id=lesson_a.id,
                time_block_id=tb_by_pos[pos_first].id,
                room_id=home_room.id,
                teacher_id=teacher.id,
            )
        )
        db_session.add(
            ScheduledLesson(
                lesson_id=lesson_b.id,
                time_block_id=tb_by_pos[pos_second].id,
                room_id=(other_room.id if place_one_outside_home_room else home_room.id),
                teacher_id=teacher.id,
            )
        )
        await db_session.flush()
        return cls.id, teacher.id

    return _seed


@pytest.fixture
async def seeded_dreizuegig_with_one_pin(
    db_session: AsyncSession,
    create_subject: CreateSubjectFn,
    create_week_scheme: CreateWeekSchemeFn,
    create_time_block: CreateTimeBlockFn,
    create_room: CreateRoomFn,
    create_teacher: CreateTeacherFn,
    create_stundentafel: CreateStundentafelFn,
    create_school_class: CreateSchoolClassFn,
) -> SeededDreizuegigWithPin:
    """Tiny two-class school with one pre-pinned ScheduledLesson row.

    Layout: one WeekScheme with four TimeBlocks (so the solver has slack),
    two Rooms, two Subjects, two Teachers each qualified for one Subject,
    one Stundentafel, two SchoolClasses. One Lesson per class, both with
    a teacher set so ``build_problem_json`` picks them up. A single
    ScheduledLesson row is pre-inserted with ``pinned=True`` for
    ``lesson_a`` at ``tb_1``; the solver respects it under
    ``respect_pins=true`` and the persist helper preserves the flag under
    ``respect_pins=false``.
    """
    subject_a = await create_subject()
    subject_b = await create_subject()
    scheme = await create_week_scheme()
    tb_1 = await create_time_block(week_scheme_id=scheme.id, day_of_week=0, position=1)
    await create_time_block(
        week_scheme_id=scheme.id,
        day_of_week=0,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    await create_time_block(
        week_scheme_id=scheme.id,
        day_of_week=1,
        position=1,
    )
    await create_time_block(
        week_scheme_id=scheme.id,
        day_of_week=1,
        position=2,
        start_time=time(8, 45),
        end_time=time(9, 30),
    )
    room_a = await create_room()
    await create_room()
    teacher_a = await create_teacher()
    teacher_b = await create_teacher()
    db_session.add_all(
        [
            TeacherQualification(teacher_id=teacher_a.id, subject_id=subject_a.id),
            TeacherQualification(teacher_id=teacher_b.id, subject_id=subject_b.id),
        ]
    )
    await db_session.flush()
    tafel = await create_stundentafel()
    cls_a = await create_school_class(
        name="ClassPin-A",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
    )
    cls_b = await create_school_class(
        name="ClassPin-B",
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
    )
    lesson_a = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject_a.id,
        teacher_id=teacher_a.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    lesson_b = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject_b.id,
        teacher_id=teacher_b.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add_all([lesson_a, lesson_b])
    await db_session.flush()
    db_session.add_all(
        [
            LessonSchoolClass(lesson_id=lesson_a.id, school_class_id=cls_a.id),
            LessonSchoolClass(lesson_id=lesson_b.id, school_class_id=cls_b.id),
            ScheduledLesson(
                lesson_id=lesson_a.id,
                time_block_id=tb_1.id,
                room_id=room_a.id,
                teacher_id=teacher_a.id,
                pin_kind=PinKind.HARD,
            ),
        ]
    )
    await db_session.flush()
    return SeededDreizuegigWithPin(
        pinned_lesson_id=lesson_a.id,
        pinned_time_block_id=tb_1.id,
    )
