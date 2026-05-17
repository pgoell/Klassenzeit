"""Pure-function predicates over a generated schedule.

Used by integration tests and (eventually) by an admin-facing quality
endpoint to surface issues without re-deriving the predicate logic per
consumer.

`QualityIssue.kind` maps onto the backend-neutral `QualityReport`
(item 50) component vector exposed by `solver_core::quality_report`.
The two are not 1:1: predicates carry thresholds and per-class shape,
the report carries unweighted axis subtotals.

Mapping (QualityIssue.kind -> QualityReport field):

- imbalance      -> class_day_balance_cost. Same axis; predicate uses
  per-class spread vs. max_spread threshold.
- home_room_miss -> home_room_misses. Same axis; predicate uses
  per-class ratio vs. min_ratio threshold.
- interior_gap   -> class_gap_hours. Same axis; predicate uses
  per-class total-gaps threshold, report carries the global sum.
- day_too_long   -> avoid_last_units (loose). Closest soft component;
  predicate's max_position is sharper than the soft axis.
- room_hop       -> (none). Hard constraint pruned by
  solver_core::validate_no_room_hopping; no soft component.

The teacher-gap axis (`QualityReport.teacher_gap_hours`) and the four
subject-timing axes (`prefer_early_units`, `avoid_first_units`,
`avoid_last_units`, `prefer_late_units`) have no QualityIssue today;
add new `kind` literals if a future integration test or admin endpoint
needs to report on them.
"""

from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field
from typing import Literal
from uuid import UUID

from fastapi import HTTPException, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher
from klassenzeit_backend.db.models.week_scheme import TimeBlock, TimeBlockKind, WeekScheme
from klassenzeit_backend.scheduling.schemas.quality_report import QualityReportResponse


@dataclass(frozen=True)
class Placement:
    """A single scheduled lesson, projected for quality-check predicates.

    ``position`` is the lesson ordinal within the day (1-indexed; breaks
    skipped) as produced by ``build_lesson_ordinal_map``. ``time_block_position``
    is the raw ``TimeBlock.position`` so cell-emitting predicates can address
    the same coordinate the frontend grid uses.
    """

    class_id: UUID
    day: int
    subject_id: UUID
    room_id: UUID
    lesson_id: UUID
    time_block_id: UUID
    position: int
    time_block_position: int


@dataclass(frozen=True)
class QualityIssue:
    """Structured record of a quality-bar violation.

    ``cells`` is a tuple of ``(day_of_week, time_block_position)`` pairs
    addressing the offending grid cells the predicate located. Cell-emitting
    predicates (``room_hop``, ``home_room_miss``, ``day_too_long``,
    ``class_teacher_subject_share``) populate this sorted ascending by
    ``(day, time_block_position)``. Class-level predicates (``imbalance``,
    ``interior_gap``) leave it at the empty default.
    """

    kind: Literal[
        "room_hop",
        "imbalance",
        "home_room_miss",
        "day_too_long",
        "interior_gap",
        "class_teacher_subject_share",
    ]
    school_class_id: UUID
    day_of_week: int | None = None
    subject_id: UUID | None = None
    detail: dict[str, object] = field(default_factory=dict)
    cells: tuple[tuple[int, int], ...] = ()


def build_lesson_ordinal_map(
    time_blocks: Sequence[TimeBlock],
) -> dict[tuple[int, int], int]:
    """Map ``(day_of_week, raw position)`` to 1-based lesson ordinal per day.

    Skips break-kind rows. Production callers wrap raw ``Placement.position``
    via this map before invoking quality predicates so phantom interior gaps
    at break slots do not surface.
    """
    rows = sorted(
        (tb for tb in time_blocks if tb.kind == TimeBlockKind.LESSON),
        key=lambda tb: (tb.day_of_week, tb.position),
    )
    ordinals: dict[tuple[int, int], int] = {}
    per_day: dict[int, int] = {}
    for tb in rows:
        per_day[tb.day_of_week] = per_day.get(tb.day_of_week, 0) + 1
        ordinals[(tb.day_of_week, tb.position)] = per_day[tb.day_of_week]
    return ordinals


async def load_placements(db: AsyncSession, *, school_id: UUID) -> list[Placement]:
    """Project persisted ScheduledLesson rows into Placement records.

    A lesson can serve multiple classes via lesson_school_classes; each
    membership produces its own Placement so per-class predicates see
    every class the lesson lands in.

    ``Placement.position`` is the lesson ordinal within the day (1 = first
    lesson slot, 2 = second, ...); ``Placement.time_block_position`` is the
    raw ``TimeBlock.position``. Break rows occupy raw positions but not
    ordinal positions.

    Args:
        db: Active async database session.
        school_id: Tenant filter; restricts the TimeBlock ordinal map and the
            placement rows to a single school via a join through ``WeekScheme``.
    """
    rows = (
        await db.execute(
            select(
                ScheduledLesson.lesson_id,
                ScheduledLesson.time_block_id,
                ScheduledLesson.room_id,
                Lesson.subject_id,
                LessonSchoolClass.school_class_id,
                TimeBlock.day_of_week,
                TimeBlock.position,
            )
            .join(Lesson, Lesson.id == ScheduledLesson.lesson_id)
            .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
            .join(TimeBlock, TimeBlock.id == ScheduledLesson.time_block_id)
            .join(WeekScheme, WeekScheme.id == TimeBlock.week_scheme_id)
            .where(WeekScheme.school_id == school_id, Lesson.school_id == school_id)
        )
    ).all()
    time_blocks = (
        (
            await db.execute(
                select(TimeBlock)
                .join(WeekScheme, WeekScheme.id == TimeBlock.week_scheme_id)
                .where(WeekScheme.school_id == school_id)
            )
        )
        .scalars()
        .all()
    )
    lesson_ordinal_by_day_pos = build_lesson_ordinal_map(time_blocks)
    return [
        Placement(
            class_id=row.school_class_id,
            day=row.day_of_week,
            subject_id=row.subject_id,
            room_id=row.room_id,
            lesson_id=row.lesson_id,
            time_block_id=row.time_block_id,
            position=lesson_ordinal_by_day_pos[(row.day_of_week, row.position)],
            time_block_position=row.position,
        )
        for row in rows
    ]


def check_room_hop(placements: list[Placement]) -> Iterable[QualityIssue]:
    """Yield one issue per `(class, day, subject)` group spanning multiple rooms."""
    grouped: dict[tuple[UUID, int, UUID], set[UUID]] = {}
    members: dict[tuple[UUID, int, UUID], list[Placement]] = {}
    for placement in placements:
        key = (placement.class_id, placement.day, placement.subject_id)
        grouped.setdefault(key, set()).add(placement.room_id)
        members.setdefault(key, []).append(placement)
    for (class_id, day, subject_id), rooms in grouped.items():
        if len(rooms) <= 1:
            continue
        cells = tuple(
            sorted((p.day, p.time_block_position) for p in members[(class_id, day, subject_id)])
        )
        yield QualityIssue(
            kind="room_hop",
            school_class_id=class_id,
            day_of_week=day,
            subject_id=subject_id,
            detail={"rooms": [str(r) for r in rooms]},
            cells=cells,
        )


def check_class_day_balance(
    counts_per_class: dict[UUID, list[int]],
    max_spread: int = 2,
) -> Iterable[QualityIssue]:
    """Yield one issue per class whose daily-load spread exceeds `max_spread`."""
    for class_id, counts in counts_per_class.items():
        if not counts or max(counts) == 0:
            continue
        spread = max(counts) - min(counts)
        if spread <= max_spread:
            continue
        yield QualityIssue(
            kind="imbalance",
            school_class_id=class_id,
            detail={
                "daily": list(counts),
                "spread": spread,
                "max_spread": max_spread,
            },
        )


def check_home_room_ratio(
    placements: list[Placement],
    home_rooms: dict[UUID, UUID],
    min_ratio: float,
    exempt_subjects: set[UUID],
) -> Iterable[QualityIssue]:
    """Yield one issue per class whose non-exempt home-room hit rate is below `min_ratio`."""
    counts: dict[UUID, tuple[int, int]] = {}  # class_id -> (hits, total)
    non_home_by_class: dict[UUID, list[Placement]] = {}
    for placement in placements:
        if placement.subject_id in exempt_subjects:
            continue
        if placement.class_id not in home_rooms:
            continue
        hits, total = counts.get(placement.class_id, (0, 0))
        total += 1
        if placement.room_id == home_rooms[placement.class_id]:
            hits += 1
        else:
            non_home_by_class.setdefault(placement.class_id, []).append(placement)
        counts[placement.class_id] = (hits, total)
    for class_id, (hits, total) in counts.items():
        if total == 0:
            continue
        if hits / total >= min_ratio:
            continue
        cells = tuple(
            sorted((p.day, p.time_block_position) for p in non_home_by_class.get(class_id, []))
        )
        yield QualityIssue(
            kind="home_room_miss",
            school_class_id=class_id,
            detail={"hits": hits, "total": total, "min_ratio": min_ratio},
            cells=cells,
        )


def check_interior_gaps(
    positions_per_class_day: dict[tuple[UUID, int], list[int]],
    max_gaps_per_class: int,
) -> Iterable[QualityIssue]:
    """Yield one issue per class whose summed interior gaps exceed `max_gaps_per_class`.

    Gap count for one day = `last - first + 1 - len(unique_positions)`.
    Days with zero or one position contribute zero gaps.
    """
    totals: dict[UUID, int] = {}
    for (class_id, _day), positions in positions_per_class_day.items():
        unique = sorted(set(positions))
        if not unique:
            continue
        gaps = unique[-1] - unique[0] + 1 - len(unique)
        if gaps <= 0:
            continue
        totals[class_id] = totals.get(class_id, 0) + gaps
    for class_id, total_gaps in totals.items():
        if total_gaps <= max_gaps_per_class:
            continue
        yield QualityIssue(
            kind="interior_gap",
            school_class_id=class_id,
            detail={
                "total_gaps": total_gaps,
                "max_gaps_per_class": max_gaps_per_class,
            },
        )


def check_day_length(
    placements: list[Placement],
    max_position: int,
) -> Iterable[QualityIssue]:
    """Yield one issue per `(class, day)` containing a placement past `max_position`."""
    worst: dict[tuple[UUID, int], int] = {}
    offending: dict[tuple[UUID, int], list[Placement]] = {}
    for placement in placements:
        if placement.position <= max_position:
            continue
        key = (placement.class_id, placement.day)
        prev = worst.get(key, 0)
        if placement.position > prev:
            worst[key] = placement.position
        offending.setdefault(key, []).append(placement)
    for (class_id, day), worst_position in worst.items():
        cells = tuple(sorted((p.day, p.time_block_position) for p in offending[(class_id, day)]))
        yield QualityIssue(
            kind="day_too_long",
            school_class_id=class_id,
            day_of_week=day,
            detail={"max_position": max_position, "worst_position": worst_position},
            cells=cells,
        )


def check_class_teacher_subject_share(
    placements: list[Placement],
    class_teacher_lookup: dict[UUID, UUID | None],
    placement_teacher_lookup: dict[UUID, UUID],
) -> Iterable[QualityIssue]:
    """Yield one issue per `(class, subject)` pair whose teacher is not the class's Klassenlehrer.

    Classes with `class_teacher_id = None` are skipped (filtered out of
    the denominator). Pairs whose teacher matches Klassenlehrer yield
    nothing; pairs whose single teacher differs yield one issue carrying
    the offending teacher set.
    """
    by_pair: dict[tuple[UUID, UUID, UUID], set[UUID]] = {}
    offending_placements: dict[tuple[UUID, UUID, UUID], list[Placement]] = {}
    for placement in placements:
        klassenlehrer = class_teacher_lookup.get(placement.class_id)
        if klassenlehrer is None:
            continue
        teacher = placement_teacher_lookup.get(placement.lesson_id)
        if teacher is None:
            continue
        pair_key = (placement.class_id, placement.subject_id, klassenlehrer)
        by_pair.setdefault(pair_key, set()).add(teacher)
        if teacher != klassenlehrer:
            offending_placements.setdefault(pair_key, []).append(placement)
    for (class_id, subject_id, klassenlehrer), teachers in by_pair.items():
        if teachers == {klassenlehrer}:
            continue
        offending = sorted(t for t in teachers if t != klassenlehrer)
        cells = tuple(
            sorted(
                (p.day, p.time_block_position)
                for p in offending_placements.get((class_id, subject_id, klassenlehrer), [])
            )
        )
        yield QualityIssue(
            kind="class_teacher_subject_share",
            school_class_id=class_id,
            subject_id=subject_id,
            detail={
                "klassenlehrer": str(klassenlehrer),
                "teachers": [str(t) for t in sorted(teachers)],
                "offending": [str(t) for t in offending],
            },
            cells=cells,
        )


MAX_DAY_LOAD_SPREAD: int = 2
MAX_INTERIOR_GAPS_PER_CLASS: int = 2
MIN_HOME_ROOM_RATIO: float = 0.6
MAX_DAY_LENGTH_ORDINAL: int = 7
MIN_KLASSENLEHRER_SHARE: float = 0.5
HOME_ROOM_EXEMPT_SHORT_NAMES: frozenset[str] = frozenset({"SP", "KU", "MU"})
_WEEKDAY_INDEX_MAX: int = 4  # Mon=0..Fri=4; weekend placements skipped.


def _counts_per_class(placements: list[Placement]) -> dict[UUID, list[int]]:
    """Build per-class daily-count lists indexed by day 0..4."""
    counts: dict[UUID, list[int]] = {}
    for placement in placements:
        per_day = counts.setdefault(placement.class_id, [0, 0, 0, 0, 0])
        if 0 <= placement.day <= _WEEKDAY_INDEX_MAX:
            per_day[placement.day] += 1
    return counts


def _positions_per_class_day(
    placements: list[Placement],
) -> dict[tuple[UUID, int], list[int]]:
    """Group lesson-ordinal positions by `(class_id, day_of_week)`."""
    positions: dict[tuple[UUID, int], list[int]] = {}
    for placement in placements:
        positions.setdefault((placement.class_id, placement.day), []).append(placement.position)
    return positions


async def _load_class_teacher_lookup(db: AsyncSession, school_id: UUID) -> dict[UUID, UUID | None]:
    """Return `{school_class_id: class_teacher_id_or_none}` for the requesting school."""
    rows = (
        await db.execute(
            select(SchoolClass.id, SchoolClass.class_teacher_id).where(
                SchoolClass.school_id == school_id
            )
        )
    ).all()
    return {row.id: row.class_teacher_id for row in rows}


async def _load_placement_teacher_lookup(db: AsyncSession) -> dict[UUID, UUID]:
    """Return `{lesson_id: teacher_id}` over every persisted ScheduledLesson row."""
    rows = (await db.execute(select(ScheduledLesson.lesson_id, ScheduledLesson.teacher_id))).all()
    return {row.lesson_id: row.teacher_id for row in rows if row.teacher_id is not None}


async def _load_home_room_lookup(db: AsyncSession, school_id: UUID) -> dict[UUID, UUID]:
    """Return `{school_class_id: home_room_id}` for the school's classes with home_room set."""
    rows = (
        await db.execute(
            select(SchoolClass.id, SchoolClass.home_room_id).where(
                SchoolClass.school_id == school_id
            )
        )
    ).all()
    return {row.id: row.home_room_id for row in rows if row.home_room_id is not None}


async def _load_exempt_subjects(db: AsyncSession, school_id: UUID) -> set[UUID]:
    """Return the set of Subject IDs (in the given school) exempt from the home-room ratio.

    Exemption is keyed by ``short_name in HOME_ROOM_EXEMPT_SHORT_NAMES`` AND
    ``school_id == school_id`` so a cross-school subject with the same short
    name cannot leak into another tenant's exempt set.
    """
    rows = (
        (
            await db.execute(
                select(Subject.id).where(
                    Subject.short_name.in_(HOME_ROOM_EXEMPT_SHORT_NAMES),
                    Subject.school_id == school_id,
                )
            )
        )
        .scalars()
        .all()
    )
    return set(rows)


async def compute_quality_issues(
    db: AsyncSession,
    class_id: UUID,
    *,
    school_id: UUID,
) -> list[QualityIssue]:
    """Run all six quality predicates and return issues filtered to `class_id`.

    Loads placements once via `load_placements`, builds shared lookups,
    invokes each predicate with the module-level thresholds, concatenates
    results, and filters by `school_class_id`. Returns `[]` when no
    schedule exists for the database.

    The ``school_id`` kwarg scopes both the existence check (cross-school
    classes return 404) and the per-tenant loader helpers, so quality
    issues never leak between tenants.

    Each cell-emitting predicate already sorts its cells ascending by
    `(day, time_block_position)`, so the orchestrator does not re-sort.

    Raises:
        HTTPException: 404 if the class is missing or belongs to another school.
    """
    cls = (
        await db.execute(
            select(SchoolClass).where(
                SchoolClass.id == class_id, SchoolClass.school_id == school_id
            )
        )
    ).scalar_one_or_none()
    if cls is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Class not found")

    placements = await load_placements(db, school_id=school_id)
    if not placements:
        return []

    counts_per_class = _counts_per_class(placements)
    positions_per_class_day = _positions_per_class_day(placements)
    home_rooms = await _load_home_room_lookup(db, school_id)
    exempt_subjects = await _load_exempt_subjects(db, school_id)
    class_teacher_lookup = await _load_class_teacher_lookup(db, school_id)
    placement_teacher_lookup = await _load_placement_teacher_lookup(db)

    issues: list[QualityIssue] = []
    issues.extend(check_room_hop(placements))
    issues.extend(check_class_day_balance(counts_per_class, max_spread=MAX_DAY_LOAD_SPREAD))
    issues.extend(
        check_home_room_ratio(
            placements,
            home_rooms=home_rooms,
            min_ratio=MIN_HOME_ROOM_RATIO,
            exempt_subjects=exempt_subjects,
        )
    )
    issues.extend(
        check_interior_gaps(
            positions_per_class_day,
            max_gaps_per_class=MAX_INTERIOR_GAPS_PER_CLASS,
        )
    )
    issues.extend(check_day_length(placements, max_position=MAX_DAY_LENGTH_ORDINAL))
    issues.extend(
        check_class_teacher_subject_share(
            placements,
            class_teacher_lookup=class_teacher_lookup,
            placement_teacher_lookup=placement_teacher_lookup,
        )
    )
    return [issue for issue in issues if issue.school_class_id == class_id]


async def compute_quality_attribution_for_class(
    db: AsyncSession,
    class_id: UUID,
    *,
    school_id: UUID,
) -> QualityReportResponse:
    """Recompute per-class QualityReport attribution from persisted placements.

    Returns a ``QualityReportResponse`` with two derivable axes populated for
    ``class_id``:

    - ``class_gap_hours_by_class[class_id]`` = sum of interior gaps over
      this class's daily placement-position sets (gap count for one day =
      ``last - first + 1 - len(unique_positions)``).
    - ``home_room_misses_by_class[class_id]`` = count of non-exempt
      placements where ``placement.room_id`` differs from the class's
      ``home_room_id``.

    Skip-zero convention: a per-class total of 0 omits the key. The
    matching scalar (``class_gap_hours`` / ``home_room_misses``) equals
    the sum of the map's values.

    All other ``QualityReport`` fields default to neutral values the
    Pydantic mirror accepts: scalar ``int`` fields to ``0``, ``dict[str,
    int]`` fields to ``{}``. These are non-derivable from persisted
    placements (solver weights, soft-pin / supervision-spread inputs not
    persisted). The POST-time ``ScheduleResponse.quality_report`` from the
    solver carries authoritative values for those fields; the frontend
    prefers POST over GET when both are present.

    Returns an all-zero / empty-map report when the class has no
    placements yet.
    """
    placements = await load_placements(db, school_id=school_id)
    placements_for_class = [p for p in placements if p.class_id == class_id]

    # Per-day interior-gap accumulator for this class only.
    positions_per_day: dict[int, list[int]] = {}
    for p in placements_for_class:
        positions_per_day.setdefault(p.day, []).append(p.position)
    gap_total = 0
    for positions in positions_per_day.values():
        unique = sorted(set(positions))
        if not unique:
            continue
        gap_total += max(0, unique[-1] - unique[0] + 1 - len(unique))

    # Per-class home-room miss accumulator for this class only.
    home_rooms = await _load_home_room_lookup(db, school_id)
    exempt = await _load_exempt_subjects(db, school_id)
    home_room_id = home_rooms.get(class_id)
    miss_total = 0
    if home_room_id is not None:
        for p in placements_for_class:
            if p.subject_id in exempt:
                continue
            if p.room_id != home_room_id:
                miss_total += 1

    class_id_str = str(class_id)
    return QualityReportResponse(
        hard_violations=0,
        unplaced_hours=0,
        class_gap_hours=gap_total,
        class_gap_hours_by_class={class_id_str: gap_total} if gap_total > 0 else {},
        teacher_gap_hours=0,
        teacher_gap_hours_by_teacher={},
        class_day_balance_cost=0,
        class_day_balance_cost_by_class={},
        home_room_misses=miss_total,
        home_room_misses_by_class={class_id_str: miss_total} if miss_total > 0 else {},
        prefer_early_units=0,
        avoid_first_units=0,
        avoid_last_units=0,
        prefer_late_units=0,
        prefer_class_teacher_misses=0,
        weighted_score=0,
        worst_per_class_spread=0,
        worst_per_class_interior_gaps=0,
    )


async def compute_quality_attribution_for_teacher(
    db: AsyncSession,
    teacher_id: UUID,
    *,
    school_id: UUID,
) -> QualityReportResponse:
    """Recompute per-teacher QualityReport attribution from persisted placements.

    Returns a ``QualityReportResponse`` with one derivable axis populated:

    - ``teacher_gap_hours_by_teacher[teacher_id]`` = sum of interior gaps
      over this teacher's daily placement-position sets (gap count for one
      day = ``last - first + 1 - len(unique_positions)``).

    Skip-zero convention: a per-teacher total of 0 omits the key. The
    matching scalar (``teacher_gap_hours``) equals the sum of the map's
    values (here: just this teacher's value).

    Implementation reuses ``load_placements`` plus
    ``_load_placement_teacher_lookup`` (which filters out rows where
    ``ScheduledLesson.teacher_id is None``); placements without a teacher
    are excluded by canonical solver-side semantics.

    Per-day positions read ``Placement.position`` (the lesson ordinal,
    1-indexed via ``build_lesson_ordinal_map``), matching the class-side
    gap formula and the canonical convention pinned by ``backend/CLAUDE.md``.

    Combined-class lessons (one ScheduledLesson joined to N classes
    through LessonSchoolClass) produce N Placement rows at the same
    ``(day, position)``; ``set(positions)`` collapses them so the gap
    count matches the solver-core canonical computation.

    All other ``QualityReport`` fields default to neutral values: scalar
    ``int`` to ``0``, ``dict[str, int]`` to ``{}``. Returns an all-zero /
    empty-map report when the teacher has no placements yet.

    Raises:
        HTTPException: 404 if the teacher doesn't exist in the user's school.
    """
    teacher_row = (
        await db.execute(
            select(Teacher).where(Teacher.id == teacher_id, Teacher.school_id == school_id)
        )
    ).scalar_one_or_none()
    if teacher_row is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Teacher not found")

    placements = await load_placements(db, school_id=school_id)
    placement_teacher_lookup = await _load_placement_teacher_lookup(db)
    placements_for_teacher = [
        p for p in placements if placement_teacher_lookup.get(p.lesson_id) == teacher_id
    ]

    positions_per_day: dict[int, list[int]] = {}
    for p in placements_for_teacher:
        positions_per_day.setdefault(p.day, []).append(p.position)
    gap_total = 0
    for positions in positions_per_day.values():
        unique = sorted(set(positions))
        if not unique:
            continue
        gap_total += max(0, unique[-1] - unique[0] + 1 - len(unique))

    teacher_id_str = str(teacher_id)
    return QualityReportResponse(
        hard_violations=0,
        unplaced_hours=0,
        class_gap_hours=0,
        class_gap_hours_by_class={},
        teacher_gap_hours=gap_total,
        teacher_gap_hours_by_teacher={teacher_id_str: gap_total} if gap_total > 0 else {},
        class_day_balance_cost=0,
        class_day_balance_cost_by_class={},
        home_room_misses=0,
        home_room_misses_by_class={},
        prefer_early_units=0,
        avoid_first_units=0,
        avoid_last_units=0,
        prefer_late_units=0,
        prefer_class_teacher_misses=0,
        weighted_score=0,
        worst_per_class_spread=0,
        worst_per_class_interior_gaps=0,
    )
