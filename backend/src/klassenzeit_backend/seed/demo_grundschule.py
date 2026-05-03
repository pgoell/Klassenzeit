"""Hessen Grundschule demo seed.

One-shot seed that creates the entities needed for the end-to-end
"login, click Generate, see a timetable" demo flow. See
``docs/superpowers/specs/2026-04-24-grundschule-seed-design.md`` for the
full design including the mapping from real Hessen Grundschule
constraints to schema columns.

Scope ends before ``lessons`` and ``scheduled_lessons``; those are created
by the ``generate-lessons`` and ``POST /schedule`` routes respectively.
"""

from datetime import time
from typing import NamedTuple

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.room import Room, RoomSubjectSuitability
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import (
    SchoolType,
    Stundentafel,
    StundentafelEntry,
)
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.db.models.week_scheme import TimeBlock, WeekScheme

WEEK_SCHEME_NAME = "Grundschule Zeitraster"
WEEK_SCHEME_DESCRIPTION = (
    "Hessen Grundschule: 5 Tage, 7 Stunden a 45 Minuten, "
    "Hofpausen nach der 2. und 4. Stunde. Stunde 7 dient als Ganztags- / "
    "AG-Zeitfenster und gibt dem Solver Slack fuer volle Stundentafeln."
)


class _PeriodTimes(NamedTuple):
    position: int
    start: time
    end: time


_PERIODS: tuple[_PeriodTimes, ...] = (
    _PeriodTimes(1, time(8, 0), time(8, 45)),
    _PeriodTimes(2, time(8, 45), time(9, 30)),
    _PeriodTimes(3, time(9, 50), time(10, 35)),
    _PeriodTimes(4, time(10, 35), time(11, 20)),
    _PeriodTimes(5, time(11, 35), time(12, 20)),
    _PeriodTimes(6, time(12, 20), time(13, 5)),
    _PeriodTimes(7, time(13, 20), time(14, 5)),
)

_DAYS_MON_TO_FRI: tuple[int, ...] = (0, 1, 2, 3, 4)


class _SubjectSpec(NamedTuple):
    name: str
    short_name: str
    color: str
    prefer_early_period: int = 0
    avoid_first_period: int = 0
    avoid_last_period: int = 0
    prefer_late_period: int = 0


_SUBJECTS: tuple[_SubjectSpec, ...] = (
    _SubjectSpec("Deutsch", "D", "chart-1", prefer_early_period=1, avoid_last_period=1),
    _SubjectSpec("Mathematik", "M", "chart-2", prefer_early_period=1, avoid_last_period=1),
    _SubjectSpec("Sachunterricht", "SU", "chart-3"),
    _SubjectSpec("Religion (kath.)", "RK", "chart-4"),
    _SubjectSpec("Religion (ev.)", "RE", "chart-4"),
    _SubjectSpec("Ethik", "ETH", "chart-4"),
    _SubjectSpec("Englisch", "E", "chart-5"),
    _SubjectSpec("Kunst", "KU", "chart-1"),
    _SubjectSpec("Musik", "MU", "chart-3"),
    _SubjectSpec("Sport", "SP", "chart-4", avoid_first_period=1),
    _SubjectSpec("Förderunterricht", "FÖ", "chart-5"),
)


# Hessen Grundschule groups Kunst/Werken/Musik as "Ästhetische Erziehung"
# (3h in grades 1/2, 4h in grades 3/4). The seed keeps Kunst and Musik
# as separate subjects and rolls Werken's hours into Kunst so the demo
# has only subjects that real Grundschule timetables treat as standalone.
_GRADE_1_2_HOURS: dict[str, int] = {
    "D": 6,
    "M": 5,
    "SU": 2,
    "ETH": 2,
    "KU": 2,
    "MU": 1,
    "SP": 3,
    "FÖ": 2,
}

_GRADE_3_4_HOURS: dict[str, int] = {
    "D": 5,
    "M": 5,
    "SU": 4,
    "E": 2,
    "ETH": 2,
    "KU": 2,
    "MU": 1,
    "SP": 3,
    "FÖ": 2,
}


# Subjects taught as Doppelstunden in the Hessen Grundschule. Maps subject
# short_name -> preferred_block_size. Subjects not listed default to 1.
# Hessen pedagogy: Sachunterricht is typically a Doppelstunde for the
# experiment-heavy lessons in Klasse 3/4; in Klasse 1/2 it's lighter but
# the flip simplifies the seed and stays divisibility-clean (2h = 1 block).
_DOPPEL_SUBJECTS: dict[str, int] = {"SU": 2}


class _TeacherSpec(NamedTuple):
    first_name: str
    last_name: str
    short_code: str
    max_hours_per_week: int
    qualified_subject_short_names: tuple[str, ...]


_TEACHERS: tuple[_TeacherSpec, ...] = (
    _TeacherSpec("Anna", "Müller", "MUE", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Beate", "Schmidt", "SCH", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Carsten", "Weber", "WEB", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Dana", "Fischer", "FIS", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Eva", "Becker", "BEC", 18, ("RK", "RE", "ETH", "MU", "FÖ")),
    _TeacherSpec("Frank", "Hoffmann", "HOF", 21, ("SP", "KU", "FÖ")),
)


class _RoomSpec(NamedTuple):
    name: str
    short_name: str
    capacity: int | None
    suitable_subject_short_names: tuple[str, ...]


_KLASSENRAUM_SUITABLE_SUBJECTS: tuple[str, ...] = ("D", "M", "SU", "RK", "RE", "ETH", "E", "FÖ")

_ROOMS: tuple[_RoomSpec, ...] = (
    _RoomSpec("Klasse 1a", "1a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 2a", "2a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 3a", "3a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 4a", "4a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Turnhalle", "TH", None, ("SP",)),
    _RoomSpec("Musikraum", "MU-R", 30, ("MU",)),
    _RoomSpec("Kunstraum", "KU-R", 20, ("KU",)),
)


class _SchoolClassSpec(NamedTuple):
    name: str
    grade_level: int


_SCHOOL_CLASSES: tuple[_SchoolClassSpec, ...] = (
    _SchoolClassSpec("1a", 1),
    _SchoolClassSpec("2a", 2),
    _SchoolClassSpec("3a", 3),
    _SchoolClassSpec("4a", 4),
)


async def seed_demo_grundschule(session: AsyncSession) -> None:
    """Seed a realistic einzügige Hessen Grundschule into ``session``.

    Caller owns the transaction: this coroutine only ``flush()``es so FK
    lookups resolve. Commit once at the end, or rollback on error.

    Raises:
        sqlalchemy.exc.IntegrityError: on any unique-name collision. The
            caller is expected to rollback the outer transaction and
            surface the error to the user.
    """
    week_scheme = WeekScheme(
        name=WEEK_SCHEME_NAME,
        description=WEEK_SCHEME_DESCRIPTION,
    )
    session.add(week_scheme)
    await session.flush()

    for day in _DAYS_MON_TO_FRI:
        for period in _PERIODS:
            session.add(
                TimeBlock(
                    week_scheme_id=week_scheme.id,
                    day_of_week=day,
                    position=period.position,
                    start_time=period.start,
                    end_time=period.end,
                )
            )
    await session.flush()

    subjects_by_short: dict[str, Subject] = {}
    for spec in _SUBJECTS:
        subject = Subject(
            name=spec.name,
            short_name=spec.short_name,
            color=spec.color,
            prefer_early_period=spec.prefer_early_period,
            avoid_first_period=spec.avoid_first_period,
            avoid_last_period=spec.avoid_last_period,
            prefer_late_period=spec.prefer_late_period,
        )
        session.add(subject)
        subjects_by_short[spec.short_name] = subject
    await session.flush()

    tafel_hours_by_grade: dict[int, dict[str, int]] = {
        1: _GRADE_1_2_HOURS,
        2: _GRADE_1_2_HOURS,
        3: _GRADE_3_4_HOURS,
        4: _GRADE_3_4_HOURS,
    }
    tafeln_by_grade: dict[int, Stundentafel] = {}
    for grade in tafel_hours_by_grade:
        tafel = Stundentafel(
            name=f"Grundschule {grade}",
            grade_level=grade,
            school_type=SchoolType.GRUNDSCHULE,
        )
        session.add(tafel)
        tafeln_by_grade[grade] = tafel
    await session.flush()

    for grade, tafel in tafeln_by_grade.items():
        for subject_short, hours in tafel_hours_by_grade[grade].items():
            session.add(
                StundentafelEntry(
                    stundentafel_id=tafel.id,
                    subject_id=subjects_by_short[subject_short].id,
                    hours_per_week=hours,
                    preferred_block_size=_DOPPEL_SUBJECTS.get(subject_short, 1),
                )
            )
    await session.flush()

    for class_spec in _SCHOOL_CLASSES:
        session.add(
            SchoolClass(
                name=class_spec.name,
                grade_level=class_spec.grade_level,
                stundentafel_id=tafeln_by_grade[class_spec.grade_level].id,
                week_scheme_id=week_scheme.id,
            )
        )
    await session.flush()

    for teacher_spec in _TEACHERS:
        teacher = Teacher(
            first_name=teacher_spec.first_name,
            last_name=teacher_spec.last_name,
            short_code=teacher_spec.short_code,
            max_hours_per_week=teacher_spec.max_hours_per_week,
            is_active=True,
        )
        session.add(teacher)
        await session.flush()
        for subject_short in teacher_spec.qualified_subject_short_names:
            session.add(
                TeacherQualification(
                    teacher_id=teacher.id,
                    subject_id=subjects_by_short[subject_short].id,
                )
            )
    await session.flush()

    for room_spec in _ROOMS:
        room = Room(
            name=room_spec.name,
            short_name=room_spec.short_name,
            capacity=room_spec.capacity,
        )
        session.add(room)
        await session.flush()
        for subject_short in room_spec.suitable_subject_short_names:
            session.add(
                RoomSubjectSuitability(
                    room_id=room.id,
                    subject_id=subjects_by_short[subject_short].id,
                )
            )
    await session.flush()

    await _assign_eponymous_home_rooms(session, {spec.name for spec in _SCHOOL_CLASSES})


async def _assign_eponymous_home_rooms(session: AsyncSession, class_short_names: set[str]) -> None:
    """Assign each class its eponymous Klassenraum as home room.

    Pure 1:1 mapping by name (room "Klasse 1a" / short_name "1a" pairs
    with class named "1a"); rooms without a matching class (Turnhalle,
    Sportplatz, Musikraum, Kunstraum) stay unmapped. Shared across the
    einzuegig, zweizuegig, and dreizuegig demo seeds.
    """
    matching_rooms = (
        (await session.execute(select(Room).where(Room.short_name.in_(class_short_names))))
        .scalars()
        .all()
    )
    rooms_by_short_name = {room.short_name: room for room in matching_rooms}
    classes = (
        (await session.execute(select(SchoolClass).where(SchoolClass.name.in_(class_short_names))))
        .scalars()
        .all()
    )
    for cls in classes:
        home_room = rooms_by_short_name.get(cls.name)
        if home_room is not None:
            cls.home_room_id = home_room.id
    await session.flush()
