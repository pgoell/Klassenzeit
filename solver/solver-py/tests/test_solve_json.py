"""Binding contract test: Placement.teacher_id round-trips via solve_json.

OPEN_THINGS items 64 + 65: every Placement emitted by the LAHC core carries
an explicit teacher_id matching the lesson's teacher_pin (set transitionally
by the route handler's auto_assign_teachers_for_lessons).
"""

import json
import uuid

from klassenzeit_solver import solve_json_with_config


def _solve_json_test_uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def test_solve_json_placement_carries_teacher_id() -> None:
    """Every Placement must carry a teacher_id matching the lesson's pin."""
    teacher_id = _solve_json_test_uuid(20)
    class_id = _solve_json_test_uuid(50)
    subject_id = _solve_json_test_uuid(40)
    room_id = _solve_json_test_uuid(30)
    tb_id = _solve_json_test_uuid(10)
    lesson_id = _solve_json_test_uuid(60)

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
        "pinned_placements": [],
    }

    solution = json.loads(solve_json_with_config(json.dumps(problem), None))
    assert solution["placements"], "expected at least one placement"
    for p in solution["placements"]:
        assert "teacher_id" in p, "Placement missing teacher_id"
        assert p["teacher_id"] == teacher_id
