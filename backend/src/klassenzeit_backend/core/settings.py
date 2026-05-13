"""Application settings, loaded from environment variables.

All env vars consumed by the backend share the ``KZ_`` prefix so they
can be distinguished from third-party vars in shared shells and CI.

The default ``.env`` path is resolved *relative to this file*, not
relative to cwd. ``uvicorn``, ``pytest``, and ``alembic`` all have
different default working directories; a relative ``env_file=".env"``
would silently resolve to the wrong file (or to nothing) depending on
which tool loaded Settings first.
"""

import os
from functools import lru_cache
from pathlib import Path
from typing import Literal, cast

from pydantic import Field, PostgresDsn, model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict

# Resolve parents[3] of this file to reach the backend/ root:
# settings.py → core/ → klassenzeit_backend/ → src/ → backend/
_BACKEND_ROOT = Path(__file__).resolve().parents[3]
_DEFAULT_ENV_FILE = _BACKEND_ROOT / ".env"


SolverBackend = Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"]
"""All known solver backends. Reused by ``solver_backend`` and
``solve_deadline_ms_by_backend``'s key type so the two fields stay in
lockstep when a new backend lands."""

_PER_BACKEND_DEADLINE_DEFAULTS: dict[SolverBackend, int] = {
    "lahc": 5000,
    "lahc_rr": 5000,
    "lahc_rr_kempe": 5000,
    "cpsat": 120000,
}

_PER_BACKEND_DEADLINE_ENV_VARS: dict[SolverBackend, str] = {
    "lahc": "KZ_SOLVE_DEADLINE_MS_LAHC",
    "lahc_rr": "KZ_SOLVE_DEADLINE_MS_LAHC_RR",
    "lahc_rr_kempe": "KZ_SOLVE_DEADLINE_MS_LAHC_RR_KEMPE",
    "cpsat": "KZ_SOLVE_DEADLINE_MS_CPSAT",
}


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
    solve_deadline_ms_by_backend: dict[SolverBackend, int] = Field(
        default_factory=lambda: dict(_PER_BACKEND_DEADLINE_DEFAULTS),
    )
    solver_backend: SolverBackend = "lahc_rr"

    @model_validator(mode="before")
    @classmethod
    def _resolve_per_backend_deadlines(cls, values: object) -> object:
        """Read per-backend deadline env vars and inject into the dict.

        Each ``KZ_SOLVE_DEADLINE_MS_<BACKEND>`` env var, if set, overrides
        the matching entry in the resolved dict. Unset entries inherit the
        ``_PER_BACKEND_DEADLINE_DEFAULTS`` value. Invalid (non-int) env
        values surface as ``ValidationError`` naming the offending env var.
        """
        if not isinstance(values, dict):
            return values
        typed_values = cast("dict[str, object]", values)
        resolved: dict[SolverBackend, int] = dict(_PER_BACKEND_DEADLINE_DEFAULTS)
        # Honor any explicit dict passed in directly (test plumbing path).
        existing = typed_values.get("solve_deadline_ms_by_backend")
        if isinstance(existing, dict):
            typed_existing = cast("dict[object, object]", existing)
            for k, v in typed_existing.items():
                if k in resolved and isinstance(v, int):
                    resolved[cast("SolverBackend", k)] = v
        for backend_key, env_name in _PER_BACKEND_DEADLINE_ENV_VARS.items():
            raw = os.environ.get(env_name)
            if raw is None:
                continue
            try:
                resolved[backend_key] = int(raw)
            except ValueError as exc:
                raise ValueError(f"{env_name} must be an integer (got {raw!r})") from exc
        typed_values["solve_deadline_ms_by_backend"] = resolved
        return typed_values

    @model_validator(mode="after")
    def _check_per_backend_deadlines_complete(self) -> "Settings":
        """Every solver backend must have an entry in the dict.

        A missing key indicates ``_PER_BACKEND_DEADLINE_DEFAULTS`` was not
        updated when the ``SolverBackend`` Literal grew a new variant.
        Surface as ``ValidationError`` at startup rather than ``KeyError``
        deep in a route handler.
        """
        expected: set[SolverBackend] = set(_PER_BACKEND_DEADLINE_ENV_VARS.keys())
        present: set[SolverBackend] = set(self.solve_deadline_ms_by_backend.keys())
        missing = expected - present
        if missing:
            raise ValueError(f"solve_deadline_ms_by_backend missing keys: {sorted(missing)}")
        return self


@lru_cache
def get_settings() -> Settings:
    """Return the process-wide Settings singleton.

    Cached so dependency-override patterns can swap the cached value in
    tests via ``get_settings.cache_clear()`` when needed.
    """
    return Settings()  # ty: ignore[missing-argument]
