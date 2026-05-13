"""QualityReportResponse: per-axis cost-vector breakdown surfaced on schedule responses.

Mirrors ``solver_core::quality::QualityReport`` (Rust) field-for-field. Pinned
parity with the solver: ``weighted_score == Solution.soft_score`` (the contract
is enforced in
``solver-core/tests/quality_property.rs::quality_report_weighted_score_matches_score_solution``
and JSON-roundtripped by
``solution_quality_report_survives_json_roundtrip_and_matches_soft_score``).

Item 58.
"""

from pydantic import BaseModel, ConfigDict


class QualityReportResponse(BaseModel):
    """Per-axis cost-vector breakdown of the solver's soft score."""

    model_config = ConfigDict(extra="forbid")

    hard_violations: int
    unplaced_hours: int
    class_gap_hours: int
    teacher_gap_hours: int
    class_day_balance_cost: int
    home_room_misses: int
    prefer_early_units: int
    avoid_first_units: int
    avoid_last_units: int
    prefer_late_units: int
    prefer_class_teacher_misses: int
    weighted_score: int
    worst_per_class_spread: int
    worst_per_class_interior_gaps: int
