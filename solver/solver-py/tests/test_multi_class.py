"""Multi-class Lesson round-trip via the JSON binding."""

import json
import uuid

import pytest

from klassenzeit_solver import solve_json_with_config


def _multi_class_uuid_str(b: int) -> str:
    return str(uuid.UUID(bytes=bytes([b] * 16)))


@pytest.fixture
def multi_class_problem() -> str:
    cid_a = _multi_class_uuid_str(50)
    cid_b = _multi_class_uuid_str(51)
    tb_zero = _multi_class_uuid_str(10)
    tb_one = _multi_class_uuid_str(11)
    teacher = _multi_class_uuid_str(20)
    subject = _multi_class_uuid_str(40)
    room = _multi_class_uuid_str(30)
    lesson = _multi_class_uuid_str(60)
    lesson_b = _multi_class_uuid_str(61)
    problem = {
        "time_blocks": [
            {"id": tb_zero, "day_of_week": 0, "position": 0},
            {"id": tb_one, "day_of_week": 0, "position": 1},
        ],
        "teachers": [{"id": teacher, "max_hours_per_week": 10}],
        "rooms": [{"id": room}],
        "subjects": [
            {
                "id": subject,
                "prefer_early_period": 0,
                "avoid_first_period": 0,
                "avoid_last_period": 0,
            }
        ],
        "school_classes": [{"id": cid_a}, {"id": cid_b}],
        "lessons": [
            {
                "id": lesson,
                "school_class_ids": [cid_a, cid_b],
                "subject_id": subject,
                "teacher_candidates": [teacher],
                "teacher_pin": teacher,
                "hours_per_week": 1,
                "preferred_block_size": 1,
            },
            {
                "id": lesson_b,
                "school_class_ids": [cid_b],
                "subject_id": subject,
                "teacher_candidates": [teacher],
                "teacher_pin": teacher,
                "hours_per_week": 1,
                "preferred_block_size": 1,
            },
        ],
        "teacher_qualifications": [{"teacher_id": teacher, "subject_id": subject}],
        "teacher_blocked_times": [],
        "room_blocked_times": [],
        "room_subject_suitabilities": [],
    }
    return json.dumps(problem)


def test_multi_class_lesson_blocks_each_class(multi_class_problem: str) -> None:
    raw = solve_json_with_config(multi_class_problem, None)
    solution = json.loads(raw)
    assert len(solution["placements"]) == 2
    assert len(solution["violations"]) == 0
    tb_ids = {p["time_block_id"] for p in solution["placements"]}
    assert len(tb_ids) == 2, "two placements must occupy two time-blocks"


def test_lesson_group_id_round_trips_through_binding(multi_class_problem: str) -> None:
    problem = json.loads(multi_class_problem)
    problem["lessons"][0]["lesson_group_id"] = _multi_class_uuid_str(99)
    raw = solve_json_with_config(json.dumps(problem), None)
    solution = json.loads(raw)
    assert len(solution["placements"]) == 2
