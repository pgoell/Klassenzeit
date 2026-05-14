"""Pydantic schema tests for Teacher.reserve_hours_per_week (Vertretungsreserve)."""

import pytest
from pydantic import ValidationError

from klassenzeit_backend.scheduling.schemas.teacher import (
    TeacherCreate,
    TeacherUpdate,
)


def test_teacher_create_default_reserve_is_zero() -> None:
    """Omitting the field on TeacherCreate falls back to 0."""
    teacher = TeacherCreate(
        first_name="Anna",
        last_name="Müller",
        short_code="AMU",
        max_hours_per_week=28,
    )
    assert teacher.reserve_hours_per_week == 0


def test_teacher_create_explicit_reserve_accepted() -> None:
    """A positive integer is round-tripped onto TeacherCreate."""
    teacher = TeacherCreate(
        first_name="Anna",
        last_name="Müller",
        short_code="AMU",
        max_hours_per_week=28,
        reserve_hours_per_week=4,
    )
    assert teacher.reserve_hours_per_week == 4


def test_teacher_create_negative_reserve_rejected() -> None:
    """Negative values fail validation (ge=0) on TeacherCreate."""
    with pytest.raises(ValidationError):
        TeacherCreate(
            first_name="Anna",
            last_name="Müller",
            short_code="AMU",
            max_hours_per_week=28,
            reserve_hours_per_week=-1,
        )


def test_teacher_update_none_means_unchanged() -> None:
    """Omitted / None reserve disappears from ``model_dump(exclude_none=True)``."""
    update = TeacherUpdate(reserve_hours_per_week=None)
    dumped = update.model_dump(exclude_none=True)
    assert "reserve_hours_per_week" not in dumped


def test_teacher_update_zero_clears_reserve() -> None:
    """Explicit 0 is preserved (caller can clear the reserve)."""
    update = TeacherUpdate(reserve_hours_per_week=0)
    assert update.reserve_hours_per_week == 0
    assert "reserve_hours_per_week" in update.model_dump(exclude_none=True)


def test_teacher_update_negative_reserve_rejected() -> None:
    """Negative values fail validation (ge=0) on TeacherUpdate."""
    with pytest.raises(ValidationError):
        TeacherUpdate(reserve_hours_per_week=-1)


def test_teacher_create_working_days_none_by_default() -> None:
    """Omitting working_days leaves it None (full-time)."""
    t = TeacherCreate(first_name="Dana", last_name="Test", short_code="DT", max_hours_per_week=20)
    assert t.working_days is None


def test_teacher_create_working_days_round_trip() -> None:
    """Mo/Di/Mi survives validation and normalizes to sorted list."""
    t = TeacherCreate(
        first_name="Dana",
        last_name="Test",
        short_code="DT",
        max_hours_per_week=20,
        working_days={2, 0, 1},
    )
    assert t.working_days == [0, 1, 2]


def test_teacher_create_working_days_rejects_empty() -> None:
    """Empty set is meaningless; reject with ValidationError."""
    with pytest.raises(ValidationError):
        TeacherCreate(
            first_name="Dana",
            last_name="Test",
            short_code="DT",
            max_hours_per_week=20,
            working_days=set(),
        )


def test_teacher_create_working_days_rejects_out_of_range() -> None:
    """Element 5 (Saturday) fails (weekdays only)."""
    with pytest.raises(ValidationError):
        TeacherCreate(
            first_name="Dana",
            last_name="Test",
            short_code="DT",
            max_hours_per_week=20,
            working_days={0, 5},
        )


def test_teacher_update_working_days_explicit_null_in_fields_set() -> None:
    """Explicit None on update lands in model_fields_set so the route can clear."""
    update = TeacherUpdate(working_days=None)
    assert "working_days" in update.model_fields_set


def test_teacher_update_working_days_omitted_not_in_fields_set() -> None:
    """Omitted working_days stays out of model_fields_set."""
    update = TeacherUpdate(first_name="X")
    assert "working_days" not in update.model_fields_set
