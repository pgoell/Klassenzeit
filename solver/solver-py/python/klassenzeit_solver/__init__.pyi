def solve_json(problem_json: str) -> str:
    """Solve a Problem encoded as JSON, returning a Solution as JSON.

    Production entry point: applies a 200 ms LAHC wall-clock budget after
    the greedy pass. See ``solve_json_with_config`` for the test-friendly
    variant that lets the caller pick (or skip) the deadline.
    """

def solve_json_with_config(
    problem_json: str,
    deadline_ms: int | None,
    lahc_rr_period: int | None = None,
    lahc_kempe_period: int | None = None,
) -> str:
    """Solve a Problem encoded as JSON with an explicit LAHC deadline.

    ``deadline_ms=None`` skips the LAHC pass entirely (greedy-only) and is
    the canonical choice for binding-contract tests. ``deadline_ms=Some(n)``
    matches the production behaviour of ``solve_json`` when ``n == 200``.

    ``lahc_rr_period`` and ``lahc_kempe_period`` enable the corresponding
    LAHC moves; both default to ``None`` (disabled), preserving the
    pre-Sprint-4 single-Change behaviour. The bake-off backends pass
    ``lahc_rr_period=25`` (R&R only) or ``lahc_rr_period=25,
    lahc_kempe_period=23`` (R&R + Kempe).

    The input JSON may include a ``pinned_placements`` array of
    ``{lesson_id, time_block_id, room_id}`` entries; the solver preserves
    those placements verbatim across both FFD seeding and LAHC moves, and
    drops any malformed entry as a ``ViolationKind::PinnedConflict`` rather
    than raising.
    """
