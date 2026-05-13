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


def _empty_per_backend_deadlines() -> dict[SolverBackend, int]:
    """Field(default_factory=...) seed for ``solve_deadline_ms_by_backend``.

    Pydantic-settings instantiates the dict before the assemble validator
    runs; the values here are placeholders that the ``mode="after"`` step
    overwrites from the four bound scalar fields. Typed return narrows
    the keys to ``SolverBackend`` for ``ty``.
    """
    return {
        "lahc": 0,
        "lahc_rr": 0,
        "lahc_rr_kempe": 0,
        "cpsat": 0,
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
    solver_backend: SolverBackend = "lahc_rr"
    # Per-backend deadline scalars. Bound to KZ_SOLVE_DEADLINE_MS_<BACKEND>
    # env vars via pydantic-settings' standard field-binding (which reads
    # env_file). Public consumers go through solve_deadline_ms_by_backend
    # below; the scalars exist so pydantic-settings actually loads them.
    solve_deadline_ms_lahc: int = 5000
    solve_deadline_ms_lahc_rr: int = 5000
    solve_deadline_ms_lahc_rr_kempe: int = 5000
    solve_deadline_ms_cpsat: int = 120000
    solve_deadline_ms_by_backend: dict[SolverBackend, int] = Field(
        default_factory=_empty_per_backend_deadlines,
    )

    @model_validator(mode="after")
    def _assemble_per_backend_deadlines(self) -> "Settings":
        """Populate ``solve_deadline_ms_by_backend`` from the four scalars.

        The four ``solve_deadline_ms_<backend>`` scalars are bound to
        ``KZ_SOLVE_DEADLINE_MS_<BACKEND>`` env vars by pydantic-settings'
        standard field-binding (which honors ``env_file``). Assembling
        the dict from those scalars here keeps the route-handler lookup
        ``settings.solve_deadline_ms_by_backend[settings.solver_backend]``
        in place while letting operators set per-backend env vars
        individually. Tests override via
        ``monkeypatch.setitem(settings.solve_deadline_ms_by_backend, ...)``
        as documented in ``backend/CLAUDE.md``; the validator only runs
        at ``__init__``, so dict mutations after that persist.
        """
        self.solve_deadline_ms_by_backend = {
            "lahc": self.solve_deadline_ms_lahc,
            "lahc_rr": self.solve_deadline_ms_lahc_rr,
            "lahc_rr_kempe": self.solve_deadline_ms_lahc_rr_kempe,
            "cpsat": self.solve_deadline_ms_cpsat,
        }
        return self


@lru_cache
def get_settings() -> Settings:
    """Return the process-wide Settings singleton.

    Cached so dependency-override patterns can swap the cached value in
    tests via ``get_settings.cache_clear()`` when needed.
    """
    return Settings()  # ty: ignore[missing-argument]
