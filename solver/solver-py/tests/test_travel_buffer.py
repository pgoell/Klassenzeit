"""Wire-format contract test for the travel-buffer Lesson fields (ADR 0044).

The `pre_buffer_minutes` / `post_buffer_minutes` fields on
`solver_core::types::Lesson` use `#[serde(default)]`, so the PyO3 binding
picks them up automatically. This test pins the wire-format contract:

* The JSON keys `pre_buffer_minutes` and `post_buffer_minutes` deserialise
  into `Lesson` without error.
* A buffered problem whose only feasible slot is an interior position is
  solved without emitting a `travel_buffer_conflict` violation.

Algorithm coverage lives in `solver-core/tests/travel_buffer.rs`; this is a
binding contract test, not an algorithm test.

CP-SAT hard-constraint coverage (ADR 0044): three cases at the bottom of
this file pin that ``klassenzeit_solver.cpsat`` honours the per-(class,
teacher) one-slot buffer rule on the chosen-teacher decision variable, not
a fan-out over ``teacher_candidates``.
"""

import json
import uuid

import pytest

from klassenzeit_solver import solve_cpsat_json, solve_json_with_config


def _buffer_test_uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def _buffered_problem() -> dict:
    """Smallest feasible problem with a buffered lesson.

    One class, one teacher, one room, one subject, one 1-hour lesson with
    non-zero pre/post buffer minutes. Three lesson-kind time blocks on one
    day so the picker can place the lesson at position 1 (positions 0 and
    2 are excluded by the day-edge buffer rule).
    """
    tb0, tb1, tb2 = _buffer_test_uuid(10), _buffer_test_uuid(11), _buffer_test_uuid(12)
    teacher = _buffer_test_uuid(20)
    room = _buffer_test_uuid(30)
    subject = _buffer_test_uuid(40)
    class_id = _buffer_test_uuid(50)
    lesson = _buffer_test_uuid(60)
    return {
        "time_blocks": [
            {"id": tb0, "day_of_week": 0, "position": 0},
            {"id": tb1, "day_of_week": 0, "position": 1},
            {"id": tb2, "day_of_week": 0, "position": 2},
        ],
        "teachers": [{"id": teacher, "max_hours_per_week": 5}],
        "rooms": [{"id": room}],
        "subjects": [
            {
                "id": subject,
                "prefer_early_period": 0,
                "avoid_first_period": 0,
                "avoid_last_period": 0,
            }
        ],
        "school_classes": [{"id": class_id}],
        "lessons": [
            {
                "id": lesson,
                "school_class_ids": [class_id],
                "subject_id": subject,
                "teacher_candidates": [teacher],
                "teacher_pin": teacher,
                "hours_per_week": 1,
                "preferred_block_size": 1,
                "pre_buffer_minutes": 15,
                "post_buffer_minutes": 15,
            }
        ],
        "teacher_qualifications": [{"teacher_id": teacher, "subject_id": subject}],
        "teacher_blocked_times": [],
        "room_blocked_times": [],
        "room_subject_suitabilities": [],
    }


def test_lesson_buffer_fields_round_trip_through_binding() -> None:
    """`pre_buffer_minutes` / `post_buffer_minutes` flow through the JSON wire
    format and a feasible buffered placement emits no TravelBufferConflict.
    """
    solution_json = solve_json_with_config(json.dumps(_buffered_problem()), None)
    solution = json.loads(solution_json)

    violations = solution.get("violations", [])
    assert not any(v["kind"] == "travel_buffer_conflict" for v in violations), (
        f"unexpected travel_buffer_conflict in {violations}"
    )

    placements = solution["placements"]
    assert len(placements) == 1, f"expected one placement, got {placements}"


# ----------------------------------------------------------------------
# CP-SAT hard-constraint cases (ADR 0044, Task 6 of schwimm-travel-buffers)
# ----------------------------------------------------------------------


def _cpsat_buffered_adjacent_problem() -> str:
    """One day with 4 positions, two same-class same-teacher lessons.

    Lesson A is a 1h buffered lesson (pre=15, post=15): must sit at
    position 1 or position 2 (positions 0 and 3 are forbidden as first/last
    slots). Lesson B is a 1h non-buffered lesson on the same class and same
    teacher; the only feasible joint placement is the two non-adjacent slots
    on either side of the buffered lesson, e.g. (A at 1, B at 3) or
    (A at 2, B at 0). Without the CP-SAT buffer hard constraint, the solver
    might place B at position 0 next to A at position 1 (or symmetric).
    """
    tb = [_buffer_test_uuid(10 + i) for i in range(4)]
    teacher = _buffer_test_uuid(20)
    room = _buffer_test_uuid(30)
    subject_a = _buffer_test_uuid(40)
    subject_b = _buffer_test_uuid(41)
    class_id = _buffer_test_uuid(50)
    lesson_a = _buffer_test_uuid(60)
    lesson_b = _buffer_test_uuid(61)
    return json.dumps(
        {
            "time_blocks": [{"id": tb[i], "day_of_week": 0, "position": i} for i in range(4)],
            "teachers": [{"id": teacher, "max_hours_per_week": 5}],
            "rooms": [{"id": room}],
            "subjects": [
                {"id": subject_a},
                {"id": subject_b},
            ],
            "school_classes": [{"id": class_id}],
            "lessons": [
                {
                    "id": lesson_a,
                    "school_class_ids": [class_id],
                    "subject_id": subject_a,
                    "teacher_candidates": [teacher],
                    "teacher_pin": teacher,
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                    "pre_buffer_minutes": 15,
                    "post_buffer_minutes": 15,
                },
                {
                    "id": lesson_b,
                    "school_class_ids": [class_id],
                    "subject_id": subject_b,
                    "teacher_candidates": [teacher],
                    "teacher_pin": teacher,
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                },
            ],
            "teacher_qualifications": [
                {"teacher_id": teacher, "subject_id": subject_a},
                {"teacher_id": teacher, "subject_id": subject_b},
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def _cpsat_first_slot_forbidden_problem() -> str:
    """3 positions, one buffered Doppelstunde. Only (1, 2) is feasible.

    The Doppelstunde could go at (0, 1) or (1, 2) modulo the buffer rule.
    Pre-buffer at start_pos == 0 is "first_slot" (validator rejects). So the
    CP-SAT model must place the Doppelstunde at positions 1+2, never 0+1.
    """
    tb = [_buffer_test_uuid(10 + i) for i in range(3)]
    teacher = _buffer_test_uuid(20)
    room = _buffer_test_uuid(30)
    subject = _buffer_test_uuid(40)
    class_id = _buffer_test_uuid(50)
    lesson = _buffer_test_uuid(60)
    return json.dumps(
        {
            "time_blocks": [{"id": tb[i], "day_of_week": 0, "position": i} for i in range(3)],
            "teachers": [{"id": teacher, "max_hours_per_week": 5}],
            "rooms": [{"id": room}],
            # prefer_early_period biases the CP-SAT objective toward
            # start_pos == 0; without the buffer hard constraint, the model
            # picks (0, 1). With the hard constraint, only (1, 2) is feasible.
            "subjects": [{"id": subject, "prefer_early_period": 1}],
            "school_classes": [{"id": class_id}],
            "lessons": [
                {
                    "id": lesson,
                    "school_class_ids": [class_id],
                    "subject_id": subject,
                    "teacher_candidates": [teacher],
                    "teacher_pin": teacher,
                    "hours_per_week": 2,
                    "preferred_block_size": 2,
                    "pre_buffer_minutes": 15,
                    "post_buffer_minutes": 0,
                }
            ],
            "teacher_qualifications": [{"teacher_id": teacher, "subject_id": subject}],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_cpsat_travel_buffer_blocks_adjacent_placement() -> None:
    """CP-SAT must not place a same-class or same-teacher lesson adjacent
    to a buffered lesson (ADR 0044 one-slot rule).
    """
    out = solve_cpsat_json(_cpsat_buffered_adjacent_problem(), deadline_ms=10_000, seed=1)
    sol = json.loads(out)
    placements = sol["placements"]
    assert len(placements) == 2, f"expected two placements, got {placements}"

    # Build (class, day, pos) and (teacher, day, pos) occupancy maps so we
    # can probe the slots adjacent to the buffered lesson directly.
    # The time-block-id encodes the position via _buffer_test_uuid(10+i).
    tb_id_to_pos = {_buffer_test_uuid(10 + i): i for i in range(4)}
    teacher = _buffer_test_uuid(20)
    lesson_a = _buffer_test_uuid(60)  # the buffered lesson

    # Find the buffered lesson's placement to identify its position.
    buffered_pos: int | None = None
    by_pos: dict[int, str] = {}
    for p in placements:
        pos = tb_id_to_pos[p["time_block_id"]]
        by_pos[pos] = p["lesson_id"]
        if p["lesson_id"] == lesson_a:
            buffered_pos = pos
        assert p["teacher_id"] == teacher, f"unexpected teacher: {p}"
    assert buffered_pos is not None, "buffered lesson not placed"
    # Day-edge forbidden: buffered lesson cannot land at the first or last slot.
    assert buffered_pos in (1, 2), (
        f"buffered lesson at edge position {buffered_pos}: travel-buffer "
        f"first/last-slot rule violated"
    )

    # No same-class or same-teacher placement adjacent to buffered_pos.
    # (In this fixture every placement shares the same class and teacher,
    # so the bare slot-occupancy check suffices.)
    for adj in (buffered_pos - 1, buffered_pos + 1):
        if 0 <= adj < 4 and adj in by_pos and by_pos[adj] != lesson_a:
            pytest.fail(
                f"adjacent placement at position {adj} (buffered at "
                f"{buffered_pos}) violates ADR 0044 one-slot buffer rule"
            )


def test_cpsat_travel_buffer_first_slot_forbidden() -> None:
    """Buffered Doppelstunde must NOT take the first-slot anchor on a day."""
    out = solve_cpsat_json(_cpsat_first_slot_forbidden_problem(), deadline_ms=10_000, seed=1)
    sol = json.loads(out)
    placements = sol["placements"]
    assert len(placements) == 2, f"expected two placements, got {placements}"

    tb_id_to_pos = {_buffer_test_uuid(10 + i): i for i in range(3)}
    positions = sorted(tb_id_to_pos[p["time_block_id"]] for p in placements)
    # Doppelstunde occupies two contiguous positions. With pre-buffer set,
    # the start position must be > 0 (first_slot forbidden).
    assert positions == [1, 2], (
        f"buffered Doppelstunde landed at {positions}; expected [1, 2] "
        f"(positions [0, 1] violate ADR 0044 first-slot pre-buffer rule)"
    )


def test_cpsat_validator_parity_on_buffered_fixture() -> None:
    """Solve a Schwimmen-shaped problem via CP-SAT; assert no TravelBufferConflict.

    Task 7's fixture extension landed the Klasse 3a Schwimmen Doppelstunde
    (pre=15, post=15) plus the Schwimmbad room in the dreizügig bench
    fixture. The solver-py crate does not expose the fixture directly, so
    this contract test builds an equivalent Python problem inline: one
    class with two Doppelstunden (a buffered Schwimmen plus one filler)
    plus several non-buffered companion lessons. CP-SAT must place them
    without emitting `travel_buffer_conflict`, exercising the hard
    constraint at parity with `validate_travel_buffer`'s semantics.
    """
    tb = [_buffer_test_uuid(10 + i) for i in range(8)]
    teacher = _buffer_test_uuid(20)
    other_teacher = _buffer_test_uuid(21)
    room_schwimm = _buffer_test_uuid(30)
    room_other = _buffer_test_uuid(31)
    subject_schwimm = _buffer_test_uuid(40)
    subject_other = _buffer_test_uuid(41)
    class_id = _buffer_test_uuid(50)
    lesson_schwimm = _buffer_test_uuid(60)
    lesson_companion = _buffer_test_uuid(61)
    problem_json = json.dumps(
        {
            "time_blocks": [{"id": tb[i], "day_of_week": 0, "position": i} for i in range(8)],
            "teachers": [
                {"id": teacher, "max_hours_per_week": 30},
                {"id": other_teacher, "max_hours_per_week": 30},
            ],
            "rooms": [{"id": room_schwimm}, {"id": room_other}],
            "subjects": [{"id": subject_schwimm}, {"id": subject_other}],
            "school_classes": [{"id": class_id}],
            "lessons": [
                {
                    "id": lesson_schwimm,
                    "school_class_ids": [class_id],
                    "subject_id": subject_schwimm,
                    "teacher_candidates": [teacher],
                    "teacher_pin": teacher,
                    "hours_per_week": 2,
                    "preferred_block_size": 2,
                    "pre_buffer_minutes": 15,
                    "post_buffer_minutes": 15,
                },
                {
                    "id": lesson_companion,
                    "school_class_ids": [class_id],
                    "subject_id": subject_other,
                    "teacher_candidates": [other_teacher],
                    "teacher_pin": other_teacher,
                    "hours_per_week": 3,
                    "preferred_block_size": 1,
                },
            ],
            "teacher_qualifications": [
                {"teacher_id": teacher, "subject_id": subject_schwimm},
                {"teacher_id": other_teacher, "subject_id": subject_other},
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [
                {"room_id": room_schwimm, "subject_id": subject_schwimm},
                {"room_id": room_other, "subject_id": subject_other},
            ],
            "pinned_placements": [],
        }
    )
    out = solve_cpsat_json(problem_json, deadline_ms=10_000, seed=1)
    sol = json.loads(out)
    violations = sol.get("violations", [])
    assert not any(v["kind"] == "travel_buffer_conflict" for v in violations), (
        f"CP-SAT emitted travel_buffer_conflict on buffered fixture: {violations}"
    )
    placements = sol["placements"]
    # All 5 lessons must be placed: Schwimmen (Doppelstunde = 2) + companion (3).
    assert len(placements) == 5, f"expected 5 placements, got {placements}"
