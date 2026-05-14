"""Pin kind discriminator for `ScheduledLesson.pin_kind`.

Hard pins survive re-solves verbatim; soft pins enter the LAHC objective
as a per-placement penalty axis. See ADR 0042.
"""

from enum import StrEnum


class PinKind(StrEnum):
    """Two-state pin discriminator. `None` on the column means unpinned."""

    HARD = "hard"
    SOFT = "soft"
