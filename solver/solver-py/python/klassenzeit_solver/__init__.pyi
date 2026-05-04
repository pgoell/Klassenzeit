def solve_cpsat_json(
    problem_json: str,
    deadline_ms: int | None,
    seed: int = 1,
) -> str:
    """Solve a Klassenzeit timetable via CP-SAT (Google OR-Tools).

    Returns a Solution JSON in the same wire format as ``solve_json``. On
    INFEASIBLE / UNKNOWN, returns a Solution with no placements and one
    NoFreeTimeBlock violation per (lesson, hour_index) with
    reason='cpsat: <status>'. On MODEL_INVALID, raises RuntimeError.
    ADR 0030.
    """

def score_solution_json(problem_json: str, placements_json: str) -> int:
    """Score a Placement[] against a Problem using production-active weights.

    Returns the integer soft-score that ``solve_json`` would produce on the
    same problem given those placements. Used by the CP-SAT seed path
    (``klassenzeit_solver.cpsat``) to populate ``Solution.soft_score``
    post-solve so all bake-off backends compare on the same Rust scorer
    (ADR 0030).

    Raises ``ValueError`` on malformed JSON in either argument.
    """

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
