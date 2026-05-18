"""Starlette middleware capturing super-admin cross-school writes (item 10g)."""

import inspect
import json
import logging
import uuid
from collections.abc import AsyncIterator
from typing import Any

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from starlette.middleware.base import BaseHTTPMiddleware, RequestResponseEndpoint
from starlette.requests import Request
from starlette.responses import Response
from starlette.types import Message

from klassenzeit_backend.auth.audit import should_audit_request
from klassenzeit_backend.auth.sessions import lookup_session
from klassenzeit_backend.db.models.school import School
from klassenzeit_backend.db.models.session import UserSession
from klassenzeit_backend.db.models.super_admin_audit_log import SuperAdminAuditLog
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.session import get_session

logger = logging.getLogger(__name__)

_BODY_CAP_BYTES = 64 * 1024
_WRITE_METHODS = frozenset({"POST", "PATCH", "PUT", "DELETE"})
_HTTP_2XX_MIN = 200
_HTTP_3XX_MIN = 300
# Auth routes are self-service (login, logout, password change, active-school
# switch). They are not tenant-aggregate writes, so audit-log capture does
# not apply even when a super-admin is the actor.
_AUDIT_EXEMPT_PREFIXES: tuple[str, ...] = ("/api/auth/",)


class SuperAdminAuditMiddleware(BaseHTTPMiddleware):
    """One audit row per super-admin write that used elevation.

    Reads the kz_session cookie, evaluates ``should_audit_request``, and
    inserts via the same ``get_session`` dependency the handlers use
    (going through ``app.dependency_overrides`` so the test fixture's
    per-test savepoint session is inherited). Never raises into the
    user's request path.
    """

    async def dispatch(self, request: Request, call_next: RequestResponseEndpoint) -> Response:
        """Buffer the body, run the handler, then maybe record an audit row."""
        body_bytes, truncated = await self._buffer_request_body(request)

        # Pre-handler snapshot for DELETE /api/schools/{school_id}: fetch
        # the school's name BEFORE the handler deletes it.
        pre_target_name: str | None = None
        if request.method == "DELETE" and request.url.path.startswith("/api/schools/"):
            school_id_str = self._extract_path_param(request.url.path, "/api/schools/")
            if school_id_str:
                pre_target_name = await self._lookup_school_name(request, school_id_str)

        response: Response = await call_next(request)

        if request.method not in _WRITE_METHODS:
            return response
        if not (_HTTP_2XX_MIN <= response.status_code < _HTTP_3XX_MIN):
            return response
        if any(request.url.path.startswith(p) for p in _AUDIT_EXEMPT_PREFIXES):
            return response

        try:
            await self._maybe_audit(request, response, body_bytes, truncated, pre_target_name)
        except Exception as exc:  # never block the user's response
            logger.error(
                "audit.insert_failed",
                extra={
                    "method": request.method,
                    "path": request.url.path,
                    "error": str(exc),
                },
            )
        return response

    async def _buffer_request_body(self, request: Request) -> tuple[bytes | None, bool]:
        """Pre-read the request body and install a one-shot replay.

        The handler can then read it normally via ``request._receive``.
        Returns the (possibly truncated) captured body and a flag that
        records whether truncation occurred.
        """
        if request.method not in {"POST", "PATCH", "PUT"}:
            return None, False
        content_type = request.headers.get("content-type", "").split(";")[0].strip().lower()
        if content_type != "application/json":
            return None, False

        raw = await request.body()
        truncated = len(raw) > _BODY_CAP_BYTES
        captured = raw[:_BODY_CAP_BYTES] if truncated else raw

        sent = False

        async def replay_request_body() -> Message:
            nonlocal sent
            if sent:
                # After more_body=False, handlers should not re-read; return
                # an empty terminator to be safe.
                return {"type": "http.request", "body": b"", "more_body": False}
            sent = True
            return {"type": "http.request", "body": raw, "more_body": False}

        request._receive = replay_request_body  # type: ignore[attr-defined]
        return captured, truncated

    def _extract_path_param(self, path: str, prefix: str) -> str | None:
        """Return the first path segment after ``prefix`` (no UUID parsing)."""
        if not path.startswith(prefix):
            return None
        rest = path[len(prefix) :]
        return rest.split("/", 1)[0] if rest else None

    async def _lookup_school_name(self, request: Request, school_id_str: str) -> str | None:
        """Resolve the school name from a path-param UUID, or None."""
        try:
            school_id = uuid.UUID(school_id_str)
        except ValueError:
            return None
        async for db in self._get_audit_session(request):
            row = await db.get(School, school_id)
            return row.name if row else None
        return None

    def _get_audit_session(self, request: Request) -> AsyncIterator[AsyncSession]:
        """Return the (overridable) get_session yielder for this request.

        In tests, ``app.dependency_overrides[get_session]`` is set to a
        function that yields the per-test ``db_session`` (savepoint-bound);
        the middleware inherits that, so audit writes roll back with the
        test. In production, the real ``get_session`` opens a fresh
        AsyncSession from ``app.state.session_factory``.

        The test override has signature ``() -> AsyncIterator[AsyncSession]``
        while the production yielder is ``(request: Request) -> ...``; we
        pass ``request`` only when the callable's signature accepts it.
        """
        yielder = request.app.dependency_overrides.get(get_session, get_session)
        try:
            sig = inspect.signature(yielder)
            takes_request = len(sig.parameters) >= 1
        except (TypeError, ValueError):
            takes_request = True
        return yielder(request) if takes_request else yielder()

    async def _maybe_audit(
        self,
        request: Request,
        response: Response,
        body_bytes: bytes | None,
        truncated: bool,
        pre_target_name: str | None,
    ) -> None:
        """Insert one audit row if the predicate matches; else no-op."""
        cookie = request.cookies.get("kz_session")
        if cookie is None:
            return
        try:
            session_id = uuid.UUID(cookie)
        except ValueError:
            return

        route_template = self._route_template(request)
        if route_template is None:
            return

        async for db in self._get_audit_session(request):
            user_session = await lookup_session(db, session_id)
            if user_session is None:
                return
            user = await db.scalar(select(User).where(User.id == user_session.user_id))
            if user is None:
                return
            # ensure memberships are loaded for the predicate
            await db.refresh(user, attribute_names=["memberships"])

            target_school_id, target_school_name = await self._resolve_target(
                request, response, route_template, user_session, db, pre_target_name
            )

            if not should_audit_request(user, target_school_id, request.method, route_template):
                return

            body_json: Any = None
            if body_bytes:
                try:
                    body_json = json.loads(body_bytes.decode("utf-8"))
                except (UnicodeDecodeError, json.JSONDecodeError):
                    body_json = None

            await _insert_audit_row(
                db,
                actor_user_id=user.id,
                actor_user_email=user.email,
                target_school_id=target_school_id,
                target_school_name=target_school_name,
                request_id=getattr(request.state, "request_id", None),
                method=request.method,
                route_template=route_template,
                path_params=dict(request.path_params),
                request_body=body_json,
                request_body_truncated=truncated,
                response_status=response.status_code,
            )

    def _route_template(self, request: Request) -> str | None:
        """Return the matched route template (e.g. ``/api/schools/{id}``)."""
        route = request.scope.get("route")
        return getattr(route, "path", None)

    async def _resolve_target(  # noqa: PLR0911 — one branch per schools verb
        self,
        request: Request,
        response: Response,
        route_template: str,
        user_session: UserSession,
        db: AsyncSession,
        pre_target_name: str | None,
    ) -> tuple[uuid.UUID | None, str | None]:
        """Identify the (school_id, school_name) pair to record on the row."""
        if request.method == "POST" and route_template == "/api/schools":
            parsed = await self._consume_and_replay_response_json(response)
            if parsed is None:
                logger.error("audit.snapshot_parse_failed", extra={"route": route_template})
                return None, None
            try:
                return uuid.UUID(parsed["id"]), parsed["name"]
            except (KeyError, ValueError):
                logger.error("audit.snapshot_parse_failed", extra={"route": route_template})
                return None, None

        if request.method == "PATCH" and route_template == "/api/schools/{school_id}":
            try:
                target_id = uuid.UUID(request.path_params["school_id"])
            except (KeyError, ValueError):
                return None, None
            parsed = await self._consume_and_replay_response_json(response)
            name = (parsed or {}).get("name") if isinstance(parsed, dict) else None
            return target_id, name or pre_target_name

        if request.method == "DELETE" and route_template == "/api/schools/{school_id}":
            # The school row was deleted by the handler in this same session;
            # inserting an audit row with target_school_id = <orphan UUID>
            # would violate the FK. The snapshot column (target_school_name)
            # preserves the trail; null out the FK.
            return None, pre_target_name

        # Tenanted routes: scope_school_id = session.active_school_id
        target_id = user_session.active_school_id
        if target_id is None:
            return None, None
        row = await db.get(School, target_id)
        return target_id, (row.name if row else None)

    async def _consume_and_replay_response_json(self, response: Response) -> dict[str, Any] | None:
        """Drain ``response.body_iterator``, reinstall it, and parse as JSON.

        Returns the parsed JSON object, or None on any read / parse failure.
        ``BaseHTTPMiddleware`` wraps the inner handler's Response in a
        ``_StreamingResponse``, so ``body_iterator`` is always present here;
        the static type is the base ``Response`` though, hence the ty hints.
        """
        body = b""
        async for chunk in response.body_iterator:  # ty: ignore[unresolved-attribute]
            body += chunk

        async def replay_response_body() -> AsyncIterator[bytes]:
            yield body

        response.body_iterator = replay_response_body()  # ty: ignore[unresolved-attribute]
        if not body:
            return None
        try:
            parsed = json.loads(body.decode("utf-8"))
            return parsed if isinstance(parsed, dict) else None
        except (UnicodeDecodeError, json.JSONDecodeError):
            return None


async def _insert_audit_row(db: AsyncSession, **kwargs: Any) -> None:
    """Insert one audit row and commit. Separate function for monkeypatch."""
    db.add(SuperAdminAuditLog(**kwargs))
    await db.commit()
