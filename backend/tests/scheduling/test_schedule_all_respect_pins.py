"""POST /api/schedule/all?respect_pins=... behaves correctly.

Sprint C Task 5. Two cases:
- ``respect_pins=true`` (default): the pinned slot survives and the row keeps
  ``pinned=True``.
- ``respect_pins=false``: the solver may move the pinned lesson, but the
  ``pinned`` flag in the DB is unchanged (per spec section 3.3).
"""

from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.pin_kind import PinKind
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.user import User
from tests.scheduling.conftest import SeededDreizuegigWithPin

CreateUserFnPins = Callable[..., Awaitable[tuple[User, str]]]
LoginFnPins = Callable[[str, str], Awaitable[None]]


@pytest.mark.asyncio
async def test_schedule_all_default_respects_pins(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user: CreateUserFnPins,
    login_as: LoginFnPins,
    seeded_dreizuegig_with_one_pin: SeededDreizuegigWithPin,
) -> None:
    """No flag passed: default to respect_pins=true; pinned slot survives."""
    await create_test_user(email="admin@all-pins-default.com", role="admin")
    await login_as("admin@all-pins-default.com", "testpassword123")
    fixture = seeded_dreizuegig_with_one_pin
    response = await client.post("/api/schedule/all")
    assert response.status_code == 200, response.text
    pinned = (
        await db_session.execute(
            select(ScheduledLesson).where(ScheduledLesson.lesson_id == fixture.pinned_lesson_id)
        )
    ).scalar_one()
    assert pinned.time_block_id == fixture.pinned_time_block_id
    assert pinned.pin_kind is PinKind.HARD


@pytest.mark.asyncio
async def test_schedule_all_respect_pins_false_keeps_pin_state(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user: CreateUserFnPins,
    login_as: LoginFnPins,
    seeded_dreizuegig_with_one_pin: SeededDreizuegigWithPin,
) -> None:
    """respect_pins=false: solver may move the pinned lesson; flag stays in DB."""
    await create_test_user(email="admin@all-pins-false.com", role="admin")
    await login_as("admin@all-pins-false.com", "testpassword123")
    fixture = seeded_dreizuegig_with_one_pin
    response = await client.post("/api/schedule/all?respect_pins=false")
    assert response.status_code == 200, response.text
    pinned_rows = (
        (
            await db_session.execute(
                select(ScheduledLesson).where(ScheduledLesson.lesson_id == fixture.pinned_lesson_id)
            )
        )
        .scalars()
        .all()
    )
    assert len(pinned_rows) == 1
    # Pin state is preserved across the run; only the slot may have changed.
    assert pinned_rows[0].pin_kind is PinKind.HARD


@pytest.mark.asyncio
async def test_schedule_all_response_carries_quality_report(
    client: AsyncClient,
    db_session: AsyncSession,
    create_test_user: CreateUserFnPins,
    login_as: LoginFnPins,
    seeded_dreizuegig_with_one_pin: SeededDreizuegigWithPin,
) -> None:
    """POST /api/schedule/all response carries a quality_report payload.

    Item 58 wire-format extension; same shape as the single-class endpoint,
    scoped to the whole-school solve. ``WholeSchoolScheduleResponse`` does
    not surface a Solution-level ``soft_score`` (only ``total_placements`` /
    ``total_violations``), so the ``weighted_score == soft_score`` parity
    cannot be asserted here directly; the invariant is pinned on the Rust
    side by ``solver-core/tests/solution_quality_report_json.rs``.
    """
    await create_test_user(email="admin@all-pins-qr.com", role="admin")
    await login_as("admin@all-pins-qr.com", "testpassword123")
    fixture = seeded_dreizuegig_with_one_pin  # noqa: F841 (binds session for the fixture)
    response = await client.post("/api/schedule/all")
    assert response.status_code == 200, response.text
    body = response.json()
    assert "quality_report" in body, "quality_report must be on the wire format"
    qr = body["quality_report"]
    expected_fields = {
        "hard_violations",
        "unplaced_hours",
        "class_gap_hours",
        "class_gap_hours_by_class",
        "teacher_gap_hours",
        "teacher_gap_hours_by_teacher",
        "class_day_balance_cost",
        "class_day_balance_cost_by_class",
        "home_room_misses",
        "home_room_misses_by_class",
        "prefer_early_units",
        "avoid_first_units",
        "avoid_last_units",
        "prefer_late_units",
        "prefer_class_teacher_misses",
        "weighted_score",
        "worst_per_class_spread",
        "worst_per_class_interior_gaps",
        "soft_pin_misses",
        "supervision_spread_raw",
    }
    assert expected_fields == set(qr.keys()), (
        f"quality_report fields drift from solver-core: "
        f"missing={expected_fields - set(qr.keys())}, "
        f"extra={set(qr.keys()) - expected_fields}"
    )
