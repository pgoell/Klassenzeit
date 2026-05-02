"""Structured logging configuration for the backend.

`configure_logging` installs a JSON formatter (or a plain-text formatter) on
the root stream handler, scoped to a single call per process via an
idempotency guard. Existing `logger.info("event.name", extra={...})` call
sites stay untouched; their `extra` payload is merged into the JSON record
at top level by `JsonFormatter`.
"""

from __future__ import annotations

import json
import logging
import sys
import traceback
import uuid
from contextvars import ContextVar
from datetime import UTC, datetime
from typing import Any, Final, Literal

# LogRecord built-in attributes; everything else on record.__dict__ came
# from the caller's extra= dict and gets merged into the output payload.
_RESERVED: Final[frozenset[str]] = frozenset(
    {
        "name",
        "msg",
        "args",
        "levelname",
        "levelno",
        "pathname",
        "filename",
        "module",
        "exc_info",
        "exc_text",
        "stack_info",
        "lineno",
        "funcName",
        "created",
        "msecs",
        "relativeCreated",
        "thread",
        "threadName",
        "processName",
        "process",
        "message",
        "taskName",
        "asctime",
    }
)

_REQUEST_ID_MAX_LEN: Final[int] = 64

request_id_var: ContextVar[str | None] = ContextVar("klassenzeit_request_id", default=None)


def _coerce(value: Any) -> Any:
    """Best-effort coerce a non-JSON-serialisable value to a string."""
    try:
        json.dumps(value)
    except TypeError:
        return str(value)
    return value


class JsonFormatter(logging.Formatter):
    """Render `LogRecord` as a single JSON object per line."""

    def format(self, record: logging.LogRecord) -> str:
        """Format a log record as a JSON string."""
        payload: dict[str, Any] = {
            "ts": datetime.fromtimestamp(record.created, UTC).isoformat(),
            "level": record.levelname,
            "logger": record.name,
            "event": record.getMessage(),
        }
        if record.exc_info:
            exc_type, exc_value, exc_tb = record.exc_info
            payload["exc_class"] = exc_type.__name__ if exc_type else None
            payload["exc_message"] = str(exc_value) if exc_value else None
            payload["exc_stack"] = "".join(traceback.format_exception(exc_type, exc_value, exc_tb))
        for key, value in record.__dict__.items():
            if key in _RESERVED or key.startswith("_"):
                continue
            payload[key] = _coerce(value)
        return json.dumps(payload, default=str, ensure_ascii=False)


def _resolve_request_id(inbound: str | None) -> str:
    """Return a usable request id from an inbound header value, or generate one."""
    if inbound is None:
        return uuid.uuid4().hex
    if len(inbound) == 0 or len(inbound) > _REQUEST_ID_MAX_LEN:
        return uuid.uuid4().hex
    return inbound


class RequestIdFilter(logging.Filter):
    """Inject `request_id` from the `request_id_var` ContextVar onto records.

    Runs before the formatter. If the record already carries `request_id`
    (e.g. an explicit `extra={"request_id": ...}` from a background task
    tagging a follow-up event), the explicit value wins. If the contextvar
    is unset (e.g. records emitted at startup or from a non-request task),
    the filter is a no-op.
    """

    def filter(self, record: logging.LogRecord) -> bool:
        """Inject `request_id` from the contextvar if absent on the record."""
        if "request_id" not in record.__dict__:
            rid = request_id_var.get()
            if rid is not None:
                record.request_id = rid
        return True


_configured: bool = False


def configure_logging(
    *,
    env: Literal["dev", "test", "prod"],
    log_format: Literal["text", "json"] | None,
    log_level: str,
) -> None:
    """Idempotently install the chosen formatter on a single stream handler.

    `log_format=None` resolves to `"json"` when `env == "prod"`, else
    `"text"`. Subsequent calls in the same process are no-ops so test
    fixtures that rebuild the FastAPI app do not stack handlers.
    """
    global _configured  # noqa: PLW0603
    if _configured:
        return
    if log_format is not None:
        resolved: Literal["text", "json"] = log_format
    elif env == "prod":
        resolved = "json"
    else:
        resolved = "text"
    handler = logging.StreamHandler(sys.stdout)
    if resolved == "json":
        handler.setFormatter(JsonFormatter())
    else:
        handler.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(name)s %(message)s"))
    handler.addFilter(RequestIdFilter())
    root = logging.getLogger()
    root.handlers = [handler]
    root.setLevel(log_level)
    logging.getLogger("klassenzeit_backend").setLevel(log_level)
    _configured = True
