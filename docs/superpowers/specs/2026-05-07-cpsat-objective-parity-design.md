# Design: CP-SAT model objective parity with `score_solution` (item 48)

**Status:** approved (autonomous mode; brainstorm at `/tmp/kz-brainstorm/brainstorm.md`).
**Date:** 2026-05-07.
**Owner:** pgoell.

## Problem

`solver/solver-py/python/klassenzeit_solver/cpsat.py` builds a CP-SAT model and calls `model.minimize(0)`. Item 41 (post-solve `score_solution_json` rescore) makes the bench *rate* CP-SAT by `solver_core::score_solution` with `PRODUCTION_ACTIVE_WEIGHTS`, but CP-SAT itself optimises a different (constant) function. The 2026-05-06 production refresh shows CP-SAT lands at solver-core soft scores 4-5x the LAHC variants on every fixture (grundschule 349 vs `lahc` 90; zweizuegig 2798 vs 775; dreizuegig 5285 vs 2235; lock_in 1289 vs 588) despite reporting OPTIMAL to its own model.

Item 48 closes the gap by porting `score_solution`'s exact weight set and per-component formula into the CP-SAT `model.minimize(...)` expression so CP-SAT's internal objective steers toward the same target the bench rates by.

## Non-goals

- Not flipping the production default. ADR 0031/0032 stay; the production-default revisit is OPEN_THINGS item 47, post this PR.
- Not changing `PRODUCTION_ACTIVE_WEIGHTS`. Same eight axes, same numeric weights.
- Not encoding the `quality::evaluate_quality` predicate (the schedule-quality bar with its `room_subject_suitabilities` exemption). That predicate is bench-time, not score-time; matching it is a future item if ever needed.
- Not touching CP-SAT's hard constraints. Cardinality, non-overlap, teacher max-hours, same-room, lesson-group co-placement, pinned placements stay verbatim.
- Not changing the `solve_cpsat_json` callable contract beyond an additive `model_objective_value` field in the returned JSON.

## Architecture

`cpsat.py`'s existing `_build_model` already calls `_emit_*` helpers per constraint family before `model.minimize(0)`. Replace the `model.minimize(0)` line with a call to a new `_emit_objective(model, problem, anchor_vars, lookups)` that builds five summands and passes their sum into `model.minimize(...)`. Each summand corresponds to one axis (or one cluster of related axes) in `score_solution`:

1. `_objective_subject_preference_terms` — per-anchor constant coefficient covering `prefer_early`, `avoid_first`, `avoid_last`, `prefer_late`. Computed at model build time from `subject` flags + `tb.position` + `max_position_for_day`. Returns `LinearExpr` summand `sum_anchor coeff * y[anchor]`.
2. `_objective_home_room_term` — per-anchor constant coefficient: `sum over class in lesson.school_class_ids of (mismatch ? weights.prefer_home_room * N : 0)`. Returns `sum_anchor coeff * y[anchor]`.
3. `_objective_class_gap_term` and `_objective_teacher_gap_term` — per-(entity, day, position) `gap` BoolVar via channeling inequalities (see encoding below). Returns `weights.<gap> * sum_(entity, day, pos) gap[entity, day, pos]`.
4. `_objective_class_day_balance_term` — per-class `quotient[class] = scaled[class] // D` with `scaled[class] = sum_day |c_count[class, day] * D - class_total[class]|`. Returns `weights.class_day_balance * sum_class quotient[class]`.

The objective is the sum of these four returns, passed to `model.minimize`.

`solve_cpsat_json` exposes the model's objective value back to callers via a new `"model_objective_value": int` field in the returned JSON. The Python property test reads this field and asserts equality with `score_solution_json(problem_json, json.dumps(out["placements"]))`.

## Encoding: gap counts (class and teacher)

For each `(entity, day)` partition where `entity` is a class id (for `class_gap`) or a teacher id (for `teacher_gap`):

For every position `p` in the day's positions list:
- `present[entity, day, p]` is an `IntVar` in `[0, 1]`. Built as `sum over anchors (l, day, start, r) where start <= p < start + N(l) and entity ∈ scope(lesson, anchor)`. For class scope: `entity ∈ lesson.school_class_ids`. For teacher scope: `entity == lesson.teacher_id`. Non-overlap (per `_emit_non_overlap`) keeps the sum at most 1 in feasible solutions; we add `model.add(present <= 1)` explicitly to make the bound visible to the solver's presolve.
- `has_left[entity, day, p]` is a `BoolVar`. Channelled via `model.add_max_equality(has_left, [present[entity, day, q] for q in positions if q < p])`. If the list is empty, `has_left` is forced to 0 by `model.add(has_left == 0)`.
- `has_right[entity, day, p]` is a `BoolVar`. Symmetric via `add_max_equality` over `q > p`.
- `gap[entity, day, p]` is a `BoolVar` with four channeling inequalities:
  - `gap >= has_left + has_right + (1 - present) - 2`
  - `gap <= has_left`
  - `gap <= has_right`
  - `gap <= 1 - present`

The first inequality forces `gap = 1` when `has_left = has_right = 1` and `present = 0`; the other three force `gap = 0` whenever any of those three sides is 0. Since `gap ∈ {0, 1}`, the inequalities are tight at the integer corners.

`gap_total[entity-kind] = sum over (entity, day, p) of gap[entity, day, p]` for `entity-kind ∈ {class, teacher}`. The objective contribution is `weights.class_gap * gap_total[class] + weights.teacher_gap * gap_total[teacher]`.

This encoding evaluates `(max_present_pos - min_present_pos + 1) - present_count` exactly, which equals `score::gap_count` on the deduped sorted position vector by construction.

## Encoding: subject_preference axes

For each anchor key `(l, d, p, r)` with subject `s` and block size `N` and `M = max_position_for_day[d]`:
- `prefer_early_coeff = weights.prefer_early_period * s.prefer_early_period * (N*p + N*(N-1)/2)`
- `avoid_first_coeff = weights.avoid_first_period * s.avoid_first_period * (1 if p == 0 else 0)`
- `avoid_last_coeff = weights.avoid_last_period * s.avoid_last_period * (1 if p + N - 1 == M else 0)`
- `prefer_late_coeff = weights.prefer_late_period * s.prefer_late_period * (N*M - N*p - N*(N-1)/2)`

Sum these into one Python `int` `subject_preference_coefficient[anchor]`. The objective contribution is `sum over anchor of subject_preference_coefficient[anchor] * y[anchor]`.

The summation `N*p + N*(N-1)/2 = sum_{i=0..N-1} (p+i)` and `N*M - N*p - N*(N-1)/2 = sum_{i=0..N-1} (M - (p+i))` are exact integer arithmetic; both are non-negative for any valid window `[p, p+N-1] ⊆ [0, M]`.

## Encoding: home_room axis

For each anchor key `(l, d, p, r)` with lesson `l` and block size `N`:
- `home_room_coeff = sum over class in l.school_class_ids of (weights.prefer_home_room * N if class.home_room_id is set and class.home_room_id != r else 0)`

Objective contribution: `sum over anchor of home_room_coeff[anchor] * y[anchor]`. Multi-class lessons accumulate per-class contributions; matches `score::home_room_penalty` exactly because each placement (one per `(time_block, room)` row in the expanded N-block) re-asserts the per-class mismatch indicator. No suitability exemption (that's `quality::evaluate_quality`'s contract, not `score_solution`'s).

## Encoding: class_day_balance axis

For each `class` in `problem.school_classes`:
- `class_total[class]: int` (Python-side constant) = `sum over lesson in problem.lessons where class in lesson.school_class_ids of lesson.hours_per_week`. Computed once at model build.
- `D: int` = number of distinct values of `tb.day_of_week` in `problem.time_blocks`. Computed once.
- `c_count[class, day]: IntVar` in `[0, class_total[class]]`. Constrained by `model.add(c_count[class, day] == sum over anchors (l, day, p, r) where day matches and class in lesson.school_class_ids of N(l) * y[l, day, p, r])`.
- `dev[class, day]: IntVar` in `[0, class_total[class] * D]`. Constrained by `model.add_abs_equality(dev[class, day], c_count[class, day] * D - class_total[class])`.
- `scaled[class]: IntVar` in `[0, class_total[class] * D * D]`. Constrained by `model.add(scaled[class] == sum_day dev[class, day])`.
- `quotient[class]: IntVar` in `[0, class_total[class] * D]`. Constrained by `model.add_division_equality(quotient[class], scaled[class], D)`.

Objective contribution: `weights.class_day_balance * sum_class quotient[class]`.

CP-SAT's `add_division_equality(target, num, denom)` implements floor division for non-negative operands; `scaled` is non-negative (sum of abs values) and `D` is a positive integer, so the division matches `score::class_day_balance_cost`'s `scaled / d` exactly.

## `solve_cpsat_json` contract change

The JSON response gains one optional field:
- `"model_objective_value": int | null` — value of the CP-SAT model objective on the returned solution. `null` when CP-SAT does not return a feasible solution (INFEASIBLE / UNKNOWN status). When non-null, equals `score_solution_json(problem_json, json.dumps(placements))` by construction.

All other fields stay verbatim. Backwards compatibility: any existing caller that does not read `model_objective_value` is unaffected. The CLI (`python -m klassenzeit_solver.cpsat`) passes the JSON through unchanged.

## Property test

`solver/solver-py/tests/test_cpsat.py::test_cpsat_objective_value_equals_score_solution_on_<fixture>`.

Fixtures:
1. `_cpsat_trivial_one_lesson_problem` (existing) — every axis evaluates to 0; both `model_objective_value` and `score_solution_json` report 0.
2. `_cpsat_doppelstunde_problem_with_subject_flags` (new variant of existing doppelstunde fixture) — sets `subject.prefer_late_period > 0` so the prefer_late axis fires; both report nonzero matching values.
3. `_cpsat_lesson_group_multi_class_problem` (existing) extended so classes have `home_room_id` set on at least one of the three classes — fires home_room axis on the per-class boolean OR.
4. `_cpsat_grundschule_subset_problem` (new) — small fixture with two classes, a teacher who teaches both, deliberately constrained TBs so CP-SAT must place classes with a class_gap > 0; exercises gap encoding and class_day_balance.

Each test calls `solve_cpsat_json(problem_json, deadline_ms=2_000, seed=1)`, parses the response, and asserts `out["model_objective_value"] == score_solution_json(problem_json, json.dumps(out["placements"]))`. `solver.objective_value()` is read from inside `solve_cpsat_json` and reported as the JSON field.

## BackendObjective declaration

`solver-core/src/quality.rs::build_backend_objectives` flips cpsat's row to:
```rust
BackendObjective {
    name: "cpsat",
    optimised: QualityComponent::ALL.iter().copied().collect(),
    declared_skipped: BTreeSet::new(),
    notes: "CP-SAT's model.minimize(...) mirrors score_solution(..., \
            PRODUCTION_ACTIVE_WEIGHTS) per item 48; gap encoding via \
            per-(entity, day, position) channeling, day-balance via \
            abs-equality plus division-equality.",
},
```

The existing test `backend_objective_cpsat_partitions_quality_components` (in `solver-core/src/quality.rs::tests`) flips its expectation from `assert!(bo.optimised.is_empty())` to `assert_eq!(bo.optimised.len(), QualityComponent::ALL.len())`.

## Acceptance criteria

1. Property test `test_cpsat_objective_value_equals_score_solution_on_<fixture>` passes for all four fixtures.
2. Existing CP-SAT tests in `solver/solver-py/tests/test_cpsat.py` and `test_cpsat_determinism.py` still pass.
3. `mise run lint` passes (ruff, ty, clippy, etc.).
4. `mise run test:py` and `mise run test:rust` pass.
5. `BackendObjective` declaration for cpsat is updated; `backend_objective_cpsat_partitions_quality_components` flipped accordingly.
6. `bench:bakeoff` smoke (`--budget 5s --seeds 4 --fixtures grundschule`) shows `cpsat` soft_score column lower than the pre-PR baseline. Production-shape refresh of `BENCH_RESULTS.md` is post-merge from master.
7. CLAUDE.md note in `solver/CLAUDE.md` describes the cpsat objective encoding shape.
8. OPEN_THINGS item 48 bullet deleted.

## Risks accepted

- **Tractability degradation on `dreizuegig`.** Model size grows from ~5k to ~15k constraints. CP-SAT may shift from OPTIMAL-in-100ms to FEASIBLE-in-5s. The OPEN_THINGS bullet acknowledges this. Mitigation: smoke after the gap-axis commit; if the cell falls off a cliff (0/N feasibility), document in PR body.
- **Objective value width.** CP-SAT objective is i64; `score_solution` saturates at u32. Realistic fixtures stay sub-million; not a guard concern.
- **Floor division corner.** `add_division_equality` for `scaled / D` matches `score_solution` only for non-negative operands. `scaled` is non-negative; `D > 0`. Safe.
- **Subject-preference window arithmetic.** `N*(N-1)/2` requires `N >= 1`; `validate_structural` guarantees `preferred_block_size >= 1`. Safe.

## References

- `solver/solver-core/src/score.rs::score_solution` (target).
- `solver/solver-core/src/types.rs::PRODUCTION_ACTIVE_WEIGHTS` (weight set).
- `solver/solver-py/python/klassenzeit_solver/cpsat.py` (file under change).
- `solver/solver-core/src/quality.rs::build_backend_objectives` (declaration to flip).
- ADR 0029 (bake-off methodology); ADR 0030 (CP-SAT introduction); ADR 0031, 0032 (production default rationale, untouched by this PR).
- OPEN_THINGS item 48 (the source bullet); item 47 (downstream production-default revisit, blocked on this PR).
