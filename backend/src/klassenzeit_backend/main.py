"""FastAPI entry point for the Klassenzeit backend.

The ``lifespan`` context manager owns the async engine, session factory,
settings, and rate limiter. They live on ``app.state`` rather than as
module-level globals so tests can override them.
"""

import logging
import os
import time
from collections.abc import AsyncIterator, Awaitable, Callable
from contextlib import asynccontextmanager
from typing import Literal

from fastapi import APIRouter, FastAPI, Request, Response
from sqlalchemy.ext.asyncio import async_sessionmaker

from klassenzeit_backend.auth.audit_middleware import SuperAdminAuditMiddleware
from klassenzeit_backend.auth.rate_limit import LoginRateLimiter
from klassenzeit_backend.auth.routes import auth_router
from klassenzeit_backend.core.logging import (
    _resolve_request_id,
    configure_logging,
    request_id_var,
)
from klassenzeit_backend.core.settings import get_settings
from klassenzeit_backend.db.engine import build_engine
from klassenzeit_backend.scheduling.routes import scheduling_router
from klassenzeit_backend.testing.mount import include_testing_router_if_enabled

_ACCESS_LOGGER = logging.getLogger("klassenzeit_backend.http.access")


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """Manage app lifecycle: initialize shared state on startup, dispose engine on shutdown."""
    settings = get_settings()
    engine = build_engine()
    app.state.settings = settings
    app.state.engine = engine
    app.state.session_factory = async_sessionmaker(
        engine,
        expire_on_commit=False,
    )
    app.state.rate_limiter = LoginRateLimiter(
        max_attempts=settings.login_max_attempts,
        lockout_minutes=settings.login_lockout_minutes,
    )
    app.state.solver_progress = {}
    try:
        yield
    finally:
        await engine.dispose()


health_router = APIRouter(tags=["health"])


@health_router.get("/health")
async def health() -> dict[str, str]:
    """Return a simple health-check response."""
    return {"status": "ok"}


def build_app(env: str | None) -> FastAPI:
    """Construct the FastAPI app with env-gated routes.

    Staging and production both run with ``KZ_ENV=prod``; the OpenAPI
    schema, Swagger UI, and ReDoc endpoints are disabled there to
    reduce API-shape recon for unauthenticated attackers. Dev and test
    environments keep them mounted at the usual paths.

    ``dump_openapi.py`` reads ``app.openapi()`` directly in-process and
    is unaffected by ``openapi_url=None``: the schema generator runs
    off the registered routes, not the HTTP endpoint.
    """
    # Read log env vars directly from os.environ to keep build_app importable
    # without a full Settings (and thus KZ_DATABASE_URL). Same rationale as
    # the KZ_ENV-from-os.environ pattern at module load below. The branchy
    # form is here because `ty` does not narrow `x in ("a", "b")` to
    # `Literal["a", "b"]`.
    log_format_env = os.environ.get("KZ_LOG_FORMAT")
    log_format: Literal["text", "json"] | None
    if log_format_env == "json":
        log_format = "json"
    elif log_format_env == "text":
        log_format = "text"
    else:
        log_format = None
    env_for_logging: Literal["dev", "test", "prod"]
    if env == "prod":
        env_for_logging = "prod"
    elif env == "test":
        env_for_logging = "test"
    else:
        env_for_logging = "dev"
    configure_logging(
        env=env_for_logging,
        log_format=log_format,
        log_level=os.environ.get("KZ_LOG_LEVEL", "INFO"),
    )
    is_prod = env == "prod"
    new_app = FastAPI(
        title="Klassenzeit",
        lifespan=lifespan,
        openapi_url=None if is_prod else "/api/openapi.json",
        docs_url=None if is_prod else "/api/docs",
        redoc_url=None if is_prod else "/api/redoc",
    )
    new_app.include_router(auth_router, prefix="/api")
    new_app.include_router(scheduling_router, prefix="/api")
    new_app.include_router(health_router, prefix="/api")
    include_testing_router_if_enabled(new_app, env)

    new_app.add_middleware(SuperAdminAuditMiddleware)

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

    return new_app


# Routing decisions happen at import time. Reading ``KZ_ENV`` directly from
# ``os.environ`` avoids constructing a full ``Settings`` at module load: the
# ``dump_openapi`` script and CI type regeneration import this module without
# a ``KZ_DATABASE_URL`` available. The factory only needs the env name, so
# the lighter dependency is appropriate.

app = build_app(os.environ.get("KZ_ENV"))
