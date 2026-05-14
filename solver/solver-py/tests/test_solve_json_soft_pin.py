"""Binding contract test: pinned_placements `kind: "soft"` round-trips.

Regression-prevention pin: the Rust side accepts the new `kind` discriminator
via `#[serde(default)]` on `PinnedPlacement.kind`. This test exercises the
wire-format passthrough end-to-end through `solve_json` and asserts the soft
pin neither errors nor emits a `pinned_conflict` violation.
"""

import json
import uuid

from klassenzeit_solver import solve_json


def _soft_pin_uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def test_soft_pin_round_trips_without_pinned_conflict() -> None:
    """A `kind: "soft"` pinned_placement must not raise nor emit a
    pinned_conflict violation; the solver still places the lesson."""
    lesson_id = _soft_pin_uuid(60)
    tb_id = _soft_pin_uuid(10)
    room_id = _soft_pin_uuid(30)
    teacher_id = _soft_pin_uuid(20)
    subject_id = _soft_pin_uuid(40)
    class_id = _soft_pin_uuid(50)

    problem = {
        "time_blocks": [{"id": tb_id, "day_of_week": 0, "position": 0}],
        "teachers": [{"id": teacher_id, "max_hours_per_week": 5}],
        "rooms": [{"id": room_id}],
        "subjects": [
            {
                "id": subject_id,
                "prefer_early_period": 0,
                "avoid_first_period": 0,
                "avoid_last_period": 0,
            }
        ],
        "school_classes": [{"id": class_id}],
        "lessons": [
            {
                "id": lesson_id,
                "school_class_ids": [class_id],
                "subject_id": subject_id,
                "teacher_candidates": [teacher_id],
                "teacher_pin": teacher_id,
                "hours_per_week": 1,
                "preferred_block_size": 1,
            }
        ],
        "teacher_qualifications": [{"teacher_id": teacher_id, "subject_id": subject_id}],
        "teacher_blocked_times": [],
        "room_blocked_times": [],
        "room_subject_suitabilities": [],
        "pinned_placements": [
            {
                "lesson_id": lesson_id,
                "time_block_id": tb_id,
                "room_id": room_id,
                "kind": "soft",
            }
        ],
    }

    solution_json = solve_json(json.dumps(problem))
    solution = json.loads(solution_json)

    violation_kinds = {v.get("kind") for v in solution.get("violations", [])}
    assert "pinned_conflict" not in violation_kinds, (
        "soft pin must not emit a pinned_conflict violation"
    )
    assert solution.get("placements"), "solver returned no placements"
