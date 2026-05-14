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

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.week_scheme import TimeBlock, TimeBlockKind


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


async def load_placements(db: AsyncSession) -> list[Placement]:
    """Project persisted ScheduledLesson rows into Placement records.

    A lesson can serve multiple classes via lesson_school_classes; each
    membership produces its own Placement so per-class predicates see
    every class the lesson lands in.

    ``Placement.position`` is the lesson ordinal within the day (1 = first
    lesson slot, 2 = second, ...); ``Placement.time_block_position`` is the
    raw ``TimeBlock.position``. Break rows occupy raw positions but not
    ordinal positions.
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
        )
    ).all()
    time_blocks = (await db.execute(select(TimeBlock))).scalars().all()
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
