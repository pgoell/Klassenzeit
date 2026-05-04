"""Test that solve_json_with_config accepts the new LAHC period kwargs."""

import inspect

from klassenzeit_solver import solve_json_with_config

_MINIMAL_PROBLEM = (
    '{"time_blocks":[{"id":"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a","day_of_week":0,"position":0}],'
    '"teachers":[],'
    '"rooms":[{"id":"1e1e1e1e-1e1e-1e1e-1e1e-1e1e1e1e1e1e"}],'
    '"subjects":[],'
    '"school_classes":[],'
    '"lessons":[],'
    '"teacher_qualifications":[],'
    '"teacher_blocked_times":[],'
    '"room_blocked_times":[],'
    '"room_subject_suitabilities":[]}'
)


def test_solve_json_with_config_signature_includes_lahc_period_kwargs() -> None:
    sig = inspect.signature(solve_json_with_config)
    assert "lahc_rr_period" in sig.parameters
    assert "lahc_kempe_period" in sig.parameters


def test_solve_json_with_config_accepts_lahc_rr_period_kwarg() -> None:
    out = solve_json_with_config(_MINIMAL_PROBLEM, None, lahc_rr_period=25)
    assert '"placements"' in out


def test_solve_json_with_config_accepts_lahc_kempe_period_kwarg() -> None:
    out = solve_json_with_config(_MINIMAL_PROBLEM, None, lahc_rr_period=25, lahc_kempe_period=23)
    assert '"placements"' in out
