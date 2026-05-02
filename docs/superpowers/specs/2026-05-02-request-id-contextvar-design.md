# Contextvars-based `request_id` propagation for backend logging

**Date:** 2026-05-02
**Status:** Design approved (autopilot autonomous mode).

## Context

ADR 0016 ships structured logging on `klassenzeit_backend`: `core/logging.py` owns a `JsonFormatter` and a one-shot `configure_logging(env, log_format, log_level)`. The HTTP access middleware in `main.py:log_http_request` resolves a `request_id` (via `_resolve_request_id`), pins it onto `request.state.request_id`, echoes it as the `X-Request-ID` response header, and passes it as `extra={"request_id": ...}` to the access-log line. No other call site in the codebase reads or threads the value, so per-request log correlation in production JSON logs is currently limited to the single `http.request` line emitted by the middleware. Routes that do log (`generate_lessons.done`, `solver.solve.done`, auth events) emit without `request_id`, breaking the correlation chain at the first inner event.

OPEN_THINGS row under "Toolchain & build friction → Structured logging follow-ups":

> **(a) `contextvars`-based request_id propagation so any in-request `logger.info` automatically carries the request_id without re-passing.**

This PR closes (a). Items (b) through (f) stay deferred.

## Goal

Make `request_id` automatically appear on every `LogRecord` emitted inside an HTTP request scope, so existing and future `logger.info("event.name", extra={...})` call sites do not need to look the value up off `request.state.request_id` and re-thread it through `extra`.

## Design

### `core/logging.py` additions

A module-level `ContextVar` plus a `logging.Filter`:

```python
request_id_var: ContextVar[str | None] = ContextVar(
    "klassenzeit_request_id", default=None
)


class RequestIdFilter(logging.Filter):
    def filter(self, record: logging.LogRecord) -> bool:
        if "request_id" not in record.__dict__:
            rid = request_id_var.get()
            if rid is not None:
                record.request_id = rid
        return True
```

The filter is attached by `configure_logging`:

```python
handler.setFormatter(...)
handler.addFilter(RequestIdFilter())
```

The existing `_configured` idempotency guard prevents the filter from stacking on repeat calls.

### Middleware contract

`main.py:log_http_request` is updated to set the contextvar around `await call_next(request)`:

```python
request_id = _resolve_request_id(request.headers.get("x-request-id"))
request.state.request_id = request_id
token = request_id_var.set(request_id)
try:
    started = time.monotonic()
    response = await call_next(request)
    duration_ms = (time.monotonic() - started) * 1000.0
    response.headers["X-Request-ID"] = request_id
    _ACCESS_LOGGER.info(
        "http.request",
        extra={
            "method": request.method,
            "path": request.url.path,
            "status": response.status_code,
            "duration_ms": duration_ms,
        },
    )
    return response
finally:
    request_id_var.reset(token)
```

Two changes to the existing middleware: (1) wrap with `set` / `reset(token)` for contextvar lifecycle hygiene; (2) drop the explicit `"request_id": request_id` key from the access-log `extra=` dict (the filter populates it from the contextvar).

`request.state.request_id` is preserved for any future code path that explicitly wants the value off the request object (e.g., a response-body interpolation); the filter and `request.state` are belt-and-suspenders for two different access patterns.

### Conflict rule (explicit-over-implicit)

If a call site explicitly passes `extra={"request_id": "..."}`, the filter does not overwrite it. This handles the legitimate case of a background task emitting a follow-up log line tagged with the request_id of the originating request:

```python
logger.info("background.followup", extra={"request_id": originating_rid, ...})
```

Today no such call site exists, but the rule keeps the door open without surprising future callers.

### Logger / handler scope

The filter attaches to the root stream handler (the same handler that owns the formatter). Filters on a handler run for every record that reaches the handler regardless of the logger that emitted the record, so third-party libraries (`uvicorn.error`, future ones) also gain `request_id` enrichment within a request scope. Narrower scope (`logging.getLogger("klassenzeit_backend")`) would lose third-party records, which is the wrong default for production tracing.

## Tests

### Unit tests (`backend/tests/core/test_logging.py`)

Three new tests on `RequestIdFilter`:

1. **Injects when contextvar set.** Set `request_id_var` to `"abc"` via `request_id_var.set("abc")`; build a `LogRecord` with no `request_id` in `__dict__`; call `filter.filter(record)`; assert `record.__dict__["request_id"] == "abc"`. Reset the token in a `try` / `finally` to keep test isolation.
2. **Skips when record already has `request_id`.** Set the contextvar to `"abc"`; build a record with `extra={"request_id": "explicit"}`; call the filter; assert the record's value is still `"explicit"`.
3. **No-op when contextvar unset.** Default contextvar value is `None`; build a record with no `request_id`; call the filter; assert `"request_id"` is absent from `record.__dict__`.

### Integration test (`backend/tests/test_http_access_middleware.py`)

One new test asserting end-to-end propagation through the middleware to a route handler's `logger.info` call:

- Build `app = build_app(env="dev")`.
- Inside the test, register an inline route `@app.get("/__probe")` whose handler emits `logger.info("test.probe", extra={"k": "v"})` via `logging.getLogger("klassenzeit_backend.tests.probe")`.
- Use `caplog.set_level(logging.INFO)` covering both `klassenzeit_backend.http.access` and `klassenzeit_backend.tests.probe`.
- Hit `/__probe` via `AsyncClient(transport=ASGITransport(app=app))`.
- Assert the probe-handler record AND the access-log record share the same `request_id` value, and that value matches the response's `X-Request-ID` header.

### Existing tests

`test_http_access_middleware.py` currently asserts `record.__dict__["request_id"]` directly on the access-log record (lines 31, 46, 61, 78). Because the filter populates the field on the LogRecord (not just in the JSON output), these assertions remain green after the explicit `extra` thread is removed from the middleware.

`test_logging.py`'s formatter and resolver tests are untouched (the formatter and `_resolve_request_id` do not change).

## Commit shape

Two commits on `feat/request-id-contextvar`:

- **Commit 1 (`feat(logging): add RequestIdFilter and request_id ContextVar`).** Structural addition: contextvar, filter, `configure_logging` attaches the filter, three filter unit tests, `backend/CLAUDE.md` "Logging" section gains one paragraph documenting the new contract. No middleware change, no observable behaviour change.
- **Commit 2 (`refactor(logging): drop manual request_id thread from access middleware`).** Behavioural switch: middleware sets / resets the contextvar around `call_next`; access-log `extra=` no longer includes `request_id`. Adds the integration test. Updates `OPEN_THINGS.md` to strike item (a) from the structured-logging follow-ups and renumber siblings.

The split honors the `.claude/CLAUDE.md` rule "A structural change and a behavioral change never ship in the same commit."

## Non-goals

- **No new ADR.** ADR 0016 already records the structured-logging architecture; this PR enriches it without an architectural pivot.
- **No retrofit of other call sites.** Routes' existing `logger.info(...)` calls already work; they get `request_id` for free after this PR. No need to touch them.
- **No `solver-py` Rust-side mirror.** That's structured-logging follow-up (e), explicitly a separate item.
- **No new logging fields beyond `request_id`.** A second contextvar (current user id, locale, tenant) is a future PR; the OPEN_THINGS item asks only for `request_id`.

## Verification

- `mise run test:py` green; the three new unit tests + one integration test pass; the four existing access-middleware assertions remain green.
- `mise run lint` green (ruff, ty, vulture); the filter class is used at module level in `configure_logging`, so vulture sees it.
- Manual smoke (optional): `mise run dev` + `curl http://localhost:8000/api/health` while tailing the JSON log; the access-log line carries `request_id`, and any DEBUG-level handler logs from the same request also carry it.
