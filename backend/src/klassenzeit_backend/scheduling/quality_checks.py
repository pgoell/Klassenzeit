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

from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import Literal
from uuid import UUID


@dataclass(frozen=True)
class Placement:
    """One placed lesson-hour, normalised for predicate evaluation."""

    class_id: UUID
    day: int
    subject_id: UUID
    room_id: UUID
    lesson_id: UUID
    time_block_id: UUID
    position: int


@dataclass(frozen=True)
class QualityIssue:
    """Structured record of a quality-bar violation."""

    kind: Literal["room_hop", "imbalance", "home_room_miss", "day_too_long", "interior_gap"]
    school_class_id: UUID
    day_of_week: int | None = None
    subject_id: UUID | None = None
    detail: dict[str, object] = field(default_factory=dict)


def check_room_hop(placements: list[Placement]) -> Iterable[QualityIssue]:
    """Yield one issue per `(class, day, subject)` group spanning multiple rooms."""
    grouped: dict[tuple[UUID, int, UUID], set[UUID]] = {}
    for placement in placements:
        key = (placement.class_id, placement.day, placement.subject_id)
        grouped.setdefault(key, set()).add(placement.room_id)
    for (class_id, day, subject_id), rooms in grouped.items():
        if len(rooms) <= 1:
            continue
        yield QualityIssue(
            kind="room_hop",
            school_class_id=class_id,
            day_of_week=day,
            subject_id=subject_id,
            detail={"rooms": [str(r) for r in rooms]},
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
    for placement in placements:
        if placement.subject_id in exempt_subjects:
            continue
        if placement.class_id not in home_rooms:
            continue
        hits, total = counts.get(placement.class_id, (0, 0))
        total += 1
        if placement.room_id == home_rooms[placement.class_id]:
            hits += 1
        counts[placement.class_id] = (hits, total)
    for class_id, (hits, total) in counts.items():
        if total == 0:
            continue
        if hits / total >= min_ratio:
            continue
        yield QualityIssue(
            kind="home_room_miss",
            school_class_id=class_id,
            detail={"hits": hits, "total": total, "min_ratio": min_ratio},
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
    for placement in placements:
        if placement.position <= max_position:
            continue
        key = (placement.class_id, placement.day)
        prev = worst.get(key, 0)
        if placement.position > prev:
            worst[key] = placement.position
    for (class_id, day), worst_position in worst.items():
        yield QualityIssue(
            kind="day_too_long",
            school_class_id=class_id,
            day_of_week=day,
            detail={"max_position": max_position, "worst_position": worst_position},
        )
