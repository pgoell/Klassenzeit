"""Hessen Grundschule (zweizuegig) demo seed.

Two-Zug variant of ``demo_grundschule``: 8 classes (1a/1b..4b), 12
teachers, 12 rooms, same WeekScheme grid + Stundentafel hours as the
einzuegig seed.

The room count diverges from the bench-fixture-matrix plan's nominal
11 rooms by adding a second SP-suitable room (``Sportplatz``). One
Turnhalle for 8 classes saturates greedy first-fit on hour 2-3 of the
last-processed class's SP lesson; a second outdoor sports room mirrors
real Grundschule resource layout (most schools with 8+ classes have
both a hall and an outdoor pitch) and lifts the contention. Task 4 of
the matrix plan (Rust bench fixture) will mirror this 12-room layout.

The module reuses the einzuegig's NamedTuple specs and module-level
constants (``_PERIODS``, ``_DAYS_MON_TO_FRI``, ``_SUBJECTS``,
``_GRADE_1_2_HOURS``, ``_GRADE_3_4_HOURS``, ``_KLASSENRAUM_SUITABLE_SUBJECTS``,
and the ``_TeacherSpec`` / ``_RoomSpec`` / ``_SchoolClassSpec`` types) by
importing them from ``demo_grundschule``. The leading underscore is the
file-local visibility convention; both seed modules sit in the same
package, share authoring intent, and the bench-fixture matrix exists
specifically to keep these tables aligned. Copying would create the drift
this PR aims to prevent.

Lesson rows themselves are not produced by this seed; they are created
by ``POST /api/classes/{id}/generate-lessons`` in the route layer. The
``_TEACHER_ASSIGNMENTS_ZWEIZUEGIG`` dict declares the canonical
``(class_name, subject_short)`` -> teacher ``short_code`` mapping that
the matching solvability test pins onto every Lesson (overriding
auto-assign for determinism), and that the Rust bench fixture mirrors.
"""

from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.room import Room, RoomSubjectSuitability
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import Stundentafel, StundentafelEntry
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.db.models.week_scheme import TimeBlock, WeekScheme
from klassenzeit_backend.seed.demo_grundschule import (
    _DAYS_MON_TO_FRI,
    _GRADE_1_2_HOURS,
    _GRADE_3_4_HOURS,
    _KLASSENRAUM_SUITABLE_SUBJECTS,
    _PERIODS,
    _SUBJECTS,
    _assign_eponymous_home_rooms,
    _RoomSpec,
    _SchoolClassSpec,
    _TeacherSpec,
)

WEEK_SCHEME_NAME = "Grundschule (zweizuegig) Zeitraster"
WEEK_SCHEME_DESCRIPTION = (
    "Hessen Grundschule, zwei Zuege pro Jahrgang: 5 Tage, 7 Stunden a 45 Minuten, "
    "Hofpausen nach der 2. und 4. Stunde. Stunde 7 dient als Ganztags- / "
    "AG-Zeitfenster und gibt dem Solver Slack fuer volle Stundentafeln."
)


_TEACHERS_ZWEIZUEGIG: tuple[_TeacherSpec, ...] = (
    _TeacherSpec("Anna", "Mueller", "MUE", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Beate", "Schmidt", "SCH", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Carsten", "Weber", "WEB", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Dana", "Fischer", "FIS", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Erik", "Klein", "KAI", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Frieda", "Lange", "LAN", 28, ("D", "M", "SU", "KU")),
    _TeacherSpec("Gustav", "Neumann", "NEU", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Helga", "Otto", "OTT", 28, ("D", "M", "SU", "E")),
    _TeacherSpec("Eva", "Becker", "BEC", 18, ("RK", "RE", "ETH", "MU", "FÖ")),
    _TeacherSpec("Frank", "Hoffmann", "HOF", 21, ("SP", "KU", "FÖ")),
    _TeacherSpec("Iris", "Wilhelm", "WIL", 14, ("RK", "RE", "ETH", "MU")),
    _TeacherSpec("Juergen", "Richter", "RIC", 21, ("SP", "FÖ")),
)


_ROOMS_ZWEIZUEGIG: tuple[_RoomSpec, ...] = (
    _RoomSpec("Klasse 1a", "1a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 1b", "1b", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 2a", "2a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 2b", "2b", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 3a", "3a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 3b", "3b", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 4a", "4a", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Klasse 4b", "4b", 25, _KLASSENRAUM_SUITABLE_SUBJECTS),
    _RoomSpec("Turnhalle", "TH", None, ("SP",)),
    _RoomSpec("Sportplatz", "SP-P", None, ("SP",)),
    _RoomSpec("Musikraum", "MU-R", 30, ("MU",)),
    _RoomSpec("Kunstraum", "KU-R", 20, ("KU",)),
)


_SCHOOL_CLASSES_ZWEIZUEGIG: tuple[_SchoolClassSpec, ...] = (
    _SchoolClassSpec("1a", 1),
    _SchoolClassSpec("1b", 1),
    _SchoolClassSpec("2a", 2),
    _SchoolClassSpec("2b", 2),
    _SchoolClassSpec("3a", 3),
    _SchoolClassSpec("3b", 3),
    _SchoolClassSpec("4a", 4),
    _SchoolClassSpec("4b", 4),
)


# Authored ``(class_name, subject_short)`` -> teacher ``short_code`` mapping.
# Walks subjects in scarcity order per class, capacity-fitting greedily.
# Every class takes ETH (single-Zug demos can't model the kath/ev/Ethik split;
# the dreizuegig seed is the first variant that exercises RK / RE / ETH per
# class). The substitution RE -> ETH is hour-neutral (still 2h per class), so
# per-teacher totals match the previous einzuegig-style pattern exactly.
# Per-teacher hour totals (verified against ``_TEACHERS_ZWEIZUEGIG.max_hours_per_week``):
#   MUE = 1a D+M+SU+KU (6+5+2+2) + 2b KU (2) + 4a KU (2)                 = 19h <= 28
#   SCH = 1b D+M+SU+KU (6+5+2+2) + 4b KU (2)                             = 17h <= 28
#   WEB = 2a D+M+SU (6+5+2)      + 3a E  (2)                             = 15h <= 28
#   FIS = 2b D+M+SU (6+5+2)      + 3b E  (2)                             = 15h <= 28
#   KAI = 3a D+M+SU+KU (5+5+4+2)                                         = 16h <= 28
#   LAN = 3b D+M+SU+KU (5+5+4+2)                                         = 16h <= 28
#   NEU = 4a D+M+SU+E  (5+5+4+2)                                         = 16h <= 28
#   OTT = 4b D+M+SU+E  (5+5+4+2)                                         = 16h <= 28
#   BEC = 1a ETH+MU (2+1) + 2a ETH+MU+FOE (2+1+2) + 3a ETH+MU+FOE (2+1+2)
#         + 4a ETH+MU (2+1)                                              = 16h <= 18
#   HOF = 1a SP+FOE (3+2) + 2a KU+SP (2+3) + 3a SP (3) + 4a SP+FOE (3+2) = 18h <= 21
#   WIL = 1b ETH+MU (2+1) + 2b ETH+MU (2+1) + 3b ETH+MU (2+1)
#         + 4b ETH+MU (2+1)                                              = 12h <= 14
#   RIC = 1b SP+FOE (3+2) + 2b SP+FOE (3+2) + 3b SP+FOE (3+2)
#         + 4b SP+FOE (3+2)                                              = 20h <= 21
_TEACHER_ASSIGNMENTS_ZWEIZUEGIG: dict[tuple[str, str], str] = {
    # Class 1a (grade 1: D6 + M5 + SU2 + ETH2 + KU2 + MU1 + SP3 + FOE2 = 23h)
    ("1a", "D"): "MUE",
    ("1a", "M"): "MUE",
    ("1a", "SU"): "MUE",
    ("1a", "KU"): "MUE",
    ("1a", "ETH"): "BEC",
    ("1a", "MU"): "BEC",
    ("1a", "SP"): "HOF",
    ("1a", "FÖ"): "HOF",
    # Class 1b (grade 1, mirror)
    ("1b", "D"): "SCH",
    ("1b", "M"): "SCH",
    ("1b", "SU"): "SCH",
    ("1b", "KU"): "SCH",
    ("1b", "ETH"): "WIL",
    ("1b", "MU"): "WIL",
    ("1b", "SP"): "RIC",
    ("1b", "FÖ"): "RIC",
    # Class 2a (grade 2)
    ("2a", "D"): "WEB",
    ("2a", "M"): "WEB",
    ("2a", "SU"): "WEB",
    ("2a", "KU"): "HOF",
    ("2a", "ETH"): "BEC",
    ("2a", "MU"): "BEC",
    ("2a", "SP"): "HOF",
    ("2a", "FÖ"): "BEC",
    # Class 2b (grade 2)
    ("2b", "D"): "FIS",
    ("2b", "M"): "FIS",
    ("2b", "SU"): "FIS",
    ("2b", "KU"): "MUE",
    ("2b", "ETH"): "WIL",
    ("2b", "MU"): "WIL",
    ("2b", "SP"): "RIC",
    ("2b", "FÖ"): "RIC",
    # Class 3a (grade 3: D5 + M5 + SU4 + E2 + ETH2 + KU2 + MU1 + SP3 + FOE2 = 26h)
    ("3a", "D"): "KAI",
    ("3a", "M"): "KAI",
    ("3a", "SU"): "KAI",
    ("3a", "KU"): "KAI",
    ("3a", "E"): "WEB",
    ("3a", "ETH"): "BEC",
    ("3a", "MU"): "BEC",
    ("3a", "SP"): "HOF",
    ("3a", "FÖ"): "BEC",
    # Class 3b (grade 3)
    ("3b", "D"): "LAN",
    ("3b", "M"): "LAN",
    ("3b", "SU"): "LAN",
    ("3b", "KU"): "LAN",
    ("3b", "E"): "FIS",
    ("3b", "ETH"): "WIL",
    ("3b", "MU"): "WIL",
    ("3b", "SP"): "RIC",
    ("3b", "FÖ"): "RIC",
    # Class 4a (grade 4)
    ("4a", "D"): "NEU",
    ("4a", "M"): "NEU",
    ("4a", "SU"): "NEU",
    ("4a", "E"): "NEU",
    ("4a", "KU"): "MUE",
    ("4a", "ETH"): "BEC",
    ("4a", "MU"): "BEC",
    ("4a", "SP"): "HOF",
    ("4a", "FÖ"): "HOF",
    # Class 4b (grade 4)
    ("4b", "D"): "OTT",
    ("4b", "M"): "OTT",
    ("4b", "SU"): "OTT",
    ("4b", "E"): "OTT",
    ("4b", "KU"): "SCH",
    ("4b", "ETH"): "WIL",
    ("4b", "MU"): "WIL",
    ("4b", "SP"): "RIC",
    ("4b", "FÖ"): "RIC",
}


async def seed_demo_grundschule_zweizuegig(session: AsyncSession) -> None:
    """Seed a realistic zweizuegige Hessen Grundschule into ``session``.

    Caller owns the transaction: this coroutine only ``flush()``es so FK
    lookups resolve. Commit once at the end, or rollback on error.

    The ``_TEACHER_ASSIGNMENTS_ZWEIZUEGIG`` dict is consumed by the
    matching solvability test (which pins ``Lesson.teacher_id`` after
    ``generate-lessons`` runs); the seed coroutine itself only populates
    entities up to the room/teacher/qualification layer.

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

    for class_spec in _SCHOOL_CLASSES_ZWEIZUEGIG:
        session.add(
            SchoolClass(
                name=class_spec.name,
                grade_level=class_spec.grade_level,
                stundentafel_id=tafeln_by_grade[class_spec.grade_level].id,
                week_scheme_id=week_scheme.id,
            )
        )
    await session.flush()

    for teacher_spec in _TEACHERS_ZWEIZUEGIG:
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

    for room_spec in _ROOMS_ZWEIZUEGIG:
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

    await _assign_eponymous_home_rooms(session, {spec.name for spec in _SCHOOL_CLASSES_ZWEIZUEGIG})
