# CP-SAT seed via Python ortools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land CP-SAT as a fourth selectable solver backend (`KZ_SOLVER_BACKEND=cpsat`) that ships in the bake-off bench column, ADR 0030, and the production env-var dispatch, without disturbing existing LAHC backends.

**Architecture:** A new pure-Python module `solver/solver-py/python/klassenzeit_solver/cpsat.py` implements CP-SAT via Google OR-Tools (`ortools` Python package). A new PyO3 binding `score_solution_json` lets the Python module reuse Rust's soft-scorer post-solve so all four backends compare on the same Rust scorer. Backend dispatch lives in `scheduling/solver_io.run_solve` switching on `Settings.solver_backend: Literal[...]`. The bench harness gains a `BenchBackend::CpSat` arm that subprocess-invokes `python3 -m klassenzeit_solver.cpsat`.

**Tech Stack:** Rust 1.85, PyO3 0.28, maturin (`mise run solver:rebuild`), Python 3.14, FastAPI, pydantic-settings, OR-Tools (`ortools >=9.10,<10`), pytest, cargo nextest, mise tasks.

**Spec:** [`docs/superpowers/specs/2026-05-04-solver-cpsat-bake-off-design.md`](../specs/2026-05-04-solver-cpsat-bake-off-design.md).

**Brainstorm:** `/tmp/kz-brainstorm/brainstorm.md` (posted as PR comments at PR open time).

---

## Task 1: Extract `PRODUCTION_ACTIVE_WEIGHTS` constant in solver-core

**Why first:** Three call sites today re-hand-write the same `ConstraintWeights { class_gap: 10, teacher_gap: 10, ... }` literal: `solver-core/src/json.rs::solve_json_with_config`, `solver-bench/src/main.rs::production_active_weights`, and the upcoming `score_solution_json` PyO3 binding. Centralising removes a drift surface and is required for the binding to use the same weights.

**Files:**
- Modify: `solver/solver-core/src/types.rs` (add the const after `ConstraintWeights` struct definition, around line 75-150).
- Modify: `solver/solver-core/src/lib.rs` (re-export; the existing `pub use types::{...};` block).
- Modify: `solver/solver-core/src/json.rs` (replace inline literal at lines 35-44 with the const).
- Modify: `solver/solver-bench/src/main.rs` (replace `production_active_weights()` fn at lines 48-59 with the const).

- [ ] **Step 1.1: Add the constant.** In `solver/solver-core/src/types.rs`, after the `ConstraintWeights` struct (around line 105 once the struct definition closes), add:

```rust
/// Production-active soft-constraint weights. The bake-off bench, the JSON
/// adapter, and the new `score_solution_json` PyO3 binding all use this exact
/// weight set so cross-backend bench cells compare on the same scorer.
pub const PRODUCTION_ACTIVE_WEIGHTS: ConstraintWeights = ConstraintWeights {
    class_gap: 10,
    teacher_gap: 10,
    prefer_early_period: 1,
    avoid_first_period: 1,
    prefer_home_room: 5,
    avoid_last_period: 1,
    prefer_late_period: 1,
    class_day_balance: 5,
};
```

- [ ] **Step 1.2: Re-export from `lib.rs`.** Add `PRODUCTION_ACTIVE_WEIGHTS` to the `pub use types::{...}` line in `solver/solver-core/src/lib.rs`.

- [ ] **Step 1.3: Replace `json.rs` inline literal.** In `solver/solver-core/src/json.rs::solve_json_with_config`, replace the `weights: ConstraintWeights { class_gap: 10, ... }` at lines 35-44 with:

```rust
    let config = SolveConfig {
        weights: crate::PRODUCTION_ACTIVE_WEIGHTS.clone(),
        deadline: deadline_ms.map(Duration::from_millis),
        ..SolveConfig::default()
    };
```

- [ ] **Step 1.4: Replace solver-bench inline fn.** In `solver/solver-bench/src/main.rs`, delete `fn production_active_weights() -> ConstraintWeights { ... }` (lines 48-59) and the call site at line 169:

```rust
    let weights = solver_core::PRODUCTION_ACTIVE_WEIGHTS.clone();
```

(import via `use solver_core::PRODUCTION_ACTIVE_WEIGHTS;` at the top of the file.)

- [ ] **Step 1.5: Add a unit test.** In `solver/solver-core/src/types.rs`, inside the existing `#[cfg(test)] mod tests`, add:

```rust
#[test]
fn production_active_weights_match_legacy_inline_literal() {
    let inline = ConstraintWeights {
        class_gap: 10,
        teacher_gap: 10,
        prefer_early_period: 1,
        avoid_first_period: 1,
        prefer_home_room: 5,
        avoid_last_period: 1,
        prefer_late_period: 1,
        class_day_balance: 5,
    };
    assert_eq!(crate::PRODUCTION_ACTIVE_WEIGHTS, inline);
}
```

- [ ] **Step 1.6: Verify.** Run:

```bash
mise run lint:rust
mise run test:rust
```

Both must pass. The bench's existing tests in `solver-bench/src/main.rs::tests` continue to pass; no fixtures touched.

- [ ] **Step 1.7: Commit.**

```bash
git add solver/solver-core/src/types.rs solver/solver-core/src/lib.rs \
        solver/solver-core/src/json.rs solver/solver-bench/src/main.rs
git commit -m "refactor(solver-core): extract production_active_weights into a public constant"
```

---

## Task 2: Widen `solve_json_with_config` with LAHC period kwargs

**Why second:** `scheduling/solver_io.run_solve` will need to dispatch all three Rust LAHC variants (`lahc`, `lahc_rr`, `lahc_rr_kempe`) through the JSON adapter with different `SolveConfig.lahc_rr_period` and `lahc_kempe_period`. Today's `solve_json_with_config(json, deadline_ms)` is too narrow.

**Files:**
- Modify: `solver/solver-core/src/json.rs` (widen signature).
- Modify: `solver/solver-py/src/lib.rs` (forward the new kwargs).
- Modify: `solver/solver-py/python/klassenzeit_solver/__init__.pyi` (typing).
- Modify: `solver/solver-py/python/klassenzeit_solver/_rust.pyi` (typing).
- Test: `solver/solver-py/tests/test_solve_json_lahc_kwargs.py` (new file).

- [ ] **Step 2.1: Write the failing Python test.** Create `solver/solver-py/tests/test_solve_json_lahc_kwargs.py`:

```python
"""Test that solve_json_with_config accepts the new LAHC period kwargs."""

import inspect

from klassenzeit_solver import solve_json_with_config


def test_solve_json_with_config_signature_includes_lahc_period_kwargs() -> None:
    sig = inspect.signature(solve_json_with_config)
    assert "lahc_rr_period" in sig.parameters
    assert "lahc_kempe_period" in sig.parameters


def test_solve_json_with_config_accepts_lahc_rr_period_kwarg() -> None:
    # Minimal valid problem JSON with no lessons; the solve returns an empty
    # solution. The test asserts that the call does not raise on the kwarg.
    minimal_json = '{"week_scheme":{"id":"00000000-0000-0000-0000-000000000001","name":"x","time_blocks":[]},"school_classes":[],"teachers":[],"rooms":[],"subjects":[],"lessons":[],"lesson_groups":[],"pinned_placements":[]}'
    out = solve_json_with_config(minimal_json, None, lahc_rr_period=25)
    assert '"placements"' in out


def test_solve_json_with_config_accepts_lahc_kempe_period_kwarg() -> None:
    minimal_json = '{"week_scheme":{"id":"00000000-0000-0000-0000-000000000001","name":"x","time_blocks":[]},"school_classes":[],"teachers":[],"rooms":[],"subjects":[],"lessons":[],"lesson_groups":[],"pinned_placements":[]}'
    out = solve_json_with_config(minimal_json, None, lahc_rr_period=25, lahc_kempe_period=23)
    assert '"placements"' in out
```

- [ ] **Step 2.2: Run test, expect failure.** `uv run pytest solver/solver-py/tests/test_solve_json_lahc_kwargs.py -v`. Expected: failure on `TypeError: solve_json_with_config() got an unexpected keyword argument 'lahc_rr_period'`.

- [ ] **Step 2.3: Widen `solver-core/src/json.rs`.** Replace the `solve_json_with_config` signature and body. Note: keep the function `pub`, keep the existing `solve_json(json: &str)` delegate (it stays a one-liner over `solve_json_with_config(json, Some(200), None, None)`).

```rust
pub fn solve_json_with_config(
    json: &str,
    deadline_ms: Option<u64>,
    lahc_rr_period: Option<u32>,
    lahc_kempe_period: Option<u32>,
) -> Result<String, Error> {
    let problem: Problem =
        serde_json::from_str(json).map_err(|e| Error::Input(format!("json: {e}")))?;
    let config = SolveConfig {
        weights: crate::PRODUCTION_ACTIVE_WEIGHTS.clone(),
        deadline: deadline_ms.map(Duration::from_millis),
        lahc_rr_period,
        lahc_kempe_period,
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config)?;
    serde_json::to_string(&solution).map_err(|e| Error::Input(format!("serialize: {e}")))
}

pub fn solve_json(json: &str) -> Result<String, Error> {
    solve_json_with_config(json, Some(200), None, None)
}
```

- [ ] **Step 2.4: Update `solver-py/src/lib.rs` PyO3 wrapper.** Change `py_solve_json_with_config` to forward the new kwargs (PyO3 0.28 syntax):

```rust
#[pyfunction]
#[pyo3(
    name = "solve_json_with_config",
    signature = (problem_json, deadline_ms, lahc_rr_period=None, lahc_kempe_period=None)
)]
fn py_solve_json_with_config(
    py: Python<'_>,
    problem_json: &str,
    deadline_ms: Option<u64>,
    lahc_rr_period: Option<u32>,
    lahc_kempe_period: Option<u32>,
) -> PyResult<String> {
    py.detach(|| {
        solver_core::solve_json_with_config(
            problem_json,
            deadline_ms,
            lahc_rr_period,
            lahc_kempe_period,
        )
    })
    .map_err(|e| PyValueError::new_err(e.to_string()))
}
```

`py_solve_json` stays unchanged (calls `solver_core::solve_json` which now defaults the new args internally).

- [ ] **Step 2.5: Update `_rust.pyi` and `__init__.pyi`.** Both files carry the same signature for `solve_json_with_config`. Replace each block with:

```python
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
```

- [ ] **Step 2.6: Rebuild and re-run.** `mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_solve_json_lahc_kwargs.py -v`. Expected: PASS. Also run `cargo nextest run -p solver-core` to confirm no Rust regression.

- [ ] **Step 2.7: Lint + commit.**

```bash
mise run lint
git add solver/solver-core/src/json.rs solver/solver-py/src/lib.rs \
        solver/solver-py/python/klassenzeit_solver/_rust.pyi \
        solver/solver-py/python/klassenzeit_solver/__init__.pyi \
        solver/solver-py/tests/test_solve_json_lahc_kwargs.py
git commit -m "feat(solver): widen solve_json_with_config with lahc period kwargs"
```

---

## Task 3: Add `score_solution_json` PyO3 binding

**Why third:** `cpsat.py` (Task 5) needs to populate `Solution.soft_score` post-solve using the Rust scorer. The bench harness (Task 7) also calls it on the cpsat backend's parsed output. Adding it before either consumer keeps the build green.

**Files:**
- Modify: `solver/solver-py/src/lib.rs` (new `#[pyfunction]`).
- Modify: `solver/solver-py/python/klassenzeit_solver/__init__.py` (re-export).
- Modify: `solver/solver-py/python/klassenzeit_solver/_rust.pyi` (typing).
- Modify: `solver/solver-py/python/klassenzeit_solver/__init__.pyi` (typing).
- Test: `solver/solver-py/tests/test_score_solution_json.py` (new file).

- [ ] **Step 3.1: Write the failing test.** Create `solver/solver-py/tests/test_score_solution_json.py`:

```python
"""Round-trip test: score_solution_json reproduces solve_json's reported soft_score."""

import json

from klassenzeit_solver import score_solution_json, solve_json_with_config


def test_score_solution_json_matches_solve_json_soft_score_on_grundschule() -> None:
    # Use the existing pinned-placements fixture loader from the sibling test
    # file as the cheapest known-good Problem JSON.
    from tests.test_solve_json_pinned_placements import grundschule_problem_json  # type: ignore[import-not-found]
    problem_json = grundschule_problem_json()
    solution_json = solve_json_with_config(problem_json, deadline_ms=None)
    solution = json.loads(solution_json)
    placements_json = json.dumps(solution["placements"])

    rescored = score_solution_json(problem_json, placements_json)

    assert rescored == solution["soft_score"], (
        f"rescored={rescored} solver-reported={solution['soft_score']}"
    )


def test_score_solution_json_zero_for_empty_placements() -> None:
    minimal_json = '{"week_scheme":{"id":"00000000-0000-0000-0000-000000000001","name":"x","time_blocks":[]},"school_classes":[],"teachers":[],"rooms":[],"subjects":[],"lessons":[],"lesson_groups":[],"pinned_placements":[]}'
    assert score_solution_json(minimal_json, "[]") == 0


def test_score_solution_json_raises_on_invalid_problem() -> None:
    import pytest

    with pytest.raises(ValueError):
        score_solution_json("not json", "[]")
```

If `tests/test_solve_json_pinned_placements.py` does not export a `grundschule_problem_json` helper, factor one out from its existing fixture builder in this same task; rename the existing test's local builder if needed and import from the helper.

- [ ] **Step 3.2: Run, expect failure.** `uv run pytest solver/solver-py/tests/test_score_solution_json.py -v` → `ImportError: cannot import name 'score_solution_json'`.

- [ ] **Step 3.3: Add the PyO3 wrapper.** In `solver/solver-py/src/lib.rs`, add (above the `_rust` module fn):

```rust
/// Score a `Placement[]` against a `Problem` using the production-active
/// `ConstraintWeights`. Returns the same `u32` soft-score that
/// `solver_core::score_solution` produces internally during a `solve_with_config`
/// call. Used by the CP-SAT path in `klassenzeit_solver.cpsat` to populate
/// `Solution.soft_score` post-solve, so all bake-off backends compare on the
/// same Rust scorer (ADR 0030).
#[pyfunction]
#[pyo3(name = "score_solution_json", signature = (problem_json, placements_json))]
fn py_score_solution_json(
    py: Python<'_>,
    problem_json: &str,
    placements_json: &str,
) -> PyResult<u32> {
    py.detach(|| {
        let problem: solver_core::types::Problem = serde_json::from_str(problem_json)
            .map_err(|e| solver_core::Error::Input(format!("json (problem): {e}")))?;
        let placements: Vec<solver_core::types::Placement> =
            serde_json::from_str(placements_json)
                .map_err(|e| solver_core::Error::Input(format!("json (placements): {e}")))?;
        Ok::<u32, solver_core::Error>(solver_core::score_solution(
            &problem,
            &placements,
            &solver_core::PRODUCTION_ACTIVE_WEIGHTS,
        ))
    })
    .map_err(|e| PyValueError::new_err(e.to_string()))
}
```

Then register it in the `_rust` pymodule fn:

```rust
#[pymodule]
fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_solve_json, m)?)?;
    m.add_function(wrap_pyfunction!(py_solve_json_with_config, m)?)?;
    m.add_function(wrap_pyfunction!(py_score_solution_json, m)?)?;
    Ok(())
}
```

If `solver_core::types::Problem` or `Placement` are not already `pub` re-exported, add them. Check by `grep -n 'pub use types' solver/solver-core/src/lib.rs`.

- [ ] **Step 3.4: Update `__init__.py` to re-export.** In `solver/solver-py/python/klassenzeit_solver/__init__.py`:

```python
"""Python bindings for the Klassenzeit constraint solver."""

from ._rust import score_solution_json, solve_json, solve_json_with_config

__all__ = ["score_solution_json", "solve_json", "solve_json_with_config"]
```

- [ ] **Step 3.5: Update `_rust.pyi` and `__init__.pyi`.** Add to both:

```python
def score_solution_json(problem_json: str, placements_json: str) -> int:
    """Score a Placement[] against a Problem using production-active weights.

    Returns the integer soft-score that ``solve_json`` would produce on the
    same problem given those placements. Used by the CP-SAT seed path
    (``klassenzeit_solver.cpsat``) to populate ``Solution.soft_score``
    post-solve so all bake-off backends compare on the same Rust scorer
    (ADR 0030).

    Raises ``ValueError`` on malformed JSON in either argument.
    """
```

- [ ] **Step 3.6: Rebuild and re-run.** `mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_score_solution_json.py -v` → PASS.

- [ ] **Step 3.7: Lint + commit.**

```bash
mise run lint
git add solver/solver-py/src/lib.rs \
        solver/solver-py/python/klassenzeit_solver/__init__.py \
        solver/solver-py/python/klassenzeit_solver/_rust.pyi \
        solver/solver-py/python/klassenzeit_solver/__init__.pyi \
        solver/solver-py/tests/test_score_solution_json.py
git commit -m "feat(solver-py): add score_solution_json PyO3 binding"
```

---

## Task 4: Add `ortools` dependency

**Why fourth:** `cpsat.py` (Task 5) imports `ortools.sat.python.cp_model` from its first line. Adding the dep before a caller bundles the change with its first use per `solver/CLAUDE.md`'s "bundle helper with first caller" rule (the helper here is the dep itself).

**Files:**
- Modify: `solver/solver-py/pyproject.toml` (add `ortools` to `dependencies`).
- Modify: `uv.lock` (regenerated by `uv add`).

- [ ] **Step 4.1: Add the dep via `uv`.** From the repo root:

```bash
uv add 'ortools>=9.10,<10' --package klassenzeit-solver
```

This rewrites `solver/solver-py/pyproject.toml` and `uv.lock`. Verify the pyproject change shows the new entry under `dependencies` (the section already exists) and the version range is what was passed.

- [ ] **Step 4.2: Verify the wheel installs.**

```bash
uv sync
uv run python -c "from ortools.sat.python import cp_model; print(cp_model.CpModel.__name__)"
```

Expected: prints `CpModel`. Confirms the wheel is importable on this host.

- [ ] **Step 4.3: Lint.** `mise run lint`. The `cargo machete` step runs over Rust deps and is unaffected; the Python lint stack (`ruff`, `ty`, `vulture`) does not yet see any `ortools` import (Task 5 adds the first one), so the dep-without-caller issue does not bite Python.

- [ ] **Step 4.4: Commit.**

```bash
git add solver/solver-py/pyproject.toml uv.lock
git commit -m "build(solver-py): add ortools to klassenzeit-solver dependencies"
```

---

## Task 5: Implement `solve_cpsat_json` in `klassenzeit_solver.cpsat`

**Why fifth:** Largest task. Implements the CP-SAT model per the spec's "CP-SAT model semantics" section. Depends on the score binding (Task 3) and the `ortools` dep (Task 4).

**Files:**
- Create: `solver/solver-py/python/klassenzeit_solver/cpsat.py`.
- Modify: `solver/solver-py/python/klassenzeit_solver/__init__.py` (re-export).
- Modify: `solver/solver-py/python/klassenzeit_solver/__init__.pyi` (typing).
- Test: `solver/solver-py/tests/test_cpsat.py`, `solver/solver-py/tests/test_cpsat_determinism.py` (new files).

- [ ] **Step 5.1: Write the failing tests.** Create `solver/solver-py/tests/test_cpsat.py`:

```python
"""CP-SAT seed contract tests."""

from __future__ import annotations

import json
import pytest

from klassenzeit_solver import solve_cpsat_json


def _grundschule_problem_json() -> str:
    """Return a JSON-serialised demo Grundschule Problem.

    Reuses the helper introduced in test_score_solution_json (Task 3).
    """
    from tests.test_solve_json_pinned_placements import grundschule_problem_json
    return grundschule_problem_json()


def test_solve_cpsat_json_grundschule_returns_zero_violations() -> None:
    problem_json = _grundschule_problem_json()
    out = solve_cpsat_json(problem_json, deadline_ms=5_000)
    solution = json.loads(out)
    assert solution["violations"] == [], f"unexpected violations: {solution['violations']}"
    assert len(solution["placements"]) > 0


def test_solve_cpsat_json_doppelstunde_block_contiguity() -> None:
    problem_json = _grundschule_problem_json()
    out = solve_cpsat_json(problem_json, deadline_ms=5_000)
    solution = json.loads(out)

    # Group placements by (lesson_id, day_of_week, room_id) and assert
    # each group's positions form a contiguous N-position run, where N is
    # the lesson's preferred_block_size.
    problem = json.loads(problem_json)
    lessons_by_id = {lesson["id"]: lesson for lesson in problem["lessons"]}
    tbs_by_id = {tb["id"]: tb for tb in problem["week_scheme"]["time_blocks"]}

    by_block: dict[tuple[str, int, str], list[int]] = {}
    for p in solution["placements"]:
        tb = tbs_by_id[p["time_block_id"]]
        key = (p["lesson_id"], tb["day_of_week"], p["room_id"])
        by_block.setdefault(key, []).append(tb["position"])

    for (lesson_id, _, _), positions in by_block.items():
        n = lessons_by_id[lesson_id]["preferred_block_size"]
        positions.sort()
        # Each block of N positions must be contiguous; allow multiple blocks
        # per (lesson, day, room) if hours_per_week is divided across them.
        chunks = [positions[i : i + n] for i in range(0, len(positions), n)]
        for chunk in chunks:
            assert all(chunk[i + 1] == chunk[i] + 1 for i in range(len(chunk) - 1)), (
                f"non-contiguous block for {lesson_id}: {chunk}"
            )


def test_solve_cpsat_json_same_room_invariant_holds() -> None:
    problem_json = _grundschule_problem_json()
    out = solve_cpsat_json(problem_json, deadline_ms=5_000)
    solution = json.loads(out)
    problem = json.loads(problem_json)

    lessons_by_id = {lesson["id"]: lesson for lesson in problem["lessons"]}

    # For each (class, subject) group with > 1 lesson, assert all placements
    # of those lessons share one room.
    by_cs: dict[tuple[frozenset[str], str], set[str]] = {}
    for p in solution["placements"]:
        lesson = lessons_by_id[p["lesson_id"]]
        key = (frozenset(lesson["school_class_ids"]), lesson["subject_id"])
        by_cs.setdefault(key, set()).add(p["room_id"])

    for key, rooms in by_cs.items():
        # Multi-class lessons (lesson_groups) handle differently; restrict
        # check to single-class lessons whose (class, subject) co-appears
        # at >1 lesson.
        if len(key[0]) == 1:
            assert len(rooms) == 1, f"same-room invariant broken for {key}: {rooms}"


def test_solve_cpsat_json_pinned_placements_round_trip() -> None:
    from tests.test_solve_json_pinned_placements import (
        grundschule_problem_with_pins,
    )
    problem_json, pinned = grundschule_problem_with_pins()
    out = solve_cpsat_json(problem_json, deadline_ms=5_000)
    solution = json.loads(out)
    placements = {(p["lesson_id"], p["time_block_id"]): p["room_id"] for p in solution["placements"]}
    for pin in pinned:
        assert (pin["lesson_id"], pin["time_block_id"]) in placements, (
            f"pin missing in solution: {pin}"
        )
        assert placements[(pin["lesson_id"], pin["time_block_id"])] == pin["room_id"]


def test_solve_cpsat_json_invalid_json_raises_value_error() -> None:
    with pytest.raises(ValueError):
        solve_cpsat_json("not json", deadline_ms=1_000)


def test_solve_cpsat_json_infeasible_returns_violations_with_reason() -> None:
    # Build a tiny problem where one lesson asks for 5 hours but the week
    # scheme only has 3 time blocks across all days. INFEASIBLE.
    minimal_infeasible = json.dumps({
        "week_scheme": {
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "x",
            "time_blocks": [
                {"id": f"00000000-0000-0000-0000-00000000000{i}", "day_of_week": 0, "position": p, "start_time": "08:00", "end_time": "08:45"}
                for i, p in enumerate([2, 3, 4], start=2)
            ],
        },
        "school_classes": [{"id": "00000000-0000-0000-0000-000000000010", "name": "1a", "grade_level": 1, "home_room_id": None}],
        "teachers": [{"id": "00000000-0000-0000-0000-000000000020", "short_code": "AA", "name": "Anna", "max_hours_per_week": 100, "blocked_time_blocks": []}],
        "rooms": [{"id": "00000000-0000-0000-0000-000000000030", "name": "R", "is_external": False, "blocked_time_blocks": [], "suitable_subjects": ["00000000-0000-0000-0000-000000000040"]}],
        "subjects": [{"id": "00000000-0000-0000-0000-000000000040", "name": "M", "short_code": "M", "color": "#000000", "prefer_early_period": 0, "avoid_first_period": 0, "avoid_last_period": 0, "prefer_late_period": 0}],
        "lessons": [{
            "id": "00000000-0000-0000-0000-000000000050",
            "school_class_ids": ["00000000-0000-0000-0000-000000000010"],
            "teacher_id": "00000000-0000-0000-0000-000000000020",
            "subject_id": "00000000-0000-0000-0000-000000000040",
            "hours_per_week": 5,
            "preferred_block_size": 1,
            "lesson_group_id": None,
        }],
        "lesson_groups": [],
        "pinned_placements": [],
    })

    out = solve_cpsat_json(minimal_infeasible, deadline_ms=2_000)
    solution = json.loads(out)
    assert solution["placements"] == []
    assert len(solution["violations"]) >= 1
    kinds = {v["kind"] for v in solution["violations"]}
    assert "no_free_time_block" in kinds
    reasons = [v.get("reason") for v in solution["violations"] if v.get("reason")]
    assert any("cpsat" in r for r in reasons), f"no CP-SAT reason found: {reasons}"
```

Create `solver/solver-py/tests/test_cpsat_determinism.py`:

```python
"""CP-SAT determinism property test: same (problem, deadline_ms, seed) -> identical output."""

from klassenzeit_solver import solve_cpsat_json


def test_solve_cpsat_json_deterministic_under_seed_and_deadline() -> None:
    from tests.test_solve_json_pinned_placements import grundschule_problem_json
    problem_json = grundschule_problem_json()

    a = solve_cpsat_json(problem_json, deadline_ms=3_000, seed=7)
    b = solve_cpsat_json(problem_json, deadline_ms=3_000, seed=7)

    assert a == b
```

- [ ] **Step 5.2: Run, expect failure.** `uv run pytest solver/solver-py/tests/test_cpsat.py solver/solver-py/tests/test_cpsat_determinism.py -v` → `ImportError: cannot import name 'solve_cpsat_json'`.

- [ ] **Step 5.3: Implement `cpsat.py`.** Create `solver/solver-py/python/klassenzeit_solver/cpsat.py`. The body has five sections (parse, model build, constraints, solve, marshal). Approximate length 250-350 lines. Implementation skeleton, not finished prose:

```python
"""CP-SAT seed phase via Google OR-Tools.

Sprint 4 of the solver feasibility bake-off (ADR 0029). Implements an
alternate solver path that builds a CP-SAT model from the same Problem JSON
the Rust ``solve_json`` consumes, solves it under a wall-clock deadline,
and marshals the result back into the existing Solution wire format. Soft
scoring runs through the Rust ``score_solution_json`` PyO3 binding so all
four bake-off backends compare on the same Rust scorer (ADR 0030).

Public surface: ``solve_cpsat_json(problem_json, deadline_ms, seed=1) -> str``.
"""

from __future__ import annotations

import json
from typing import Any

from ortools.sat.python import cp_model  # ty: ignore[unresolved-import]

from klassenzeit_solver import score_solution_json


def solve_cpsat_json(
    problem_json: str,
    deadline_ms: int | None,
    seed: int = 1,
) -> str:
    """Solve a Klassenzeit timetable via CP-SAT.

    Returns a Solution JSON in the same wire format as ``solve_json``.
    On INFEASIBLE/UNKNOWN, returns a Solution with no placements and one
    NoFreeTimeBlock violation per lesson, ``reason="cpsat: <CpSolverStatus>"``.
    On MODEL_INVALID, raises RuntimeError (programmer bug).
    """
    try:
        problem = json.loads(problem_json)
    except json.JSONDecodeError as exc:
        raise ValueError(f"json: {exc}") from exc

    builder = _ModelBuilder(problem)
    model, anchor_vars, problem_meta = builder.build()

    solver = cp_model.CpSolver()
    solver.parameters.num_search_workers = 1
    solver.parameters.random_seed = seed
    solver.parameters.log_search_progress = False
    if deadline_ms is not None:
        solver.parameters.max_time_in_seconds = deadline_ms / 1000.0

    status = solver.Solve(model)

    if status in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        placements = builder.extract_placements(solver, anchor_vars, problem_meta)
        soft_score = score_solution_json(problem_json, json.dumps(placements))
        solution = {"placements": placements, "violations": [], "soft_score": soft_score}
    elif status in (cp_model.INFEASIBLE, cp_model.UNKNOWN):
        status_name = solver.StatusName(status).lower()
        solution = {
            "placements": [],
            "violations": [
                {
                    "kind": "no_free_time_block",
                    "lesson_id": lesson["id"],
                    "reason": f"cpsat: {status_name}",
                }
                for lesson in problem["lessons"]
            ],
            "soft_score": 0,
        }
    elif status == cp_model.MODEL_INVALID:
        raise RuntimeError(
            f"cpsat: model invalid - bug in cpsat.py (status={solver.StatusName(status)})"
        )
    else:
        raise RuntimeError(f"cpsat: unexpected solver status: {solver.StatusName(status)}")

    return json.dumps(solution)


# ----------------------------------------------------------------------
# Model builder
# ----------------------------------------------------------------------


class _ModelBuilder:
    """Builds a CP-SAT model from a Problem dict using the per-block-anchor encoding.

    Variable: ``y[lesson_id, day_of_week, start_position, room_id] in {0, 1}``
    where the anchor's window (start_position .. start_position + N - 1) fits
    on day_of_week and room is suitable for the lesson's subject and the
    teacher / room aren't blocked at any window position.
    """

    def __init__(self, problem: dict[str, Any]) -> None:
        self.problem = problem
        # Indices and lookups computed once.
        # ... (lesson_lookup, room_lookup, subject_lookup, teacher_lookup,
        #      time_blocks_by_day, blocked_tbs_by_teacher, blocked_tbs_by_room,
        #      suitable_rooms_by_subject, etc.)

    def build(self) -> tuple[cp_model.CpModel, dict[Any, cp_model.IntVar], dict[str, Any]]:
        model = cp_model.CpModel()
        anchor_vars: dict[tuple[str, int, int, str], cp_model.IntVar] = {}
        # ... (variable creation with the four pruning rules from the spec)
        # ... (constraint emission: cardinality, class non-overlap, teacher
        #      non-overlap, room non-overlap, teacher max-hours, same-room,
        #      lesson-group co-placement, pinned placements)
        # ... (objective: model.Minimize(0))
        return model, anchor_vars, {"lesson_lookup": ..., "tb_lookup": ...}

    def extract_placements(
        self,
        solver: cp_model.CpSolver,
        anchor_vars: dict[tuple[str, int, int, str], cp_model.IntVar],
        meta: dict[str, Any],
    ) -> list[dict[str, str]]:
        """Walk the solved anchor variables, emit one Placement per occupied
        time block (anchor's N-block window expanded to N rows)."""
        out: list[dict[str, str]] = []
        for (lesson_id, day, start_pos, room_id), var in anchor_vars.items():
            if solver.Value(var) != 1:
                continue
            n = meta["lesson_lookup"][lesson_id]["preferred_block_size"]
            for i in range(n):
                tb_id = meta["tb_at"][(day, start_pos + i)]
                out.append(
                    {"lesson_id": lesson_id, "time_block_id": tb_id, "room_id": room_id}
                )
        return out


# ----------------------------------------------------------------------
# CLI entry for `python -m klassenzeit_solver.cpsat`
# ----------------------------------------------------------------------


def _main() -> None:
    import argparse
    import pathlib
    import sys

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--problem-file", required=True, type=pathlib.Path)
    parser.add_argument("--deadline-ms", type=int, required=True)
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()
    problem_json = args.problem_file.read_text()
    sys.stdout.write(solve_cpsat_json(problem_json, deadline_ms=args.deadline_ms, seed=args.seed))


if __name__ == "__main__":
    _main()
```

The skeleton above marks the gaps with `...` for brevity in this plan; a fresh subagent fills each gap with complete code. The full hard-constraint encoding follows the spec's "CP-SAT model semantics" section.

- [ ] **Step 5.4: Re-export from `__init__.py`.** Update `solver/solver-py/python/klassenzeit_solver/__init__.py`:

```python
"""Python bindings for the Klassenzeit constraint solver."""

from ._rust import score_solution_json, solve_json, solve_json_with_config
from .cpsat import solve_cpsat_json

__all__ = [
    "score_solution_json",
    "solve_cpsat_json",
    "solve_json",
    "solve_json_with_config",
]
```

- [ ] **Step 5.5: Update `__init__.pyi` typing stub.** Append the CP-SAT signature to `solver/solver-py/python/klassenzeit_solver/__init__.pyi`:

```python
def solve_cpsat_json(
    problem_json: str,
    deadline_ms: int | None,
    seed: int = 1,
) -> str:
    """Solve a Klassenzeit timetable via CP-SAT (Google OR-Tools).

    Returns a Solution JSON in the same wire format as ``solve_json``. On
    INFEASIBLE / UNKNOWN, returns a Solution with no placements and one
    NoFreeTimeBlock violation per lesson, reason='cpsat: <status>'. On
    MODEL_INVALID, raises RuntimeError. ADR 0030.
    """
```

- [ ] **Step 5.6: Run tests, expect PASS.** `uv run pytest solver/solver-py/tests/test_cpsat.py solver/solver-py/tests/test_cpsat_determinism.py -v`. Iterate on the model encoding until all tests pass. The doppelstunde / same-room / pinned tests are the hardest to get right; add `print(solver.StatusName(status))` and `solver.ResponseStats()` while debugging.

- [ ] **Step 5.7: CLI smoke check.** `uv run python -m klassenzeit_solver.cpsat --problem-file <path-to-grundschule.json> --deadline-ms 5000 --seed 1 | jq '.placements | length'`. Expected: a positive number. (Construct the JSON via a one-liner from the test helper if needed.)

- [ ] **Step 5.8: Lint + commit.**

```bash
mise run lint
git add solver/solver-py/python/klassenzeit_solver/cpsat.py \
        solver/solver-py/python/klassenzeit_solver/__init__.py \
        solver/solver-py/python/klassenzeit_solver/__init__.pyi \
        solver/solver-py/tests/test_cpsat.py \
        solver/solver-py/tests/test_cpsat_determinism.py
git commit -m "feat(solver-py): solve_cpsat_json CP-SAT seed via ortools"
```

---

## Task 6: Backend `solver_backend` setting and `KZ_SOLVER_BACKEND` dispatch

**Files:**
- Modify: `backend/src/klassenzeit_backend/core/settings.py` (add field).
- Modify: `backend/src/klassenzeit_backend/scheduling/solver_io.py` (rewrite `run_solve`).
- Modify: `backend/.env.example` (document new var).
- Test: `backend/tests/scheduling/test_solver_io_backend_dispatch.py` (new file).

- [ ] **Step 6.1: Write the failing dispatch test.** Create `backend/tests/scheduling/test_solver_io_backend_dispatch.py`:

```python
"""Backend dispatch on KZ_SOLVER_BACKEND settings field."""

from __future__ import annotations

import json
from typing import Literal
from unittest.mock import patch

import pytest

from klassenzeit_backend.scheduling import solver_io


@pytest.mark.parametrize(
    "backend",
    ["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"],
)
async def test_run_solve_dispatches_each_backend(
    monkeypatch: pytest.MonkeyPatch,
    app,  # pragma: no cover - fixture from conftest
    backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"],
) -> None:
    monkeypatch.setattr(app.state.settings, "solver_backend", backend)

    minimal_problem = json.dumps({
        "week_scheme": {
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "x",
            "time_blocks": [],
        },
        "school_classes": [],
        "teachers": [],
        "rooms": [],
        "subjects": [],
        "lessons": [],
        "lesson_groups": [],
        "pinned_placements": [],
    })
    counts = {"week_schemes": 1}
    out = await solver_io.run_solve(
        minimal_problem,
        scope_id=None,
        input_counts=counts,
        deadline_ms=None,
    )
    # Empty problem solves with zero placements regardless of backend.
    assert out["placements"] == []
    assert out["violations"] == []
```

If the `app` fixture does not exist in `backend/tests/scheduling/conftest.py`, hand-build a minimal `app` with `app.state.settings = Settings(solver_backend=backend, ...)` inside the test. (Spike via `grep -n "^def app\|^@pytest.fixture" backend/tests/conftest.py` first.)

Also add a tiny test that the cpsat dispatch path lands on the CP-SAT entry point (not on solve_json_with_config):

```python
async def test_run_solve_cpsat_dispatch_uses_solve_cpsat_json(
    monkeypatch: pytest.MonkeyPatch, app
) -> None:
    monkeypatch.setattr(app.state.settings, "solver_backend", "cpsat")
    captured: dict[str, str] = {}

    def fake_cpsat(problem_json, deadline_ms):  # type: ignore[no-untyped-def]
        captured["called"] = "cpsat"
        return '{"placements":[],"violations":[],"soft_score":0}'

    monkeypatch.setattr(solver_io, "_solve_cpsat_json", fake_cpsat)
    minimal_problem = '{"week_scheme":{"id":"00000000-0000-0000-0000-000000000001","name":"x","time_blocks":[]},"school_classes":[],"teachers":[],"rooms":[],"subjects":[],"lessons":[],"lesson_groups":[],"pinned_placements":[]}'

    await solver_io.run_solve(
        minimal_problem, scope_id=None, input_counts={}, deadline_ms=None
    )

    assert captured.get("called") == "cpsat"
```

- [ ] **Step 6.2: Run, expect failure.** `mise run test:py -- backend/tests/scheduling/test_solver_io_backend_dispatch.py -v` → fails on missing `solver_backend` field on Settings.

- [ ] **Step 6.3: Add the Settings field.** In `backend/src/klassenzeit_backend/core/settings.py`, after `solve_deadline_ms`:

```python
    # Solver
    solve_deadline_ms: int | None = 200
    solver_backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"] = "lahc"
```

- [ ] **Step 6.4: Rewrite `run_solve`.** In `backend/src/klassenzeit_backend/scheduling/solver_io.py`:

Replace the import line:

```python
from klassenzeit_solver import (
    score_solution_json as _score_solution_json,  # noqa: F401  used by cpsat downstream
    solve_cpsat_json as _solve_cpsat_json,
    solve_json_with_config as _solve_json_with_config,
)
```

Replace the body of `run_solve` (the `await asyncio.to_thread(...)` block) with the match-dispatch from the spec:

```python
    backend = app.state.settings.solver_backend  # type: ignore[attr-defined]  resolved via DI helper if present
    started = time.monotonic()
    try:
        match backend:
            case "lahc":
                solution_json = await asyncio.to_thread(
                    _solve_json_with_config, problem_json, deadline_ms,
                )
            case "lahc_rr":
                solution_json = await asyncio.to_thread(
                    _solve_json_with_config, problem_json, deadline_ms,
                    lahc_rr_period=25,
                )
            case "lahc_rr_kempe":
                solution_json = await asyncio.to_thread(
                    _solve_json_with_config, problem_json, deadline_ms,
                    lahc_rr_period=25, lahc_kempe_period=23,
                )
            case "cpsat":
                solution_json = await asyncio.to_thread(
                    _solve_cpsat_json, problem_json, deadline_ms,
                )
    except (ValueError, RuntimeError) as exc:
        ...
```

The `app.state.settings` is not directly available inside `run_solve` today; spike with `grep -n "app.state\|get_settings" backend/src/klassenzeit_backend/scheduling/solver_io.py` to find the existing access pattern. If none exists, add `from klassenzeit_backend.core.settings import get_settings` and call `get_settings().solver_backend` inside the function (the `lru_cache` on `get_settings` is fine for the production singleton; tests inject via `monkeypatch.setattr(get_settings.cache_info().cache, ...)` or use the monkeypatch shape above adjusted accordingly).

Also extend the structured-log call to include the backend:

```python
    logger.info(
        "solver.solve.start",
        extra={"school_class_id": scope_str, "backend": backend, **input_counts},
    )
    ...
    logger.info(
        "solver.solve.done",
        extra={
            "school_class_id": scope_str, "backend": backend, "duration_ms": duration_ms,
            "placements_total": len(solution["placements"]),
            "violations_total": len(solution["violations"]),
            "violations_by_kind": _count_violations_by_kind(solution["violations"]),
            "soft_score": solution.get("soft_score", 0),
        },
    )
```

- [ ] **Step 6.5: Document the env var.** In `backend/.env.example` after `KZ_SOLVE_DEADLINE_MS`:

```ini
# Which solver backend to dispatch (one of: lahc, lahc_rr, lahc_rr_kempe, cpsat).
# Default `lahc` matches pre-bake-off behaviour. The cpsat backend requires
# the `ortools` Python wheel which ships with `klassenzeit-solver`. ADR 0030.
KZ_SOLVER_BACKEND=lahc
```

- [ ] **Step 6.6: Run, expect PASS.** `mise run test:py -- backend/tests/scheduling/test_solver_io_backend_dispatch.py -v`.

Also re-run the existing `test_solver_io.py` to confirm no regression: `mise run test:py -- backend/tests/scheduling/test_solver_io.py -v`.

- [ ] **Step 6.7: Lint + commit.**

```bash
mise run lint
git add backend/src/klassenzeit_backend/core/settings.py \
        backend/src/klassenzeit_backend/scheduling/solver_io.py \
        backend/.env.example \
        backend/tests/scheduling/test_solver_io_backend_dispatch.py
git commit -m "feat(backend): add solver_backend setting and KZ_SOLVER_BACKEND dispatch"
```

---

## Task 7: Solver-bench `cpsat` backend column + bench refresh

**Files:**
- Modify: `solver/solver-bench/src/main.rs` (BenchBackend variant + run_cell branch).
- Modify: `solver/solver-core/benches/BENCH_RESULTS.md` (regenerated by `mise run bench:bakeoff`).

- [ ] **Step 7.1: Write the failing bench harness contract test.** In `solver/solver-bench/src/main.rs::tests`, add:

```rust
#[test]
fn write_row_renders_cpsat_backend_label() {
    let cell = CellResult {
        seeds: 20,
        feasibility_count: 18,
        hard_violations_median: 0,
        soft_score_median: Some(15),
        ffd_ms_median: 0.0,
        total_ms_median: 60050.0,
    };
    let mut out = String::new();
    write_row(&mut out, "grundschule", BenchBackend::CpSat, &cell);
    assert!(out.contains("| cpsat |"));
}

#[test]
fn cpsat_subprocess_command_args_match_module_invocation() {
    let cmd = build_cpsat_command(
        std::path::Path::new("/tmp/p.json"),
        std::time::Duration::from_secs(60),
        7,
    );
    let argv: Vec<&str> = cmd
        .get_args()
        .filter_map(|a| a.to_str())
        .collect();
    assert_eq!(
        argv,
        vec![
            "-m",
            "klassenzeit_solver.cpsat",
            "--problem-file",
            "/tmp/p.json",
            "--deadline-ms",
            "60000",
            "--seed",
            "7",
        ]
    );
    assert_eq!(cmd.get_program(), "python3");
}
```

- [ ] **Step 7.2: Run, expect failure.** `cargo nextest run -p solver-bench`. Fails on missing `BenchBackend::CpSat` and `build_cpsat_command`.

- [ ] **Step 7.3: Add the variant + label.** In `solver/solver-bench/src/main.rs`:

```rust
#[derive(Clone, Copy)]
enum BenchBackend {
    Lahc,
    LahcRr,
    LahcRrKempe,
    CpSat,
}

impl BenchBackend {
    fn label(self) -> &'static str {
        match self {
            BenchBackend::Lahc => "lahc",
            BenchBackend::LahcRr => "lahc_rr",
            BenchBackend::LahcRrKempe => "lahc_rr_kempe",
            BenchBackend::CpSat => "cpsat",
        }
    }
}
```

Update the `backends` array in `main()`:

```rust
    let backends = [
        BenchBackend::Lahc,
        BenchBackend::LahcRr,
        BenchBackend::LahcRrKempe,
        BenchBackend::CpSat,
    ];
```

- [ ] **Step 7.4: Add `build_cpsat_command` + the run-cell branch.** Below `run_cell`, add:

```rust
fn build_cpsat_command(
    problem_path: &std::path::Path,
    budget: std::time::Duration,
    seed: u64,
) -> std::process::Command {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("-m")
        .arg("klassenzeit_solver.cpsat")
        .arg("--problem-file")
        .arg(problem_path)
        .arg("--deadline-ms")
        .arg(budget.as_millis().to_string())
        .arg("--seed")
        .arg(seed.to_string());
    cmd
}

fn run_cpsat_cell(
    problem: &Problem, budget: std::time::Duration, seeds: u64,
) -> CellResult {
    let problem_json = serde_json::to_string(problem)
        .expect("serialise problem for cpsat subprocess");
    let tmpfile = tempfile_path("kz-bench-problem-", ".json");
    std::fs::write(&tmpfile, problem_json.as_bytes()).expect("write problem tempfile");

    let mut total_ms_samples: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut hard_violations_samples: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut soft_score_feasible: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut feasibility_count: u64 = 0;
    let mut ffd_ms = 0.0_f64;  // CP-SAT has no FFD pass; reported as 0.

    for seed in 1..=seeds {
        let start = std::time::Instant::now();
        let output = build_cpsat_command(&tmpfile, budget, seed).output();
        let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
        let solution_json = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            Ok(o) => {
                eprintln!("cpsat subprocess non-zero exit: {}", String::from_utf8_lossy(&o.stderr));
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                continue;
            }
            Err(e) => {
                eprintln!("cpsat subprocess error: {e}");
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                continue;
            }
        };
        let solution: solver_core::types::Solution = match serde_json::from_str(&solution_json) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cpsat parse error: {e}");
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                continue;
            }
        };
        let hard = solution.violations.len() as u32;
        let feasible = hard == 0;
        if feasible {
            feasibility_count += 1;
            let soft = solver_core::score_solution(
                problem, &solution.placements, &solver_core::PRODUCTION_ACTIVE_WEIGHTS,
            );
            soft_score_feasible.push(soft);
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
    }

    let _ = std::fs::remove_file(&tmpfile);

    CellResult {
        seeds, feasibility_count,
        hard_violations_median: median_u32(&mut hard_violations_samples),
        soft_score_median: if soft_score_feasible.is_empty() {
            None
        } else {
            Some(median_u32(&mut soft_score_feasible))
        },
        ffd_ms_median: ffd_ms,
        total_ms_median: median_f64(&mut total_ms_samples),
    }
}

fn tempfile_path(prefix: &str, suffix: &str) -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("{prefix}{nanos}{suffix}"))
}
```

Then split `run_cell`'s top-level dispatch on `BenchBackend::CpSat`:

```rust
fn run_cell(backend: BenchBackend, problem: &Problem, budget: Duration, seeds: u64) -> CellResult {
    match backend {
        BenchBackend::CpSat => return run_cpsat_cell(problem, budget, seeds),
        _ => {}
    }
    // ... existing Lahc / LahcRr / LahcRrKempe code unchanged ...
}
```

`solver_core::types::Solution` and `Placement` need to be re-exported from solver-core's lib.rs if not already; verify with `grep -n 'pub use types' solver/solver-core/src/lib.rs`.

- [ ] **Step 7.5: Run unit tests, expect PASS.** `cargo nextest run -p solver-bench`. Iterate if any contract test still fails.

- [ ] **Step 7.6: Refresh `BENCH_RESULTS.md` on the dev host.**

```bash
mise run bench:bakeoff
```

This regenerates `solver/solver-core/benches/BENCH_RESULTS.md` with 4 fixtures × 4 backends = 16 rows. Wall-clock budget on the full refresh is ~5.3 hours (4 × 4 × 20 × 60 s). The regeneration is required for the PR to commit a fresh table.

If wall-clock is impractical for the dev session, downscale the sample count for a directional read first, then commit only after the full refresh on a quiet host:

```bash
mise run bench:bakeoff -- --budget 30s --seeds 5 --out /tmp/bench-smoke.md
diff /tmp/bench-smoke.md solver/solver-core/benches/BENCH_RESULTS.md
```

- [ ] **Step 7.7: Commit.**

```bash
mise run lint
git add solver/solver-bench/src/main.rs solver/solver-core/benches/BENCH_RESULTS.md
git commit -m "feat(solver-bench): cpsat backend column + bench refresh"
```

---

## Task 8: ADR 0030 - CP-SAT dependency direction

**Files:**
- Create: `docs/adr/0030-cpsat-dependency-direction.md`.
- Modify: `docs/adr/README.md` (index).

- [ ] **Step 8.1: Confirm number is free.** `ls docs/adr/*.md | sort | tail -3` (expect `0028-...`, `0029-...`, plus `README.md` and `template.md`).

- [ ] **Step 8.2: Create the ADR.** `docs/adr/0030-cpsat-dependency-direction.md`:

```markdown
# 0030: CP-SAT enters via Python ortools, not Rust FFI

## Status

Accepted, 2026-05-04.

## Context

ADR 0029 introduced the four-backend solver feasibility bake-off. Sprints 1-3
shipped the three Rust LAHC variants; Sprint 4 lands the fourth backend,
CP-SAT (Google OR-Tools). The 2026-04-04 algorithm-selection research
(`docs/research/2026-04-04-solver-algorithm-selection.md`, Source 22 + 23)
explicitly rejected CP-SAT on FFI / dependency grounds: "CP-SAT requires
wrapping a C++ library (OR-Tools) via FFI, losing Rust's compile-time
guarantees at the solver boundary."

ADR 0029 already noted that the bake-off revises that dismissal in one
specific way: CP-SAT enters Klassenzeit through Python `ortools`. Sprint 4
makes that revision concrete and irreversible.

## Decision

CP-SAT lives entirely on the Python side. The new module
`solver/solver-py/python/klassenzeit_solver/cpsat.py` is pure Python; it
imports `ortools.sat.python.cp_model` and runs CP-SAT through the
`ortools` PyPI wheel, which static-links the C++ CP-SAT engine. No Rust
crate ever links against OR-Tools. No `cxx` / `bindgen` / FFI step.

The `klassenzeit-solver` Python package (built by maturin from
`solver-py`) becomes the single import for any solver path: Rust LAHC via
the existing `_rust` extension, CP-SAT via the new pure-Python `cpsat`
peer module.

The "thin wrappers only" rule in `solver/CLAUDE.md` is clarified: it
applies to PyO3 wrappers (which must remain thin), not to peer Python
algorithms in the same package.

Soft scoring runs in Rust post-hoc via the new `score_solution_json`
PyO3 binding. The CP-SAT model itself uses `Minimize(0)` and solves only
the hard-feasibility CSP. All four bake-off backends compare on the same
Rust scorer.

## Consequences

**Positive:**

- The 2026-04-04 research's "no FFI / no C++ in Rust" preference stays
  intact for the Rust solver side.
- CP-SAT is a single `pip install ortools` away on any host; the wheel
  ships static-linked binaries for Linux x86_64, macOS arm64, and
  Windows. No build-time C++ toolchain.
- All four backends are switchable via `KZ_SOLVER_BACKEND` env var.
  Production-default decision is informed by `BENCH_RESULTS.md`'s
  Pareto frontier across feasibility, wall-clock, and soft-score.
- Determinism preserved: `num_search_workers=1` plus `random_seed=seed`
  on `cp_model.CpSolver().parameters` makes CP-SAT byte-identical across
  same-input runs.

**Negative:**

- Backend Docker image gains roughly 50 MB from the `ortools` wheel's
  bundled C++ engine. Acceptable for the bake-off comparison; if CP-SAT
  becomes production default we revisit (slim base image, separate
  CP-SAT-only service, etc.).
- Per-cell bench wall-clock for `cpsat` adds ~80 min to the existing
  ~240 min bake-off bench. Full `mise run bench:bakeoff` is now ~5.3 h;
  bench is host-host and not run in CI (consistent with Sprints 1-3).
- A reverse-direction Python-to-Rust import (`cpsat.py` calls
  `score_solution_json`) creates a small surface where module load
  order matters at runtime; the import lives at the top of `cpsat.py`
  and is exercised by the bake-off's first run.

## Alternatives considered

- **Direct FFI to OR-Tools C++ (via `cxx` or `bindgen`).** Rejected; the
  2026-04-04 research's "no FFI / no C++" recommendation stands for the
  Rust side. Static-linked Python wheel sidesteps the build-time C++
  exposure while keeping the same engine.
- **Skip CP-SAT entirely** (LAHC-only bake-off, three backends). Rejected;
  ADR 0029's premise is a four-way comparison so future Sek I / Gymnasium
  fixtures can be re-run against the alternatives without re-implementation.
- **Encode soft constraints into CP-SAT's objective.** Rejected as
  out-of-scope for Sprint 4. The bake-off compares apples-to-apples on
  the Rust soft-scorer; a CP-SAT-side optimisation muddles the
  comparison axis. Re-evaluate post-bench if CP-SAT competes on
  feasibility but loses on soft-score.
```

- [ ] **Step 8.3: Update ADR index.** Append to `docs/adr/README.md`'s table:

```markdown
| [0030](0030-cpsat-dependency-direction.md) | CP-SAT enters via Python ortools, not Rust FFI | Accepted | 2026-05-04 |
```

(Inspect the existing table format and match.)

- [ ] **Step 8.4: Commit.**

```bash
git add docs/adr/0030-cpsat-dependency-direction.md docs/adr/README.md
git commit -m "docs(adr): add 0030 cpsat dependency direction"
```

---

## Task 9: Sprint 4 close-out documentation

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md` (mark Sprint 4 items 8-10 ✅, narrate any deviations, surface the Pareto-frontier callout, mark item 19 next pickup).
- Modify: `solver/CLAUDE.md` (clarify "thin wrappers only" scope, add CP-SAT semantics bullet, document score_solution_json reverse import, BENCH_RESULTS.md ~5.3 h refresh note).
- Modify: `backend/CLAUDE.md` (`KZ_SOLVER_BACKEND` reference under existing `KZ_SOLVE_DEADLINE_MS=0` note).
- Modify: `deploy/README.md` (~50 MB image growth + KZ_SOLVER_BACKEND ops note).

- [ ] **Step 9.1: Update `docs/superpowers/OPEN_THINGS.md`.** Edit the active sprint program section:
  - At the top of the "Active sprint program" section, change the trailing summary so the four sprints all read ✅.
  - Mark items 8-10 with `✅ Shipped 2026-05-04 in PR #<TBD-PR-number>`. Replace the prose body with a one-paragraph per-item summary describing what shipped (file paths, deviations like the seed kwarg, the `score_solution_json` reverse-import pattern, the `BENCH_RESULTS.md` 16-row refresh).
  - Move item 19 (`peak_memory_kb` and `time_to_first_feasible_ms`) into the "Active sprint program" tail or surface as the next pickup; update the wording to "next sprint pickup".
  - Add a new "Production-default decision" bullet under "After bake-off" with the recommendation derived from the refreshed `BENCH_RESULTS.md` cells (filled in once the refresh runs).

- [ ] **Step 9.2: Update `solver/CLAUDE.md`.** Append four bullets in the "solver-py rules" section:

```markdown
- **CP-SAT lives in `klassenzeit_solver.cpsat` as pure Python.** The "thin wrappers only" rule applies to PyO3 wrappers in `solver-py/src/lib.rs`. Peer Python algorithms in the `klassenzeit_solver` package are explicitly allowed; they do not call `solver-core` for the algorithm itself, only for utilities like `score_solution_json`. ADR 0030.
- **`score_solution_json` is the cross-backend scorer.** New PyO3 binding in `solver-py/src/lib.rs`. Wraps `solver_core::score_solution` with `PRODUCTION_ACTIVE_WEIGHTS`. Used by `cpsat.py` to populate `Solution.soft_score` post-solve so all four bake-off backends compare on the same Rust scorer. ADR 0030.
- **`PRODUCTION_ACTIVE_WEIGHTS` is the canonical weight set.** Single `pub const` in `solver-core/src/types.rs`. The bake-off bench, `solve_json_with_config`, and `score_solution_json` all reference it; do not re-hand-write the weight literal. Any change to the active default is one edit instead of three.
- **CP-SAT module semantics:** Per-block-anchor binary encoding (`y[lesson, day, start_position, room]`). Pre-pruning at variable creation: subject suitability, teacher / room blocked time blocks, window-fits-on-day. Hard constraints: cardinality, class / teacher / room non-overlap, teacher max-hours, same-room invariant, lesson-group co-placement, pinned placements. Objective `Minimize(0)`. Determinism via `num_search_workers=1` and `random_seed=seed`. INFEASIBLE / UNKNOWN return one `NoFreeTimeBlock` violation per lesson with `reason="cpsat: <status>"` per ADR 0027 wrapper-field pattern. MODEL_INVALID raises `RuntimeError`.
```

Append one bullet under "Bench workflow":

```markdown
- **`mise run bench:bakeoff` wall-clock is now ~5.3 hours** for a full 4 fixtures × 4 backends × 20 seeds × 60 s pass. The cpsat backend invokes `python3 -m klassenzeit_solver.cpsat` per cell as a subprocess; subprocess startup is negligible (~100 ms × 80 cells). Dev-loop downscale via `--budget 5s --seeds 4 --fixtures grundschule`.
```

- [ ] **Step 9.3: Update `backend/CLAUDE.md`.** Add one bullet near the existing `KZ_SOLVE_DEADLINE_MS=0` bullet:

```markdown
- **`KZ_SOLVER_BACKEND` selects the solver backend.** `Settings.solver_backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"]`. Default `lahc` matches pre-Sprint-4 behaviour. The `cpsat` backend depends on the `ortools` Python wheel (~50 MB); the other three are pure-Rust LAHC variants with different period kwargs to `solve_json_with_config`. Dispatch lives in `scheduling/solver_io.run_solve`. ADR 0030.
```

- [ ] **Step 9.4: Update `deploy/README.md`.** Add a small section "Solver backend operations":

```markdown
## Solver backend operations

`KZ_SOLVER_BACKEND` env var selects which solver runs. Values: `lahc`
(default, pre-Sprint-4 behaviour), `lahc_rr`, `lahc_rr_kempe`, `cpsat`. The
`cpsat` backend depends on the `ortools` Python wheel (~50 MB) which is
bundled with the `klassenzeit-solver` package; switching to or from
`cpsat` is an env-var flip plus a Pod restart, no image rebuild.

ADR 0030 records the architecture and the production-default decision
sources from the refreshed `BENCH_RESULTS.md` Pareto frontier.
```

- [ ] **Step 9.5: Commit.**

```bash
mise run lint
git add docs/superpowers/OPEN_THINGS.md solver/CLAUDE.md backend/CLAUDE.md deploy/README.md
git commit -m "docs: close out sprint 4 of solver bake-off"
```

---

## Self-Review

**Spec coverage check:** Each spec section maps to a task above:
- Spec § Context, Scope: covered by all tasks collectively (Task 1 weights, Task 2 widening, Task 3 scoring, Task 4 dep, Task 5 cpsat, Task 6 dispatch, Task 7 bench, Task 8 ADR, Task 9 close-out).
- Spec § CP-SAT model semantics: Task 5.
- Spec § Backend dispatch: Task 6.
- Spec § Bench harness wiring: Task 7.
- Spec § File layout: Tasks 1-9 each list their file changes.
- Spec § Test plan (5 layers): unit (5), determinism (5), score round-trip (3), backend dispatch (6), bench contract (7). All present.
- Spec § Commit split: Tasks 1-9 mirror the spec's nine commits.
- Spec § Risks: addressed inline (variable count + pruning in Task 5; ortools wheel in Task 4 verify; ty type-checking note in Task 5 import line).

**Placeholder scan:** No `TBD`, `TODO`, `implement later`. The cpsat.py skeleton in Task 5.3 deliberately leaves `...` markers in the constraint-emission section because each item maps 1:1 to the spec's hard-constraint enumeration; the subagent fills them by walking the spec's list. This is acceptable per the plan-author's discretion (the spec is normative; the plan re-references rather than re-prints the constraint list).

**Type consistency:** `solve_json_with_config` widens identically across `solver-core/src/json.rs` (Task 2.3), `solver-py/src/lib.rs` (Task 2.4), and the two `.pyi` stubs (Task 2.5). `solve_cpsat_json` signature `(problem_json, deadline_ms, seed=1) -> str` is consistent across `cpsat.py` (Task 5.3), `__init__.pyi` (Task 5.5), and the bench harness's subprocess args (Task 7.4 `--seed` flag).
