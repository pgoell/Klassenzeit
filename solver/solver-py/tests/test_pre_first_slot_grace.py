"""Wire-format + CP-SAT parity tests for ``Problem.pre_first_slot_grace_minutes``.

ADR 0044 amendment. When the school has a non-zero pre-school grace window,
buffered lessons whose ``pre_buffer_minutes`` fit inside the grace may be
placed at day-position 0 (the implicit pre-school window covers the
Wegezeit). The Rust LAHC path is gated in ``validate_travel_buffer`` /
``would_violate_travel_buffer``; this file pins CP-SAT parity.

Five cases:

1. Default round-trip: a problem omitting the field deserialises to grace=0
   and the historic reject-pos0 semantic holds.
2. LAHC grace-covers: grace >= pre_buffer_minutes allows pos=0.
3. LAHC grace-too-small: grace < pre_buffer_minutes still rejects pos=0.
4. CP-SAT grace-covers: same as (2) on the CP-SAT backend.
5. CP-SAT grace-too-small: same as (3) on the CP-SAT backend.
"""

import json
import uuid

from klassenzeit_solver import solve_json_with_config
from klassenzeit_solver.cpsat import solve_cpsat_json


def _grace_test_uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def _grace_minimal_problem(*, pre_buffer: int, grace: int | None) -> dict:
    """One class, one teacher, one room, one buffered 1-hour lesson on a
    single-slot day. The only feasible placement is pos=0.

    When ``grace`` is None, the field is omitted from the payload (tests the
    serde-default round-trip).
    """
    tb = _grace_test_uuid(10)
    teacher = _grace_test_uuid(20)
    room = _grace_test_uuid(30)
    subject = _grace_test_uuid(40)
    class_id = _grace_test_uuid(50)
    lesson = _grace_test_uuid(60)
    problem: dict = {
        "time_blocks": [{"id": tb, "day_of_week": 0, "position": 0}],
        "teachers": [{"id": teacher, "max_hours_per_week": 5}],
        "rooms": [{"id": room}],
        "subjects": [{"id": subject}],
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
                "pre_buffer_minutes": pre_buffer,
                "post_buffer_minutes": 0,
            }
        ],
        "teacher_qualifications": [{"teacher_id": teacher, "subject_id": subject}],
        "teacher_blocked_times": [],
        "room_blocked_times": [],
        "room_subject_suitabilities": [],
        "pinned_placements": [],
    }
    if grace is not None:
        problem["pre_first_slot_grace_minutes"] = grace
    return problem


def test_pre_first_slot_grace_default_round_trips_as_zero() -> None:
    """Omitting ``pre_first_slot_grace_minutes`` deserialises to 0; a
    buffered lesson on a single-slot day stays unplaced under the historic
    reject-pos0 semantic.
    """
    problem = _grace_minimal_problem(pre_buffer=15, grace=None)
    out = solve_json_with_config(json.dumps(problem), None)
    sol = json.loads(out)
    # Grace defaults to 0 < pre_buffer=15: pos=0 still forbidden, lesson
    # unplaced. The validator MUST not surface a deserialisation error.
    assert sol["placements"] == [], (
        f"expected no placement under default grace=0; got {sol['placements']}"
    )


def test_lahc_pre_first_slot_grace_covers_allows_pos_zero() -> None:
    """LAHC: grace >= pre_buffer_minutes lets the buffered lesson sit at pos=0."""
    problem = _grace_minimal_problem(pre_buffer=15, grace=15)
    out = solve_json_with_config(json.dumps(problem), None)
    sol = json.loads(out)
    placements = sol["placements"]
    assert len(placements) == 1, (
        f"expected pos=0 placement under grace=15 >= pre=15; got {placements}"
    )
    violations = sol.get("violations", [])
    assert not any(v["kind"] == "travel_buffer_conflict" for v in violations), (
        f"unexpected travel_buffer_conflict under grace>=pre: {violations}"
    )


def test_lahc_pre_first_slot_grace_too_small_still_rejects_pos_zero() -> None:
    """LAHC: grace < pre_buffer_minutes still forbids pos=0."""
    problem = _grace_minimal_problem(pre_buffer=15, grace=10)
    out = solve_json_with_config(json.dumps(problem), None)
    sol = json.loads(out)
    assert sol["placements"] == [], (
        f"expected no placement under grace=10 < pre=15; got {sol['placements']}"
    )


def test_cpsat_pre_first_slot_grace_covers_allows_pos_zero() -> None:
    """CP-SAT: grace >= pre_buffer_minutes lets the buffered lesson sit at pos=0."""
    problem = _grace_minimal_problem(pre_buffer=15, grace=15)
    out = solve_cpsat_json(json.dumps(problem), deadline_ms=10_000, seed=1)
    sol = json.loads(out)
    placements = sol["placements"]
    assert len(placements) == 1, (
        f"CP-SAT: expected pos=0 placement under grace=15 >= pre=15; got {placements}"
    )
    violations = sol.get("violations", [])
    assert not any(v["kind"] == "travel_buffer_conflict" for v in violations), (
        f"CP-SAT emitted travel_buffer_conflict under grace>=pre: {violations}"
    )


def test_cpsat_pre_first_slot_grace_too_small_still_rejects_pos_zero() -> None:
    """CP-SAT: grace < pre_buffer_minutes still forbids pos=0 (no feasible solution)."""
    problem = _grace_minimal_problem(pre_buffer=15, grace=10)
    out = solve_cpsat_json(json.dumps(problem), deadline_ms=10_000, seed=1)
    sol = json.loads(out)
    assert sol["placements"] == [], (
        f"CP-SAT: expected no placement under grace=10 < pre=15; got {sol['placements']}"
    )
