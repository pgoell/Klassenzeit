"""Application settings, loaded from environment variables.

All env vars consumed by the backend share the ``KZ_`` prefix so they
can be distinguished from third-party vars in shared shells and CI.

The default ``.env`` path is resolved *relative to this file*, not
relative to cwd. ``uvicorn``, ``pytest``, and ``alembic`` all have
different default working directories; a relative ``env_file=".env"``
would silently resolve to the wrong file (or to nothing) depending on
which tool loaded Settings first.
"""

from functools import lru_cache
from pathlib import Path
from typing import Literal

from pydantic import PostgresDsn
from pydantic_settings import BaseSettings, SettingsConfigDict

# Resolve parents[3] of this file to reach the backend/ root:
# settings.py → core/ → klassenzeit_backend/ → src/ → backend/
_BACKEND_ROOT = Path(__file__).resolve().parents[3]
_DEFAULT_ENV_FILE = _BACKEND_ROOT / ".env"


class Settings(BaseSettings):
    """Backend configuration loaded from environment variables with ``KZ_`` prefix."""

    model_config = SettingsConfigDict(
        env_file=str(_DEFAULT_ENV_FILE),
        env_prefix="KZ_",
        extra="ignore",
    )

    database_url: PostgresDsn
    db_pool_size: int = 5
    db_max_overflow: int = 10
    db_echo: bool = False

    env: Literal["dev", "test", "prod"] = "dev"

    # Auth
    cookie_secure: bool = True
    cookie_domain: str | None = None
    session_ttl_days: int = 14
    password_min_length: int = 12
    login_max_attempts: int = 5
    login_lockout_minutes: int = 15

    # Logging
    log_format: Literal["text", "json"] | None = None
    log_level: str = "INFO"

    # Solver
    solve_deadline_ms: int | None = 200
    solver_backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"] = "lahc_rr_kempe"


@lru_cache
def get_settings() -> Settings:
    """Return the process-wide Settings singleton.

    Cached so dependency-override patterns can swap the cached value in
    tests via ``get_settings.cache_clear()`` when needed.
    """
    return Settings()  # ty: ignore[missing-argument]
