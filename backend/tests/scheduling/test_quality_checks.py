"""Unit tests for the schedule quality_checks predicate module.

The predicates are pure functions over normalised placement / position
inputs. No DB, no fixtures, just UUID-keyed dicts and lists.
"""

import dataclasses
import datetime as dt
import uuid
from uuid import UUID

import pytest

from klassenzeit_backend.db.models.week_scheme import TimeBlock, TimeBlockKind
from klassenzeit_backend.scheduling.quality_checks import (
    Placement,
    QualityIssue,
    build_lesson_ordinal_map,
    check_class_day_balance,
    check_class_teacher_subject_share,
    check_day_length,
    check_home_room_ratio,
    check_interior_gaps,
    check_room_hop,
)


def _placement(
    class_id: UUID,
    day: int,
    subject_id: UUID,
    room_id: UUID,
    *,
    lesson_id: UUID | None = None,
    time_block_id: UUID | None = None,
    position: int = 1,
) -> Placement:
    return Placement(
        class_id=class_id,
        day=day,
        subject_id=subject_id,
        room_id=room_id,
        lesson_id=lesson_id or uuid.uuid4(),
        time_block_id=time_block_id or uuid.uuid4(),
        position=position,
        time_block_position=position,
    )


# ---------------------------------------------------------------------------
# check_room_hop
# ---------------------------------------------------------------------------


def test_check_room_hop_returns_issue_for_two_rooms_one_subject_one_day() -> None:
    c1 = uuid.uuid4()
    deutsch = uuid.uuid4()
    room_a = uuid.uuid4()
    room_b = uuid.uuid4()
    placements = [
        _placement(c1, 0, deutsch, room_a, position=1),
        _placement(c1, 0, deutsch, room_b, position=4),
    ]
    issues = list(check_room_hop(placements))
    assert len(issues) == 1
    assert issues[0].kind == "room_hop"
    assert issues[0].school_class_id == c1
    assert issues[0].day_of_week == 0
    assert issues[0].subject_id == deutsch
    rooms = issues[0].detail["rooms"]
    assert isinstance(rooms, list)
    assert sorted(rooms) == sorted([str(room_a), str(room_b)])


def test_check_room_hop_returns_empty_for_single_room() -> None:
    c1 = uuid.uuid4()
    deutsch = uuid.uuid4()
    room_a = uuid.uuid4()
    placements = [
        _placement(c1, 0, deutsch, room_a, position=1),
        _placement(c1, 0, deutsch, room_a, position=4),
    ]
    assert list(check_room_hop(placements)) == []


def test_check_room_hop_returns_empty_for_empty_input() -> None:
    assert list(check_room_hop([])) == []


def test_check_room_hop_ignores_single_placement_groups() -> None:
    c1 = uuid.uuid4()
    deutsch = uuid.uuid4()
    room_a = uuid.uuid4()
    placements = [_placement(c1, 0, deutsch, room_a, position=1)]
    assert list(check_room_hop(placements)) == []


def test_check_room_hop_does_not_cross_classes_subjects_or_days() -> None:
    c1 = uuid.uuid4()
    c2 = uuid.uuid4()
    deutsch = uuid.uuid4()
    mathe = uuid.uuid4()
    room_a = uuid.uuid4()
    room_b = uuid.uuid4()
    # Different classes (same subject, same day, different rooms): fine.
    # Different subjects (same class, same day, different rooms): fine.
    # Different days (same class, same subject, different rooms): fine.
    placements = [
        _placement(c1, 0, deutsch, room_a),
        _placement(c2, 0, deutsch, room_b),
        _placement(c1, 0, mathe, room_b),
        _placement(c1, 1, deutsch, room_b),
    ]
    assert list(check_room_hop(placements)) == []


# ---------------------------------------------------------------------------
# check_class_day_balance
# ---------------------------------------------------------------------------


def test_check_class_day_balance_returns_issue_when_spread_exceeds_limit() -> None:
    c1 = uuid.uuid4()
    counts = {c1: [6, 6, 2, 6, 6]}  # spread = 4
    issues = list(check_class_day_balance(counts, max_spread=2))
    assert len(issues) == 1
    issue = issues[0]
    assert issue.kind == "imbalance"
    assert issue.school_class_id == c1
    assert issue.detail["daily"] == [6, 6, 2, 6, 6]
    assert issue.detail["spread"] == 4
    assert issue.detail["max_spread"] == 2


def test_check_class_day_balance_clean_for_spread_within_limit() -> None:
    c1 = uuid.uuid4()
    counts = {c1: [5, 5, 4, 5, 5]}  # spread = 1
    assert list(check_class_day_balance(counts, max_spread=2)) == []


def test_check_class_day_balance_skips_all_zero_counts() -> None:
    c1 = uuid.uuid4()
    counts = {c1: [0, 0, 0, 0, 0]}
    assert list(check_class_day_balance(counts, max_spread=2)) == []


def test_check_class_day_balance_handles_empty_input() -> None:
    assert list(check_class_day_balance({}, max_spread=2)) == []


# ---------------------------------------------------------------------------
# check_home_room_ratio
# ---------------------------------------------------------------------------


def test_check_home_room_ratio_flags_class_below_threshold() -> None:
    c1 = uuid.uuid4()
    home = uuid.uuid4()
    other = uuid.uuid4()
    deutsch = uuid.uuid4()
    sport = uuid.uuid4()  # exempt
    placements = [
        _placement(c1, 0, deutsch, other, position=1),
        _placement(c1, 0, deutsch, other, position=2),
        _placement(c1, 0, deutsch, home, position=3),
        _placement(c1, 1, sport, other, position=1),  # exempt, ignored
    ]
    issues = list(
        check_home_room_ratio(
            placements,
            home_rooms={c1: home},
            min_ratio=0.8,
            exempt_subjects={sport},
        )
    )
    assert len(issues) == 1
    assert issues[0].kind == "home_room_miss"
    assert issues[0].school_class_id == c1
    # 1 hit / 3 non-exempt placements = 0.333
    assert issues[0].detail["hits"] == 1
    assert issues[0].detail["total"] == 3
    assert issues[0].detail["min_ratio"] == 0.8


def test_check_home_room_ratio_clean_when_above_threshold() -> None:
    c1 = uuid.uuid4()
    home = uuid.uuid4()
    other = uuid.uuid4()
    deutsch = uuid.uuid4()
    placements = [
        _placement(c1, 0, deutsch, home, position=1),
        _placement(c1, 0, deutsch, home, position=2),
        _placement(c1, 0, deutsch, home, position=3),
        _placement(c1, 1, deutsch, other, position=1),
    ]
    # 3/4 = 0.75; threshold 0.7
    assert (
        list(
            check_home_room_ratio(
                placements,
                home_rooms={c1: home},
                min_ratio=0.7,
                exempt_subjects=set(),
            )
        )
        == []
    )


def test_check_home_room_ratio_skips_class_without_home_room() -> None:
    c1 = uuid.uuid4()
    other = uuid.uuid4()
    deutsch = uuid.uuid4()
    placements = [
        _placement(c1, 0, deutsch, other, position=1),
        _placement(c1, 0, deutsch, other, position=2),
    ]
    assert (
        list(
            check_home_room_ratio(
                placements,
                home_rooms={},
                min_ratio=0.9,
                exempt_subjects=set(),
            )
        )
        == []
    )


def test_check_home_room_ratio_skips_class_with_only_exempt_placements() -> None:
    c1 = uuid.uuid4()
    home = uuid.uuid4()
    gym = uuid.uuid4()
    sport = uuid.uuid4()
    placements = [
        _placement(c1, 0, sport, gym, position=1),
        _placement(c1, 1, sport, gym, position=1),
    ]
    assert (
        list(
            check_home_room_ratio(
                placements,
                home_rooms={c1: home},
                min_ratio=0.9,
                exempt_subjects={sport},
            )
        )
        == []
    )


# ---------------------------------------------------------------------------
# check_interior_gaps
# ---------------------------------------------------------------------------


def test_check_interior_gaps_flags_class_with_too_many_gaps() -> None:
    c1 = uuid.uuid4()
    # day 0: positions 1,2,5  -> last-first+1-count = 5-1+1-3 = 2
    # day 1: positions 1,4    -> 4-1+1-2 = 2
    positions = {(c1, 0): [1, 2, 5], (c1, 1): [1, 4]}
    issues = list(check_interior_gaps(positions, max_gaps_per_class=2))
    assert len(issues) == 1
    assert issues[0].kind == "interior_gap"
    assert issues[0].school_class_id == c1
    assert issues[0].detail["total_gaps"] == 4
    assert issues[0].detail["max_gaps_per_class"] == 2


def test_check_interior_gaps_clean_when_within_budget() -> None:
    c1 = uuid.uuid4()
    positions = {(c1, 0): [1, 2, 3, 4], (c1, 1): [1, 2, 3]}
    assert list(check_interior_gaps(positions, max_gaps_per_class=2)) == []


def test_check_interior_gaps_handles_empty_and_single_position_days() -> None:
    c1 = uuid.uuid4()
    positions = {(c1, 0): [], (c1, 1): [3]}
    assert list(check_interior_gaps(positions, max_gaps_per_class=0)) == []


def test_check_interior_gaps_deduplicates_doubled_block_positions() -> None:
    c1 = uuid.uuid4()
    # A 2-hour block places two rows at the same position. The dedup'd
    # position list is [1, 2], which has zero gaps.
    positions = {(c1, 0): [1, 1, 2, 2]}
    assert list(check_interior_gaps(positions, max_gaps_per_class=0)) == []


# ---------------------------------------------------------------------------
# check_day_length
# ---------------------------------------------------------------------------


def test_check_day_length_flags_placements_past_max_position() -> None:
    c1 = uuid.uuid4()
    deutsch = uuid.uuid4()
    room = uuid.uuid4()
    placements = [
        _placement(c1, 0, deutsch, room, position=4),
        _placement(c1, 0, deutsch, room, position=7),  # > 6
        _placement(c1, 1, deutsch, room, position=8),  # > 6, separate day
    ]
    issues = list(check_day_length(placements, max_position=6))
    assert len(issues) == 2
    assert all(issue.kind == "day_too_long" for issue in issues)
    by_day = {issue.day_of_week: issue for issue in issues}
    assert by_day[0].detail["max_position"] == 6
    assert by_day[0].detail["worst_position"] == 7
    assert by_day[1].detail["worst_position"] == 8


def test_check_day_length_clean_when_all_within_limit() -> None:
    c1 = uuid.uuid4()
    deutsch = uuid.uuid4()
    room = uuid.uuid4()
    placements = [
        _placement(c1, 0, deutsch, room, position=1),
        _placement(c1, 0, deutsch, room, position=6),
    ]
    assert list(check_day_length(placements, max_position=6)) == []


def test_check_day_length_empty_input() -> None:
    assert list(check_day_length([], max_position=6)) == []


def test_check_day_length_collapses_multiple_violations_per_day_into_one_issue() -> None:
    c1 = uuid.uuid4()
    deutsch = uuid.uuid4()
    room = uuid.uuid4()
    placements = [
        _placement(c1, 0, deutsch, room, position=7),
        _placement(c1, 0, deutsch, room, position=8),
    ]
    issues = list(check_day_length(placements, max_position=6))
    assert len(issues) == 1
    assert issues[0].detail["worst_position"] == 8


# ---------------------------------------------------------------------------
# QualityIssue dataclass sanity
# ---------------------------------------------------------------------------


def test_quality_issue_is_frozen_dataclass() -> None:
    issue = QualityIssue(kind="room_hop", school_class_id=uuid.uuid4())
    assert issue.detail == {}
    with pytest.raises(dataclasses.FrozenInstanceError):
        # Frozen dataclasses reject attribute assignment at runtime; use the
        # `setattr` builtin so static type checkers don't reject the test itself.
        setattr(issue, "day_of_week", 1)  # noqa: B010


# ---------------------------------------------------------------------------
# check_class_teacher_subject_share
# ---------------------------------------------------------------------------


def test_check_class_teacher_subject_share_yields_nothing_when_no_class_has_klassenlehrer() -> None:
    c1 = uuid.uuid4()
    subj = uuid.uuid4()
    room = uuid.uuid4()
    lesson = uuid.uuid4()
    teacher = uuid.uuid4()
    placements = [
        _placement(c1, 0, subj, room, lesson_id=lesson, position=1),
    ]
    class_teacher_lookup: dict[UUID, UUID | None] = {c1: None}
    placement_teacher_lookup: dict[UUID, UUID] = {lesson: teacher}
    issues = list(
        check_class_teacher_subject_share(
            placements, class_teacher_lookup, placement_teacher_lookup
        )
    )
    assert issues == []


def test_check_class_teacher_subject_share_yields_nothing_when_all_pairs_match() -> None:
    c1 = uuid.uuid4()
    mathe = uuid.uuid4()
    deutsch = uuid.uuid4()
    room = uuid.uuid4()
    lesson_m = uuid.uuid4()
    lesson_d = uuid.uuid4()
    klassenlehrer = uuid.uuid4()
    placements = [
        _placement(c1, 0, mathe, room, lesson_id=lesson_m, position=1),
        _placement(c1, 1, deutsch, room, lesson_id=lesson_d, position=2),
    ]
    class_teacher_lookup: dict[UUID, UUID | None] = {c1: klassenlehrer}
    placement_teacher_lookup: dict[UUID, UUID] = {lesson_m: klassenlehrer, lesson_d: klassenlehrer}
    issues = list(
        check_class_teacher_subject_share(
            placements, class_teacher_lookup, placement_teacher_lookup
        )
    )
    assert issues == []


def test_check_class_teacher_subject_share_yields_issue_per_mismatched_pair() -> None:
    c1 = uuid.uuid4()
    mathe = uuid.uuid4()
    deutsch = uuid.uuid4()
    room = uuid.uuid4()
    lesson_m = uuid.uuid4()
    lesson_d = uuid.uuid4()
    klassenlehrer = uuid.uuid4()
    other = uuid.uuid4()
    placements = [
        _placement(c1, 0, mathe, room, lesson_id=lesson_m, position=1),
        _placement(c1, 1, deutsch, room, lesson_id=lesson_d, position=2),
    ]
    class_teacher_lookup: dict[UUID, UUID | None] = {c1: klassenlehrer}
    placement_teacher_lookup: dict[UUID, UUID] = {lesson_m: klassenlehrer, lesson_d: other}
    issues = list(
        check_class_teacher_subject_share(
            placements, class_teacher_lookup, placement_teacher_lookup
        )
    )
    assert len(issues) == 1
    assert issues[0].kind == "class_teacher_subject_share"
    assert issues[0].school_class_id == c1
    assert issues[0].subject_id == deutsch


def test_check_class_teacher_subject_share_skips_classes_without_klassenlehrer() -> None:
    c1 = uuid.uuid4()
    c2 = uuid.uuid4()
    mathe = uuid.uuid4()
    room = uuid.uuid4()
    lesson_c1 = uuid.uuid4()
    lesson_c2 = uuid.uuid4()
    klassenlehrer_c1 = uuid.uuid4()
    other_teacher = uuid.uuid4()
    placements = [
        _placement(c1, 0, mathe, room, lesson_id=lesson_c1, position=1),
        _placement(c2, 0, mathe, room, lesson_id=lesson_c2, position=2),
    ]
    class_teacher_lookup: dict[UUID, UUID | None] = {c1: klassenlehrer_c1, c2: None}
    placement_teacher_lookup: dict[UUID, UUID] = {
        lesson_c1: klassenlehrer_c1,
        lesson_c2: other_teacher,
    }
    issues = list(
        check_class_teacher_subject_share(
            placements, class_teacher_lookup, placement_teacher_lookup
        )
    )
    assert issues == [], "c2 has no Klassenlehrer set, must not yield issues; c1's pair matches"


# ---------------------------------------------------------------------------
# build_lesson_ordinal_map
# ---------------------------------------------------------------------------


def test_build_lesson_ordinal_map_skips_break_rows() -> None:
    ws_id = uuid.uuid4()
    blocks = [
        TimeBlock(
            id=uuid.uuid4(),
            week_scheme_id=ws_id,
            day_of_week=0,
            position=1,
            start_time=dt.time(8, 0),
            end_time=dt.time(8, 45),
            kind=TimeBlockKind.LESSON,
        ),
        TimeBlock(
            id=uuid.uuid4(),
            week_scheme_id=ws_id,
            day_of_week=0,
            position=2,
            start_time=dt.time(8, 45),
            end_time=dt.time(9, 30),
            kind=TimeBlockKind.LESSON,
        ),
        TimeBlock(
            id=uuid.uuid4(),
            week_scheme_id=ws_id,
            day_of_week=0,
            position=3,
            start_time=dt.time(9, 30),
            end_time=dt.time(9, 50),
            kind=TimeBlockKind.BREAK,
        ),
        TimeBlock(
            id=uuid.uuid4(),
            week_scheme_id=ws_id,
            day_of_week=0,
            position=4,
            start_time=dt.time(9, 50),
            end_time=dt.time(10, 35),
            kind=TimeBlockKind.LESSON,
        ),
    ]
    ordinals = build_lesson_ordinal_map(blocks)
    assert ordinals == {(0, 1): 1, (0, 2): 2, (0, 4): 3}


def test_placement_carries_time_block_position() -> None:
    """Placement now exposes the raw TimeBlock.position alongside the ordinal."""
    placement = Placement(
        class_id=uuid.uuid4(),
        day=1,
        subject_id=uuid.uuid4(),
        room_id=uuid.uuid4(),
        lesson_id=uuid.uuid4(),
        time_block_id=uuid.uuid4(),
        position=2,
        time_block_position=4,
    )
    assert placement.position == 2
    assert placement.time_block_position == 4


# ---------------------------------------------------------------------------
# QualityIssue.cells field + per-predicate population
# ---------------------------------------------------------------------------


def test_quality_issue_cells_defaults_to_empty_tuple() -> None:
    issue = QualityIssue(kind="imbalance", school_class_id=uuid.uuid4(), detail={})
    assert issue.cells == ()


def test_check_room_hop_cells_carries_all_matching_placements() -> None:
    """room_hop emits one issue per (class, day, subject); cells contains every
    matching placement sorted ascending by (day, time_block_position)."""
    class_id = uuid.uuid4()
    subject_id = uuid.uuid4()
    room_a = uuid.uuid4()
    room_b = uuid.uuid4()
    lesson_id = uuid.uuid4()

    placements = [
        # Insert in reverse so the predicate must sort.
        Placement(
            class_id=class_id,
            day=1,
            subject_id=subject_id,
            room_id=room_b,
            lesson_id=lesson_id,
            time_block_id=uuid.uuid4(),
            position=2,
            time_block_position=4,
        ),
        Placement(
            class_id=class_id,
            day=1,
            subject_id=subject_id,
            room_id=room_a,
            lesson_id=lesson_id,
            time_block_id=uuid.uuid4(),
            position=1,
            time_block_position=2,
        ),
    ]
    issues = list(check_room_hop(placements))
    assert len(issues) == 1
    assert issues[0].kind == "room_hop"
    assert issues[0].cells == ((1, 2), (1, 4))


def test_check_home_room_ratio_cells_carries_non_home_placements() -> None:
    """home_room_miss cells include every non-exempt placement that lands
    outside the class's home room, sorted by (day, time_block_position)."""
    c1 = uuid.uuid4()
    home = uuid.uuid4()
    other = uuid.uuid4()
    deutsch = uuid.uuid4()
    sport = uuid.uuid4()  # exempt

    placements = [
        # Insert in reverse to confirm sorting.
        Placement(
            class_id=c1,
            day=2,
            subject_id=deutsch,
            room_id=other,
            lesson_id=uuid.uuid4(),
            time_block_id=uuid.uuid4(),
            position=1,
            time_block_position=3,
        ),
        Placement(
            class_id=c1,
            day=1,
            subject_id=deutsch,
            room_id=other,
            lesson_id=uuid.uuid4(),
            time_block_id=uuid.uuid4(),
            position=2,
            time_block_position=5,
        ),
        # Exempt placement in a non-home room must NOT appear in cells.
        Placement(
            class_id=c1,
            day=0,
            subject_id=sport,
            room_id=other,
            lesson_id=uuid.uuid4(),
            time_block_id=uuid.uuid4(),
            position=1,
            time_block_position=1,
        ),
        # A placement in the home room must NOT appear in cells.
        Placement(
            class_id=c1,
            day=0,
            subject_id=deutsch,
            room_id=home,
            lesson_id=uuid.uuid4(),
            time_block_id=uuid.uuid4(),
            position=2,
            time_block_position=2,
        ),
    ]
    issues = list(
        check_home_room_ratio(
            placements,
            home_rooms={c1: home},
            min_ratio=0.9,
            exempt_subjects={sport},
        )
    )
    assert len(issues) == 1
    assert issues[0].kind == "home_room_miss"
    assert issues[0].cells == ((1, 5), (2, 3))


def test_check_day_length_cells_carries_placements_past_max_position() -> None:
    """day_too_long cells include every placement with ordinal position past
    max_position, sorted by (day, time_block_position) and using raw positions."""
    c1 = uuid.uuid4()
    deutsch = uuid.uuid4()
    room = uuid.uuid4()
    placements = [
        # ordinal 8 (raw 9), should appear.
        Placement(
            class_id=c1,
            day=0,
            subject_id=deutsch,
            room_id=room,
            lesson_id=uuid.uuid4(),
            time_block_id=uuid.uuid4(),
            position=8,
            time_block_position=9,
        ),
        # ordinal 6 (raw 7) should NOT appear (max_position=7 means past 7).
        Placement(
            class_id=c1,
            day=0,
            subject_id=deutsch,
            room_id=room,
            lesson_id=uuid.uuid4(),
            time_block_id=uuid.uuid4(),
            position=6,
            time_block_position=7,
        ),
        # ordinal 7 should not be in cells (not past max_position=7).
        Placement(
            class_id=c1,
            day=0,
            subject_id=deutsch,
            room_id=room,
            lesson_id=uuid.uuid4(),
            time_block_id=uuid.uuid4(),
            position=7,
            time_block_position=8,
        ),
    ]
    issues = list(check_day_length(placements, max_position=7))
    assert len(issues) == 1
    assert issues[0].kind == "day_too_long"
    assert issues[0].cells == ((0, 9),)


def test_check_day_length_cells_sorted_ascending() -> None:
    """day_too_long emits one issue per (class, day); each issue's cells are
    sorted ascending by (day, time_block_position)."""
    c1 = uuid.uuid4()
    deutsch = uuid.uuid4()
    room = uuid.uuid4()
    placements = [
        Placement(
            class_id=c1,
            day=0,
            subject_id=deutsch,
            room_id=room,
            lesson_id=uuid.uuid4(),
            time_block_id=uuid.uuid4(),
            position=9,
            time_block_position=11,
        ),
        Placement(
            class_id=c1,
            day=0,
            subject_id=deutsch,
            room_id=room,
            lesson_id=uuid.uuid4(),
            time_block_id=uuid.uuid4(),
            position=8,
            time_block_position=9,
        ),
    ]
    issues = list(check_day_length(placements, max_position=7))
    assert len(issues) == 1
    assert issues[0].cells == ((0, 9), (0, 11))


def test_check_class_teacher_subject_share_cells_carries_offending_placements() -> None:
    """class_teacher_subject_share cells include every placement for the offending
    (class, subject) where the teacher is not the class's Klassenlehrer."""
    c1 = uuid.uuid4()
    deutsch = uuid.uuid4()
    room = uuid.uuid4()
    lesson_a = uuid.uuid4()
    lesson_b = uuid.uuid4()
    klassenlehrer = uuid.uuid4()
    teacher_other_1 = uuid.uuid4()
    teacher_other_2 = uuid.uuid4()
    placements = [
        # Insert in reverse to verify sorting.
        Placement(
            class_id=c1,
            day=2,
            subject_id=deutsch,
            room_id=room,
            lesson_id=lesson_b,
            time_block_id=uuid.uuid4(),
            position=3,
            time_block_position=4,
        ),
        Placement(
            class_id=c1,
            day=1,
            subject_id=deutsch,
            room_id=room,
            lesson_id=lesson_a,
            time_block_id=uuid.uuid4(),
            position=1,
            time_block_position=2,
        ),
    ]
    class_teacher_lookup: dict[UUID, UUID | None] = {c1: klassenlehrer}
    placement_teacher_lookup: dict[UUID, UUID] = {
        lesson_a: teacher_other_1,
        lesson_b: teacher_other_2,
    }
    issues = list(
        check_class_teacher_subject_share(
            placements, class_teacher_lookup, placement_teacher_lookup
        )
    )
    assert len(issues) == 1
    assert issues[0].kind == "class_teacher_subject_share"
    assert issues[0].cells == ((1, 2), (2, 4))


def test_check_class_day_balance_cells_empty() -> None:
    """imbalance issues carry an empty cells tuple in v1 (class-level)."""
    class_id = uuid.uuid4()
    counts_per_class = {class_id: [2, 5, 5, 5, 5]}  # spread = 3
    issues = list(check_class_day_balance(counts_per_class, max_spread=2))
    assert len(issues) == 1
    assert issues[0].kind == "imbalance"
    assert issues[0].cells == ()


def test_check_interior_gaps_cells_empty() -> None:
    """interior_gap issues carry an empty cells tuple in v1 (class-level)."""
    c1 = uuid.uuid4()
    # day 0: positions 1,2,5  -> 5-1+1-3 = 2 gaps; day 1: 1,4 -> 2 gaps
    positions = {(c1, 0): [1, 2, 5], (c1, 1): [1, 4]}
    issues = list(check_interior_gaps(positions, max_gaps_per_class=2))
    assert len(issues) == 1
    assert issues[0].kind == "interior_gap"
    assert issues[0].cells == ()
