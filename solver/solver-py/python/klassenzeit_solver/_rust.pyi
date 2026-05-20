from typing import Literal, TypedDict

PinKind = Literal["hard", "soft"]
"""Discriminator for a `PinnedPlacement.kind` entry on the JSON wire format.

Mirrors `solver_core::types::PinKind` (snake_case serde tag). Callers omitting
`kind` deserialise to `"hard"` (today's binary-hard semantic). See ADR 0042.
"""

class ConstraintWeights(TypedDict, total=False):
    """Wire-format mirror of `solver_core::types::ConstraintWeights`.

    Only the soft-pin axis is pinned here; the broader field set is not
    consumed type-statically by Python callers today. Extend as new axes
    surface in Python consumers.
    """

    soft_pin_miss: int

class QualityReport(TypedDict, total=False):
    """Wire-format mirror of `solver_core::quality::QualityReport`.

    Only `soft_pin_misses` is pinned in the stub; the broader field set is
    not consumed type-statically by Python callers today. Extend as new
    axes surface in Python consumers.
    """

    soft_pin_misses: int

class ProgressSnapshotDict(TypedDict):
    """Snapshot of a `ProgressHandle`'s underlying atomic counters."""

    iter: int
    placement_count: int
    best_score: int
    is_feasible: bool
    cancel_requested: bool

class ProgressHandle:
    """PyO3 wrapper around `solver_core::ProgressBeacon`.

    Pass an instance to `solve_json_with_progress` to receive live progress
    via `snapshot()` and trigger a soft-cancel via `cancel()`. The handle
    holds an `Arc<ProgressBeacon>` so both Python and the solver thread
    observe the same atomics.
    """

    def __init__(self) -> None: ...
    def snapshot(self) -> ProgressSnapshotDict:
        """Return a dict with the five beacon fields."""

    def cancel(self) -> None:
        """Request cancellation of the running solve. Idempotent."""

def solve_json_with_progress(
    problem_json: str,
    deadline_ms: int | None,
    progress: ProgressHandle,
    lahc_rr_period: int | None = ...,
    lahc_kempe_period: int | None = ...,
    lahc_home_room_period: int | None = ...,
) -> str:
    """Solve a Problem encoded as JSON with a live ProgressHandle.

    Like `solve_json_with_config` but writes per-iteration progress into
    `progress` and checks `progress.cancel()` at every LAHC iteration. The
    returned Solution JSON carries `was_cancelled: true` when the loop
    exited because of a cancel request.

    Raises ``ValueError`` on malformed JSON or solver-side input errors.
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

def quality_report_json(
    problem_json: str,
    placements_json: str,
    violations_json: str,
) -> str:
    """Compute the QualityReport for the given Placement[] + Violation[].

    Returns the per-axis cost-vector breakdown as a JSON object string,
    using production-active weights. The contract
    ``quality_report.weighted_score == score_solution_json(problem, placements)``
    holds; the CP-SAT path (``klassenzeit_solver.cpsat``) uses both functions
    to populate ``Solution.soft_score`` and ``Solution.quality_report``
    post-solve so all bake-off backends surface the same wire-format breakdown
    (ADR 0030).

    Raises ``ValueError`` on malformed JSON in any argument.
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
    lahc_home_room_period: int | None = None,
) -> str:
    """Solve a Problem encoded as JSON with an explicit LAHC deadline.

    ``deadline_ms=None`` skips the LAHC pass entirely (greedy-only) and is
    the canonical choice for binding-contract tests. ``deadline_ms=Some(n)``
    matches the production behaviour of ``solve_json`` when ``n == 200``.

    ``lahc_rr_period``, ``lahc_kempe_period``, and ``lahc_home_room_period``
    enable the corresponding LAHC moves; all default to ``None`` (disabled),
    preserving the pre-Sprint-4 single-Change behaviour. The bake-off
    backends pass ``lahc_rr_period=25`` (R&R only) or ``lahc_rr_period=25,
    lahc_kempe_period=23`` (R&R + Kempe). ``lahc_home_room_period`` enables
    the home-room-repair move (ADR 0050, item 86 option b); production
    callers pass ``Some(7)``.

    The input JSON may include a ``pinned_placements`` array of
    ``{lesson_id, time_block_id, room_id, teacher_id?}`` entries; the
    solver preserves those placements verbatim across both FFD seeding
    and LAHC moves, and drops any malformed entry as a
    ``ViolationKind::PinnedConflict`` rather than raising. The optional
    ``teacher_id`` (item 77) carries the picker's chosen teacher from a
    prior solve so the seed Placement reflects the real teacher; when
    omitted the solver falls back to ``teacher_candidates[0]`` and can
    false-positive ``validate_no_double_booking`` under unpinned mode.
    """
