"""Tests for klassenzeit_backend.core.logging."""

import json
import logging
import sys
from uuid import UUID, uuid4

import pytest

import klassenzeit_backend.core.logging as kz_logging_module
from klassenzeit_backend.core.logging import (
    JsonFormatter,
    RequestIdFilter,
    _coerce,
    _resolve_request_id,
    configure_logging,
    request_id_var,
)


def _make_record(
    level: int = logging.INFO,
    msg: str = "test.event",
    extra: dict | None = None,
    exc_info=None,
) -> logging.LogRecord:
    record = logging.LogRecord(
        name="klassenzeit_backend.core.logging",
        level=level,
        pathname=__file__,
        lineno=10,
        msg=msg,
        args=(),
        exc_info=exc_info,
    )
    if extra:
        for key, value in extra.items():
            record.__dict__[key] = value
    return record


def test_json_formatter_emits_top_level_keys() -> None:
    record = _make_record()
    payload = json.loads(JsonFormatter().format(record))
    assert set(payload) >= {"ts", "level", "logger", "event"}
    assert payload["level"] == "INFO"
    assert payload["logger"] == "klassenzeit_backend.core.logging"
    assert payload["event"] == "test.event"
    assert payload["ts"].endswith("+00:00")


def test_json_formatter_merges_extra_payload() -> None:
    record = _make_record(extra={"a": 1, "b": "two"})
    payload = json.loads(JsonFormatter().format(record))
    assert payload["a"] == 1
    assert payload["b"] == "two"


def test_json_formatter_excludes_reserved_keys() -> None:
    record = _make_record()
    payload = json.loads(JsonFormatter().format(record))
    assert "pathname" not in payload
    assert "created" not in payload
    assert "msg" not in payload
    assert "args" not in payload


def test_json_formatter_serialises_uuid_via_str_fallback() -> None:
    uid = uuid4()
    record = _make_record(extra={"id": uid})
    payload = json.loads(JsonFormatter().format(record))
    assert payload["id"] == str(uid)
    assert UUID(payload["id"]) == uid


def test_json_formatter_includes_exc_info_on_error() -> None:
    try:
        raise RuntimeError("boom")
    except RuntimeError:
        record = _make_record(level=logging.ERROR, msg="solver.fail", exc_info=sys.exc_info())
    payload = json.loads(JsonFormatter().format(record))
    assert payload["exc_class"] == "RuntimeError"
    assert payload["exc_message"] == "boom"
    assert "Traceback" in payload["exc_stack"]


def test_coerce_returns_jsonable_values_unchanged() -> None:
    assert _coerce(1) == 1
    assert _coerce("x") == "x"
    assert _coerce([1, 2, 3]) == [1, 2, 3]


def test_coerce_falls_back_to_str_for_non_jsonable() -> None:
    uid = uuid4()
    assert _coerce(uid) == str(uid)


@pytest.fixture(autouse=True)
def _reset_configured_flag(monkeypatch: pytest.MonkeyPatch) -> None:
    """Reset the module-level idempotency guard before each test in this file."""
    monkeypatch.setattr(kz_logging_module, "_configured", False)


def test_configure_logging_is_idempotent() -> None:
    configure_logging(env="dev", log_format="text", log_level="INFO")
    handlers_after_first = list(logging.getLogger().handlers)
    configure_logging(env="dev", log_format="text", log_level="INFO")
    handlers_after_second = list(logging.getLogger().handlers)
    assert handlers_after_first == handlers_after_second
    assert len(handlers_after_second) == 1


def test_configure_logging_resolves_default_format_per_env() -> None:
    configure_logging(env="prod", log_format=None, log_level="INFO")
    handler = logging.getLogger().handlers[0]
    assert isinstance(handler.formatter, JsonFormatter)


def test_configure_logging_dev_default_uses_text_formatter() -> None:
    configure_logging(env="dev", log_format=None, log_level="INFO")
    handler = logging.getLogger().handlers[0]
    assert not isinstance(handler.formatter, JsonFormatter)


def test_configure_logging_explicit_text_overrides_prod_default() -> None:
    configure_logging(env="prod", log_format="text", log_level="INFO")
    handler = logging.getLogger().handlers[0]
    assert not isinstance(handler.formatter, JsonFormatter)


def test_configure_logging_sets_root_and_namespace_levels() -> None:
    configure_logging(env="dev", log_format="text", log_level="DEBUG")
    assert logging.getLogger().level == logging.DEBUG
    assert logging.getLogger("klassenzeit_backend").level == logging.DEBUG


def test_resolve_request_id_passes_valid_inbound_through() -> None:
    assert _resolve_request_id("abc123") == "abc123"


def test_resolve_request_id_generates_when_missing() -> None:
    rid = _resolve_request_id(None)
    assert len(rid) == 32
    UUID(rid)


def test_resolve_request_id_regenerates_on_oversized_inbound() -> None:
    rid = _resolve_request_id("x" * 100)
    assert rid != "x" * 100
    assert len(rid) == 32


def test_resolve_request_id_regenerates_on_empty_inbound() -> None:
    rid = _resolve_request_id("")
    assert len(rid) == 32


def test_request_id_filter_injects_value_from_contextvar() -> None:
    token = request_id_var.set("rid-from-ctx")
    try:
        record = _make_record()
        assert "request_id" not in record.__dict__
        result = RequestIdFilter().filter(record)
        assert result is True
        assert record.__dict__["request_id"] == "rid-from-ctx"
    finally:
        request_id_var.reset(token)


def test_request_id_filter_does_not_overwrite_explicit_value() -> None:
    token = request_id_var.set("rid-from-ctx")
    try:
        record = _make_record(extra={"request_id": "explicit"})
        RequestIdFilter().filter(record)
        assert record.__dict__["request_id"] == "explicit"
    finally:
        request_id_var.reset(token)


def test_request_id_filter_no_op_when_contextvar_unset() -> None:
    record = _make_record()
    RequestIdFilter().filter(record)
    assert "request_id" not in record.__dict__
