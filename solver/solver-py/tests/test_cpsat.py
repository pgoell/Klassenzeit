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
                    "teacher_candidates": [_cpsat_uuid(20)],
                    "teacher_pin": _cpsat_uuid(20),
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
                    "teacher_candidates": [_cpsat_uuid(20)],
                    "teacher_pin": _cpsat_uuid(20),
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
                    "teacher_candidates": [_cpsat_uuid(20)],
                    "teacher_pin": _cpsat_uuid(20),
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
    # Item 57 widened the canonical objective: a single placement on day 0
    # has per-class spread = 1 - 0 = 1 (Rust fixed-width day axis 0..5) and
    # weight 10. No other axis fires on this trivial fixture.
    assert sol["soft_score"] == 10


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
                    "teacher_candidates": [teachers[i]],
                    "teacher_pin": teachers[i],
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
                    "teacher_candidates": [_cpsat_uuid(20)],
                    "teacher_pin": _cpsat_uuid(20),
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
    p=0 (positions 0,1) which would contribute 5. Item 57 widens the
    canonical objective with max_per_class_spread: 2 placements on day 0
    and 0 on days 1-4 (Rust fixed-width day axis) gives spread = 2 - 0 = 2
    and weight 10, contributing 20. Total = 1 + 20 = 21.
    """
    problem_json = _cpsat_doppelstunde_with_prefer_late_subject()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
    # Witness that CP-SAT actually steers: prefer_late is 1 not 5, and the
    # per-class spread axis adds 20 for an unavoidable lopsided one-day shape.
    assert out["model_objective_value"] == 21


def _cpsat_home_room_problem() -> str:
    """Two classes with distinct home_rooms, one shared lesson placed in
    one room. Per score_solution: per-placement, per-class additive
    penalty when class.home_room_id != placement.room_id.

    Two TBs, two rooms (one is class 50's home, one is class 51's home),
    one shared single-block 2h lesson. Model must pick a room and pay
    weights.prefer_home_room * 2 (mismatched class, 2 placements).
    """
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
            ],
            "teachers": [{"id": _cpsat_uuid(20), "max_hours_per_week": 5}],
            "rooms": [
                {"id": _cpsat_uuid(30)},
                {"id": _cpsat_uuid(31)},
            ],
            "subjects": [{"id": _cpsat_uuid(40)}],
            "school_classes": [
                {"id": _cpsat_uuid(50), "home_room_id": _cpsat_uuid(30)},
                {"id": _cpsat_uuid(51), "home_room_id": _cpsat_uuid(31)},
            ],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [_cpsat_uuid(50), _cpsat_uuid(51)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_candidates": [_cpsat_uuid(20)],
                    "teacher_pin": _cpsat_uuid(20),
                    "hours_per_week": 2,
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


def test_cpsat_objective_value_equals_score_solution_on_home_room_problem() -> None:
    """home_room axis: a multi-class lesson placed in either room
    mismatches exactly one class's home_room, contributing
    weights.prefer_home_room (= 5) per placement. With 2 placements (2h
    single-block), the per-block contribution is 10. Both placements
    accumulate so total = 10 * 2 = 20... no, score_solution iterates per
    placement; 2 placements * 1 mismatched class * 5 = 10. Item 57 widens
    the canonical objective with max_per_class_spread: each class has 2
    placements on day 0 and 0 on days 1-4, so per-class spread = 2 and
    worst (over both classes) = 2, weight 10 → 20. Total = 10 + 20 = 30.
    """
    problem_json = _cpsat_home_room_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
    # Witness: every room is one class's home and the other's mismatch
    # (10), plus the per-class spread axis bills 20 for two classes each
    # confined to a single day.
    assert out["model_objective_value"] == 30


def _cpsat_forced_class_gap_problem() -> str:
    """Three TBs on day 0 (positions 0, 1, 2), one teacher, one room, one
    class. Two single-hour lessons of the same class with the same
    teacher. Each lesson has hours_per_week=1, preferred_block_size=1.

    But TB at position 1 is teacher-blocked. Both placements must use
    positions 0 and 2 with a forced gap at position 1. score_solution
    reports class_gap=1 (* 10) + teacher_gap=1 (* 10) = 20.
    """
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
                {"id": _cpsat_uuid(12), "day_of_week": 0, "position": 2},
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
                    "teacher_candidates": [_cpsat_uuid(20)],
                    "teacher_pin": _cpsat_uuid(20),
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                },
                {
                    "id": _cpsat_uuid(61),
                    "school_class_ids": [_cpsat_uuid(50)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_candidates": [_cpsat_uuid(20)],
                    "teacher_pin": _cpsat_uuid(20),
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                },
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [
                {"teacher_id": _cpsat_uuid(20), "time_block_id": _cpsat_uuid(11)}
            ],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_cpsat_objective_value_equals_score_solution_on_forced_gap_problem() -> None:
    """class_gap and teacher_gap axes: forced gap at position 1; class
    contributes 1 gap-hour (weight 10), teacher contributes 1 gap-hour
    (weight 10); subtotal = 20. Item 57 widens the canonical objective with
    max_per_class_spread (2 placements on day 0, 0 on days 1-4 → spread=2,
    weight 10 → 20) and max_per_class_interior_gaps (1 gap, weight 10 → 10).
    Total = 20 + 20 + 10 = 50.
    """
    problem_json = _cpsat_forced_class_gap_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
    assert out["model_objective_value"] == 50


def _cpsat_forced_lopsided_spread_problem() -> str:
    """Two days, three TBs on day 0 (positions 0, 1, 2), zero TBs on day
    1. One class, one teacher, one room. One lesson with hours_per_week=3,
    preferred_block_size=1. Every placement must land on day 0 (no TBs on
    day 1). Spread is 3/0; D=2 days.

    score_solution: c[0]=3, c[1]=0, sum=3, D=2.
    scaled = |3*2 - 3| + |0*2 - 3| = 3 + 3 = 6
    quotient = 6 // 2 = 3
    Total class_day_balance = 5 * 3 = 15.

    No class_gap (3 contiguous placements, no interior missing). No
    teacher_gap. Only class_day_balance fires.
    """
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
                {"id": _cpsat_uuid(12), "day_of_week": 0, "position": 2},
                {"id": _cpsat_uuid(13), "day_of_week": 1, "position": 0},
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
                    "teacher_candidates": [_cpsat_uuid(20)],
                    "teacher_pin": _cpsat_uuid(20),
                    "hours_per_week": 3,
                    "preferred_block_size": 1,
                }
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [
                {"teacher_id": _cpsat_uuid(20), "time_block_id": _cpsat_uuid(13)}
            ],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_cpsat_objective_value_equals_score_solution_on_lopsided_spread_problem() -> None:
    """class_day_balance axis: 3 placements on day 0, 0 on day 1.
    quotient = (|3*2-3| + |0*2-3|) // 2 = 6 // 2 = 3; weighted = 5 * 3 = 15.
    Item 57 widens the canonical objective with max_per_class_spread (3
    placements on day 0, 0 on days 1-4 → spread=3, weight 10 → 30).
    Total = 15 + 30 = 45.
    """
    problem_json = _cpsat_forced_lopsided_spread_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
    assert out["model_objective_value"] == 45


def test_solve_cpsat_placement_carries_teacher_id() -> None:
    """CP-SAT must stamp teacher_id on every Placement matching the pin.

    OPEN_THINGS items 64 + 65: the CP-SAT bridge writes the lesson's
    teacher_pin into every emitted Placement so the persistence layer can
    record the solver's pick without dereferencing the input.
    """
    out = solve_cpsat_json(_cpsat_trivial_one_lesson_problem(), deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["violations"] == []
    assert sol["placements"], "expected at least one placement"
    for p in sol["placements"]:
        assert p.get("teacher_id") == _cpsat_uuid(20)


def _cpsat_two_unpinned_class_subject_lessons_problem(
    *,
    class_teacher_id: str | None = None,
    teacher_pin_for_lesson_1: str | None = None,
) -> str:
    """Two single-hour Mathematik lessons in the same class.

    Both lessons share teacher_candidates=[T1, T2]; subject lists both as
    qualified. Used by the items 66 / 67 / 68 CP-SAT tests to assert
    pairwise per-(class, subject) uniformity, klassenlehrer preference,
    and pin propagation through the uniformity constraint.
    """
    cls_id = _cpsat_uuid(50)
    subject_id = _cpsat_uuid(40)
    t1 = _cpsat_uuid(20)
    t2 = _cpsat_uuid(21)
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
                {"id": _cpsat_uuid(12), "day_of_week": 1, "position": 0},
                {"id": _cpsat_uuid(13), "day_of_week": 1, "position": 1},
            ],
            "teachers": [
                {"id": t1, "max_hours_per_week": 5},
                {"id": t2, "max_hours_per_week": 5},
            ],
            "rooms": [{"id": _cpsat_uuid(30)}],
            "subjects": [{"id": subject_id}],
            "school_classes": [
                {
                    "id": cls_id,
                    **({"class_teacher_id": class_teacher_id} if class_teacher_id else {}),
                }
            ],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [cls_id],
                    "subject_id": subject_id,
                    "teacher_candidates": [t1, t2],
                    "teacher_pin": teacher_pin_for_lesson_1,
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                },
                {
                    "id": _cpsat_uuid(61),
                    "school_class_ids": [cls_id],
                    "subject_id": subject_id,
                    "teacher_candidates": [t1, t2],
                    "teacher_pin": None,
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                },
            ],
            "teacher_qualifications": [
                {"teacher_id": t1, "subject_id": subject_id},
                {"teacher_id": t2, "subject_id": subject_id},
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_cpsat_picks_uniform_teacher_per_class_subject_pair_when_unpinned() -> None:
    """Item 66 CP-SAT side: pairwise per-(class, subject) uniformity.

    Two unpinned Mathematik lessons in the same class, each with two
    candidates. Without uniformity, CP-SAT could pick T1 for one and T2
    for the other (both feasible, soft cost identical). The pairwise
    uniformity constraint forces both to land on the same teacher.
    """
    problem_json = _cpsat_two_unpinned_class_subject_lessons_problem()
    out = solve_cpsat_json(problem_json, deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["violations"] == []
    assert len(sol["placements"]) == 2
    teachers = {p["teacher_id"] for p in sol["placements"]}
    assert len(teachers) == 1, f"expected uniform teacher per (class, subject): {teachers}"


def test_cpsat_prefers_klassenlehrer_when_qualified_unpinned() -> None:
    """Item 67 CP-SAT side: prefer_class_teacher soft cost steers the pick.

    Same fixture, class_teacher_id=T1; T1 is qualified for the subject.
    Both teachers are otherwise indifferent (no other soft-cost
    differential). The prefer_class_teacher term costs 5 per (class,
    subject) pair when a non-klt is chosen; CP-SAT minimises and picks T1.
    """
    t1 = _cpsat_uuid(20)
    problem_json = _cpsat_two_unpinned_class_subject_lessons_problem(class_teacher_id=t1)
    out = solve_cpsat_json(problem_json, deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["violations"] == []
    assert len(sol["placements"]) == 2
    for p in sol["placements"]:
        assert p["teacher_id"] == t1, f"expected klassenlehrer T1, got {p['teacher_id']}"


def test_cpsat_respects_teacher_pin_in_uniformity_pair() -> None:
    """Item 66 + 68 CP-SAT side: pin on one lesson propagates via uniformity.

    Lesson 1 pinned to T2; lesson 2 unpinned with [T1, T2] candidates.
    Per-(class, subject) uniformity forces both to T2 even though klt
    soft cost would prefer T1.
    """
    t1 = _cpsat_uuid(20)
    t2 = _cpsat_uuid(21)
    problem_json = _cpsat_two_unpinned_class_subject_lessons_problem(
        class_teacher_id=t1,
        teacher_pin_for_lesson_1=t2,
    )
    out = solve_cpsat_json(problem_json, deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["violations"] == []
    assert len(sol["placements"]) == 2
    for p in sol["placements"]:
        assert p["teacher_id"] == t2, f"expected pinned T2, got {p['teacher_id']}"
