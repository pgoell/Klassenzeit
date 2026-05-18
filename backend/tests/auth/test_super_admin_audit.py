"""Tests for super_admin_audit_log capture (item 10g)."""

import uuid
from dataclasses import dataclass

import pytest

from klassenzeit_backend.auth.audit import should_audit_request


@dataclass
class _Membership:
    school_id: uuid.UUID


@dataclass
class _User:
    """Duck-typed User for predicate unit tests.

    The real ``is_super_admin(user)`` from ``auth.dependencies`` checks
    ``user.role == "super_admin"`` and the predicate reads ``user.school_id``
    / ``user.memberships``; this dataclass matches that surface.
    """

    id: uuid.UUID
    role: str
    school_id: uuid.UUID
    memberships: list[_Membership]


HOME = uuid.UUID("00000000-0000-0000-0000-000000000001")
MEMBER = uuid.UUID("00000000-0000-0000-0000-000000000002")
NON_MEMBER = uuid.UUID("00000000-0000-0000-0000-000000000003")


def _user(role: str) -> _User:
    return _User(
        id=uuid.uuid4(),
        role=role,
        school_id=HOME,
        memberships=[_Membership(school_id=MEMBER)],
    )


@pytest.mark.parametrize(
    ("role", "target", "method", "route", "expected"),
    [
        # super-admin on home school via tenanted route -> NO
        ("super_admin", HOME, "POST", "/api/rooms", False),
        ("super_admin", HOME, "PATCH", "/api/rooms/{room_id}", False),
        # super-admin on member school via tenanted route -> NO
        ("super_admin", MEMBER, "POST", "/api/rooms", False),
        # super-admin on non-member school via tenanted route -> YES
        ("super_admin", NON_MEMBER, "POST", "/api/rooms", True),
        ("super_admin", NON_MEMBER, "PATCH", "/api/rooms/{room_id}", True),
        ("super_admin", NON_MEMBER, "DELETE", "/api/rooms/{room_id}", True),
        (
            "super_admin",
            NON_MEMBER,
            "PUT",
            "/api/teachers/{teacher_id}/qualifications",
            True,
        ),
        # super-admin on /api/schools/* -> always YES (regardless of target_school_id)
        ("super_admin", None, "POST", "/api/schools", True),
        ("super_admin", HOME, "PATCH", "/api/schools/{school_id}", True),
        ("super_admin", NON_MEMBER, "DELETE", "/api/schools/{school_id}", True),
        # super-admin on read methods -> NO
        ("super_admin", NON_MEMBER, "GET", "/api/rooms", False),
        ("super_admin", None, "GET", "/api/schools", False),
        ("super_admin", None, "OPTIONS", "/api/schools", False),
        # regular admin on any school via any method -> NO
        ("admin", HOME, "POST", "/api/rooms", False),
        ("admin", NON_MEMBER, "POST", "/api/rooms", False),
        ("admin", None, "POST", "/api/schools", False),
        # regular user -> NO (defensive; should never reach a write route)
        ("user", NON_MEMBER, "POST", "/api/rooms", False),
    ],
)
def test_should_audit_request(
    role: str,
    target: uuid.UUID | None,
    method: str,
    route: str,
    expected: bool,
) -> None:
    user = _user(role=role)
    assert (
        should_audit_request(user, target, method, route)  # ty: ignore[invalid-argument-type]
        is expected
    )


def test_should_audit_request_returns_false_for_none_user() -> None:
    assert should_audit_request(None, None, "POST", "/api/rooms") is False


# ─── Integration tests (Task 3): SuperAdminAuditMiddleware ─────────────────

import logging  # noqa: E402

from httpx import AsyncClient  # noqa: E402
from sqlalchemy import select  # noqa: E402
from sqlalchemy.ext.asyncio import AsyncSession  # noqa: E402

from klassenzeit_backend.auth import audit_middleware  # noqa: E402
from klassenzeit_backend.auth.sessions import delete_user_sessions  # noqa: E402
from klassenzeit_backend.db.models.school import School  # noqa: E402
from klassenzeit_backend.db.models.super_admin_audit_log import (  # noqa: E402
    SuperAdminAuditLog,
)
from klassenzeit_backend.db.models.user_school_membership import (  # noqa: E402
    UserSchoolMembership,
)


async def _setup_two_schools(db_session: AsyncSession) -> tuple[School, School]:
    school_a = School(name="School A audit", short_name="SAA")
    school_b = School(name="School B audit", short_name="SBA")
    db_session.add_all([school_a, school_b])
    await db_session.flush()
    return school_a, school_b


@pytest.mark.asyncio
async def test_middleware_captures_super_admin_write_to_non_member_school(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    school_a, school_b = await _setup_two_schools(db_session)
    sa, _ = await create_test_user(
        email="sa-audit@test.com",
        password="pw_sa_audit_1234",  # noqa: S106
        role="super_admin",
        school_id=school_a.id,
    )
    await db_session.commit()
    await login_as("sa-audit@test.com", "pw_sa_audit_1234")
    # switch active school to school_b (a non-member school for sa)
    resp = await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})
    assert resp.status_code == 200

    resp = await client.post(
        "/api/rooms",
        json={"name": "Audit Room", "short_name": "AR", "capacity": 30},
    )
    assert resp.status_code == 201, resp.text

    rows = (await db_session.execute(select(SuperAdminAuditLog))).scalars().all()
    assert len(rows) == 1
    row = rows[0]
    assert row.actor_user_id == sa.id
    assert row.actor_user_email == "sa-audit@test.com"
    assert row.target_school_id == school_b.id
    assert row.target_school_name == "School B audit"
    assert row.method == "POST"
    assert row.route_template == "/api/rooms"
    assert row.path_params == {}
    assert row.request_body == {
        "name": "Audit Room",
        "short_name": "AR",
        "capacity": 30,
    }
    assert row.request_body_truncated is False
    assert row.response_status == 201
    # request_id correlates with the JSON log
    assert row.request_id is not None and len(row.request_id) > 0


@pytest.mark.asyncio
async def test_middleware_skips_super_admin_write_to_member_school(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    school_a, school_b = await _setup_two_schools(db_session)
    sa, _ = await create_test_user(
        email="sa-member@test.com",
        password="pw_sa_member_1234",  # noqa: S106
        role="super_admin",
        school_id=school_a.id,
    )
    db_session.add(UserSchoolMembership(user_id=sa.id, school_id=school_b.id))
    await db_session.commit()
    await login_as("sa-member@test.com", "pw_sa_member_1234")
    resp = await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})
    assert resp.status_code == 200

    resp = await client.post(
        "/api/rooms",
        json={"name": "Member Room", "short_name": "MR", "capacity": 25},
    )
    assert resp.status_code == 201, resp.text

    rows = (await db_session.execute(select(SuperAdminAuditLog))).scalars().all()
    assert rows == []


@pytest.mark.asyncio
async def test_middleware_skips_regular_admin_write(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    school_a, _ = await _setup_two_schools(db_session)
    _ra, _ = await create_test_user(
        email="ra-audit@test.com",
        password="pw_ra_audit_1234",  # noqa: S106
        role="admin",
        school_id=school_a.id,
    )
    await db_session.commit()
    await login_as("ra-audit@test.com", "pw_ra_audit_1234")

    resp = await client.post(
        "/api/rooms",
        json={"name": "Admin Room", "short_name": "AR2", "capacity": 30},
    )
    assert resp.status_code == 201, resp.text

    rows = (await db_session.execute(select(SuperAdminAuditLog))).scalars().all()
    assert rows == []


@pytest.mark.asyncio
async def test_middleware_skips_failed_write(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    school_a, school_b = await _setup_two_schools(db_session)
    _sa, _ = await create_test_user(
        email="sa-422@test.com",
        password="pw_sa_422_1234",  # noqa: S106
        role="super_admin",
        school_id=school_a.id,
    )
    await db_session.commit()
    await login_as("sa-422@test.com", "pw_sa_422_1234")
    resp = await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})
    assert resp.status_code == 200

    # missing required field "short_name" -> 422
    resp = await client.post("/api/rooms", json={"name": "OnlyName"})
    assert resp.status_code == 422

    rows = (await db_session.execute(select(SuperAdminAuditLog))).scalars().all()
    assert rows == []


@pytest.mark.asyncio
async def test_middleware_captures_post_schools(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    school_a, _ = await _setup_two_schools(db_session)
    _sa, _ = await create_test_user(
        email="sa-create@test.com",
        password="pw_sa_create_1234",  # noqa: S106
        role="super_admin",
        school_id=school_a.id,
    )
    await db_session.commit()
    await login_as("sa-create@test.com", "pw_sa_create_1234")

    resp = await client.post("/api/schools", json={"name": "New School", "short_name": "NS"})
    assert resp.status_code == 201, resp.text
    new_school_id = resp.json()["id"]

    rows = (await db_session.execute(select(SuperAdminAuditLog))).scalars().all()
    assert len(rows) == 1
    row = rows[0]
    assert row.method == "POST"
    assert row.route_template == "/api/schools"
    assert str(row.target_school_id) == new_school_id
    assert row.target_school_name == "New School"
    assert row.response_status == 201
    assert row.request_body == {"name": "New School", "short_name": "NS"}


@pytest.mark.asyncio
async def test_middleware_captures_delete_schools_with_pre_delete_name(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    school_a, school_b = await _setup_two_schools(db_session)
    _sa, _ = await create_test_user(
        email="sa-del@test.com",
        password="pw_sa_del_1234",  # noqa: S106
        role="super_admin",
        school_id=school_a.id,
    )
    await db_session.commit()
    await login_as("sa-del@test.com", "pw_sa_del_1234")

    resp = await client.delete(f"/api/schools/{school_b.id}")
    assert resp.status_code == 204

    rows = (await db_session.execute(select(SuperAdminAuditLog))).scalars().all()
    assert len(rows) == 1
    row = rows[0]
    assert row.method == "DELETE"
    assert row.route_template == "/api/schools/{school_id}"
    # the school row is deleted by the handler; SET NULL on FK fires only on
    # COMMIT inside the same nested savepoint context, but the audit row was
    # inserted AFTER the school delete (in the same session). Both behaviours
    # are acceptable for the test contract:
    # - target_school_id may be the now-orphan UUID, OR NULL if SET NULL fired
    # The snapshot column always survives:
    assert row.target_school_name == "School B audit"
    assert row.response_status == 204


@pytest.mark.asyncio
async def test_middleware_body_truncation(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    school_a, school_b = await _setup_two_schools(db_session)
    _sa, _ = await create_test_user(
        email="sa-big@test.com",
        password="pw_sa_big_1234",  # noqa: S106
        role="super_admin",
        school_id=school_a.id,
    )
    await db_session.commit()
    await login_as("sa-big@test.com", "pw_sa_big_1234")
    resp = await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})
    assert resp.status_code == 200

    # POST a body > 64 KiB by bloating with extra keys that Pydantic ignores
    # (RoomCreate has no extra="forbid"). The handler returns 201 because the
    # required fields are valid; the middleware captures the raw body BEFORE
    # the handler and marks it truncated because it exceeds the 64 KiB cap.
    padding = "X" * 70_000
    huge_payload = {
        "name": "Big Room",
        "short_name": "BR",
        "capacity": 30,
        "padding": padding,
    }
    resp = await client.post("/api/rooms", json=huge_payload)
    assert resp.status_code == 201, resp.text
    rows = (await db_session.execute(select(SuperAdminAuditLog))).scalars().all()
    assert len(rows) == 1
    assert rows[0].request_body_truncated is True


@pytest.mark.asyncio
async def test_middleware_snapshot_retention_on_user_delete(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user,
    login_as,
) -> None:
    school_a, school_b = await _setup_two_schools(db_session)
    sa, _ = await create_test_user(
        email="sa-snap@test.com",
        password="pw_sa_snap_1234",  # noqa: S106
        role="super_admin",
        school_id=school_a.id,
    )
    await db_session.commit()
    await login_as("sa-snap@test.com", "pw_sa_snap_1234")
    resp = await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})
    assert resp.status_code == 200
    resp = await client.post(
        "/api/rooms",
        json={"name": "Snap Room", "short_name": "SR", "capacity": 30},
    )
    assert resp.status_code == 201

    # delete the actor; FK SET NULL on super_admin_audit_log keeps the audit
    # row alive. The sessions table has ON DELETE RESTRICT (no cascade), so
    # clear the user's sessions first via the helper used by the auth flow.
    await delete_user_sessions(db_session, sa.id)
    await db_session.delete(sa)
    await db_session.commit()

    row = (await db_session.execute(select(SuperAdminAuditLog))).scalar_one()
    assert row.actor_user_id is None
    assert row.actor_user_email == "sa-snap@test.com"


@pytest.mark.asyncio
async def test_middleware_logs_db_insert_failure_without_500(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user,
    login_as,
    monkeypatch,
    caplog,
) -> None:
    school_a, school_b = await _setup_two_schools(db_session)
    _sa, _ = await create_test_user(
        email="sa-fail@test.com",
        password="pw_sa_fail_1234",  # noqa: S106
        role="super_admin",
        school_id=school_a.id,
    )
    await db_session.commit()
    await login_as("sa-fail@test.com", "pw_sa_fail_1234")
    resp = await client.post("/api/auth/switch-school", json={"school_id": str(school_b.id)})
    assert resp.status_code == 200

    # monkeypatch the middleware's insert helper to raise
    async def boom(*args, **kwargs):
        raise RuntimeError("simulated audit DB error")

    monkeypatch.setattr(audit_middleware, "_insert_audit_row", boom)

    with caplog.at_level(logging.ERROR):
        resp = await client.post(
            "/api/rooms",
            json={"name": "Fail Room", "short_name": "FR", "capacity": 30},
        )
    assert resp.status_code == 201  # user's write succeeds despite audit failure
    assert any(
        "audit.insert_failed" in (rec.message or "") or "audit.insert_failed" in (rec.msg or "")
        for rec in caplog.records
    )
