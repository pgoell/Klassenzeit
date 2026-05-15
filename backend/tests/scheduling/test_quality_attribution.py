"""Tests for ``compute_quality_attribution_for_class``.

The orchestrator recomputes the two derivable QualityReport attribution
metrics (class gap hours, home-room misses) from persisted placements.
Mirrors the recompute-on-GET pattern ``compute_quality_issues`` ships.
"""

import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.scheduling.quality_checks import (
    compute_quality_attribution_for_class,
)
from klassenzeit_backend.scheduling.schemas.quality_report import QualityReportResponse

pytestmark = pytest.mark.asyncio


async def test_compute_quality_attribution_for_class_returns_per_class_subtotals(
    db_session: AsyncSession,
    seed_placements_for_attribution,
) -> None:
    """Two seeded placements force one interior gap + one home-room miss."""
    class_id = await seed_placements_for_attribution(
        gap_positions_for_a_day=(1, 3),  # positions 1 + 3 on the same day = 1 gap hour
        place_one_outside_home_room=True,
    )
    report = await compute_quality_attribution_for_class(db_session, class_id)
    assert isinstance(report, QualityReportResponse)
    assert report.class_gap_hours_by_class[str(class_id)] == 1
    assert report.class_gap_hours == 1
    assert report.home_room_misses_by_class[str(class_id)] == 1
    assert report.home_room_misses == 1


async def test_compute_quality_attribution_for_class_skips_zero_entries(
    db_session: AsyncSession,
    seed_placements_for_attribution,
) -> None:
    """No gaps, all placements in home room: maps are empty per the skip-zero convention."""
    class_id = await seed_placements_for_attribution(
        gap_positions_for_a_day=(1, 2),  # contiguous, no gap
        place_one_outside_home_room=False,
    )
    report = await compute_quality_attribution_for_class(db_session, class_id)
    assert report.class_gap_hours_by_class == {}
    assert report.class_gap_hours == 0
    assert report.home_room_misses_by_class == {}
    assert report.home_room_misses == 0


async def test_compute_quality_attribution_for_class_zeros_non_derivable_fields(
    db_session: AsyncSession,
    seed_placements_for_attribution,
) -> None:
    """Non-derivable fields default to neutral values the Pydantic mirror accepts.

    Scalar int fields default to 0; ``dict[str, int]`` fields default to
    ``{}``. Documented in the schema docstring.
    """
    class_id = await seed_placements_for_attribution(
        gap_positions_for_a_day=(1, 3),
        place_one_outside_home_room=False,
    )
    report = await compute_quality_attribution_for_class(db_session, class_id)
    # Scalars default to 0.
    assert report.weighted_score == 0
    assert report.soft_pin_misses == 0
    assert report.supervision_spread_raw == 0
    assert report.class_day_balance_cost == 0
    assert report.teacher_gap_hours == 0
    assert report.hard_violations == 0
    assert report.unplaced_hours == 0
    assert report.prefer_early_units == 0
    assert report.avoid_first_units == 0
    assert report.avoid_last_units == 0
    assert report.prefer_late_units == 0
    assert report.prefer_class_teacher_misses == 0
    assert report.worst_per_class_spread == 0
    assert report.worst_per_class_interior_gaps == 0
    # Maps default to {}.
    assert report.class_day_balance_cost_by_class == {}
    assert report.teacher_gap_hours_by_teacher == {}


async def test_compute_quality_attribution_for_class_returns_zeros_for_unscheduled_class(
    db_session: AsyncSession,
    create_stundentafel,
    create_week_scheme,
    create_school_class,
) -> None:
    """A class with no placements yields an all-zero/empty QualityReportResponse."""
    tafel = await create_stundentafel()
    scheme = await create_week_scheme()
    cls = await create_school_class(stundentafel_id=tafel.id, week_scheme_id=scheme.id)
    report = await compute_quality_attribution_for_class(db_session, cls.id)
    assert report.class_gap_hours == 0
    assert report.class_gap_hours_by_class == {}
    assert report.home_room_misses == 0
    assert report.home_room_misses_by_class == {}
