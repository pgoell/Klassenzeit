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
constants (``_DAYS_MON_TO_FRI``, ``_SUBJECTS``,
``_KLASSENRAUM_SUITABLE_SUBJECTS``, and the ``_TeacherSpec`` /
``_RoomSpec`` / ``_SchoolClassSpec`` types). The Stundentafel hour dicts
diverge: ``_GRADE_1_2_HOURS_DREIZUEGIG`` and ``_GRADE_3_4_HOURS_DREIZUEGIG``
drop the ``ETH`` row because Religion is delivered via the cross-class
trio, not the Stundentafel. The 8-period grid is defined locally
(``_PERIODS_DREIZUEGIG``) so dreizuegig's Ganztagsschule shape is
independent of einzuegig's ``_PERIODS``.

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
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import (
    SchoolType,
    Stundentafel,
    StundentafelEntry,
)
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.db.models.week_scheme import TimeBlock, TimeBlockKind, WeekScheme
from klassenzeit_backend.seed.demo_grundschule import (
    _DAYS_MON_TO_FRI,
    _KLASSENRAUM_SUITABLE_SUBJECTS,
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
    "Hofpausen nach der 2. und 4. Stunde, Mittagspause nach der 6. Stunde. "
    "Die Stunden 7 und 8 (am Nachmittag) dienen als Ganztags- / AG-Zeitfenster "
    "und geben dem Solver Slack fuer drei Zuege plus die jahrgangsweite "
    "Religionsdreiergruppe (RK/RE/ETH via lesson_group_id). Die Stundentafel "
    "enthaelt deshalb keine ETH-Zeile."
)


# Dreizuegig is a Ganztagsschule pattern: 11 ordinal positions per day
# (8 lesson slots interleaved with 2 Hofpausen plus 1 Mittagspause),
# defined locally so the shape is independent of einzuegig's ``_PERIODS``.
# The two morning Hofpausen sit after lesson 2 (Hofpause 09:30-09:50,
# 20 min) and lesson 4 (Hofpause 11:20-11:35, 15 min); the 45-minute
# Mittagspause sits after lesson 6 (13:05-13:50). Afternoon lesson slots
# at 13:50-14:35 and 14:35-15:20 give the FFD greedy enough slack to
# place all 12 classes' Stundentafel-driven lessons plus the cross-class
# Religion trio (3 lessons per Jahrgang, each spanning 3 classes) without
# UUID-tiebreak-dependent flakiness.
_PERIODS_DREIZUEGIG: tuple[_PeriodTimes, ...] = (
    _PeriodTimes(1, time(8, 0), time(8, 45)),
    _PeriodTimes(2, time(8, 45), time(9, 30)),
    _PeriodTimes(3, time(9, 30), time(9, 50), TimeBlockKind.BREAK),
    _PeriodTimes(4, time(9, 50), time(10, 35)),
    _PeriodTimes(5, time(10, 35), time(11, 20)),
    _PeriodTimes(6, time(11, 20), time(11, 35), TimeBlockKind.BREAK),
    _PeriodTimes(7, time(11, 35), time(12, 20)),
    _PeriodTimes(8, time(12, 20), time(13, 5)),
    _PeriodTimes(9, time(13, 5), time(13, 50), TimeBlockKind.BREAK),
    _PeriodTimes(10, time(13, 50), time(14, 35)),
    _PeriodTimes(11, time(14, 35), time(15, 20)),
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


# Override: the Zug-a sport specialist (HOF) also teaches the Klasse 3a
# Schwimmen Doppelstunde appended below (ADR 0044 travel-buffer fixture).
# Bumps HOF's max_hours_per_week from 28 to 30 (2h Schwimmen on top of the
# 28h Sport/MU/FOE/KU load) and widens qualifications to include Schwimmen.
# Mirrors `teacher_max_hours[12] = 30` in `solver-core/src/test_fixtures.rs`.
def _bump_hof_for_schwimmen(spec: _TeacherSpec) -> _TeacherSpec:
    if spec.short_code != "HOF":
        return spec
    return _TeacherSpec(
        spec.first_name,
        spec.last_name,
        spec.short_code,
        30,
        (*spec.qualified_subject_short_names, "SwM"),
    )


_TEACHERS_DREIZUEGIG = tuple(_bump_hof_for_schwimmen(t) for t in _TEACHERS_DREIZUEGIG)


# Schwimmen Subject + Schwimmbad Room (external venue) mirror the
# `dreizuegig_fixture` extension in `solver-core/src/test_fixtures.rs`
# for the ADR 0044 travel-buffer constraint. Schwimmen sits outside the
# Stundentafel (it's a per-class extra-curricular Doppelstunde, not a
# regular curriculum row), so the seed inserts the Lesson directly.
class _ExtraSubjectSpec(NamedTuple):
    name: str
    short_name: str
    color: str


_EXTRA_SUBJECTS_DREIZUEGIG: tuple[_ExtraSubjectSpec, ...] = (
    _ExtraSubjectSpec("Schwimmen", "SwM", "chart-2"),
)


# Schwimmbad: external venue with the Schwimmen suitability flag. `is_external`
# turns on the travel-buffer enforcement contract for any lesson placed in it.
class _ExtraRoomSpec(NamedTuple):
    name: str
    short_name: str
    capacity: int | None
    is_external: bool
    suitable_subject_short_names: tuple[str, ...]


_EXTRA_ROOMS_DREIZUEGIG: tuple[_ExtraRoomSpec, ...] = (
    _ExtraRoomSpec("Schwimmbad", "SB", None, True, ("SwM",)),
)


class _ExtraLessonSpec(NamedTuple):
    class_name: str
    subject_short: str
    teacher_short: str
    hours_per_week: int
    preferred_block_size: int
    pre_buffer_minutes: int
    post_buffer_minutes: int


_EXTRA_LESSONS_DREIZUEGIG: tuple[_ExtraLessonSpec, ...] = (
    # Klasse 3a Schwimmen Doppelstunde, pinned to HOF (Zug-a sport teacher).
    # 15-minute pre/post travel buffers per ADR 0044.
    _ExtraLessonSpec("3a", "SwM", "HOF", 2, 2, 15, 15),
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
# drift across solver-driven runs (item 63 + ADR 0036).
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
                    kind=period.kind,
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
    # ADR 0044 extra subjects (Schwimmen) live outside the Stundentafel.
    for extra_spec in _EXTRA_SUBJECTS_DREIZUEGIG:
        extra_subject = Subject(
            name=extra_spec.name,
            short_name=extra_spec.short_name,
            color=extra_spec.color,
        )
        session.add(extra_subject)
        subjects_by_short[extra_spec.short_name] = extra_subject
    await session.flush()

    tafel_hours_by_grade: dict[int, dict[str, int]] = {
        1: _GRADE_1_2_HOURS_DREIZUEGIG,
        2: _GRADE_1_2_HOURS_DREIZUEGIG,
        3: _GRADE_3_4_HOURS_DREIZUEGIG,
        4: _GRADE_3_4_HOURS_DREIZUEGIG,
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
            school_id=DEFAULT_SCHOOL_ID,
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
            school_id=DEFAULT_SCHOOL_ID,
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

    # One Teilzeit teacher (Mo/Di/Mi only). Qualifications overlap the
    # grades 1/2 Klassenlehrer pool (D/M/SU/KU) so that the existing
    # _TEACHER_ASSIGNMENTS_DREIZUEGIG pin map stays feasible: this teacher
    # is in the candidate pool but no Lesson pins them, so the solver
    # only reaches for them when their three days fit. max_hours_per_week
    # 14 keeps the weekly load under 5h/day across the three Arbeitstage.
    # Not registered as class_teacher_id on any SchoolClass because
    # Klassenlehrer need full-week presence.
    teilzeit_teacher = Teacher(
        first_name="Tina",
        last_name="Zander",
        short_code="TZD",
        max_hours_per_week=14,
        is_active=True,
        working_days=[0, 1, 2],
        school_id=DEFAULT_SCHOOL_ID,
    )
    session.add(teilzeit_teacher)
    await session.flush()
    teachers_by_short["TZD"] = teilzeit_teacher
    for subject_short in ("D", "M", "SU", "KU"):
        session.add(
            TeacherQualification(
                teacher_id=teilzeit_teacher.id,
                subject_id=subjects_by_short[subject_short].id,
            )
        )
    await session.flush()

    await _insert_rooms_dreizuegig(session, subjects_by_short=subjects_by_short)

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

    # ADR 0044 extra Lessons (Klasse 3a Schwimmen). Pinned at seed time
    # mirroring the Religion trio pattern; the Lesson sits outside the
    # Stundentafel so `generate-lessons` does not create or duplicate it.
    await _insert_extra_lessons(
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


async def _insert_rooms_dreizuegig(
    session: AsyncSession,
    *,
    subjects_by_short: dict[str, Subject],
) -> None:
    """Insert the dreizügig room set and ADR 0044 external venues.

    Mirrors the per-room `RoomSubjectSuitability` shape from the
    one-room loop in the original seed body. Schwimmbad is marked
    `is_external=True` so the travel-buffer enforcement applies to any
    lesson placed in it.
    """
    for room_spec in _ROOMS_DREIZUEGIG:
        room = Room(
            name=room_spec.name,
            short_name=room_spec.short_name,
            capacity=room_spec.capacity,
            school_id=DEFAULT_SCHOOL_ID,
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
    for extra_room_spec in _EXTRA_ROOMS_DREIZUEGIG:
        extra_room = Room(
            name=extra_room_spec.name,
            short_name=extra_room_spec.short_name,
            capacity=extra_room_spec.capacity,
            is_external=extra_room_spec.is_external,
            school_id=DEFAULT_SCHOOL_ID,
        )
        session.add(extra_room)
        await session.flush()
        for subject_short in extra_room_spec.suitable_subject_short_names:
            session.add(
                RoomSubjectSuitability(
                    room_id=extra_room.id,
                    subject_id=subjects_by_short[subject_short].id,
                )
            )
    await session.flush()


async def _insert_extra_lessons(
    session: AsyncSession,
    *,
    classes_by_name: dict[str, SchoolClass],
    teachers_by_short: dict[str, Teacher],
    subjects_by_short: dict[str, Subject],
) -> None:
    """Insert ADR 0044 extra Lessons (Klasse 3a Schwimmen Doppelstunde).

    Mirrors the per-(class, subject) Lesson + LessonSchoolClass shape from
    `_insert_religion_trios` but for single-class rows, with the
    teacher pinned (lesson.teacher_id) and pre/post buffer minutes set.
    """
    for spec in _EXTRA_LESSONS_DREIZUEGIG:
        school_class = classes_by_name[spec.class_name]
        teacher = teachers_by_short[spec.teacher_short]
        lesson = Lesson(
            subject_id=subjects_by_short[spec.subject_short].id,
            teacher_id=teacher.id,
            hours_per_week=spec.hours_per_week,
            preferred_block_size=spec.preferred_block_size,
            pre_buffer_minutes=spec.pre_buffer_minutes,
            post_buffer_minutes=spec.post_buffer_minutes,
        )
        session.add(lesson)
        await session.flush()
        session.add(
            LessonSchoolClass(
                lesson_id=lesson.id,
                school_class_id=school_class.id,
            )
        )
    await session.flush()
