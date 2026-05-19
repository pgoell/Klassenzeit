"""Integration tests for GET /api/auth/admin/audit-log (item 10g.1)."""

import uuid
from datetime import UTC, datetime, timedelta

import pytest
from httpx import AsyncClient
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID, School
from klassenzeit_backend.db.models.super_admin_audit_log import SuperAdminAuditLog


async def _seed_audit_row(
    db: AsyncSession,
    *,
    actor_id: uuid.UUID | None,
    actor_email: str,
    target_school_id: uuid.UUID | None,
    target_school_name: str | None,
    ts: datetime,
    method: str = "PATCH",
    route_template: str = "/api/schools/{school_id}",
    response_status: int = 200,
) -> SuperAdminAuditLog:
    row = SuperAdminAuditLog(
        actor_user_id=actor_id,
        actor_user_email=actor_email,
        target_school_id=target_school_id,
        target_school_name=target_school_name,
        ts=ts,
        method=method,
        route_template=route_template,
        path_params={"school_id": str(target_school_id) if target_school_id else "deleted"},
        request_body=None,
        request_body_truncated=False,
        response_status=response_status,
    )
    db.add(row)
    await db.flush()
    return row


async def _make_super_admin(db: AsyncSession, create_test_user, *, email: str) -> tuple:
    """Helper: create a user with role super_admin.

    Tries ``role=`` kwarg first; if the fixture doesn't accept it, falls back
    to a direct mutation. Either path returns (user, password).
    """
    try:
        return await create_test_user(email=email, role="super_admin")
    except TypeError:
        user, password = await create_test_user(email=email)
        user.role = "super_admin"
        await db.flush()
        return user, password


@pytest.mark.asyncio
async def test_audit_log_read_unauthenticated_returns_401(client: AsyncClient) -> None:
    resp = await client.get("/api/auth/admin/audit-log")
    assert resp.status_code == 401


@pytest.mark.asyncio
async def test_audit_log_read_admin_role_returns_403(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await create_test_user(email="admin1@example.com")
    # User defaults to role 'admin'; explicit mutation only if default differs.
    if user.role != "admin":
        user.role = "admin"
        await db_session.flush()
    await login_as(user.email, password)
    resp = await client.get("/api/auth/admin/audit-log")
    assert resp.status_code == 403


@pytest.mark.asyncio
async def test_audit_log_read_super_admin_empty(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super1@example.com"
    )
    await login_as(user.email, password)
    resp = await client.get("/api/auth/admin/audit-log")
    assert resp.status_code == 200
    assert resp.json() == {"items": [], "total": 0}


@pytest.mark.asyncio
async def test_audit_log_read_pagination_defaults(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super2@example.com"
    )
    base_ts = datetime(2026, 1, 1, 12, 0, 0, tzinfo=UTC)
    for i in range(60):
        await _seed_audit_row(
            db_session,
            actor_id=user.id,
            actor_email=user.email,
            target_school_id=DEFAULT_SCHOOL_ID,
            target_school_name="Default Schule",
            ts=base_ts + timedelta(seconds=i),
        )
    await db_session.commit()
    await login_as(user.email, password)

    body = (await client.get("/api/auth/admin/audit-log")).json()
    assert len(body["items"]) == 50
    assert body["total"] == 60

    body_page2 = (await client.get("/api/auth/admin/audit-log?skip=50")).json()
    assert len(body_page2["items"]) == 10
    assert body_page2["total"] == 60

    body_limited = (await client.get("/api/auth/admin/audit-log?limit=10")).json()
    assert len(body_limited["items"]) == 10


@pytest.mark.asyncio
async def test_audit_log_read_order_is_ts_desc(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super3@example.com"
    )
    t0 = datetime(2026, 1, 1, tzinfo=UTC)
    for offset in [0, 1, 2]:
        await _seed_audit_row(
            db_session,
            actor_id=user.id,
            actor_email=user.email,
            target_school_id=DEFAULT_SCHOOL_ID,
            target_school_name=f"School {offset}",
            ts=t0 + timedelta(hours=offset),
        )
    await db_session.commit()
    await login_as(user.email, password)
    body = (await client.get("/api/auth/admin/audit-log")).json()
    assert [item["target_school_name"] for item in body["items"]] == [
        "School 2",
        "School 1",
        "School 0",
    ]


@pytest.mark.asyncio
async def test_audit_log_read_filter_by_actor(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user_a, _ = await _make_super_admin(db_session, create_test_user, email="actor-a@example.com")
    user_b, _ = await _make_super_admin(db_session, create_test_user, email="actor-b@example.com")
    viewer, password = await _make_super_admin(
        db_session, create_test_user, email="viewer1@example.com"
    )
    base = datetime(2026, 2, 1, tzinfo=UTC)
    await _seed_audit_row(
        db_session,
        actor_id=user_a.id,
        actor_email=user_a.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="X",
        ts=base,
    )
    await _seed_audit_row(
        db_session,
        actor_id=user_b.id,
        actor_email=user_b.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="X",
        ts=base + timedelta(seconds=1),
    )
    await db_session.commit()
    await login_as(viewer.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log?actor_user_id={user_a.id}")).json()
    assert body["total"] == 1
    assert body["items"][0]["actor_user_email"] == "actor-a@example.com"


@pytest.mark.asyncio
async def test_audit_log_read_filter_by_target_school(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    viewer, password = await _make_super_admin(
        db_session, create_test_user, email="viewer2@example.com"
    )
    school_b = School(name="School B", short_name="SB")
    db_session.add(school_b)
    await db_session.flush()
    base = datetime(2026, 3, 1, tzinfo=UTC)
    await _seed_audit_row(
        db_session,
        actor_id=viewer.id,
        actor_email=viewer.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="Default Schule",
        ts=base,
    )
    await _seed_audit_row(
        db_session,
        actor_id=viewer.id,
        actor_email=viewer.email,
        target_school_id=school_b.id,
        target_school_name="School B",
        ts=base + timedelta(seconds=1),
    )
    await db_session.commit()
    await login_as(viewer.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log?target_school_id={school_b.id}")).json()
    assert body["total"] == 1
    assert body["items"][0]["target_school_name"] == "School B"


@pytest.mark.asyncio
async def test_audit_log_read_filter_time_range_inclusive(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    viewer, password = await _make_super_admin(
        db_session, create_test_user, email="viewer3@example.com"
    )
    t0 = datetime(2026, 4, 1, tzinfo=UTC)
    for offset in [-1, 0, 1]:
        await _seed_audit_row(
            db_session,
            actor_id=viewer.id,
            actor_email=viewer.email,
            target_school_id=DEFAULT_SCHOOL_ID,
            target_school_name="X",
            ts=t0 + timedelta(hours=offset),
        )
    await db_session.commit()
    await login_as(viewer.email, password)
    body = (
        await client.get(
            "/api/auth/admin/audit-log",
            params={
                "from_ts": t0.isoformat(),
                "to_ts": (t0 + timedelta(hours=1)).isoformat(),
            },
        )
    ).json()
    assert body["total"] == 2


@pytest.mark.asyncio
async def test_audit_log_read_multi_filter_intersection(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    actor, _ = await _make_super_admin(db_session, create_test_user, email="actor-mf@example.com")
    viewer, password = await _make_super_admin(
        db_session, create_test_user, email="viewer-mf@example.com"
    )
    school_b = School(name="School MF", short_name="MF")
    db_session.add(school_b)
    await db_session.flush()
    t0 = datetime(2026, 5, 1, tzinfo=UTC)
    await _seed_audit_row(
        db_session,
        actor_id=actor.id,
        actor_email=actor.email,
        target_school_id=school_b.id,
        target_school_name="School MF",
        ts=t0,
    )
    await _seed_audit_row(
        db_session,
        actor_id=viewer.id,
        actor_email=viewer.email,
        target_school_id=school_b.id,
        target_school_name="School MF",
        ts=t0,
    )
    await _seed_audit_row(
        db_session,
        actor_id=actor.id,
        actor_email=actor.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="Default Schule",
        ts=t0,
    )
    await _seed_audit_row(
        db_session,
        actor_id=actor.id,
        actor_email=actor.email,
        target_school_id=school_b.id,
        target_school_name="School MF",
        ts=t0 + timedelta(days=10),
    )
    await db_session.commit()
    await login_as(viewer.email, password)
    body = (
        await client.get(
            "/api/auth/admin/audit-log",
            params={
                "actor_user_id": str(actor.id),
                "target_school_id": str(school_b.id),
                "from_ts": t0.isoformat(),
                "to_ts": (t0 + timedelta(days=1)).isoformat(),
            },
        )
    ).json()
    assert body["total"] == 1
    assert body["items"][0]["actor_user_email"] == "actor-mf@example.com"


@pytest.mark.asyncio
async def test_audit_log_read_from_after_to_returns_422(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    viewer, password = await _make_super_admin(
        db_session, create_test_user, email="viewer4@example.com"
    )
    await login_as(viewer.email, password)
    t = datetime(2026, 6, 1, tzinfo=UTC)
    resp = await client.get(
        "/api/auth/admin/audit-log",
        params={
            "from_ts": (t + timedelta(hours=1)).isoformat(),
            "to_ts": t.isoformat(),
        },
    )
    assert resp.status_code == 422


@pytest.mark.asyncio
async def test_audit_log_read_null_snapshot_row(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    """Post-FK-delete shape: actor_user_id null, target_school_id null,
    target_school_name null; actor_user_email stays non-null (snapshot)."""
    viewer, password = await _make_super_admin(
        db_session, create_test_user, email="viewer5@example.com"
    )
    await _seed_audit_row(
        db_session,
        actor_id=None,
        actor_email="deleted-user@example.com",
        target_school_id=None,
        target_school_name=None,
        ts=datetime(2026, 7, 1, tzinfo=UTC),
    )
    await db_session.commit()
    await login_as(viewer.email, password)
    body = (await client.get("/api/auth/admin/audit-log")).json()
    assert body["total"] == 1
    row = body["items"][0]
    assert row["actor_user_id"] is None
    assert row["actor_user_email"] == "deleted-user@example.com"
    assert row["target_school_id"] is None
    assert row["target_school_name"] is None


# --- detail endpoint (item 10g.1b) ---


async def _seed_audit_row_with_body(
    db: AsyncSession,
    *,
    actor_id: uuid.UUID | None,
    actor_email: str,
    target_school_id: uuid.UUID | None,
    target_school_name: str | None,
    ts: datetime,
    path_params: dict[str, object] | None = None,
    request_body: dict[str, object] | list[object] | None = None,
    request_body_truncated: bool = False,
    request_id: str | None = None,
    method: str = "PATCH",
    route_template: str = "/api/schools/{school_id}",
    response_status: int = 200,
) -> SuperAdminAuditLog:
    default_pp = {"school_id": str(target_school_id) if target_school_id else "deleted"}
    row = SuperAdminAuditLog(
        actor_user_id=actor_id,
        actor_user_email=actor_email,
        target_school_id=target_school_id,
        target_school_name=target_school_name,
        ts=ts,
        method=method,
        route_template=route_template,
        path_params=path_params or default_pp,
        request_body=request_body,
        request_body_truncated=request_body_truncated,
        request_id=request_id,
        response_status=response_status,
    )
    db.add(row)
    await db.flush()
    return row


@pytest.mark.asyncio
async def test_audit_log_detail_unauthenticated_returns_401(client: AsyncClient) -> None:
    row_id = uuid.uuid4()
    resp = await client.get(f"/api/auth/admin/audit-log/{row_id}")
    assert resp.status_code == 401


@pytest.mark.asyncio
async def test_audit_log_detail_admin_role_returns_403(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await create_test_user(email="admin-detail@example.com")
    if user.role != "admin":
        user.role = "admin"
        await db_session.flush()
    await login_as(user.email, password)
    row_id = uuid.uuid4()
    resp = await client.get(f"/api/auth/admin/audit-log/{row_id}")
    assert resp.status_code == 403


@pytest.mark.asyncio
async def test_audit_log_detail_returns_404_for_missing_id(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-missing@example.com"
    )
    await login_as(user.email, password)
    resp = await client.get(f"/api/auth/admin/audit-log/{uuid.uuid4()}")
    assert resp.status_code == 404


@pytest.mark.asyncio
async def test_audit_log_detail_returns_422_for_malformed_uuid(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-422@example.com"
    )
    await login_as(user.email, password)
    resp = await client.get("/api/auth/admin/audit-log/not-a-uuid")
    assert resp.status_code == 422


@pytest.mark.asyncio
async def test_audit_log_detail_super_admin_happy_path(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-ok@example.com"
    )
    row = await _seed_audit_row_with_body(
        db_session,
        actor_id=user.id,
        actor_email=user.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="Default Schule",
        ts=datetime(2026, 7, 2, tzinfo=UTC),
        path_params={"school_id": str(DEFAULT_SCHOOL_ID)},
        request_body={"name": "Renamed"},
        request_body_truncated=False,
        request_id="req-abc",
    )
    await db_session.commit()
    await login_as(user.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log/{row.id}")).json()
    assert body["id"] == str(row.id)
    assert body["actor_user_email"] == user.email
    assert body["path_params"] == {"school_id": str(DEFAULT_SCHOOL_ID)}
    assert body["request_body"] == {"name": "Renamed"}
    assert body["request_body_truncated"] is False
    assert body["request_id"] == "req-abc"


@pytest.mark.asyncio
async def test_audit_log_detail_redacts_top_level_sensitive_keys(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-redact-top@example.com"
    )
    row = await _seed_audit_row_with_body(
        db_session,
        actor_id=user.id,
        actor_email=user.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="X",
        ts=datetime(2026, 7, 3, tzinfo=UTC),
        request_body={"password": "p455w0rd", "token": "tk", "name": "keep"},
    )
    await db_session.commit()
    await login_as(user.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log/{row.id}")).json()
    assert body["request_body"] == {
        "password": "[REDACTED]",
        "token": "[REDACTED]",
        "name": "keep",
    }


@pytest.mark.asyncio
async def test_audit_log_detail_redacts_nested_sensitive_keys(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-redact-nested@example.com"
    )
    row = await _seed_audit_row_with_body(
        db_session,
        actor_id=user.id,
        actor_email=user.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="X",
        ts=datetime(2026, 7, 4, tzinfo=UTC),
        request_body={"credentials": {"password_hash": "h", "user": "alice"}, "keep": 1},
    )
    await db_session.commit()
    await login_as(user.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log/{row.id}")).json()
    assert body["request_body"] == {
        "credentials": {"password_hash": "[REDACTED]", "user": "alice"},
        "keep": 1,
    }


@pytest.mark.asyncio
async def test_audit_log_detail_redacts_in_list_elements(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-redact-list@example.com"
    )
    row = await _seed_audit_row_with_body(
        db_session,
        actor_id=user.id,
        actor_email=user.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="X",
        ts=datetime(2026, 7, 5, tzinfo=UTC),
        request_body={"users": [{"name": "a", "pin": "1234"}, {"name": "b", "secret": "s"}]},
    )
    await db_session.commit()
    await login_as(user.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log/{row.id}")).json()
    assert body["request_body"] == {
        "users": [
            {"name": "a", "pin": "[REDACTED]"},
            {"name": "b", "secret": "[REDACTED]"},
        ],
    }


@pytest.mark.asyncio
async def test_audit_log_detail_redacts_mixed_case_keys(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-redact-case@example.com"
    )
    row = await _seed_audit_row_with_body(
        db_session,
        actor_id=user.id,
        actor_email=user.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="X",
        ts=datetime(2026, 7, 6, tzinfo=UTC),
        request_body={"Password": "P", "API_KEY": "K", "name": "keep"},
    )
    await db_session.commit()
    await login_as(user.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log/{row.id}")).json()
    assert body["request_body"] == {
        "Password": "[REDACTED]",
        "API_KEY": "[REDACTED]",
        "name": "keep",
    }


@pytest.mark.asyncio
async def test_audit_log_detail_handles_list_top_level_body(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-list-top@example.com"
    )
    row = await _seed_audit_row_with_body(
        db_session,
        actor_id=user.id,
        actor_email=user.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="X",
        ts=datetime(2026, 7, 7, tzinfo=UTC),
        request_body=[{"name": "a"}, {"password": "p"}],
    )
    await db_session.commit()
    await login_as(user.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log/{row.id}")).json()
    assert body["request_body"] == [{"name": "a"}, {"password": "[REDACTED]"}]


@pytest.mark.asyncio
async def test_audit_log_detail_handles_null_request_body(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-null-body@example.com"
    )
    row = await _seed_audit_row_with_body(
        db_session,
        actor_id=user.id,
        actor_email=user.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="X",
        ts=datetime(2026, 7, 8, tzinfo=UTC),
        request_body=None,
    )
    await db_session.commit()
    await login_as(user.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log/{row.id}")).json()
    assert body["request_body"] is None


@pytest.mark.asyncio
async def test_audit_log_detail_passes_truncated_flag_through(
    client: AsyncClient, db_session: AsyncSession, create_test_user, login_as
) -> None:
    user, password = await _make_super_admin(
        db_session, create_test_user, email="super-detail-truncated@example.com"
    )
    row = await _seed_audit_row_with_body(
        db_session,
        actor_id=user.id,
        actor_email=user.email,
        target_school_id=DEFAULT_SCHOOL_ID,
        target_school_name="X",
        ts=datetime(2026, 7, 9, tzinfo=UTC),
        request_body={"name": "x"},
        request_body_truncated=True,
    )
    await db_session.commit()
    await login_as(user.email, password)
    body = (await client.get(f"/api/auth/admin/audit-log/{row.id}")).json()
    assert body["request_body_truncated"] is True
