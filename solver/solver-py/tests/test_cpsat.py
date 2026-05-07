"""CP-SAT seed contract tests."""

from __future__ import annotations

import json
import uuid

import pytest

from klassenzeit_solver import score_solution_json, solve_cpsat_json


def _cpsat_uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def _cpsat_trivial_one_lesson_problem() -> str:
    """One teacher, one room, one class, one subject, one TB, one 1h lesson."""
    return json.dumps(
        {
            "time_blocks": [{"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0}],
            "teachers": [{"id": _cpsat_uuid(20), "max_hours_per_week": 5}],
            "rooms": [{"id": _cpsat_uuid(30)}],
            "subjects": [{"id": _cpsat_uuid(40)}],
            "school_classes": [{"id": _cpsat_uuid(50)}],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [_cpsat_uuid(50)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_id": _cpsat_uuid(20),
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                }
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def _cpsat_doppelstunde_problem() -> str:
    """Two TBs same day consecutive positions, one 2h doppelstunde lesson."""
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
                {
                    "id": _cpsat_uuid(12),
                    "day_of_week": 1,
                    "position": 0,
                },  # different day - must NOT be used
                {"id": _cpsat_uuid(13), "day_of_week": 1, "position": 1},
            ],
            "teachers": [{"id": _cpsat_uuid(20), "max_hours_per_week": 5}],
            "rooms": [{"id": _cpsat_uuid(30)}],
            "subjects": [{"id": _cpsat_uuid(40)}],
            "school_classes": [{"id": _cpsat_uuid(50)}],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [_cpsat_uuid(50)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_id": _cpsat_uuid(20),
                    "hours_per_week": 2,
                    "preferred_block_size": 2,
                }
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def _cpsat_infeasible_problem() -> str:
    """One lesson asks 5h, only 3 TBs exist."""
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10 + i), "day_of_week": 0, "position": i} for i in range(3)
            ],
            "teachers": [{"id": _cpsat_uuid(20), "max_hours_per_week": 5}],
            "rooms": [{"id": _cpsat_uuid(30)}],
            "subjects": [{"id": _cpsat_uuid(40)}],
            "school_classes": [{"id": _cpsat_uuid(50)}],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [_cpsat_uuid(50)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_id": _cpsat_uuid(20),
                    "hours_per_week": 5,
                    "preferred_block_size": 1,
                }
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_solve_cpsat_json_trivial_problem_places_lesson() -> None:
    out = solve_cpsat_json(_cpsat_trivial_one_lesson_problem(), deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["violations"] == []
    assert len(sol["placements"]) == 1
    assert sol["soft_score"] == 0


def test_solve_cpsat_json_doppelstunde_block_contiguity() -> None:
    out = solve_cpsat_json(_cpsat_doppelstunde_problem(), deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["violations"] == []
    assert len(sol["placements"]) == 2
    # Both placements must be on the SAME day in the SAME room and at consecutive positions.
    problem = json.loads(_cpsat_doppelstunde_problem())
    tbs_by_id = {tb["id"]: tb for tb in problem["time_blocks"]}
    days = {tbs_by_id[p["time_block_id"]]["day_of_week"] for p in sol["placements"]}
    rooms = {p["room_id"] for p in sol["placements"]}
    positions = sorted(tbs_by_id[p["time_block_id"]]["position"] for p in sol["placements"])
    assert len(days) == 1, f"placements span multiple days: {days}"
    assert len(rooms) == 1, f"placements span multiple rooms: {rooms}"
    assert positions[1] == positions[0] + 1, f"positions not consecutive: {positions}"


def test_solve_cpsat_json_infeasible_returns_violations_with_reason() -> None:
    out = solve_cpsat_json(_cpsat_infeasible_problem(), deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["placements"] == []
    assert len(sol["violations"]) >= 1
    assert all(v["kind"] == "no_free_time_block" for v in sol["violations"])
    reasons = {v.get("reason") for v in sol["violations"]}
    assert any(r and "cpsat" in r for r in reasons), f"no cpsat reason: {reasons}"


def test_solve_cpsat_json_pinned_placement_round_trip() -> None:
    problem = json.loads(_cpsat_trivial_one_lesson_problem())
    problem["pinned_placements"] = [
        {"lesson_id": _cpsat_uuid(60), "time_block_id": _cpsat_uuid(10), "room_id": _cpsat_uuid(30)}
    ]
    out = solve_cpsat_json(json.dumps(problem), deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["violations"] == []
    pin = sol["placements"][0]
    assert pin["lesson_id"] == _cpsat_uuid(60)
    assert pin["time_block_id"] == _cpsat_uuid(10)
    assert pin["room_id"] == _cpsat_uuid(30)


def test_solve_cpsat_json_invalid_json_raises_value_error() -> None:
    with pytest.raises(ValueError):
        solve_cpsat_json("not json", deadline_ms=1_000)


def _cpsat_lesson_group_multi_class_problem() -> str:
    """Religion-trio shape: 3 lessons sharing one lesson_group_id, each spanning
    the same 3 classes (multi-class), each with its own teacher and qualifying
    on its own subject. Mirrors the dreizuegige Religion trio that broke the
    smoke bake-off bench: lesson-group co-placement forces all 3 to land at
    the same (day, pos), so class non-overlap must dedup by group or the
    model is INFEASIBLE.
    """
    classes = [_cpsat_uuid(50 + i) for i in range(3)]
    subjects = [_cpsat_uuid(40 + i) for i in range(3)]
    teachers = [_cpsat_uuid(20 + i) for i in range(3)]
    rooms = [_cpsat_uuid(30 + i) for i in range(3)]
    group_id = _cpsat_uuid(99)
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10 + p), "day_of_week": 0, "position": p} for p in range(5)
            ],
            "teachers": [{"id": tid, "max_hours_per_week": 5} for tid in teachers],
            "rooms": [{"id": rid} for rid in rooms],
            "subjects": [{"id": sid} for sid in subjects],
            "school_classes": [{"id": cid} for cid in classes],
            "lessons": [
                {
                    "id": _cpsat_uuid(60 + i),
                    "school_class_ids": classes,
                    "subject_id": subjects[i],
                    "teacher_id": teachers[i],
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                    "lesson_group_id": group_id,
                }
                for i in range(3)
            ],
            "teacher_qualifications": [
                {"teacher_id": teachers[i], "subject_id": subjects[i]} for i in range(3)
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_solve_cpsat_json_multi_class_lesson_group_co_places_at_same_slot() -> None:
    out = solve_cpsat_json(_cpsat_lesson_group_multi_class_problem(), deadline_ms=5_000)
    sol = json.loads(out)
    assert sol["violations"] == [], f"expected feasible: {sol['violations'][:3]}"
    assert len(sol["placements"]) == 3
    times = {p["time_block_id"] for p in sol["placements"]}
    assert len(times) == 1, f"placements not co-placed: {times}"
    rooms = {p["room_id"] for p in sol["placements"]}
    assert len(rooms) == 3, f"placements share rooms: {rooms}"


def test_solve_cpsat_json_emits_observability_fields_when_optimal() -> None:
    out = solve_cpsat_json(_cpsat_trivial_one_lesson_problem(), deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["violations"] == []
    assert "peak_rss_kb" in sol
    assert isinstance(sol["peak_rss_kb"], int)
    assert sol["peak_rss_kb"] > 0
    assert isinstance(sol["time_to_first_feasible_ms"], float)
    assert sol["time_to_first_feasible_ms"] >= 0.0
    assert isinstance(sol["time_to_optimal_ms"], float)
    assert sol["time_to_optimal_ms"] >= 0.0
    assert sol["time_to_first_feasible_ms"] <= sol["time_to_optimal_ms"] + 1e-6


def test_solve_cpsat_json_omits_tto_when_not_optimal() -> None:
    out = solve_cpsat_json(_cpsat_infeasible_problem(), deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["placements"] == []
    assert "peak_rss_kb" in sol
    assert isinstance(sol["peak_rss_kb"], int)
    assert sol["peak_rss_kb"] > 0
    assert sol["time_to_first_feasible_ms"] is None
    assert sol["time_to_optimal_ms"] is None


def test_solve_cpsat_json_reported_soft_score_equals_canonical_score() -> None:
    """Item 51 acceptance #1 (CP-SAT): the reported `soft_score` on a
    returned solution must equal `score_solution_json(problem, placements)`.

    Tautological today (cpsat.py computes `soft_score` via
    `score_solution_json`), but the test is a regression guard against any
    future swap of the post-solve scorer for an internal CP-SAT objective
    expression. Item 48 ports the canonical objective into the model itself;
    even after that lands, this assertion still holds because the *reported*
    score on the returned placements is a function of the placements alone.
    """
    problem_json = _cpsat_doppelstunde_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["soft_score"] == canonical


def test_cpsat_objective_value_equals_score_solution_on_trivial_problem() -> None:
    """Item 48 acceptance: the CP-SAT model objective value on the returned
    solution must equal `score_solution(problem, placements, PRODUCTION_ACTIVE_WEIGHTS)`.

    Trivial fixture has every axis evaluating to 0 today; the test passes
    even before any axis is ported. It locks the contract so subsequent
    axis ports can extend the test set without re-deriving the harness.
    """
    problem_json = _cpsat_trivial_one_lesson_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical


def _cpsat_doppelstunde_with_prefer_late_subject() -> str:
    """Doppelstunde fixture variant where subject.prefer_late_period = 1, so
    score_solution's prefer_late axis fires per placement."""
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
                {"id": _cpsat_uuid(12), "day_of_week": 0, "position": 2},
                {"id": _cpsat_uuid(13), "day_of_week": 0, "position": 3},
            ],
            "teachers": [{"id": _cpsat_uuid(20), "max_hours_per_week": 5}],
            "rooms": [{"id": _cpsat_uuid(30)}],
            "subjects": [{"id": _cpsat_uuid(40), "prefer_late_period": 1}],
            "school_classes": [{"id": _cpsat_uuid(50)}],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [_cpsat_uuid(50)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_id": _cpsat_uuid(20),
                    "hours_per_week": 2,
                    "preferred_block_size": 2,
                }
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_cpsat_objective_value_equals_score_solution_on_subject_preference_problem() -> None:
    """prefer_late axis: max_position_for_day=3, weights.prefer_late_period=1,
    subject.prefer_late_period=1; doppelstunde block contributes
    (3-p) + (3-(p+1)) per placement, weighted by 1*1.

    The CP-SAT objective should drive the doppelstunde to anchor at p=2
    (positions 2,3) so prefer_late contribution is (3-2) + (3-3) = 1, not
    p=0 (positions 0,1) which would contribute 5.
    """
    problem_json = _cpsat_doppelstunde_with_prefer_late_subject()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
    # Witness that CP-SAT actually steers: objective is 1, not the worst-case 5.
    assert out["model_objective_value"] == 1
