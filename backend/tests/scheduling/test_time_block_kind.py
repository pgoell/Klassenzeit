"""Tests for TimeBlock.kind (lesson | break)."""

import datetime as dt
import json
import uuid
from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.room import Room
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.stundentafel import Stundentafel
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher, TeacherQualification
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.models.week_scheme import (
    TimeBlock,
    TimeBlockKind,
    WeekScheme,
)
from klassenzeit_backend.scheduling import solver_io
from klassenzeit_backend.scheduling.schemas.week_scheme import (
    TimeBlockCreate,
    TimeBlockResponse,
    TimeBlockUpdate,
)

type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]


@pytest.mark.asyncio
async def test_time_block_kind_defaults_to_lesson(db_session: AsyncSession) -> None:
    """A TimeBlock with no explicit kind defaults to LESSON after flush+refresh."""
    ws = WeekScheme(name=f"orm-default-{uuid.uuid4().hex[:8]}", description=None)
    db_session.add(ws)
    await db_session.flush()
    tb = TimeBlock(
        week_scheme_id=ws.id,
        day_of_week=0,
        position=1,
        start_time=dt.time(8, 0),
        end_time=dt.time(8, 45),
    )
    db_session.add(tb)
    await db_session.flush()
    await db_session.refresh(tb)
    assert tb.kind is TimeBlockKind.LESSON


@pytest.mark.asyncio
async def test_time_block_kind_persists_break(db_session: AsyncSession) -> None:
    """An explicit kind=BREAK round-trips through the native Postgres enum."""
    ws = WeekScheme(name=f"orm-break-{uuid.uuid4().hex[:8]}", description=None)
    db_session.add(ws)
    await db_session.flush()
    tb = TimeBlock(
        week_scheme_id=ws.id,
        day_of_week=0,
        position=1,
        start_time=dt.time(9, 30),
        end_time=dt.time(9, 50),
        kind=TimeBlockKind.BREAK,
    )
    db_session.add(tb)
    await db_session.flush()
    fetched = (
        await db_session.execute(select(TimeBlock).where(TimeBlock.id == tb.id))
    ).scalar_one()
    assert fetched.kind is TimeBlockKind.BREAK


def test_time_block_create_defaults_to_lesson_kind() -> None:
    """TimeBlockCreate defaults kind to LESSON when the field is omitted."""
    body = TimeBlockCreate(
        day_of_week=0, position=1, start_time=dt.time(8, 0), end_time=dt.time(8, 45)
    )
    assert body.kind.value == "lesson"


def test_time_block_create_accepts_break_kind() -> None:
    """TimeBlockCreate accepts an explicit BREAK kind via the enum."""
    body = TimeBlockCreate(
        day_of_week=0,
        position=1,
        start_time=dt.time(9, 30),
        end_time=dt.time(9, 50),
        kind=TimeBlockKind.BREAK,
    )
    assert body.kind.value == "break"


def test_time_block_create_parses_kind_from_json_string() -> None:
    """TimeBlockCreate.model_validate parses the wire string "break" to the enum."""
    body = TimeBlockCreate.model_validate(
        {
            "day_of_week": 0,
            "position": 1,
            "start_time": "09:30:00",
            "end_time": "09:50:00",
            "kind": "break",
        }
    )
    assert body.kind is TimeBlockKind.BREAK


def test_time_block_update_omits_kind_when_absent() -> None:
    """TimeBlockUpdate does not include kind in model_fields_set when omitted."""
    body = TimeBlockUpdate(start_time=dt.time(8, 5))
    assert "kind" not in body.model_fields_set


def test_time_block_update_carries_kind_when_set() -> None:
    """TimeBlockUpdate sets kind in model_fields_set when explicitly provided."""
    body = TimeBlockUpdate(kind=TimeBlockKind.BREAK)
    assert "kind" in body.model_fields_set
    assert body.kind is not None
    assert body.kind.value == "break"


def test_time_block_response_serializes_kind_as_lowercase_value() -> None:
    """TimeBlockResponse.model_dump emits the lowercase enum value, not the name."""
    body = TimeBlockResponse(
        id=uuid.UUID("00000000-0000-0000-0000-000000000000"),
        day_of_week=0,
        position=1,
        start_time=dt.time(8, 0),
        end_time=dt.time(8, 45),
        kind=TimeBlockKind.LESSON,
    )
    dumped = body.model_dump(mode="json")
    assert dumped["kind"] == "lesson"


@pytest.mark.asyncio
async def test_post_time_block_defaults_kind_to_lesson(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /time-blocks without kind in the body returns kind=lesson."""
    await create_test_user(email="admin@tbk1.com", role="admin")
    await login_as("admin@tbk1.com", "testpassword123")
    r = await client.post("/api/week-schemes", json={"name": f"ws-{uuid.uuid4().hex[:8]}"})
    assert r.status_code == 201, r.text
    ws_id = r.json()["id"]
    payload = {
        "day_of_week": 0,
        "position": 1,
        "start_time": "08:00:00",
        "end_time": "08:45:00",
    }
    r = await client.post(f"/api/week-schemes/{ws_id}/time-blocks", json=payload)
    assert r.status_code == 201, r.text
    assert r.json()["kind"] == "lesson"


@pytest.mark.asyncio
async def test_post_time_block_persists_break_kind(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /time-blocks with kind=break round-trips the wire value."""
    await create_test_user(email="admin@tbk2.com", role="admin")
    await login_as("admin@tbk2.com", "testpassword123")
    r = await client.post("/api/week-schemes", json={"name": f"ws-{uuid.uuid4().hex[:8]}"})
    assert r.status_code == 201, r.text
    ws_id = r.json()["id"]
    payload = {
        "day_of_week": 0,
        "position": 1,
        "start_time": "09:30:00",
        "end_time": "09:50:00",
        "kind": "break",
    }
    r = await client.post(f"/api/week-schemes/{ws_id}/time-blocks", json=payload)
    assert r.status_code == 201, r.text
    assert r.json()["kind"] == "break"


@pytest.mark.asyncio
async def test_patch_time_block_updates_kind_only(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /time-blocks/{id} with kind=break flips kind without touching other fields."""
    await create_test_user(email="admin@tbk3.com", role="admin")
    await login_as("admin@tbk3.com", "testpassword123")
    r = await client.post("/api/week-schemes", json={"name": f"ws-{uuid.uuid4().hex[:8]}"})
    assert r.status_code == 201, r.text
    ws_id = r.json()["id"]
    r = await client.post(
        f"/api/week-schemes/{ws_id}/time-blocks",
        json={
            "day_of_week": 0,
            "position": 1,
            "start_time": "08:00:00",
            "end_time": "08:45:00",
        },
    )
    assert r.status_code == 201
    tb_id = r.json()["id"]
    r = await client.patch(f"/api/week-schemes/{ws_id}/time-blocks/{tb_id}", json={"kind": "break"})
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["kind"] == "break"
    assert body["start_time"] == "08:00:00"  # other fields untouched


@pytest.mark.asyncio
async def test_patch_time_block_preserves_kind_when_omitted(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /time-blocks/{id} that omits kind leaves the existing kind in place."""
    await create_test_user(email="admin@tbk4.com", role="admin")
    await login_as("admin@tbk4.com", "testpassword123")
    r = await client.post("/api/week-schemes", json={"name": f"ws-{uuid.uuid4().hex[:8]}"})
    assert r.status_code == 201
    ws_id = r.json()["id"]
    r = await client.post(
        f"/api/week-schemes/{ws_id}/time-blocks",
        json={
            "day_of_week": 0,
            "position": 1,
            "start_time": "09:30:00",
            "end_time": "09:50:00",
            "kind": "break",
        },
    )
    assert r.status_code == 201
    tb_id = r.json()["id"]
    r = await client.patch(
        f"/api/week-schemes/{ws_id}/time-blocks/{tb_id}",
        json={"start_time": "09:35:00"},
    )
    assert r.status_code == 200
    body = r.json()
    assert body["kind"] == "break"
    assert body["start_time"] == "09:35:00"


@pytest.mark.asyncio
async def test_build_problem_json_excludes_break_time_blocks(
    db_session: AsyncSession,
) -> None:
    """build_problem_json must drop kind=break rows from the solver payload.

    Fixture lays out 8 TimeBlocks on day 0 in the order L,L,B,L,L,B,L,L
    (positions 1..8). The solver payload must surface only the 6 LESSON rows
    and must exclude positions 3 and 6.
    """
    subject = Subject(name=f"Subj-{uuid.uuid4().hex[:8]}", short_name="S1", color="chart-1")
    db_session.add(subject)
    scheme = WeekScheme(name=f"ws-mixed-{uuid.uuid4().hex[:8]}", description=None)
    db_session.add(scheme)
    await db_session.flush()

    kinds_by_position = {
        1: TimeBlockKind.LESSON,
        2: TimeBlockKind.LESSON,
        3: TimeBlockKind.BREAK,
        4: TimeBlockKind.LESSON,
        5: TimeBlockKind.LESSON,
        6: TimeBlockKind.BREAK,
        7: TimeBlockKind.LESSON,
        8: TimeBlockKind.LESSON,
    }
    for pos, kind in kinds_by_position.items():
        start = dt.time(8 + pos - 1, 0)
        end = dt.time(8 + pos - 1, 45)
        db_session.add(
            TimeBlock(
                week_scheme_id=scheme.id,
                day_of_week=0,
                position=pos,
                start_time=start,
                end_time=end,
                kind=kind,
            )
        )

    room = Room(name=f"Room-{uuid.uuid4().hex[:8]}", short_name="R1", capacity=None)
    db_session.add(room)
    teacher = Teacher(
        first_name="T",
        last_name=f"Teach-{uuid.uuid4().hex[:8]}",
        short_code=f"TC-{uuid.uuid4().hex[:6]}",
        max_hours_per_week=24,
        reserve_hours_per_week=0,
    )
    db_session.add(teacher)
    tafel = Stundentafel(name=f"Tafel-{uuid.uuid4().hex[:8]}", grade_level=5)
    db_session.add(tafel)
    await db_session.flush()
    cls = SchoolClass(
        name=f"Class-{uuid.uuid4().hex[:6]}",
        grade_level=5,
        stundentafel_id=tafel.id,
        week_scheme_id=scheme.id,
        home_room_id=None,
    )
    db_session.add(cls)
    await db_session.flush()
    lesson = Lesson(
        subject_id=subject.id,
        teacher_id=teacher.id,
        hours_per_week=1,
        preferred_block_size=1,
    )
    db_session.add(lesson)
    await db_session.flush()
    db_session.add(LessonSchoolClass(lesson_id=lesson.id, school_class_id=cls.id))
    db_session.add(TeacherQualification(teacher_id=teacher.id, subject_id=subject.id))
    await db_session.flush()

    problem_json, _, _ = await solver_io.build_problem_json(db_session, cls.id)
    payload = json.loads(problem_json)
    payload_positions = {(tb["day_of_week"], tb["position"]) for tb in payload["time_blocks"]}
    # 8 rows inserted, 6 LESSON + 2 BREAK; expect only the 6 LESSON rows.
    assert len(payload["time_blocks"]) == 6, payload["time_blocks"]
    assert (0, 3) not in payload_positions  # break position must be absent
    assert (0, 6) not in payload_positions
