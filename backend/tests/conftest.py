"""Shared pytest fixtures for the backend test suite.

Layered fixture design:

1. ``settings`` / ``engine``          — session-scoped, bound to the test DB.
2. ``apply_migrations``               — session-scoped, autouse; resets schema once.
3. ``db_session``                     — per-test, transaction-rollback isolated.
4. ``client``                         — per-test; reuses ``db_session`` via dependency override.
5. ``create_test_user`` / ``login_as`` — per-test auth helpers, available to all test packages.

Pytest is invoked from the repo root (see ``[tool.pytest.ini_options]
testpaths`` in the root ``pyproject.toml``), so every file path is
resolved relative to ``__file__``, not cwd.

Implementation notes:

- ``apply_migrations`` is a **synchronous** fixture even though the rest of
  the harness is async.  Alembic's ``command.downgrade/upgrade`` internally
  calls ``asyncio.run()`` (via ``env.py``'s ``run_migrations_online``).
  ``asyncio.run()`` cannot be called from inside a running event loop, so
  ``apply_migrations`` must not be async.  A sync session-scoped fixture runs
  once per pytest session, before any async fixtures are initialised, and has
  no event loop conflict.

- ``apply_migrations`` depends on ``settings`` (sync, session-scoped) rather
  than ``engine`` (async) so that the fixture ordering is clean and no async
  context manager is needed.

- ``db_session``'s savepoint-restart event listener accesses
  ``transaction._parent.nested`` (private SQLAlchemy attribute).  This is the
  canonical pattern from the SQLAlchemy docs.  Do not rewrite it to avoid the
  private access — that breaks the fixture.
"""

import logging
import os
import subprocess
import sys
from collections.abc import AsyncIterator, Awaitable, Callable
from pathlib import Path

import pytest
from httpx import ASGITransport, AsyncClient
from sqlalchemy import event
from sqlalchemy.ext.asyncio import (
    AsyncEngine,
    AsyncSession,
    async_sessionmaker,
    create_async_engine,
)
from sqlalchemy.pool import NullPool

from tests._xdist_db import (
    clone_database_from_template,
    ensure_database_exists,
    ensure_template_database,
    parse_dbname,
    read_env_test_database_url,
    worker_database_url,
)

# Must precede any ``from klassenzeit_backend`` import. ``get_settings()`` is
# lru_cache'd and ``main.py`` mounts routers at module-load time based on the
# Settings instance it constructs. ``KZ_ENV=test`` needs to be in the process
# env before that happens so the testing router (Tasks 3-5) is mounted.
os.environ.setdefault("KZ_ENV", "test")

# Per-worker test DB isolation for pytest-xdist. ``PYTEST_XDIST_WORKER`` is
# set to ``gw0``/``gw1``/... inside each worker process and ``master`` in the
# coordinator (or absent when running without xdist). The base URL comes from
# ``backend/.env.test``; we suffix the dbname with the worker name and write
# it back to the env so pydantic-settings (which reads env vars over dotenv
# files by default) and the subprocess alembic both see the per-worker URL.
_BACKEND_ROOT = Path(__file__).resolve().parent.parent  # repo/backend
_ENV_TEST = _BACKEND_ROOT / ".env.test"
_WORKER = os.environ.get("PYTEST_XDIST_WORKER", "master")
if _WORKER != "master":
    os.environ["KZ_DATABASE_URL"] = worker_database_url(
        read_env_test_database_url(_ENV_TEST), _WORKER
    )

from klassenzeit_backend.auth.passwords import hash_password  # noqa: E402
from klassenzeit_backend.auth.rate_limit import LoginRateLimiter  # noqa: E402
from klassenzeit_backend.core.settings import Settings  # noqa: E402
from klassenzeit_backend.db.models.user import User  # noqa: E402
from klassenzeit_backend.db.session import get_session  # noqa: E402
from klassenzeit_backend.main import app  # noqa: E402

# Type aliases for the auth factory callables
type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]


# ─── Layer 1: engine ────────────────────────────────────────────────────────


@pytest.fixture(scope="session")
def settings() -> Settings:
    return Settings(_env_file=str(_ENV_TEST))  # ty: ignore[missing-argument, unknown-argument]


@pytest.fixture
async def engine(settings: Settings) -> AsyncIterator[AsyncEngine]:
    # Function-scoped with NullPool: each test gets a fresh engine bound to
    # the current event loop. pytest-asyncio 1.3.0 on Python 3.14 sometimes
    # fails to honor the session-scoped event loop for per-test fixtures,
    # leading to "Future attached to a different loop" errors when a
    # session-scoped engine's connection is checked out in a different loop.
    eng = create_async_engine(str(settings.database_url), poolclass=NullPool)
    try:
        yield eng
    finally:
        await eng.dispose()


# ─── Layer 2: migrations ────────────────────────────────────────────────────


@pytest.fixture(scope="session", autouse=True)
def apply_migrations(settings: Settings) -> None:
    """Migrate the per-worker database via a template DB cache.

    First worker creates and migrates ``klassenzeit_test_template`` under
    an advisory lock; subsequent workers ``CREATE DATABASE ... TEMPLATE``
    from it (single-digit ms in Postgres). Falls back to the per-worker
    alembic flow if the template path raises (locale mismatch, permission
    error, etc.).
    """
    base_url = read_env_test_database_url(_ENV_TEST)
    target_url = str(settings.database_url)
    template_url = f"{base_url}_template"
    template_name = parse_dbname(template_url)

    try:
        ensure_template_database(template_url, alembic_cwd=str(_BACKEND_ROOT))
        clone_database_from_template(base_url=target_url, template_name=template_name)
        return
    except Exception as exc:  # opportunistic fallback to per-worker alembic
        logging.getLogger(__name__).warning(
            "template_db.fallback",
            extra={"reason": type(exc).__name__, "detail": str(exc)},
        )

    ensure_database_exists(target_url)
    env = os.environ.copy()
    env["KZ_DATABASE_URL"] = target_url
    # Downgrade, then upgrade, each in a separate subprocess for clean state.
    for args in (["downgrade", "base"], ["upgrade", "head"]):
        subprocess.run(  # noqa: S603
            [sys.executable, "-m", "alembic", *args],
            check=True,
            cwd=str(_BACKEND_ROOT),
            env=env,
        )


# ─── Layer 3: per-test session with savepoint restart ──────────────────────


@pytest.fixture
async def db_session(engine: AsyncEngine) -> AsyncIterator[AsyncSession]:
    async with engine.connect() as connection:
        trans = await connection.begin()
        factory = async_sessionmaker(bind=connection, expire_on_commit=False)
        try:
            async with factory() as session:
                await session.begin_nested()

                @event.listens_for(session.sync_session, "after_transaction_end")
                def restart_savepoint(sess, transaction):
                    if transaction.nested and not transaction._parent.nested:
                        sess.begin_nested()

                yield session
        finally:
            await trans.rollback()


# ─── Layer 4: FastAPI ASGI client sharing the per-test session ─────────────


@pytest.fixture
async def client(
    db_session: AsyncSession,
    settings: Settings,
) -> AsyncIterator[AsyncClient]:
    async def override_get_session() -> AsyncIterator[AsyncSession]:
        yield db_session

    app.state.settings = settings
    app.state.rate_limiter = LoginRateLimiter(
        max_attempts=settings.login_max_attempts,
        lockout_minutes=settings.login_lockout_minutes,
    )
    app.state.solver_progress = {}
    app.dependency_overrides[get_session] = override_get_session
    try:
        async with AsyncClient(
            transport=ASGITransport(app=app),
            base_url="http://test",
        ) as c:
            yield c
    finally:
        app.dependency_overrides.clear()


# ─── Layer 5: auth helpers available to all test packages ──────────────────


@pytest.fixture
def create_test_user(db_session: AsyncSession) -> CreateUserFn:
    """Factory fixture: ``await create_test_user(email=..., password=...)``.

    Args:
        db_session: The per-test async DB session (injected by pytest).

    Returns:
        An async callable that inserts a User row and flushes.
    """

    async def _make_test_user(
        *,
        email: str = "user@test.com",
        password: str = "testpassword123",  # noqa: S107
        role: str = "user",
        is_active: bool = True,
        force_password_change: bool = False,
    ) -> tuple[User, str]:
        user = User(
            email=email.lower(),
            password_hash=hash_password(password),
            role=role,
            is_active=is_active,
            force_password_change=force_password_change,
        )
        db_session.add(user)
        await db_session.flush()
        return user, password

    return _make_test_user


@pytest.fixture
def login_as(client: AsyncClient) -> LoginFn:
    """Factory fixture: ``await login_as(email, password)``.

    Args:
        client: The async test HTTP client (injected by pytest).

    Returns:
        An async callable that POSTs to /auth/login and asserts 204.
    """

    async def _do_login(email: str, password: str) -> None:
        response = await client.post(
            "/api/auth/login",
            json={"email": email, "password": password},
        )
        assert response.status_code == 204, response.text

    return _do_login
