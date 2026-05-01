"""Hessen Grundschule (dreizuegig) demo seed.

Three-Zug variant of ``demo_grundschule``: 12 classes (1a/1b/1c..4c),
18 teachers, 16 rooms, same WeekScheme grid + Stundentafel hours as the
einzuegig seed but with Religion delivered via a cross-class trio per
Jahrgang instead of a per-class Stundentafel entry. This is the first
seed variant that actually exercises the ``Lesson`` <-> ``SchoolClass``
many-to-many shape: each Religion lesson is one ``Lesson`` row spanning
three ``SchoolClass`` rows via ``LessonSchoolClass`` and sharing a
``lesson_group_id`` with the other two Religionsgruppen for the same
Jahrgang.

The seed coroutine inserts the cross-class Religion lessons itself; the
``POST /api/classes/{id}/generate-lessons`` route handler reads
``LessonSchoolClass`` to detect already-served subjects and silently
skips them, so the Stundentafel-driven generate path produces the
remaining (non-Religion) lessons after the seed runs.

The module reuses einzuegig's NamedTuple specs and module-level
constants (``_PERIODS``, ``_DAYS_MON_TO_FRI``, ``_SUBJECTS``,
``_KLASSENRAUM_SUITABLE_SUBJECTS``, and the ``_TeacherSpec`` /
``_RoomSpec`` / ``_SchoolClassSpec`` types). The Stundentafel hour dicts
diverge: ``_GRADE_1_2_HOURS_DREIZUEGIG`` and ``_GRADE_3_4_HOURS_DREIZUEGIG``
drop the ``ETH`` row because Religion is delivered via the cross-class
trio, not the Stundentafel.

Lesson rows for non-Religion subjects are not produced by this seed;
they are created by ``POST /api/classes/{id}/generate-lessons`` in the
route layer. The ``_TEACHER_ASSIGNMENTS_DREIZUEGIG`` dict declares the
canonical ``(class_name, subject_short)`` -> teacher ``short_code``
mapping that the matching solvability test pins onto every non-Religion
Lesson (overriding auto-assign for determinism). Religion teachers are
pinned by the seed itself via ``Lesson.teacher_id``.
"""

import uuid
from datetime import time
from typing import NamedTuple

from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.room import Room, RoomSubjectSuitability
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import Stundentafel, StundentafelEntry
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.db.models.week_scheme import TimeBlock, WeekScheme
from klassenzeit_backend.seed.demo_grundschule import (
    _DAYS_MON_TO_FRI,
    _KLASSENRAUM_SUITABLE_SUBJECTS,
    _PERIODS,
    _SUBJECTS,
    _assign_eponymous_home_rooms,
    _PeriodTimes,
    _RoomSpec,
    _SchoolClassSpec,
    _TeacherSpec,
)

WEEK_SCHEME_NAME = "Grundschule (dreizuegig) Zeitraster"
WEEK_SCHEME_DESCRIPTION = (
    "Hessen Grundschule, drei Zuege pro Jahrgang: 5 Tage, 8 Stunden a 45 Minuten, "
    "Hofpausen nach der 2. und 4. Stunde. Stunden 7 und 8 dienen als Ganztags- / "
    "AG-Zeitfenster und geben dem Solver Slack fuer drei Zuege plus die "
    "jahrgangsweite Religionsdreiergruppe (RK/RE/ETH via lesson_group_id). "
    "Die Stundentafel enthaelt deshalb keine ETH-Zeile."
)


# Dreizuegig extends the einzuegig 7-period grid with an eighth ganztags
# period so the FFD greedy can place all 12 classes' Stundentafel-driven
# lessons plus the cross-class Religion trio (3 lessons per Jahrgang, each
# spanning 3 classes) without UUID-tiebreak-dependent flakiness. The 8th
# period (14:05 to 14:50) follows the existing 7-period pattern.
_PERIODS_DREIZUEGIG: tuple[_PeriodTimes, ...] = (
    *_PERIODS,
    _PeriodTimes(8, time(14, 5), time(14, 50)),
)


# Dreizuegige Stundentafel: drops the ``ETH`` row from the einzuegig
# tables because Religion is delivered as a cross-class trio (RK/RE/ETH)
# per Jahrgang. Each class still receives 2h Religion via the trio, so
# the per-class total weekly hours stay 23 (grades 1/2) and 26 (grades
# 3/4), matching the einzuegig totals.
_GRADE_1_2_HOURS_DREIZUEGIG: dict[str, int] = {
    "D": 6,
    "M": 5,
    "SU": 2,
    "KU": 2,
    "MU": 1,
    "SP": 3,
    "FÖ": 2,
}  # 21h total (Religion trio adds 6h slot consumption per class)

_GRADE_3_4_HOURS_DREIZUEGIG: dict[str, int] = {
    "D": 5,
    "M": 5,
    "SU": 4,
    "E": 2,
    "KU": 2,
    "MU": 1,
    "SP": 3,
    "FÖ": 2,
}  # 24h total (Religion trio adds 6h slot consumption per class)


_TEACHERS_DREIZUEGIG: tuple[_TeacherSpec, ...] = (
    # Twelve Klassenlehrer (one per class). Grades 1/2 cover D/M/SU/KU,
    # grades 3/4 cover D/M/SU/E. Per-class load: 15h (grades 1/2) or
    # 16h (grades 3/4), well under max_hours_per_week.
    _TeacherSpec("Anna", "Mueller", "MUE", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Beate", "Schmidt", "SCH", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Clara", "Diehl", "DIE", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Dora", "Engel", "ENG", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Erik", "Klein", "KAI", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Frieda", "Lange", "LAN", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Greta", "Nolte", "NOL", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Heinrich", "Roth", "ROT", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Inge", "Stahl", "STA", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Jonas", "Braun", "BRA", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Karla", "Huber", "HUB", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Lutz", "Frey", "FRE", 28, ("D", "M", "SU", "E")),
    # Three Zug-bound specialists. Each handles the SP/MU/FOE workload
    # of one Zug across all four Jahrgaenge plus the leftover KU for
    # grades 3/4 (Klassenlehrer of grades 3/4 take E, not KU). Per-Zug
    # load: SP 12h + FÖ 8h + MU 4h + KU 4h = 28h.
    _TeacherSpec("Frank", "Hoffmann", "HOF", 28, ("SP", "MU", "FÖ", "KU")),
    _TeacherSpec("Juergen", "Richter", "RIC", 28, ("SP", "MU", "FÖ", "KU")),
    _TeacherSpec("Sandra", "Schuster", "SCS", 28, ("SP", "MU", "FÖ", "KU")),
    # Three Religion-trio specialists. Each teaches one Religionsfach
    # across all four Jahrgaenge: 4 Jahrgaenge x 1 cross-class lesson x
    # 2h = 8h per teacher per week.
    _TeacherSpec("Pfarrer", "Klein", "PFK", 14, ("RK",)),
    _TeacherSpec("Pastorin", "Lange", "PSL", 14, ("RE",)),
    _TeacherSpec("Philipp", "Otto", "PHL", 14, ("ETH",)),
)


_ROOMS_DREIZUEGIG: tuple[_RoomSpec, ...] = (
    _RoomSpec("Klasse 1a", "1a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 1b", "1b", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 1c", "1c", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 2a", "2a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 2b", "2b", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 2c", "2c", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 3a", "3a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 3b", "3b", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 3c", "3c", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 4a", "4a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 4b", "4b", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 4c", "4c", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Turnhalle", "TH", None, ("SP",)),
    _RoomSpec("Sportplatz", "SP-P", None, ("SP",)),
    _RoomSpec("Musikraum", "MU-R", 30, ("MU",)),
    _RoomSpec("Kunstraum", "KU-R", 20, ("KU",)),
)


_SCHOOL_CLASSES_DREIZUEGIG: tuple[_SchoolClassSpec, ...] = (
    _SchoolClassSpec("1a", 1),
    _SchoolClassSpec("1b", 1),
    _SchoolClassSpec("1c", 1),
    _SchoolClassSpec("2a", 2),
    _SchoolClassSpec("2b", 2),
    _SchoolClassSpec("2c", 2),
    _SchoolClassSpec("3a", 3),
    _SchoolClassSpec("3b", 3),
    _SchoolClassSpec("3c", 3),
    _SchoolClassSpec("4a", 4),
    _SchoolClassSpec("4b", 4),
    _SchoolClassSpec("4c", 4),
)


class _ReligionLessonSpec(NamedTuple):
    """A single (jahrgang, subject, teacher) triple for a Religion lesson.

    All three lessons that share a ``jahrgang`` form one
    ``lesson_group_id`` group and serve the three classes of that
    Jahrgang via ``LessonSchoolClass``.
    """

    jahrgang: int
    subject_short: str
    teacher_short: str


_RELIGION_LESSONS_DREIZUEGIG: tuple[_ReligionLessonSpec, ...] = (
    _ReligionLessonSpec(1, "RK", "PFK"),
    _ReligionLessonSpec(1, "RE", "PSL"),
    _ReligionLessonSpec(1, "ETH", "PHL"),
    _ReligionLessonSpec(2, "RK", "PFK"),
    _ReligionLessonSpec(2, "RE", "PSL"),
    _ReligionLessonSpec(2, "ETH", "PHL"),
    _ReligionLessonSpec(3, "RK", "PFK"),
    _ReligionLessonSpec(3, "RE", "PSL"),
    _ReligionLessonSpec(3, "ETH", "PHL"),
    _ReligionLessonSpec(4, "RK", "PFK"),
    _ReligionLessonSpec(4, "RE", "PSL"),
    _ReligionLessonSpec(4, "ETH", "PHL"),
)


# Authored ``(class_name, subject_short)`` -> teacher ``short_code`` mapping
# for the non-Religion subjects (Religion is pinned on the Lesson by the
# seed itself). The matching solvability test runs SQL UPDATE after
# ``generate-lessons`` so the bench-fixture-stable allocation does not
# drift as ``auto_assign_teachers_for_lessons`` evolves.
#
# Per-teacher hour totals (verified against ``_TEACHERS_DREIZUEGIG.max_hours_per_week``):
#   Klassenlehrer grades 1/2 (D6+M5+SU2+KU2 = 15h):
#     MUE = 1a               = 15h <= 28
#     SCH = 1b               = 15h <= 28
#     DIE = 1c               = 15h <= 28
#     ENG = 2a               = 15h <= 28
#     KAI = 2b               = 15h <= 28
#     LAN = 2c               = 15h <= 28
#   Klassenlehrer grades 3/4 (D5+M5+SU4+E2 = 16h):
#     NOL = 3a               = 16h <= 28
#     ROT = 3b               = 16h <= 28
#     STA = 3c               = 16h <= 28
#     BRA = 4a               = 16h <= 28
#     HUB = 4b               = 16h <= 28
#     FRE = 4c               = 16h <= 28
#   Zug-bound specialists (SP 12h + FÖ 8h + MU 4h + KU 4h = 28h):
#     HOF = Zug a SP+FÖ+MU+KU(3a,4a only) = 28h <= 28
#     RIC = Zug b SP+FÖ+MU+KU(3b,4b only) = 28h <= 28
#     SCS = Zug c SP+FÖ+MU+KU(3c,4c only) = 28h <= 28
#   Religion specialists (4 Jahrgaenge x 2h = 8h, pinned by the seed):
#     PFK = RK across grades 1..4 = 8h <= 14
#     PSL = RE across grades 1..4 = 8h <= 14
#     PHL = ETH across grades 1..4 = 8h <= 14
_TEACHER_ASSIGNMENTS_DREIZUEGIG: dict[tuple[str, str], str] = {
    # Class 1a (grade 1, Zug a)
    ("1a", "D"): "MUE",
    ("1a", "M"): "MUE",
    ("1a", "SU"): "MUE",
    ("1a", "KU"): "MUE",
    ("1a", "MU"): "HOF",
    ("1a", "SP"): "HOF",
    ("1a", "FÖ"): "HOF",
    # Class 1b (grade 1, Zug b)
    ("1b", "D"): "SCH",
    ("1b", "M"): "SCH",
    ("1b", "SU"): "SCH",
    ("1b", "KU"): "SCH",
    ("1b", "MU"): "RIC",
    ("1b", "SP"): "RIC",
    ("1b", "FÖ"): "RIC",
    # Class 1c (grade 1, Zug c)
    ("1c", "D"): "DIE",
    ("1c", "M"): "DIE",
    ("1c", "SU"): "DIE",
    ("1c", "KU"): "DIE",
    ("1c", "MU"): "SCS",
    ("1c", "SP"): "SCS",
    ("1c", "FÖ"): "SCS",
    # Class 2a (grade 2, Zug a)
    ("2a", "D"): "ENG",
    ("2a", "M"): "ENG",
    ("2a", "SU"): "ENG",
    ("2a", "KU"): "ENG",
    ("2a", "MU"): "HOF",
    ("2a", "SP"): "HOF",
    ("2a", "FÖ"): "HOF",
    # Class 2b (grade 2, Zug b)
    ("2b", "D"): "KAI",
    ("2b", "M"): "KAI",
    ("2b", "SU"): "KAI",
    ("2b", "KU"): "KAI",
    ("2b", "MU"): "RIC",
    ("2b", "SP"): "RIC",
    ("2b", "FÖ"): "RIC",
    # Class 2c (grade 2, Zug c)
    ("2c", "D"): "LAN",
    ("2c", "M"): "LAN",
    ("2c", "SU"): "LAN",
    ("2c", "KU"): "LAN",
    ("2c", "MU"): "SCS",
    ("2c", "SP"): "SCS",
    ("2c", "FÖ"): "SCS",
    # Class 3a (grade 3, Zug a) - Klassenlehrer takes E, specialist takes KU
    ("3a", "D"): "NOL",
    ("3a", "M"): "NOL",
    ("3a", "SU"): "NOL",
    ("3a", "E"): "NOL",
    ("3a", "KU"): "HOF",
    ("3a", "MU"): "HOF",
    ("3a", "SP"): "HOF",
    ("3a", "FÖ"): "HOF",
    # Class 3b (grade 3, Zug b)
    ("3b", "D"): "ROT",
    ("3b", "M"): "ROT",
    ("3b", "SU"): "ROT",
    ("3b", "E"): "ROT",
    ("3b", "KU"): "RIC",
    ("3b", "MU"): "RIC",
    ("3b", "SP"): "RIC",
    ("3b", "FÖ"): "RIC",
    # Class 3c (grade 3, Zug c)
    ("3c", "D"): "STA",
    ("3c", "M"): "STA",
    ("3c", "SU"): "STA",
    ("3c", "E"): "STA",
    ("3c", "KU"): "SCS",
    ("3c", "MU"): "SCS",
    ("3c", "SP"): "SCS",
    ("3c", "FÖ"): "SCS",
    # Class 4a (grade 4, Zug a)
    ("4a", "D"): "BRA",
    ("4a", "M"): "BRA",
    ("4a", "SU"): "BRA",
    ("4a", "E"): "BRA",
    ("4a", "KU"): "HOF",
    ("4a", "MU"): "HOF",
    ("4a", "SP"): "HOF",
    ("4a", "FÖ"): "HOF",
    # Class 4b (grade 4, Zug b)
    ("4b", "D"): "HUB",
    ("4b", "M"): "HUB",
    ("4b", "SU"): "HUB",
    ("4b", "E"): "HUB",
    ("4b", "KU"): "RIC",
    ("4b", "MU"): "RIC",
    ("4b", "SP"): "RIC",
    ("4b", "FÖ"): "RIC",
    # Class 4c (grade 4, Zug c)
    ("4c", "D"): "FRE",
    ("4c", "M"): "FRE",
    ("4c", "SU"): "FRE",
    ("4c", "E"): "FRE",
    ("4c", "KU"): "SCS",
    ("4c", "MU"): "SCS",
    ("4c", "SP"): "SCS",
    ("4c", "FÖ"): "SCS",
}


async def seed_demo_grundschule_dreizuegig(session: AsyncSession) -> None:
    """Seed a realistic dreizuegige Hessen Grundschule into ``session``.

    Caller owns the transaction: this coroutine only ``flush()``es so FK
    lookups resolve. Commit once at the end, or rollback on error.

    The ``_TEACHER_ASSIGNMENTS_DREIZUEGIG`` dict is consumed by the
    matching solvability test (which pins ``Lesson.teacher_id`` after
    ``generate-lessons`` runs); the seed coroutine itself populates
    entities up to the room/teacher/qualification layer, then inserts
    the cross-class Religion lessons (RK/RE/ETH per Jahrgang, sharing
    ``lesson_group_id``) with their teachers already pinned.

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
        for period in _PERIODS_DREIZUEGIG:
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
            prefer_early_periods=spec.prefer_early_periods,
            avoid_first_period=spec.avoid_first_period,
            avoid_last_period=spec.avoid_last_period,
        )
        session.add(subject)
        subjects_by_short[spec.short_name] = subject
    await session.flush()

    tafel_hours_by_grade: dict[int, dict[str, int]] = {
        1: _GRADE_1_2_HOURS_DREIZUEGIG,
        2: _GRADE_1_2_HOURS_DREIZUEGIG,
        3: _GRADE_3_4_HOURS_DREIZUEGIG,
        4: _GRADE_3_4_HOURS_DREIZUEGIG,
    }
    tafeln_by_grade: dict[int, Stundentafel] = {}
    for grade in tafel_hours_by_grade:
        tafel = Stundentafel(name=f"Grundschule {grade}", grade_level=grade)
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
                    preferred_block_size=1,
                )
            )
    await session.flush()

    classes_by_name: dict[str, SchoolClass] = {}
    for class_spec in _SCHOOL_CLASSES_DREIZUEGIG:
        school_class = SchoolClass(
            name=class_spec.name,
            grade_level=class_spec.grade_level,
            stundentafel_id=tafeln_by_grade[class_spec.grade_level].id,
            week_scheme_id=week_scheme.id,
        )
        session.add(school_class)
        classes_by_name[class_spec.name] = school_class
    await session.flush()

    teachers_by_short: dict[str, Teacher] = {}
    for teacher_spec in _TEACHERS_DREIZUEGIG:
        teacher = Teacher(
            first_name=teacher_spec.first_name,
            last_name=teacher_spec.last_name,
            short_code=teacher_spec.short_code,
            max_hours_per_week=teacher_spec.max_hours_per_week,
            is_active=True,
        )
        session.add(teacher)
        await session.flush()
        teachers_by_short[teacher_spec.short_code] = teacher
        for subject_short in teacher_spec.qualified_subject_short_names:
            session.add(
                TeacherQualification(
                    teacher_id=teacher.id,
                    subject_id=subjects_by_short[subject_short].id,
                )
            )
    await session.flush()

    for room_spec in _ROOMS_DREIZUEGIG:
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

    await _assign_eponymous_home_rooms(session, set(classes_by_name))

    # Cross-class Religion trio per Jahrgang. Each Jahrgang gets one
    # ``lesson_group_id`` shared by RK / RE / ETH; each lesson spans the
    # three classes of the Jahrgang via ``LessonSchoolClass``. Teachers
    # are pinned at insert time so the solver sees teacher_id IS NOT NULL
    # without the route's auto-assign step running.
    await _insert_religion_trios(
        session,
        classes_by_name=classes_by_name,
        teachers_by_short=teachers_by_short,
        subjects_by_short=subjects_by_short,
    )


async def _insert_religion_trios(
    session: AsyncSession,
    *,
    classes_by_name: dict[str, SchoolClass],
    teachers_by_short: dict[str, Teacher],
    subjects_by_short: dict[str, Subject],
) -> None:
    """Insert the per-Jahrgang Religion trio (RK/RE/ETH multi-class lessons)."""
    for grade in (1, 2, 3, 4):
        group_id = uuid.uuid4()
        classes_in_jahrgang = [
            classes_by_name[spec.name]
            for spec in _SCHOOL_CLASSES_DREIZUEGIG
            if spec.grade_level == grade
        ]
        trio_for_grade = [spec for spec in _RELIGION_LESSONS_DREIZUEGIG if spec.jahrgang == grade]
        for spec in trio_for_grade:
            teacher = teachers_by_short[spec.teacher_short]
            lesson = Lesson(
                subject_id=subjects_by_short[spec.subject_short].id,
                teacher_id=teacher.id,
                hours_per_week=2,
                preferred_block_size=1,
                lesson_group_id=group_id,
            )
            session.add(lesson)
            await session.flush()
            for school_class in classes_in_jahrgang:
                session.add(
                    LessonSchoolClass(
                        lesson_id=lesson.id,
                        school_class_id=school_class.id,
                    )
                )
    await session.flush()
