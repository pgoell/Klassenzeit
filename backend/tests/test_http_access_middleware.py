"""Tests for the http.request access middleware in main.build_app."""

import logging
import re

import pytest
from httpx import ASGITransport, AsyncClient

from klassenzeit_backend.main import build_app


async def test_access_middleware_emits_http_request_event(
    caplog: pytest.LogCaptureFixture,
) -> None:
    caplog.set_level(logging.INFO, logger="klassenzeit_backend.http.access")
    app = build_app(env="dev")
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        response = await client.get("/api/health")

    assert response.status_code == 200
    records = [r for r in caplog.records if r.name == "klassenzeit_backend.http.access"]
    assert len(records) == 1
    record = records[0]
    assert record.message == "http.request"
    assert record.__dict__["method"] == "GET"
    assert record.__dict__["path"] == "/api/health"
    assert record.__dict__["status"] == 200
    assert isinstance(record.__dict__["duration_ms"], float)
    assert record.__dict__["duration_ms"] >= 0.0
    assert isinstance(record.__dict__["request_id"], str)
    assert len(record.__dict__["request_id"]) > 0


async def test_access_middleware_propagates_inbound_request_id(
    caplog: pytest.LogCaptureFixture,
) -> None:
    caplog.set_level(logging.INFO, logger="klassenzeit_backend.http.access")
    app = build_app(env="dev")
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        response = await client.get("/api/health", headers={"X-Request-ID": "client-corr-1"})

    assert response.headers["X-Request-ID"] == "client-corr-1"
    record = next(r for r in caplog.records if r.name == "klassenzeit_backend.http.access")
    assert record.__dict__["request_id"] == "client-corr-1"


async def test_access_middleware_generates_request_id_on_absence(
    caplog: pytest.LogCaptureFixture,
) -> None:
    caplog.set_level(logging.INFO, logger="klassenzeit_backend.http.access")
    app = build_app(env="dev")
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        response = await client.get("/api/health")

    rid = response.headers["X-Request-ID"]
    assert re.fullmatch(r"[0-9a-f]{32}", rid) is not None
    record = next(r for r in caplog.records if r.name == "klassenzeit_backend.http.access")
    assert record.__dict__["request_id"] == rid


async def test_access_middleware_caps_oversized_request_id(
    caplog: pytest.LogCaptureFixture,
) -> None:
    caplog.set_level(logging.INFO, logger="klassenzeit_backend.http.access")
    app = build_app(env="dev")
    transport = ASGITransport(app=app)
    oversized = "x" * 100
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        response = await client.get("/api/health", headers={"X-Request-ID": oversized})

    rid = response.headers["X-Request-ID"]
    assert rid != oversized
    assert re.fullmatch(r"[0-9a-f]{32}", rid) is not None
    record = next(r for r in caplog.records if r.name == "klassenzeit_backend.http.access")
    assert record.__dict__["request_id"] == rid


async def test_access_middleware_records_duration_under_one_second_for_health_check(
    caplog: pytest.LogCaptureFixture,
) -> None:
    caplog.set_level(logging.INFO, logger="klassenzeit_backend.http.access")
    app = build_app(env="dev")
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        await client.get("/api/health")

    record = next(r for r in caplog.records if r.name == "klassenzeit_backend.http.access")
    assert 0.0 <= record.__dict__["duration_ms"] < 1000.0


async def test_request_id_propagates_to_route_handler_log(
    caplog: pytest.LogCaptureFixture,
) -> None:
    caplog.set_level(logging.INFO, logger="klassenzeit_backend.http.access")
    caplog.set_level(logging.INFO, logger="klassenzeit_backend.tests.probe")
    app = build_app(env="dev")
    probe_logger = logging.getLogger("klassenzeit_backend.tests.probe")

    @app.get("/__probe")
    async def _probe() -> dict[str, str]:
        probe_logger.info("test.probe", extra={"k": "v"})
        return {"ok": "1"}

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        response = await client.get("/__probe", headers={"X-Request-ID": "rid-probe-1"})

    assert response.status_code == 200
    assert response.headers["X-Request-ID"] == "rid-probe-1"
    access_record = next(r for r in caplog.records if r.name == "klassenzeit_backend.http.access")
    probe_record = next(r for r in caplog.records if r.name == "klassenzeit_backend.tests.probe")
    assert access_record.__dict__["request_id"] == "rid-probe-1"
    assert probe_record.__dict__["request_id"] == "rid-probe-1"
