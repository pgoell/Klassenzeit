"""CP-SAT determinism: same inputs -> byte-identical output."""

import json
import uuid

from klassenzeit_solver import solve_cpsat_json


def _cpsat_det_uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def _cpsat_det_problem() -> str:
    # Two TBs, two lessons (1h each, same teacher/room/class), some choice in placement.
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_det_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_det_uuid(11), "day_of_week": 0, "position": 1},
            ],
            "teachers": [{"id": _cpsat_det_uuid(20), "max_hours_per_week": 5}],
            "rooms": [{"id": _cpsat_det_uuid(30)}],
            "subjects": [{"id": _cpsat_det_uuid(40)}],
            "school_classes": [{"id": _cpsat_det_uuid(50)}],
            "lessons": [
                {
                    "id": _cpsat_det_uuid(60),
                    "school_class_ids": [_cpsat_det_uuid(50)],
                    "subject_id": _cpsat_det_uuid(40),
                    "teacher_id": _cpsat_det_uuid(20),
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                },
                {
                    "id": _cpsat_det_uuid(61),
                    "school_class_ids": [_cpsat_det_uuid(50)],
                    "subject_id": _cpsat_det_uuid(40),
                    "teacher_id": _cpsat_det_uuid(20),
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                },
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_det_uuid(20), "subject_id": _cpsat_det_uuid(40)}
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_solve_cpsat_json_deterministic_under_seed_and_deadline() -> None:
    p = _cpsat_det_problem()
    a = json.loads(solve_cpsat_json(p, deadline_ms=2_000, seed=7))
    b = json.loads(solve_cpsat_json(p, deadline_ms=2_000, seed=7))
    # Determinism applies to the solver outputs (placements, violations,
    # soft_score). The observability fields (peak_rss_kb,
    # time_to_first_feasible_ms, time_to_optimal_ms) are wall-clock /
    # process-state measurements and necessarily differ per run.
    for k in ("peak_rss_kb", "time_to_first_feasible_ms", "time_to_optimal_ms"):
        a.pop(k, None)
        b.pop(k, None)
    assert a == b
