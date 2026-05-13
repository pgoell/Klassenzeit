"""In-process registry of in-flight solver progress handles.

``app.state.solver_progress`` is a dict keyed by class UUID. The schedule
POST handler registers an entry for the duration of the solve and unregisters
it in a ``finally`` so a crashing solver still leaves the registry clean.
The GET ``/schedule/progress`` and POST ``/schedule/cancel`` routes read
from this registry.
"""

import time
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from uuid import UUID

from klassenzeit_solver import ProgressHandle


@dataclass
class RegistrationEntry:
    """Per-solve registration record stored in ``app.state.solver_progress``."""

    handle: ProgressHandle
    started_at: float
    deadline_ms: int
    total_lessons: int


@contextmanager
def register_progress(
    registry: dict[UUID, RegistrationEntry],
    *,
    class_id: UUID,
    deadline_ms: int,
    total_lessons: int,
) -> Iterator[RegistrationEntry]:
    """Register a fresh progress entry for the lifetime of a solve.

    On context exit (success or exception) the entry is removed so the
    GET ``/schedule/progress`` endpoint returns 404 once the solve has
    finished. ``time.monotonic()`` (not ``time.time()``) is used for the
    start timestamp to avoid wall-clock drift between request and read.
    """
    entry = RegistrationEntry(
        handle=ProgressHandle(),
        started_at=time.monotonic(),
        deadline_ms=deadline_ms,
        total_lessons=total_lessons,
    )
    registry[class_id] = entry
    try:
        yield entry
    finally:
        registry.pop(class_id, None)
