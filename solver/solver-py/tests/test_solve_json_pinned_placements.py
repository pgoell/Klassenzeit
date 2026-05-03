"""Binding contract test: pinned_placements wire field round-trips."""

import json
import uuid

from klassenzeit_solver import solve_json_with_config


def _pin_uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def test_solve_json_round_trips_pinned_placement() -> None:
    """Problem JSON with one pinned_placement yields a Solution where the pin
    appears verbatim in placements; deadline_ms=None per the solver/CLAUDE.md
    binding-contract rule (skip the 200 ms LAHC pass)."""
    lesson_id = _pin_uuid(60)
    tb_id = _pin_uuid(10)
    room_id = _pin_uuid(30)
    teacher_id = _pin_uuid(20)
    subject_id = _pin_uuid(40)
    class_id = _pin_uuid(50)

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
                "teacher_id": teacher_id,
                "hours_per_week": 1,
                "preferred_block_size": 1,
            }
        ],
        "teacher_qualifications": [{"teacher_id": teacher_id, "subject_id": subject_id}],
        "teacher_blocked_times": [],
        "room_blocked_times": [],
        "room_subject_suitabilities": [],
        "pinned_placements": [{"lesson_id": lesson_id, "time_block_id": tb_id, "room_id": room_id}],
    }

    solution_json = solve_json_with_config(json.dumps(problem), None)
    solution = json.loads(solution_json)

    pinned = [p for p in solution["placements"] if p["lesson_id"] == lesson_id]
    assert len(pinned) == 1
    assert pinned[0]["time_block_id"] == tb_id
    assert pinned[0]["room_id"] == room_id
