# Request-ID contextvar implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `request_id` automatically appear on every `LogRecord` emitted inside an HTTP request, so route handlers no longer need to thread it through `extra=` manually.

**Architecture:** A module-level `contextvars.ContextVar` in `core/logging.py`, set in the access-log middleware, read by a `logging.Filter` attached to the root stream handler. Filter is explicit-over-implicit: it only injects `request_id` if the LogRecord does not already carry the field. Two commits: structural addition (filter + contextvar + unit tests + CLAUDE.md note) followed by behavioural switch (middleware sets/resets the contextvar, drops its own manual `extra` thread, integration test, OPEN_THINGS edit).

**Tech Stack:** Python 3.14, `contextvars` stdlib, `logging` stdlib, FastAPI middleware, pytest.

---

## File structure

- **Modify:** `backend/src/klassenzeit_backend/core/logging.py`
  - Add `request_id_var: ContextVar[str | None]`.
  - Add `RequestIdFilter(logging.Filter)`.
  - In `configure_logging`, call `handler.addFilter(RequestIdFilter())` next to `handler.setFormatter(...)`.
- **Modify:** `backend/tests/core/test_logging.py`
  - Add three unit tests on `RequestIdFilter`.
- **Modify:** `backend/CLAUDE.md`
  - Append one bullet under the existing "Logging" section.
- **Modify:** `backend/src/klassenzeit_backend/main.py:log_http_request`
  - Wrap `await call_next(request)` in `set` / `reset(token)` for the contextvar.
  - Drop `"request_id": request_id` from the access-log `extra=` dict.
  - Import `request_id_var` from `klassenzeit_backend.core.logging`.
- **Modify:** `backend/tests/test_http_access_middleware.py`
  - Add one integration test that asserts a route handler's `logger.info` record carries the same `request_id` as the access-log record and the `X-Request-ID` response header.
- **Modify:** `docs/superpowers/OPEN_THINGS.md`
  - Under "Toolchain & build friction → Structured logging follow-ups", strike item (a) and renumber (b)-(f) to (a)-(e).

---

## Commit 1 (structural): RequestIdFilter and contextvar

### Task 1: Add three failing unit tests for `RequestIdFilter`

**Files:**
- Test: `backend/tests/core/test_logging.py`

- [ ] **Step 1: Write the failing tests.**

Append to `backend/tests/core/test_logging.py` (after the existing tests, before the `_reset_configured_flag` fixture if that fixture is at the bottom; otherwise just at the end of the file):

```python
def test_request_id_filter_injects_value_from_contextvar() -> None:
    from klassenzeit_backend.core.logging import RequestIdFilter, request_id_var

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
    from klassenzeit_backend.core.logging import RequestIdFilter, request_id_var

    token = request_id_var.set("rid-from-ctx")
    try:
        record = _make_record(extra={"request_id": "explicit"})
        RequestIdFilter().filter(record)
        assert record.__dict__["request_id"] == "explicit"
    finally:
        request_id_var.reset(token)


def test_request_id_filter_no_op_when_contextvar_unset() -> None:
    from klassenzeit_backend.core.logging import RequestIdFilter

    record = _make_record()
    RequestIdFilter().filter(record)
    assert "request_id" not in record.__dict__
```

The three tests cover the explicit-over-implicit contract: contextvar fills the gap, explicit `extra` wins, and no-op when there is no request scope.

The imports stay inline at the top of each test function to make the failure mode obvious during the red step (the symbols do not exist yet, so the import errors first). After green, the imports can be hoisted to the top of the test file in a follow-up cleanup step (Task 3).

- [ ] **Step 2: Run the tests to verify they fail.**

Run: `mise run test:py -- backend/tests/core/test_logging.py -k request_id_filter -v`
Expected: 3 failures with `ImportError: cannot import name 'RequestIdFilter' from 'klassenzeit_backend.core.logging'` (or the equivalent ty-blocked import error if pre-commit ty runs first; the test runtime failure is the gate here).

### Task 2: Implement `RequestIdFilter` and `request_id_var`

**Files:**
- Modify: `backend/src/klassenzeit_backend/core/logging.py`

- [ ] **Step 1: Add the imports and module-level contextvar.**

At the top of `backend/src/klassenzeit_backend/core/logging.py`, after the existing imports, add:

```python
from contextvars import ContextVar
```

After `_REQUEST_ID_MAX_LEN: Final[int] = 64` (line 50), add:

```python
request_id_var: ContextVar[str | None] = ContextVar(
    "klassenzeit_request_id", default=None
)
```

- [ ] **Step 2: Add `RequestIdFilter` class.**

After `_resolve_request_id` (which ends around line 91), add:

```python
class RequestIdFilter(logging.Filter):
    """Inject `request_id` from the `request_id_var` ContextVar onto records.

    Runs before the formatter. If the record already carries `request_id`
    (e.g. an explicit `extra={"request_id": ...}` from a background task
    tagging a follow-up event), the explicit value wins. If the contextvar
    is unset (e.g. records emitted at startup or from a non-request task),
    the filter is a no-op.
    """

    def filter(self, record: logging.LogRecord) -> bool:
        if "request_id" not in record.__dict__:
            rid = request_id_var.get()
            if rid is not None:
                record.request_id = rid
        return True
```

- [ ] **Step 3: Attach the filter inside `configure_logging`.**

In `configure_logging`, immediately after `handler.setFormatter(...)` (whichever branch ran), add:

```python
    handler.addFilter(RequestIdFilter())
```

The full `configure_logging` body becomes:

```python
def configure_logging(
    *,
    env: Literal["dev", "test", "prod"],
    log_format: Literal["text", "json"] | None,
    log_level: str,
) -> None:
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
```

- [ ] **Step 4: Run the unit tests to verify they pass.**

Run: `mise run test:py -- backend/tests/core/test_logging.py -k request_id_filter -v`
Expected: 3 PASS.

- [ ] **Step 5: Run the full `test_logging.py` to verify no regression.**

Run: `mise run test:py -- backend/tests/core/test_logging.py -v`
Expected: all tests PASS (the new three plus the existing 14).

### Task 3: Hoist test imports + lint pass

**Files:**
- Modify: `backend/tests/core/test_logging.py`

- [ ] **Step 1: Move the inline imports to the top of the file.**

Add `RequestIdFilter` and `request_id_var` to the existing `from klassenzeit_backend.core.logging import ...` block at the top. Remove the inline `from klassenzeit_backend.core.logging import ...` lines from each of the three new tests. Ruff's `PLC0415` rule rejects in-function imports per the project standard in `backend/CLAUDE.md`.

The import block at the top becomes:

```python
from klassenzeit_backend.core.logging import (
    JsonFormatter,
    RequestIdFilter,
    _coerce,
    _resolve_request_id,
    configure_logging,
    request_id_var,
)
```

- [ ] **Step 2: Run lint to verify.**

Run: `mise run lint:py`
Expected: `All checks passed!`.

- [ ] **Step 3: Re-run the unit tests.**

Run: `mise run test:py -- backend/tests/core/test_logging.py -v`
Expected: all tests PASS.

### Task 4: Document the new contract in `backend/CLAUDE.md`

**Files:**
- Modify: `backend/CLAUDE.md`

- [ ] **Step 1: Append a bullet to the "Logging" section.**

Find the "## Logging" section in `backend/CLAUDE.md`. After the existing "Per-request access log lives on `klassenzeit_backend.http.access`..." bullet, append:

```markdown
- **Per-request `request_id` propagates automatically via `request_id_var`** (a `ContextVar[str | None]` in `core/logging.py`). The access middleware sets it after `_resolve_request_id` and resets it in a `finally`. A `RequestIdFilter` on the root stream handler injects the value onto any `LogRecord` emitted inside the request scope that does not already carry `request_id`. Routes do not need to thread `request_id` through `extra=`; explicit `extra={"request_id": "..."}` still wins (e.g. for a background task tagging a follow-up event with the originating request's id). Records emitted outside a request scope (startup, lifespan, non-request asyncio tasks) leave `request_id` absent.
```

- [ ] **Step 2: Verify the file parses (no markdown syntax break).**

Run: `head -200 backend/CLAUDE.md | tail -40` and visually confirm the bullet renders as a list item under "Logging".

### Task 5: Commit 1 (structural)

- [ ] **Step 1: Stage the structural changes.**

```bash
git add backend/src/klassenzeit_backend/core/logging.py \
        backend/tests/core/test_logging.py \
        backend/CLAUDE.md
git status
```
Expected: only those three files staged.

- [ ] **Step 2: Commit.**

```bash
git commit -m "$(cat <<'EOF'
feat(logging): add RequestIdFilter and request_id ContextVar

Module-level `request_id_var: ContextVar[str | None]` plus a
`RequestIdFilter` attached to the root stream handler in
`configure_logging`. The filter copies the contextvar value onto a
LogRecord that does not already carry `request_id`, so explicit
`extra={"request_id": ...}` still wins.

No caller sets the contextvar yet; the access middleware switch lands
in the next commit. Three unit tests cover the explicit-over-implicit
contract and the no-op-when-unset path.
EOF
)"
```
Expected: pre-commit lint passes; commit-msg `cog verify` passes.

## Commit 2 (behavioural): middleware uses the contextvar

### Task 6: Add a failing integration test

**Files:**
- Test: `backend/tests/test_http_access_middleware.py`

- [ ] **Step 1: Add the new test at the end of the file.**

```python
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
    access_record = next(
        r for r in caplog.records if r.name == "klassenzeit_backend.http.access"
    )
    probe_record = next(
        r for r in caplog.records if r.name == "klassenzeit_backend.tests.probe"
    )
    assert access_record.__dict__["request_id"] == "rid-probe-1"
    assert probe_record.__dict__["request_id"] == "rid-probe-1"
```

- [ ] **Step 2: Run the test to verify it fails.**

Run: `mise run test:py -- backend/tests/test_http_access_middleware.py::test_request_id_propagates_to_route_handler_log -v`
Expected: FAIL on `assert probe_record.__dict__["request_id"] == "rid-probe-1"` with `KeyError: 'request_id'`. The probe logger's record does not yet carry `request_id` because the middleware does not set the contextvar.

### Task 7: Update the middleware to set/reset the contextvar

**Files:**
- Modify: `backend/src/klassenzeit_backend/main.py`

- [ ] **Step 1: Update the import.**

Change line 20 of `backend/src/klassenzeit_backend/main.py` from:

```python
from klassenzeit_backend.core.logging import _resolve_request_id, configure_logging
```

to:

```python
from klassenzeit_backend.core.logging import (
    _resolve_request_id,
    configure_logging,
    request_id_var,
)
```

- [ ] **Step 2: Wrap `await call_next(request)` in `set`/`reset` and drop `request_id` from the access-log `extra=`.**

Replace the body of `log_http_request` (lines 110-130 in the current file) with:

```python
    @new_app.middleware("http")
    async def log_http_request(
        request: Request,
        call_next: Callable[[Request], Awaitable[Response]],
    ) -> Response:
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

Two changes from the current code: (1) `token = request_id_var.set(request_id)` after `request.state.request_id = ...`, with a matching `finally: request_id_var.reset(token)`; (2) drop `"request_id": request_id` from the `extra=` dict (the filter populates it).

- [ ] **Step 3: Run the integration test to verify it passes.**

Run: `mise run test:py -- backend/tests/test_http_access_middleware.py::test_request_id_propagates_to_route_handler_log -v`
Expected: PASS.

- [ ] **Step 4: Run the full middleware test file to verify no regression.**

Run: `mise run test:py -- backend/tests/test_http_access_middleware.py -v`
Expected: all 6 tests PASS (the new one plus the existing 5).

### Task 8: Update `OPEN_THINGS.md`

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Edit the structured-logging follow-ups bullet.**

Find the bullet in the "Toolchain & build friction" group beginning `**Structured logging follow-ups.**`. Strike `(a) `contextvars`-based request_id propagation so any in-request `logger.info` automatically carries the request_id without re-passing;` and renumber the remaining items so `(b)` becomes `(a)`, `(c)` becomes `(b)`, `(d)` becomes `(c)`, `(e)` becomes `(d)`, `(f)` becomes `(e)`.

The full updated bullet text:

```markdown
- **Structured logging follow-ups.** From `feat/structured-logging-backend` (ADR 0016): (a) replace or mute uvicorn's `--access-log` so production emits one access line per request, not two; (b) request / response body or `Content-Length` logging (needs PII review); (c) GCP / ECS / CloudWatch field renames once a log shipper is wired; (d) `solver-py` Rust-side structured logging once the Rust worker is real; (e) instrumenting CRUD route handlers on success and error. JSON formatter + `http.request` middleware shipped 2026-04-28; remaining items deferred from the DX/CI sprint with no firm owner. Item (a) (`contextvars`-based request_id propagation) shipped 2026-05-02 in `feat/request-id-contextvar`; siblings renumbered.
```

(The trailing sentence "Item (a) ... shipped 2026-05-02 ... siblings renumbered." documents the rename so future readers see why the (a)-(e) numbering looks different from prior PR descriptions.)

- [ ] **Step 2: Verify the edit.**

Run: `grep -A1 "Structured logging follow-ups" docs/superpowers/OPEN_THINGS.md | head -5`
Expected: the updated text shows `(a) replace or mute uvicorn's --access-log` as the first sub-item.

### Task 9: Run the full backend test suite

- [ ] **Step 1: Run all backend tests.**

Run: `mise run test:py`
Expected: all PASS.

- [ ] **Step 2: Run lint across the whole repo.**

Run: `mise run lint`
Expected: `All checks passed!` for ruff/ty/vulture/biome/clippy/etc.

### Task 10: Commit 2 (behavioural)

- [ ] **Step 1: Stage the behavioural changes.**

```bash
git add backend/src/klassenzeit_backend/main.py \
        backend/tests/test_http_access_middleware.py \
        docs/superpowers/OPEN_THINGS.md
git status
```
Expected: only those three files staged.

- [ ] **Step 2: Commit.**

```bash
git commit -m "$(cat <<'EOF'
refactor(logging): drop manual request_id thread from access middleware

Middleware now sets `request_id_var` after resolving the id and
resets it in a finally. The access-log `extra=` no longer includes
`request_id`; the RequestIdFilter populates it from the contextvar.

Adds an integration test that asserts a route handler's `logger.info`
record carries the same `request_id` as the access-log record and the
`X-Request-ID` response header.

Closes structured-logging follow-up (a) from OPEN_THINGS; remaining
items renumbered.
EOF
)"
```
Expected: pre-commit lint passes; commit-msg `cog verify` passes.

---

## Self-review checklist

- **Spec coverage.** Every spec section has a task: contextvar + filter (Task 2), `configure_logging` attaches it (Task 2 step 3), explicit-over-implicit (Task 1 + Task 2), middleware set/reset (Task 7), drop manual `extra` thread (Task 7), unit tests (Task 1), integration test (Task 6), CLAUDE.md doc (Task 4), OPEN_THINGS edit (Task 8), two-commit shape (Task 5 + Task 10).
- **Placeholders.** None. Every step shows the exact code or command.
- **Type consistency.** `request_id_var` (snake_case) is consistent across all references. `RequestIdFilter` (PascalCase) consistent. `klassenzeit_backend.tests.probe` logger name consistent between Task 4 description and Task 6 test.
- **Risks recheck.** The integration test uses `caplog.set_level` for both loggers explicitly, which is the pattern from the existing tests (line 15 of `test_http_access_middleware.py`); not a new pattern. The inline `@app.get("/__probe")` route registration on a `build_app(env="dev")` instance works because FastAPI accepts route registration on a constructed app prior to ASGI handshake; any other test in the file uses the constructed app immediately, so adding one route mid-test is safe.
