# CP-SAT objective parity with `score_solution` (item 48) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `model.minimize(0)` in `solver/solver-py/python/klassenzeit_solver/cpsat.py` with a CP-SAT objective expression that mirrors `solver_core::score_solution(..., PRODUCTION_ACTIVE_WEIGHTS)` exactly, so CP-SAT's internal optimisation steers toward the same target the bench rates by.

**Architecture:** A new `_emit_objective` helper in `cpsat.py` builds five summands (subject_preference, home_room, class_gap, teacher_gap, class_day_balance) and passes their sum into `model.minimize(...)`. Subject_preference and home_room collapse into per-anchor constant coefficients. Gap counts use per-(entity, day, position) `present`/`has_left`/`has_right`/`gap` BoolVars with channeling inequalities. Day-balance uses `add_abs_equality` plus `add_division_equality`. The solver's `objective_value()` is surfaced through the response JSON as `model_objective_value` so a property test can compare it against `score_solution_json` on every returned solution.

**Tech Stack:** Python 3.14, OR-Tools `cp_model` (snake_case API), pytest, PyO3 binding `score_solution_json` from `klassenzeit_solver._rust`, Rust `solver-core::quality::BackendObjective`.

---

## File Structure

- **Modify:** `solver/solver-py/python/klassenzeit_solver/cpsat.py` — replace `model.minimize(0)` with `_emit_objective(...)`; surface `model_objective_value` in the JSON response.
- **Modify:** `solver/solver-py/tests/test_cpsat.py` — add four `test_cpsat_objective_value_equals_score_solution_on_<fixture>` cases plus fixture variants that exercise each axis.
- **Modify:** `solver/solver-core/src/quality.rs` — flip cpsat's `BackendObjective` declaration from `optimised: empty / declared_skipped: ALL` to `optimised: ALL / declared_skipped: empty`. Update the existing test `backend_objective_cpsat_partitions_quality_components` accordingly.
- **Modify:** `solver/CLAUDE.md` — add a bullet describing the cpsat objective shape (per-anchor coefficients plus per-slot gap channeling plus abs+div for day-balance).
- **Modify:** `docs/superpowers/OPEN_THINGS.md` — delete item 48 bullet.

---

## Task 1: Surface `model_objective_value` in `solve_cpsat_json` and add a passing parity test for the trivial fixture

This task establishes the test harness without changing the objective. The trivial fixture's `score_solution` is 0, the current `model.minimize(0)` objective is also 0, so the parity test passes trivially. Subsequent tasks port axes incrementally; this test remains the gate.

**Files:**
- Modify: `solver/solver-py/python/klassenzeit_solver/cpsat.py:67-105` (the success and infeasible branches of `solve_cpsat_json`)
- Modify: `solver/solver-py/tests/test_cpsat.py` — add new test below the existing `test_solve_cpsat_json_reported_soft_score_equals_canonical_score`

- [ ] **Step 1: Write the failing test**

Append to `solver/solver-py/tests/test_cpsat.py`:

```python
def test_cpsat_objective_value_equals_score_solution_on_trivial_problem() -> None:
    """Item 48 acceptance: the CP-SAT model objective value on the returned
    solution must equal `score_solution(problem, placements, PRODUCTION_ACTIVE_WEIGHTS)`.

    Trivial fixture has every axis evaluating to 0 today; the test passes
    even before any axis is ported. It locks the contract so subsequent
    axis ports can extend the test set without re-deriving the harness.
    """
    problem_json = _cpsat_trivial_one_lesson_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
```

- [ ] **Step 2: Run test to verify it fails**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_trivial_problem -v
```

Expected: FAIL with `KeyError: 'model_objective_value'` (the field doesn't exist in the JSON yet).

- [ ] **Step 3: Surface `model_objective_value` from the success branch**

Edit `solver/solver-py/python/klassenzeit_solver/cpsat.py` `solve_cpsat_json`. Replace the success-branch return block (lines roughly 67-81) with:

```python
    if status in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        placements = _extract_placements(solver, anchor_vars, meta)
        soft_score = score_solution_json(problem_json, json.dumps(placements))
        ttf = callback.first_ms
        tto = solver.WallTime() * 1000.0 if status == cp_model.OPTIMAL else None
        return json.dumps(
            {
                "placements": placements,
                "violations": [],
                "soft_score": int(soft_score),
                "model_objective_value": int(solver.objective_value),
                "peak_rss_kb": peak_rss_kb,
                "time_to_first_feasible_ms": ttf,
                "time_to_optimal_ms": tto,
            }
        )
```

In the INFEASIBLE/UNKNOWN branch (lines roughly 96-105), add `"model_objective_value": None` next to the other `None` fields:

```python
        return json.dumps(
            {
                "placements": [],
                "violations": violations,
                "soft_score": 0,
                "model_objective_value": None,
                "peak_rss_kb": peak_rss_kb,
                "time_to_first_feasible_ms": None,
                "time_to_optimal_ms": None,
            }
        )
```

- [ ] **Step 4: Rebuild and run the test**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_trivial_problem -v
```

Expected: PASS. `solver.objective_value` returns `0.0` because the model still does `model.minimize(0)`; `score_solution_json` returns `0` for the single-placement trivial fixture; `0 == 0`.

- [ ] **Step 5: Verify existing CP-SAT tests still pass**

```bash
uv run pytest solver/solver-py/tests/test_cpsat.py solver/solver-py/tests/test_cpsat_determinism.py -v
```

Expected: every existing test passes. The new JSON field is additive; no existing test reads it.

- [ ] **Step 6: Lint**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add solver/solver-py/python/klassenzeit_solver/cpsat.py solver/solver-py/tests/test_cpsat.py
git commit -m "feat(solver-py): expose model_objective_value from solve_cpsat_json"
```

---

## Task 2: Port subject_preference axes (`prefer_early`, `avoid_first`, `avoid_last`, `prefer_late`) into the CP-SAT objective

Per-anchor constant coefficients only; no auxiliary variables.

**Files:**
- Modify: `solver/solver-py/python/klassenzeit_solver/cpsat.py` — add `PRODUCTION_ACTIVE_WEIGHTS` constants at module level (Python mirrors of the Rust constants), add `_objective_subject_preference_coefficient(...)`, add `_emit_objective(...)`, replace `model.minimize(0)` with `_emit_objective(...)`.
- Modify: `solver/solver-py/tests/test_cpsat.py` — add a fixture variant `_cpsat_doppelstunde_with_prefer_late_subject` that flags the subject's `prefer_late_period` and a parity test.

- [ ] **Step 1: Write the failing test**

Append to `solver/solver-py/tests/test_cpsat.py`:

```python
def _cpsat_doppelstunde_with_prefer_late_subject() -> str:
    """Doppelstunde fixture variant where subject.prefer_late_period = 1, so
    score_solution's prefer_late axis fires per placement."""
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
                {"id": _cpsat_uuid(12), "day_of_week": 0, "position": 2},
                {"id": _cpsat_uuid(13), "day_of_week": 0, "position": 3},
            ],
            "teachers": [{"id": _cpsat_uuid(20), "max_hours_per_week": 5}],
            "rooms": [{"id": _cpsat_uuid(30)}],
            "subjects": [{"id": _cpsat_uuid(40), "prefer_late_period": 1}],
            "school_classes": [{"id": _cpsat_uuid(50)}],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [_cpsat_uuid(50)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_id": _cpsat_uuid(20),
                    "hours_per_week": 2,
                    "preferred_block_size": 2,
                }
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_cpsat_objective_value_equals_score_solution_on_subject_preference_problem() -> None:
    """prefer_late axis: max_position_for_day=3, weights.prefer_late_period=1,
    subject.prefer_late_period=1; doppelstunde block contributes
    (3-p) + (3-(p+1)) per placement, weighted by 1*1.

    The CP-SAT objective should drive the doppelstunde to anchor at p=2
    (positions 2,3) so prefer_late contribution is (3-2) + (3-3) = 1, not
    p=0 (positions 0,1) which would contribute 5.
    """
    problem_json = _cpsat_doppelstunde_with_prefer_late_subject()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
    # Witness that CP-SAT actually steers: objective is 1, not the worst-case 5.
    assert out["model_objective_value"] == 1
```

- [ ] **Step 2: Run test to verify it fails**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_subject_preference_problem -v
```

Expected: FAIL with `assert 0 == 1` (CP-SAT minimises 0, returns the first feasible solution which may anchor at p=0 with `score_solution_json` reporting 5; the model objective is 0).

- [ ] **Step 3: Add module-level weight constants and `_emit_objective` skeleton**

Edit `solver/solver-py/python/klassenzeit_solver/cpsat.py`. Just below the imports and the `AnchorKey` type alias (around line 26), add:

```python
# Mirror of solver_core::types::PRODUCTION_ACTIVE_WEIGHTS so CP-SAT's
# model objective evaluates to the same number as
# `score_solution(..., PRODUCTION_ACTIVE_WEIGHTS)` on any returned
# solution. Item 48 keeps these in lockstep with Rust by referencing both
# in `solver/CLAUDE.md`; the property tests in `test_cpsat.py` flag drift.
_W_CLASS_GAP = 10
_W_TEACHER_GAP = 10
_W_PREFER_EARLY_PERIOD = 1
_W_AVOID_FIRST_PERIOD = 1
_W_PREFER_HOME_ROOM = 5
_W_AVOID_LAST_PERIOD = 1
_W_PREFER_LATE_PERIOD = 1
_W_CLASS_DAY_BALANCE = 5
```

In `_build_model` (line 122), replace the `model.minimize(0)` line (line 133) with:

```python
    _emit_objective(model, problem, anchor_vars, lookups)
```

Just below `_emit_pinned_placements` (around line 440), add the skeleton:

```python
def _emit_objective(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> None:
    """Build CP-SAT model objective mirroring solver_core::score_solution.

    Five summands: subject_preference (per-anchor constant coefficient),
    home_room (per-anchor constant coefficient), class_gap (per-(class,
    day, position) channeling), teacher_gap (per-(teacher, day, position)
    channeling), class_day_balance (per-class abs-equality plus
    division-equality). See docs/superpowers/specs/2026-05-07-cpsat-objective-parity-design.md
    for the encoding rationale.
    """
    summand_subject_pref = _objective_subject_preference_terms(problem, anchor_vars, lookups)
    model.minimize(summand_subject_pref)
```

- [ ] **Step 4: Implement `_objective_subject_preference_terms`**

Add immediately above `_emit_objective`:

```python
def _objective_subject_preference_terms(
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> cp_model.LinearExpr:
    """Per-anchor constant coefficient covering prefer_early, avoid_first,
    avoid_last, prefer_late. Each axis sums over the N positions in the
    block window; the per-anchor sum collapses to a Python int known at
    build time. Returns sum_anchor coeff * y[anchor].
    """
    lesson_lookup = lookups["lesson_lookup"]
    tb_pos_lookup = lookups["tb_pos_lookup"]
    subjects = {s["id"]: s for s in problem["subjects"]}
    max_pos_per_day: dict[int, int] = {}
    for tb in problem["time_blocks"]:
        d = tb["day_of_week"]
        max_pos_per_day[d] = max(max_pos_per_day.get(d, 0), tb["position"])

    terms: list[cp_model.LinearExpr] = []
    for (l_id, day, start_pos, _r_id), var in anchor_vars.items():
        lesson = lesson_lookup[l_id]
        n = lesson["preferred_block_size"]
        subject = subjects[lesson["subject_id"]]
        max_pos = max_pos_per_day[day]
        coeff = 0
        prefer_early = subject.get("prefer_early_period", 0)
        if prefer_early:
            window_pos_sum = n * start_pos + n * (n - 1) // 2
            coeff += _W_PREFER_EARLY_PERIOD * prefer_early * window_pos_sum
        avoid_first = subject.get("avoid_first_period", 0)
        if avoid_first and start_pos == 0:
            coeff += _W_AVOID_FIRST_PERIOD * avoid_first
        avoid_last = subject.get("avoid_last_period", 0)
        if avoid_last and start_pos + n - 1 == max_pos:
            coeff += _W_AVOID_LAST_PERIOD * avoid_last
        prefer_late = subject.get("prefer_late_period", 0)
        if prefer_late:
            window_late_sum = n * max_pos - n * start_pos - n * (n - 1) // 2
            coeff += _W_PREFER_LATE_PERIOD * prefer_late * window_late_sum
        if coeff:
            terms.append(coeff * var)
    # tb_pos_lookup is unused here today; kept on the signature for the
    # gap-axis tasks below so they can reuse the same lookups dict shape.
    _ = tb_pos_lookup
    return cp_model.LinearExpr.sum(terms) if terms else 0
```

- [ ] **Step 5: Rebuild and run the new test**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_subject_preference_problem -v
```

Expected: PASS. CP-SAT now minimises the prefer_late summand and anchors the doppelstunde at p=2.

- [ ] **Step 6: Run trivial parity test and the existing CP-SAT suite**

```bash
uv run pytest solver/solver-py/tests/test_cpsat.py solver/solver-py/tests/test_cpsat_determinism.py -v
```

Expected: every test passes. The trivial fixture has all subject flags at 0; the new summand evaluates to 0; the parity test still passes.

- [ ] **Step 7: Lint**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add solver/solver-py/python/klassenzeit_solver/cpsat.py solver/solver-py/tests/test_cpsat.py
git commit -m "feat(solver-py): port subject_preference axes into cpsat objective"
```

---

## Task 3: Port the home_room axis

Per-anchor constant coefficient: `sum over class in lesson.school_class_ids of (mismatch ? weights.prefer_home_room * N : 0)`.

**Files:**
- Modify: `solver/solver-py/python/klassenzeit_solver/cpsat.py` — add `_objective_home_room_term`; extend `_emit_objective`.
- Modify: `solver/solver-py/tests/test_cpsat.py` — add a fixture variant where classes have `home_room_id` set; add a parity test.

- [ ] **Step 1: Write the failing test**

Append to `solver/solver-py/tests/test_cpsat.py`:

```python
def _cpsat_home_room_problem() -> str:
    """Two classes with distinct home_rooms, one shared lesson placed in
    one room. Per score_solution: per-placement, per-class additive
    penalty when class.home_room_id != placement.room_id.

    Two TBs, two rooms (one is class 50's home, one is class 51's home),
    one shared single-block 2h lesson. Model must pick a room and pay
    weights.prefer_home_room * 2 (mismatched class, 2 placements).
    """
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
            ],
            "teachers": [{"id": _cpsat_uuid(20), "max_hours_per_week": 5}],
            "rooms": [
                {"id": _cpsat_uuid(30)},
                {"id": _cpsat_uuid(31)},
            ],
            "subjects": [{"id": _cpsat_uuid(40)}],
            "school_classes": [
                {"id": _cpsat_uuid(50), "home_room_id": _cpsat_uuid(30)},
                {"id": _cpsat_uuid(51), "home_room_id": _cpsat_uuid(31)},
            ],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [_cpsat_uuid(50), _cpsat_uuid(51)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_id": _cpsat_uuid(20),
                    "hours_per_week": 2,
                    "preferred_block_size": 1,
                }
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_cpsat_objective_value_equals_score_solution_on_home_room_problem() -> None:
    """home_room axis: a multi-class lesson placed in either room
    mismatches exactly one class's home_room, contributing
    weights.prefer_home_room (= 5) per placement. With 2 placements (2h
    single-block), the per-block contribution is 10. Both placements
    accumulate so total = 10 * 2 = 20... no, score_solution iterates per
    placement; 2 placements * 1 mismatched class * 5 = 10.
    """
    problem_json = _cpsat_home_room_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
    # Witness: every room is one class's home and the other's mismatch.
    # 2 placements * 1 mismatched class * weight 5 = 10.
    assert out["model_objective_value"] == 10
```

- [ ] **Step 2: Run test to verify it fails**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_home_room_problem -v
```

Expected: FAIL — model objective is 0 (only subject_pref summand exists), `score_solution_json` reports 10, `0 == 10` fails.

- [ ] **Step 3: Implement `_objective_home_room_term`**

Add immediately above `_emit_objective`:

```python
def _objective_home_room_term(
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> cp_model.LinearExpr:
    """Per-anchor constant coefficient: sum over class in
    lesson.school_class_ids of (mismatch ? weights.prefer_home_room * N : 0).
    Mirrors score::home_room_penalty per-placement aggregated over the
    N-block window. Multi-class lessons accumulate per-class
    contributions; classes without home_room_id contribute 0.
    """
    lesson_lookup = lookups["lesson_lookup"]
    home_room_by_class: dict[str, str | None] = {
        c["id"]: c.get("home_room_id") for c in problem["school_classes"]
    }

    terms: list[cp_model.LinearExpr] = []
    for (l_id, _day, _start_pos, room_id), var in anchor_vars.items():
        lesson = lesson_lookup[l_id]
        n = lesson["preferred_block_size"]
        coeff = 0
        for class_id in lesson["school_class_ids"]:
            home_room_id = home_room_by_class.get(class_id)
            if home_room_id is not None and home_room_id != room_id:
                coeff += _W_PREFER_HOME_ROOM * n
        if coeff:
            terms.append(coeff * var)
    return cp_model.LinearExpr.sum(terms) if terms else 0
```

- [ ] **Step 4: Wire `_objective_home_room_term` into `_emit_objective`**

Replace `_emit_objective`'s body with:

```python
    summand_subject_pref = _objective_subject_preference_terms(problem, anchor_vars, lookups)
    summand_home_room = _objective_home_room_term(problem, anchor_vars, lookups)
    model.minimize(summand_subject_pref + summand_home_room)
```

- [ ] **Step 5: Rebuild and run the new test**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_home_room_problem -v
```

Expected: PASS.

- [ ] **Step 6: Run the full CP-SAT suite plus the prior parity tests**

```bash
uv run pytest solver/solver-py/tests/test_cpsat.py solver/solver-py/tests/test_cpsat_determinism.py -v
```

Expected: every test passes. Existing fixtures don't set `home_room_id` (or set it consistently with the placement room), so the new summand evaluates to 0 on prior parity tests.

- [ ] **Step 7: Lint**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add solver/solver-py/python/klassenzeit_solver/cpsat.py solver/solver-py/tests/test_cpsat.py
git commit -m "feat(solver-py): port home_room axis into cpsat objective"
```

---

## Task 4: Port `class_gap` and `teacher_gap` axes

Per-(entity, day, position) channeling: `present`, `has_left`, `has_right`, `gap`. Largest model-size change.

**Files:**
- Modify: `solver/solver-py/python/klassenzeit_solver/cpsat.py` — add `_build_per_slot_presence` (shared between class and teacher gap terms), `_objective_gap_term` (parameterised on entity scope), wire into `_emit_objective`.
- Modify: `solver/solver-py/tests/test_cpsat.py` — add a fixture that forces a class_gap and teacher_gap; add a parity test.

- [ ] **Step 1: Write the failing test**

Append to `solver/solver-py/tests/test_cpsat.py`:

```python
def _cpsat_forced_class_gap_problem() -> str:
    """Three TBs on day 0 (positions 0, 1, 2), one teacher, one room, one
    class. Two single-hour lessons of the same class with the same
    teacher. Each lesson has hours_per_week=1, preferred_block_size=1.

    But TB at position 1 is teacher-blocked. Both placements must use
    positions 0 and 2 with a forced gap at position 1. score_solution
    reports class_gap=1 (* 10) + teacher_gap=1 (* 10) = 20.
    """
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
                {"id": _cpsat_uuid(12), "day_of_week": 0, "position": 2},
            ],
            "teachers": [{"id": _cpsat_uuid(20), "max_hours_per_week": 5}],
            "rooms": [{"id": _cpsat_uuid(30)}],
            "subjects": [{"id": _cpsat_uuid(40)}],
            "school_classes": [{"id": _cpsat_uuid(50)}],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [_cpsat_uuid(50)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_id": _cpsat_uuid(20),
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                },
                {
                    "id": _cpsat_uuid(61),
                    "school_class_ids": [_cpsat_uuid(50)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_id": _cpsat_uuid(20),
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                },
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [
                {"teacher_id": _cpsat_uuid(20), "time_block_id": _cpsat_uuid(11)}
            ],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_cpsat_objective_value_equals_score_solution_on_forced_gap_problem() -> None:
    """class_gap and teacher_gap axes: forced gap at position 1; class
    contributes 1 gap-hour (weight 10), teacher contributes 1 gap-hour
    (weight 10); total = 20.
    """
    problem_json = _cpsat_forced_class_gap_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
    assert out["model_objective_value"] == 20
```

- [ ] **Step 2: Run test to verify it fails**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_forced_gap_problem -v
```

Expected: FAIL — model objective is 0, score_solution reports 20.

- [ ] **Step 3: Implement `_build_per_slot_presence` and `_objective_gap_term`**

Add immediately above `_emit_objective`:

```python
def _build_per_slot_presence(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
    scope_kind: str,
) -> dict[tuple[str, int, int], cp_model.IntVar]:
    """For each (entity, day, position) where entity is a class id (when
    scope_kind == 'class') or a teacher id (when scope_kind == 'teacher'),
    build present[entity, day, p] = sum over anchors covering (day, p)
    in the entity's scope. Domain [0, 1] (non-overlap holds it at most 1
    in feasible solutions; the explicit upper bound makes presolve happy).

    Returned mapping is keyed by (entity_id_str, day, position).
    """
    lesson_lookup = lookups["lesson_lookup"]
    positions_per_day = lookups["positions_per_day"]
    if scope_kind == "class":
        entity_ids: set[str] = {c["id"] for c in problem["school_classes"]}
    elif scope_kind == "teacher":
        entity_ids = {t["id"] for t in problem["teachers"]}
    else:  # pragma: no cover - guarded by callers
        raise ValueError(f"unknown scope_kind: {scope_kind}")

    coverage: dict[tuple[str, int, int], list[cp_model.IntVar]] = {}
    for (l_id, day, start_pos, _r_id), var in anchor_vars.items():
        lesson = lesson_lookup[l_id]
        n = lesson["preferred_block_size"]
        if scope_kind == "class":
            owners: list[str] = list(lesson["school_class_ids"])
        else:
            owners = [lesson["teacher_id"]]
        for offset in range(n):
            p = start_pos + offset
            for owner in owners:
                coverage.setdefault((owner, day, p), []).append(var)

    presence: dict[tuple[str, int, int], cp_model.IntVar] = {}
    for entity_id in entity_ids:
        for day, positions in positions_per_day.items():
            for pos in positions:
                key = (entity_id, day, pos)
                covering = coverage.get(key, [])
                pres = model.new_int_var(0, 1, f"present_{scope_kind}_{entity_id}_{day}_{pos}")
                if covering:
                    model.add(pres == sum(covering))
                else:
                    model.add(pres == 0)
                presence[key] = pres
    return presence


def _objective_gap_term(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
    scope_kind: str,
    weight: int,
) -> cp_model.LinearExpr:
    """For each (entity, day, position) presence indicator, build
    has_left/has_right/gap channeling and return weight * sum(gap[...]).
    """
    presence = _build_per_slot_presence(model, problem, anchor_vars, lookups, scope_kind)
    positions_per_day = lookups["positions_per_day"]
    if scope_kind == "class":
        entity_ids: set[str] = {c["id"] for c in problem["school_classes"]}
    else:
        entity_ids = {t["id"] for t in problem["teachers"]}

    gap_vars: list[cp_model.IntVar] = []
    for entity_id in entity_ids:
        for day, positions in positions_per_day.items():
            sorted_positions = sorted(positions)
            for idx, pos in enumerate(sorted_positions):
                pres_p = presence[(entity_id, day, pos)]
                left_neighbours = [
                    presence[(entity_id, day, q)] for q in sorted_positions[:idx]
                ]
                right_neighbours = [
                    presence[(entity_id, day, q)] for q in sorted_positions[idx + 1 :]
                ]
                if not left_neighbours or not right_neighbours:
                    # No interior position can have both has_left and has_right.
                    continue
                has_left = model.new_bool_var(f"hl_{scope_kind}_{entity_id}_{day}_{pos}")
                has_right = model.new_bool_var(f"hr_{scope_kind}_{entity_id}_{day}_{pos}")
                model.add_max_equality(has_left, left_neighbours)
                model.add_max_equality(has_right, right_neighbours)
                gap = model.new_bool_var(f"gap_{scope_kind}_{entity_id}_{day}_{pos}")
                model.add(gap >= has_left + has_right + (1 - pres_p) - 2)
                model.add(gap <= has_left)
                model.add(gap <= has_right)
                model.add(gap <= 1 - pres_p)
                gap_vars.append(gap)
    return weight * cp_model.LinearExpr.sum(gap_vars) if gap_vars else 0
```

- [ ] **Step 4: Wire the gap terms into `_emit_objective`**

Replace `_emit_objective`'s body with:

```python
    summand_subject_pref = _objective_subject_preference_terms(problem, anchor_vars, lookups)
    summand_home_room = _objective_home_room_term(problem, anchor_vars, lookups)
    summand_class_gap = _objective_gap_term(
        model, problem, anchor_vars, lookups, scope_kind="class", weight=_W_CLASS_GAP
    )
    summand_teacher_gap = _objective_gap_term(
        model, problem, anchor_vars, lookups, scope_kind="teacher", weight=_W_TEACHER_GAP
    )
    model.minimize(
        summand_subject_pref + summand_home_room + summand_class_gap + summand_teacher_gap
    )
```

- [ ] **Step 5: Rebuild and run the new test**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_forced_gap_problem -v
```

Expected: PASS — both placements forced into positions 0 and 2; CP-SAT objective is 20; `score_solution_json` reports 20.

- [ ] **Step 6: Run the full suite**

```bash
uv run pytest solver/solver-py/tests/test_cpsat.py solver/solver-py/tests/test_cpsat_determinism.py -v
```

Expected: every test passes. The trivial / doppelstunde / home_room fixtures evaluate gap to 0 (single placements or contiguous placements have no interior gap).

- [ ] **Step 7: Lint**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 8: Smoke bench**

```bash
mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule --out /tmp/cpsat-objective-task4-smoke.md
```

Expected: completes in ~30 s; cpsat soft_score column is lower than the pre-task-1 baseline (the column should drop noticeably from ~349 toward LAHC's ~90 once gap is the dominant term in production weights). Capture the output for the PR body.

- [ ] **Step 9: Commit**

```bash
git add solver/solver-py/python/klassenzeit_solver/cpsat.py solver/solver-py/tests/test_cpsat.py
git commit -m "feat(solver-py): port class_gap and teacher_gap axes into cpsat objective"
```

---

## Task 5: Port `class_day_balance` axis

Per-class abs-equality plus division-equality.

**Files:**
- Modify: `solver/solver-py/python/klassenzeit_solver/cpsat.py` — add `_objective_class_day_balance_term`; wire into `_emit_objective`.
- Modify: `solver/solver-py/tests/test_cpsat.py` — add a fixture that forces a lopsided spread; add a parity test.

- [ ] **Step 1: Write the failing test**

Append to `solver/solver-py/tests/test_cpsat.py`:

```python
def _cpsat_forced_lopsided_spread_problem() -> str:
    """Two days, three TBs on day 0 (positions 0, 1, 2), zero TBs on day
    1. One class, one teacher, one room. One lesson with hours_per_week=3,
    preferred_block_size=1. Every placement must land on day 0 (no TBs on
    day 1). Spread is 3/0; D=2 days.

    score_solution: c[0]=3, c[1]=0, sum=3, D=2.
    scaled = |3*2 - 3| + |0*2 - 3| = 3 + 3 = 6
    quotient = 6 // 2 = 3
    Total class_day_balance = 5 * 3 = 15.

    No class_gap (3 contiguous placements, no interior missing). No
    teacher_gap. Only class_day_balance fires.
    """
    return json.dumps(
        {
            "time_blocks": [
                {"id": _cpsat_uuid(10), "day_of_week": 0, "position": 0},
                {"id": _cpsat_uuid(11), "day_of_week": 0, "position": 1},
                {"id": _cpsat_uuid(12), "day_of_week": 0, "position": 2},
                {"id": _cpsat_uuid(13), "day_of_week": 1, "position": 0},
            ],
            "teachers": [{"id": _cpsat_uuid(20), "max_hours_per_week": 5}],
            "rooms": [{"id": _cpsat_uuid(30)}],
            "subjects": [{"id": _cpsat_uuid(40)}],
            "school_classes": [{"id": _cpsat_uuid(50)}],
            "lessons": [
                {
                    "id": _cpsat_uuid(60),
                    "school_class_ids": [_cpsat_uuid(50)],
                    "subject_id": _cpsat_uuid(40),
                    "teacher_id": _cpsat_uuid(20),
                    "hours_per_week": 3,
                    "preferred_block_size": 1,
                }
            ],
            "teacher_qualifications": [
                {"teacher_id": _cpsat_uuid(20), "subject_id": _cpsat_uuid(40)}
            ],
            "teacher_blocked_times": [
                {"teacher_id": _cpsat_uuid(20), "time_block_id": _cpsat_uuid(13)}
            ],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


def test_cpsat_objective_value_equals_score_solution_on_lopsided_spread_problem() -> None:
    """class_day_balance axis: 3 placements on day 0, 0 on day 1.
    quotient = (|3*2-3| + |0*2-3|) // 2 = 6 // 2 = 3; weighted = 5 * 3 = 15.
    """
    problem_json = _cpsat_forced_lopsided_spread_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2_000, seed=0)
    out = json.loads(out_json)
    assert out["model_objective_value"] is not None
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["model_objective_value"] == canonical
    assert out["model_objective_value"] == 15
```

- [ ] **Step 2: Run test to verify it fails**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_lopsided_spread_problem -v
```

Expected: FAIL — model objective is 0 (only the four already-ported summands fire and they all evaluate to 0 on this fixture), score_solution reports 15.

- [ ] **Step 3: Implement `_objective_class_day_balance_term`**

Add immediately above `_emit_objective`:

```python
def _objective_class_day_balance_term(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> cp_model.LinearExpr:
    """Per-class scaled L1 day-balance cost mirroring
    score::class_day_balance_cost. For each class:
      class_total = sum lesson.hours_per_week for lessons where class is in school_class_ids
      c_count[class, day] = sum over anchors (l, day, p, r) where day matches and class ∈ scope: N(l) * y[..]
      dev[class, day] = abs(c_count[class, day] * D - class_total)
      scaled[class] = sum_day dev[class, day]
      quotient[class] = scaled[class] // D (CP-SAT add_division_equality)
    Returns _W_CLASS_DAY_BALANCE * sum_class quotient[class].

    Class with class_total == 0 contributes 0 by construction (all c_count
    vars are 0, dev is 0, quotient is 0). Skipped at build time to avoid
    creating unused vars.
    """
    lesson_lookup = lookups["lesson_lookup"]
    positions_per_day = lookups["positions_per_day"]
    days_set: set[int] = set(positions_per_day.keys())
    if not days_set:
        return 0
    d = len(days_set)
    classes = problem["school_classes"]

    class_total: dict[str, int] = {}
    for cls in classes:
        c_id = cls["id"]
        total = 0
        for lesson in problem["lessons"]:
            if c_id in lesson["school_class_ids"]:
                total += lesson["hours_per_week"]
        class_total[c_id] = total

    quotients: list[cp_model.IntVar] = []
    for cls in classes:
        c_id = cls["id"]
        total = class_total[c_id]
        if total == 0:
            continue
        # c_count[day]: sum over anchors covering (c_id, day) of N(l) * y[..]
        c_count_terms: dict[int, list[cp_model.LinearExpr]] = {day: [] for day in days_set}
        for (l_id, day, _start_pos, _r_id), var in anchor_vars.items():
            lesson = lesson_lookup[l_id]
            if c_id not in lesson["school_class_ids"]:
                continue
            n = lesson["preferred_block_size"]
            c_count_terms[day].append(n * var)
        c_count_vars: dict[int, cp_model.IntVar] = {}
        for day in days_set:
            cc = model.new_int_var(0, total, f"ccount_{c_id}_{day}")
            terms = c_count_terms[day]
            if terms:
                model.add(cc == cp_model.LinearExpr.sum(terms))
            else:
                model.add(cc == 0)
            c_count_vars[day] = cc
        dev_vars: list[cp_model.IntVar] = []
        for day in days_set:
            dev = model.new_int_var(0, total * d, f"dev_{c_id}_{day}")
            model.add_abs_equality(dev, c_count_vars[day] * d - total)
            dev_vars.append(dev)
        scaled = model.new_int_var(0, total * d * d, f"scaled_{c_id}")
        model.add(scaled == cp_model.LinearExpr.sum(dev_vars))
        quotient = model.new_int_var(0, total * d, f"quotient_{c_id}")
        model.add_division_equality(quotient, scaled, d)
        quotients.append(quotient)

    return _W_CLASS_DAY_BALANCE * cp_model.LinearExpr.sum(quotients) if quotients else 0
```

- [ ] **Step 4: Wire into `_emit_objective`**

Replace `_emit_objective`'s body with:

```python
    summand_subject_pref = _objective_subject_preference_terms(problem, anchor_vars, lookups)
    summand_home_room = _objective_home_room_term(problem, anchor_vars, lookups)
    summand_class_gap = _objective_gap_term(
        model, problem, anchor_vars, lookups, scope_kind="class", weight=_W_CLASS_GAP
    )
    summand_teacher_gap = _objective_gap_term(
        model, problem, anchor_vars, lookups, scope_kind="teacher", weight=_W_TEACHER_GAP
    )
    summand_class_day_balance = _objective_class_day_balance_term(
        model, problem, anchor_vars, lookups
    )
    model.minimize(
        summand_subject_pref
        + summand_home_room
        + summand_class_gap
        + summand_teacher_gap
        + summand_class_day_balance
    )
```

- [ ] **Step 5: Rebuild and run the new test**

```bash
mise run solver:rebuild && uv run pytest solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_lopsided_spread_problem -v
```

Expected: PASS.

- [ ] **Step 6: Run the full suite**

```bash
uv run pytest solver/solver-py/tests/test_cpsat.py solver/solver-py/tests/test_cpsat_determinism.py -v
```

Expected: every test passes. Existing fixtures with single-day placements evaluate the day-balance term to 0 (one day's count == class_total, the other days are 0; `|class_total * D - class_total| + ... + |0 - class_total| = class_total * (D-1) + class_total * (D-1) = ...` — actually a single-day placement does fire balance cost. Re-derive: 1 placement on day 0, D=1: class_total=1, c[0]=1, |1*1-1| = 0 → balance = 0. Single TB problems: D=1 so balance is always 0. Multi-day with single placement: c[0]=1, c[others]=0, sum=1, D=days; scaled = |1*D-1| + (D-1) * |0*D-1| = (D-1) + (D-1) = 2(D-1); quotient = 2(D-1) // D. For D=2: quotient=1; weighted=5. Doppelstunde fixture (2 TBs same day, single block): D=1, balance=0. The home_room fixture: 2 placements on day 0, D=1, balance=0. The forced-gap fixture: 2 placements on day 0, D=1, balance=0. So existing fixtures don't fire the term. The lopsided fixture is the first that does.)

If existing tests fail because a previously-zero score now picks up day-balance contribution, audit the fixture's day count: any single-day fixture must have D=1 (one distinct `tb.day_of_week`), in which case the balance term is structurally 0.

- [ ] **Step 7: Lint**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add solver/solver-py/python/klassenzeit_solver/cpsat.py solver/solver-py/tests/test_cpsat.py
git commit -m "feat(solver-py): port class_day_balance axis into cpsat objective"
```

---

## Task 6: Update `BackendObjective` declaration for cpsat

Flip cpsat's row to `optimised: ALL`, `declared_skipped: empty`. Update the test that pins the declaration.

**Files:**
- Modify: `solver/solver-core/src/quality.rs::build_backend_objectives` — flip cpsat's BTreeSets and notes.
- Modify: `solver/solver-core/src/quality.rs::tests::backend_objective_cpsat_partitions_quality_components` — flip expectations.

- [ ] **Step 1: Update the test expectation first (TDD red)**

Edit `solver/solver-core/src/quality.rs` `tests::backend_objective_cpsat_partitions_quality_components`. Find the existing test (around line 693) and replace its body with the post-port expectation:

```rust
    #[test]
    fn backend_objective_cpsat_partitions_quality_components() {
        // After item 48: cpsat optimises every component via the ported
        // model.minimize(...) (subject_preference + home_room + gap +
        // class_day_balance summands).
        let bo = backend_objective("cpsat").expect("registered");
        assert_eq!(
            bo.optimised.len(),
            QualityComponent::ALL.len(),
            "cpsat should optimise every component after item 48"
        );
        assert!(
            bo.declared_skipped.is_empty(),
            "cpsat should not declare any skipped components after item 48"
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo nextest run -p solver-core --test-threads=1 -E 'test(=quality::tests::backend_objective_cpsat_partitions_quality_components)'
```

Expected: FAIL — current declaration has `optimised: empty` and `declared_skipped: ALL`.

- [ ] **Step 3: Update the declaration**

Edit `solver/solver-core/src/quality.rs` `build_backend_objectives` (around line 381). Replace the cpsat block:

```rust
    let cpsat_optimised: BTreeSet<QualityComponent> =
        QualityComponent::ALL.iter().copied().collect();
    let cpsat_skipped: BTreeSet<QualityComponent> = BTreeSet::new();
    let cpsat_notes = "CP-SAT's model.minimize(...) mirrors \
                       score_solution(..., PRODUCTION_ACTIVE_WEIGHTS) per item 48; \
                       gap encoding via per-(entity, day, position) channeling, \
                       day-balance via abs-equality plus division-equality.";
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cargo nextest run -p solver-core --test-threads=1 -E 'test(=quality::tests::backend_objective_cpsat_partitions_quality_components)'
```

Expected: PASS.

- [ ] **Step 5: Run the full quality test set**

```bash
cargo nextest run -p solver-core
```

Expected: every test passes.

- [ ] **Step 6: Lint**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add solver/solver-core/src/quality.rs
git commit -m "refactor(solver-core): declare cpsat optimises every QualityComponent"
```

---

## Task 7: Document the cpsat objective shape in `solver/CLAUDE.md`

**Files:**
- Modify: `solver/CLAUDE.md` — add a bullet under "solver-py rules" describing the cpsat objective encoding.

- [ ] **Step 1: Add the bullet**

Edit `solver/CLAUDE.md`. Find the existing CP-SAT bullet `**CP-SAT module semantics:**` (under "solver-py rules"). Add a new bullet immediately after it:

```markdown
- **CP-SAT model objective mirrors `score_solution` exactly (item 48).** `_emit_objective` builds five summands: (1) per-anchor constant coefficient for `prefer_early` + `avoid_first` + `avoid_last` + `prefer_late` (window-position arithmetic known at model build), (2) per-anchor constant coefficient for `prefer_home_room` (per-class mismatch boolean × N), (3-4) per-(entity, day, position) `present`/`has_left`/`has_right`/`gap` BoolVars with four channeling inequalities for `class_gap` and `teacher_gap`, (5) per-class `c_count`/`dev`/`scaled`/`quotient` chain via `add_abs_equality` + `add_division_equality` for `class_day_balance`. Every weight in the objective mirrors `solver_core::types::PRODUCTION_ACTIVE_WEIGHTS`; module-level constants `_W_*` in `cpsat.py` keep them legible. The contract `solver.objective_value() == score_solution_json(problem, placements)` is pinned by `test_cpsat_objective_value_equals_score_solution_on_<fixture>` in `solver/solver-py/tests/test_cpsat.py`. When a new soft-cost axis lands in `score_solution` (new `ConstraintWeights` field plus a new term in the weighted sum), extend `_emit_objective` in lockstep or the property test goes red.
```

- [ ] **Step 2: Lint**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add solver/CLAUDE.md
git commit -m "docs(solver): document cpsat objective encoding shape (item 48)"
```

---

## Task 8: Delete OPEN_THINGS item 48 bullet

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md` — remove the item 48 line.

- [ ] **Step 1: Delete the bullet**

Edit `docs/superpowers/OPEN_THINGS.md`. Find the line beginning `48. **CP-SAT model objective parity with solver-core's \`score_solution\`.**` and delete it (the bullet is one line; surrounding bullets stay in their numbered positions per the file's "Open solver follow-ups" convention).

- [ ] **Step 2: Lint**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/OPEN_THINGS.md
git commit -m "docs(open-things): delete shipped item 48 bullet"
```

---

## Task 9: Final smoke bench and full test suite

**Files:**
- None modified.

- [ ] **Step 1: Run a side-by-side smoke against master**

Capture the current branch's smoke first:

```bash
mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule --out /tmp/cpsat-objective-final-branch.md
```

Then check out master, run the same command, and check back:

```bash
git checkout master
mise run solver:rebuild
mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule --out /tmp/cpsat-objective-final-master.md
git checkout refactor/cpsat-objective-parity
mise run solver:rebuild
```

Expected: branch's `cpsat` soft_score column drops noticeably from master's (~349 baseline) toward LAHC's range (~90). Capture both files for the PR body's "Bench evidence" section.

- [ ] **Step 2: Run the full test suite**

```bash
mise run test
```

Expected: every Rust + Python + frontend test passes.

- [ ] **Step 3: Run the full lint pass**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 4: Done**

No commit at this step; the smoke output goes into the PR body in step 7 of the autopilot flow.

---

## Self-review checklist (executed before handoff)

- [x] Spec coverage: Task 1 covers JSON contract change. Task 2 covers subject_preference axes (4 axes). Task 3 covers home_room. Task 4 covers class_gap + teacher_gap. Task 5 covers class_day_balance. Task 6 covers BackendObjective. Task 7 covers CLAUDE.md note. Task 8 covers OPEN_THINGS delete. Task 9 covers bench evidence + final test sweep. Every spec acceptance criterion (1-8) maps to a task.
- [x] No placeholders.
- [x] Type consistency: `_emit_objective` signature matches across tasks; `_objective_*` helpers all return `cp_model.LinearExpr` or 0; `_build_per_slot_presence` returns `dict[tuple[str, int, int], cp_model.IntVar]` consistently used by `_objective_gap_term`.
