"""CP-SAT soft-pin objective parity (item 5 / ADR 0042).

The CP-SAT model objective must sum the `soft_pin_miss` axis (weight 5 per
unhonored soft pin) so cross-backend bench cells compare on the same Rust
scalar. Mirrors `solver_core::score::score_solution`'s new axis via three
sites in `klassenzeit_solver.cpsat`: `_W_SOFT_PIN_MISS`,
`_objective_soft_pin_term`, and the summand in `_emit_objective`. Also
asserts that soft pins are NOT forced by `_emit_pinned_placements` (they
are aspirational; hard pins remain forced).
"""

from __future__ import annotations

import json
import uuid

from klassenzeit_solver import score_solution_json, solve_cpsat_json


def _cpsat_soft_pin_uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def _cpsat_doppelstunde_with_off_pin_problem() -> dict[str, object]:
    """Doppelstunde fixture (prefer_late) with a soft pin at the EARLY slot.

    Five TBs on day 0 (positions 0..4) with subject.prefer_late_period=1.
    Anchoring the 2-block lesson at position 0 costs (4-0)+(4-1)=7 from
    prefer_late; anchoring at position 3 costs (4-3)+(4-4)=1. Honoring a
    soft pin at TB(position=0) would force the anchor=0 (window 0,1) and
    pay 7 - 1 = 6 extra. The miss penalty is `_W_SOFT_PIN_MISS * 1 = 5 < 6`,
    so the solver prefers the late anchor and accepts one soft-pin miss.
    """
    return {
        "time_blocks": [
            {"id": _cpsat_soft_pin_uuid(10), "day_of_week": 0, "position": 0},
            {"id": _cpsat_soft_pin_uuid(11), "day_of_week": 0, "position": 1},
            {"id": _cpsat_soft_pin_uuid(12), "day_of_week": 0, "position": 2},
            {"id": _cpsat_soft_pin_uuid(13), "day_of_week": 0, "position": 3},
            {"id": _cpsat_soft_pin_uuid(14), "day_of_week": 0, "position": 4},
        ],
        "teachers": [{"id": _cpsat_soft_pin_uuid(20), "max_hours_per_week": 5}],
        "rooms": [{"id": _cpsat_soft_pin_uuid(30)}],
        "subjects": [{"id": _cpsat_soft_pin_uuid(40), "prefer_late_period": 1}],
        "school_classes": [{"id": _cpsat_soft_pin_uuid(50)}],
        "lessons": [
            {
                "id": _cpsat_soft_pin_uuid(60),
                "school_class_ids": [_cpsat_soft_pin_uuid(50)],
                "subject_id": _cpsat_soft_pin_uuid(40),
                "teacher_candidates": [_cpsat_soft_pin_uuid(20)],
                "teacher_pin": _cpsat_soft_pin_uuid(20),
                "hours_per_week": 2,
                "preferred_block_size": 2,
            }
        ],
        "teacher_qualifications": [
            {"teacher_id": _cpsat_soft_pin_uuid(20), "subject_id": _cpsat_soft_pin_uuid(40)}
        ],
        "teacher_blocked_times": [],
        "room_blocked_times": [],
        "room_subject_suitabilities": [],
        "pinned_placements": [
            {
                "lesson_id": _cpsat_soft_pin_uuid(60),
                "time_block_id": _cpsat_soft_pin_uuid(10),
                "room_id": _cpsat_soft_pin_uuid(30),
                "kind": "soft",
            }
        ],
    }


def test_cpsat_objective_equals_score_solution_with_off_pin_soft_miss() -> None:
    """CP-SAT objective must equal `score_solution_json` when the optimal
    placement misses a soft pin: parity contract under the new axis."""
    problem_json = json.dumps(_cpsat_doppelstunde_with_off_pin_problem())
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["violations"] == [], f"expected feasible: {out['violations'][:3]}"
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical, (
        f"parity mismatch: cpsat={out['model_objective_value']} canonical={canonical}"
    )
    # Witness: max_pos=4; prefer_late steers to (3,4) for cost (4-3)+(4-4)=1;
    # per-class spread bills 20 for a one-day shape (spread=2 over Rust fixed
    # day axis 0..5, weight 10). Adding one unhonored soft pin contributes
    # _W_SOFT_PIN_MISS * 1 = 5. Total = 1 + 20 + 5 = 26.
    assert out["model_objective_value"] == 26


def test_cpsat_does_not_force_soft_pin_placement() -> None:
    """Soft pins are aspirational, not constraints: the solver chooses the
    late slot even though a soft pin sits at position 0. Companion check
    that `_emit_pinned_placements` filters on `kind != "soft"`."""
    problem_json = json.dumps(_cpsat_doppelstunde_with_off_pin_problem())
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["violations"] == []
    pinned_tb = _cpsat_soft_pin_uuid(10)
    placed_tbs = {p["time_block_id"] for p in out["placements"]}
    assert pinned_tb not in placed_tbs, (
        f"soft pin must not be forced by the model; placed_tbs={placed_tbs}"
    )
