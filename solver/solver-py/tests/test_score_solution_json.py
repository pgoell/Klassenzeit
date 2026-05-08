"""Round-trip test: score_solution_json reproduces solve_json_with_config's reported soft_score."""

from __future__ import annotations

import json
import uuid

import pytest

from klassenzeit_solver import score_solution_json, solve_json_with_config


def _score_test_uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def _trivial_one_lesson_problem() -> str:
    """A minimal Problem with one teacher, one room, one class, one subject,
    one time block, one lesson. Solver places the lesson; score is 0."""
    tb_id = _score_test_uuid(10)
    teacher_id = _score_test_uuid(20)
    room_id = _score_test_uuid(30)
    subject_id = _score_test_uuid(40)
    class_id = _score_test_uuid(50)
    lesson_id = _score_test_uuid(60)
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
    return json.dumps(problem)


def test_score_solution_json_matches_solve_json_with_config_soft_score() -> None:
    problem_json = _trivial_one_lesson_problem()
    solution_json = solve_json_with_config(problem_json, deadline_ms=None)
    solution = json.loads(solution_json)
    placements_json = json.dumps(solution["placements"])

    rescored = score_solution_json(problem_json, placements_json)

    assert rescored == solution["soft_score"], (
        f"rescored={rescored} solver-reported={solution['soft_score']}"
    )


def test_score_solution_json_zero_for_empty_placements() -> None:
    problem_json = _trivial_one_lesson_problem()
    assert score_solution_json(problem_json, "[]") == 0


def test_score_solution_json_raises_on_invalid_problem_json() -> None:
    with pytest.raises(ValueError):
        score_solution_json("not json", "[]")


def test_score_solution_json_raises_on_invalid_placements_json() -> None:
    problem_json = _trivial_one_lesson_problem()
    with pytest.raises(ValueError):
        score_solution_json(problem_json, "not json")
