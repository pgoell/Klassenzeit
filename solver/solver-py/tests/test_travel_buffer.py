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
"""

import json
import uuid

from klassenzeit_solver import solve_json_with_config


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
