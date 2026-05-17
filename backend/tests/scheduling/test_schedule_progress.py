"""HTTP integration tests for live solve progress and soft-cancel.

Item 3 from docs/OPEN_THINGS.md. Validates the `/schedule/progress` GET,
`/schedule/cancel` POST, the `was_cancelled` field on the schedule
response, and the registry-cleanup invariant on `app.state.solver_progress`.
"""

import asyncio
import time
import uuid
from datetime import time as dtime

import pytest
from fastapi import FastAPI
from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.teacher import TeacherQualification
from klassenzeit_backend.main import app as fastapi_app


@pytest.fixture
def app() -> FastAPI:
    """The ASGI app under test; mirrors the conftest's `client` import."""
    return fastapi_app


@pytest.fixture
async def seeded_class_id(
    db_session: AsyncSession,
    create_subject,
    create_week_scheme,
    create_time_block,
    create_room,
    create_teacher,
    create_stundentafel,
    create_school_class,
) -> uuid.UUID:
    """Seed a solvable single-class problem; return the class id.

    Mirrors `_seed_solvable_class` in `test_schedule_route.py` but exposes
    only the class id so the progress tests stay terse.
    """
    subject = await create_subject()
    week_scheme = await create_week_scheme()
    await create_time_block(
        week_scheme_id=week_scheme.id,
        position=0,
        start_time=dtime(8, 0),
        end_time=dtime(8, 45),
    )
    await create_room()
    teacher = await create_teacher()
    tafel = await create_stundentafel()
    cls = await create_school_class(
        name="1a-progress",
        stundentafel_id=tafel.id,
        week_scheme_id=week_scheme.id,
    )
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    lesson = Lesson(
        school_id=DEFAULT_SCHOOL_ID,
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    await db_session.flush()
    return cls.id


async def _login_admin(client: AsyncClient, create_test_user, login_as, label: str) -> None:
    """Create an admin and log them in. Encapsulates the boilerplate."""
    await create_test_user(email=f"admin@{label}.com", role="admin")
    await login_as(f"admin@{label}.com", "testpassword123")


async def test_progress_404_when_no_solve(
    client: AsyncClient,
    seeded_class_id: uuid.UUID,
    create_test_user,
    login_as,
) -> None:
    """GET /schedule/progress returns 404 outside of an in-flight solve."""
    await _login_admin(client, create_test_user, login_as, "progress-404")
    res = await client.get(f"/api/classes/{seeded_class_id}/schedule/progress")
    assert res.status_code == 404


async def test_cancel_404_when_no_solve(
    client: AsyncClient,
    seeded_class_id: uuid.UUID,
    create_test_user,
    login_as,
) -> None:
    """POST /schedule/cancel returns 404 outside of an in-flight solve."""
    await _login_admin(client, create_test_user, login_as, "cancel-404")
    res = await client.post(f"/api/classes/{seeded_class_id}/schedule/cancel")
    assert res.status_code == 404


async def test_happy_path_was_cancelled_false(
    client: AsyncClient,
    seeded_class_id: uuid.UUID,
    create_test_user,
    login_as,
) -> None:
    """A normal solve returns Schedule with was_cancelled=false."""
    await _login_admin(client, create_test_user, login_as, "happy-was-cancelled")
    res = await client.post(f"/api/classes/{seeded_class_id}/schedule")
    assert res.status_code == 200, res.text
    body = res.json()
    assert body["was_cancelled"] is False


async def test_progress_during_solve(
    client: AsyncClient,
    seeded_class_id: uuid.UUID,
    create_test_user,
    login_as,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A concurrent GET on /progress surfaces the merged snapshot."""
    await _login_admin(client, create_test_user, login_as, "progress-during")
    # Force the LAHC backend to spin for ~3s so the GET observes an in-flight solve.
    monkeypatch.setattr(fastapi_app.state.settings, "solver_backend", "lahc_rr")
    monkeypatch.setitem(fastapi_app.state.settings.solve_deadline_ms_by_backend, "lahc_rr", 3000)

    solve_task = asyncio.create_task(client.post(f"/api/classes/{seeded_class_id}/schedule"))
    snapshots: list[dict] = []
    deadline = time.monotonic() + 5.0
    try:
        while time.monotonic() < deadline:
            await asyncio.sleep(0.05)
            res = await client.get(f"/api/classes/{seeded_class_id}/schedule/progress")
            if res.status_code == 200:
                snapshots.append(res.json())
                break
    finally:
        await solve_task

    assert snapshots, "progress endpoint never returned 200 during the solve"
    last = snapshots[-1]
    assert last["iter"] >= 0
    assert last["placement_count"] >= 0
    assert "total_lessons" in last
    assert "deadline_ms" in last
    assert "elapsed_ms" in last
    assert last["cancel_requested"] is False


async def test_cancel_returns_was_cancelled_true(
    client: AsyncClient,
    seeded_class_id: uuid.UUID,
    create_test_user,
    login_as,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """POST /cancel mid-solve; original POST returns was_cancelled=true."""
    await _login_admin(client, create_test_user, login_as, "cancel-returns-true")
    monkeypatch.setattr(fastapi_app.state.settings, "solver_backend", "lahc_rr")
    monkeypatch.setitem(fastapi_app.state.settings.solve_deadline_ms_by_backend, "lahc_rr", 10_000)

    solve_task = asyncio.create_task(client.post(f"/api/classes/{seeded_class_id}/schedule"))
    # Wait for the solve to register
    for _ in range(80):
        await asyncio.sleep(0.025)
        res = await client.get(f"/api/classes/{seeded_class_id}/schedule/progress")
        if res.status_code == 200:
            break
    else:
        solve_task.cancel()
        pytest.fail("progress endpoint never registered the in-flight solve")

    cancel_res = await client.post(f"/api/classes/{seeded_class_id}/schedule/cancel")
    assert cancel_res.status_code == 204, cancel_res.text

    solve_res = await solve_task
    assert solve_res.status_code == 200, solve_res.text
    assert solve_res.json()["was_cancelled"] is True


async def test_cleanup_after_solve(
    client: AsyncClient,
    app: FastAPI,
    seeded_class_id: uuid.UUID,
    create_test_user,
    login_as,
) -> None:
    """`app.state.solver_progress` is emptied after the solve completes."""
    await _login_admin(client, create_test_user, login_as, "cleanup-after-solve")
    await client.post(f"/api/classes/{seeded_class_id}/schedule")
    assert seeded_class_id not in app.state.solver_progress
