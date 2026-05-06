# Reconcile `solution.soft_score` with the full weighted cost spec (active sprint, item 41)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Correctness phase: item 41.
**Goal.** `solver/solver-core/src/solve.rs:249` sets `solution.soft_score = state.soft_score`. `state.soft_score` is the LAHC running slice (`class_gap + teacher_gap + subject_pref`); it omits `prefer_home_room: 5` and `class_day_balance: 5`, which `PRODUCTION_ACTIVE_WEIGHTS` activates. The cpsat backend, by contrast, populates `Solution.soft_score` via `score_solution_json` (full cost). Bake-off cells therefore compare LAHC and cpsat on different objectives, partly accounting for the `lahc_rr_kempe` advantage in `BENCH_RESULTS.md`. Replace the assignment with `solution.soft_score = score_solution(problem, &solution.placements, &config.weights)` so every backend reports the same number.

**Non-goal.** Not aligning the LAHC inner-loop optimisation objective with the full cost (separate, much larger item; would require running `score_solution` per LAHC accept). No new public API, no field rename, no wire-format break. No bake-off bench refresh in this PR (queued as a sprint-tidy follow-up since it is ~80 min wall-clock). No backend, frontend, or solver-py code change.

## Context

`solver-core` runs LAHC with a partition-delta scoring model. `GreedyState.soft_score` (the running total) covers the three axes whose deltas are cheap to compute against per-(entity, day) gap counts and per-lesson subject-preference scalars: `class_gap`, `teacher_gap`, and `subject_pref` (sum over `avoid_first_period`, `avoid_last_period`, `prefer_late_period`, `prefer_early_period`). The full scorer at `solver-core/src/score.rs::score_solution` adds `prefer_home_room` (one walk over placements per `(class, subject) -> room` lock) and `class_day_balance` (sum over per-class-day counts vs. their mean). Both are O(P) but require a full walk that the LAHC inner loop avoids by design.

`solver-core/src/solve.rs:249` ends `solve_with_config` with `solution.soft_score = state.soft_score`. That assignment is the boundary where the slice leaks into the reported solution. Everywhere else the slice stays internal:

- The LAHC outer loop's optimisation target is `state.soft_score`; that is correct for the loop.
- `score_solution` is the canonical cross-backend scorer. The cpsat backend (`klassenzeit_solver.cpsat`) builds placements from CP-SAT's solution and calls `score_solution_json` to populate `Solution.soft_score` (`solver-py/python/klassenzeit_solver/cpsat.py:49`).
- `solver-py/tests/test_score_solution_json.py` round-trips `solve_json_with_config` through `score_solution_json` and asserts they agree on the slice axes; the active fixture in that test uses default weights (slice axes only), so the round-trip currently masks the gap.
- `solver-bench/src/main.rs:349-354` re-scores the cpsat output via Rust-side `score_solution(...)` rather than reading `solution.soft_score` from the JSON. Once both backends report the same number, that recompute becomes redundant.

The `lahc_rr_kempe` advantage in `BENCH_RESULTS.md` is partly an artifact: `lahc_rr_kempe` minimises the slice and reports the slice; `cpsat` minimises the full cost (objective `Minimize(0)` plus the constraint shape) and reports the full cost. A LAHC plan that moves a class out of its home room saves slice cost but raises the full cost; today the bench credits the slice savings without debiting the home-room penalty. PR #189 (item 39) and PR #190 (item 40) closed orthogonal bugs on the Kempe and validator side; this PR closes the reporting gap.

The brainstorm (`/tmp/kz-brainstorm/brainstorm.md` for this run) settled four judgment calls. First, replace at the boundary (option a) rather than rename and ship a sibling field (option b); the slice has no caller outside the LAHC inner loop, so a wire-format break is pure cost. Second, drop the bench's defence-in-depth recompute on the cpsat arm; once both arms route through `score_solution`, the recompute is duplicate coverage. Third, do not refresh `BENCH_RESULTS.md` in this PR; queue it as a sprint-tidy follow-up because the 80 min bake-off wall-clock would inflate the PR cycle. Fourth, do not refresh `BASELINE.md` unless the criterion bench drifts >3%; the extra `score_solution` call is sub-millisecond per solve.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 41. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Replace the assignment at `solver/solver-core/src/solve.rs:249`:
  - Before: `solution.soft_score = state.soft_score;`
  - After: `solution.soft_score = score::score_solution(problem, &solution.placements, &config.weights);`
- Update the surrounding rustdoc comment to record the new contract: `Solution.soft_score` is the full weighted cost; `state.soft_score` is the LAHC slice.
- Add a regression test `solve_soft_score_under_production_weights_equals_score_solution` in `solver/solver-core/tests/score_property.rs`. The test builds a small problem where home-room or class-day-balance contributes a non-zero penalty under `PRODUCTION_ACTIVE_WEIGHTS`, runs `solve_with_config`, and asserts `solution.soft_score == score_solution(&problem, &solution.placements, &PRODUCTION_ACTIVE_WEIGHTS)`. Today's slice equality holds trivially under the existing tests' weights; the new test pins the production contract.
- Drop the duplicate `score_solution` recompute from `solver/solver-bench/src/main.rs:349-354`. The cpsat arm's `solution.soft_score` already carries the full cost from `score_solution_json` inside `cpsat.py`; push that value directly into `soft_score_feasible`. Keep the `serde_json::from_str` parse so a malformed cpsat response still surfaces.
- Update the rustdoc on `Solution::soft_score` (`solver-core/src/types.rs:348`) to note the field is the full weighted cost on every code path.
- Delete OPEN_THINGS item 41. Advance the active-sprint preamble's "next pickup" line to the next P0 item (item 30, `peak_memory_kb` columns).
- Update auto-memory `project_roadmap_status.md` to reflect item 41 shipped and what is next.

**Out of scope.**

- LAHC inner-loop optimisation objective alignment. The loop continues to optimise the slice; this PR fixes reporting only. Aligning the loop's target to the full cost would require either per-iteration `score_solution` (~20-100x slower) or new partition-delta machinery for the home-room and class-day-balance axes; both are larger items that need their own brainstorm and bench evidence.
- `BENCH_RESULTS.md` refresh (~80 min). Queued as the next item in the sprint-tidy phase.
- `BASELINE.md` refresh, unless the criterion bench shows >3% drift on the run that ships the fix.
- ADR. The change is a single-line correctness fix with a documented contract; ADRs are reserved for load-bearing architectural decisions.
- Renaming `state.soft_score`. The internal field still describes what it is (the LAHC slice running total); a name like `slice_score` would churn the LAHC code without a caller asking for the rename.
- Backend, frontend, solver-py source changes. The wire format and Pydantic schemas already accept the field as `int` with no range constraint beyond `ge=0`. The frontend renders the number as-is.

## Code change

`solver/solver-core/src/solve.rs:249`:

```rust
// Before
solution.soft_score = state.soft_score;

// After
solution.soft_score = score::score_solution(problem, &solution.placements, &config.weights);
```

Surrounding doc comment update on the same block (3-4 lines):

```rust
// state.soft_score is the LAHC running slice (class_gap + teacher_gap +
// subject_pref). Solution.soft_score is the full weighted cost on the
// final placements, including prefer_home_room and class_day_balance,
// so consumers compare every backend on the same number.
solution.soft_score = score::score_solution(problem, &solution.placements, &config.weights);
```

`solver/solver-core/src/types.rs:348` rustdoc on `Solution::soft_score`:

```rust
/// Full weighted soft-constraint cost on the final placements. Computed via
/// `score_solution(problem, placements, weights)` at the end of every
/// `solve_with_config`. The LAHC inner loop optimises a faster slice
/// (`class_gap + teacher_gap + subject_pref`) for delta efficiency; this
/// reported field is the canonical objective so cross-backend bench cells
/// (LAHC, cpsat) compare on the same number.
pub soft_score: u32,
```

`solver/solver-bench/src/main.rs:349-354`:

```rust
// Before
let soft = solver_core::score_solution(
    problem,
    &solution.placements,
    &solver_core::PRODUCTION_ACTIVE_WEIGHTS,
);
soft_score_feasible.push(soft);

// After
soft_score_feasible.push(solution.soft_score);
```

The `Solution: serde_json::from_str` parse stays; the duplicate scorer call goes.

## Test changes

New test in `solver/solver-core/tests/score_property.rs`:

```rust
#[test]
fn solve_soft_score_under_production_weights_equals_score_solution() {
    // Hand-built problem where the FFD-greedy plan exercises home-room and
    // class-day-balance axes (non-zero contribution under
    // PRODUCTION_ACTIVE_WEIGHTS). The slice score and the full cost
    // diverge; the assertion is that solution.soft_score matches the full
    // recompute, not the slice.
    let problem = build_production_weight_problem();
    let cfg = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        ..SolveConfig::default()
    };
    let sol = solve_with_config(&problem, &cfg).expect("solve");
    let recomputed = score_solution(&problem, &sol.placements, &PRODUCTION_ACTIVE_WEIGHTS);
    assert_eq!(sol.soft_score, recomputed);
}
```

`build_production_weight_problem` is a small helper in the same test file. Shape: 1 class, 2 subjects (one with `home_room_id = Some(r0)`, the other no home-room hint), 2 teachers, 3 rooms, 5 time-blocks (one school day with 5 positions). The home-room subject's placements that land in `r1` or `r2` will drive `prefer_home_room` cost > 0 under PRODUCTION_ACTIVE_WEIGHTS. The fixture is structurally reachable by the FFD greedy without LAHC needing to do anything; deadline kept at the default so the test runs fast.

Why a unit test, not a proptest: the proptest generators (`small_problem` in `score_property.rs`, `lahc_small_problem` in `lahc_property.rs`) already pass round-trip equality under their own `weights()` generators. Those generators only set `class_gap` and `teacher_gap`, so under them slice == full holds trivially. Widening the proptest weight generators to include `prefer_home_room` and `class_day_balance` is a separate widening (similar shape to item 40); doing both in one PR mixes the change. The targeted unit test pins the new contract immediately and the generator widening can be queued under the active sprint's tidy phase or the score_property follow-ups.

Existing tests verified to still pass:
- `lahc_property.rs` four `lahc.soft_score == score_solution(...)` assertions: trivially equal post-fix (both sides become the recompute).
- `score_property.rs:110` `solve_soft_score_equals_score_solution`: same trivial pass.
- `tests/early_exit.rs:95`, `solve.rs:1654`, `solve.rs:1736` `soft_score == 0` assertions: the test fixtures use weights that zero out home-room and class-day-balance, so the full-cost recompute matches the slice (both 0).

## Bench impact

Criterion bench (`mise run bench`):
- `solve_with_config` gains one `score_solution` call per solve. `score_solution` is O(P + classes*days + teachers*days). For dreizuegige (P=294, 12 classes, 5 days, ~10 teachers), this is roughly 294 + 60 + 50 = 404 hash-map lookups, sub-millisecond.
- LAHC's hot path (the iteration loop) is unchanged.
- Expected drift: <1% on grundschule (P=45), <1% on zweizuegig (P=196), <1% on dreizuegig (P=294). Refresh `BASELINE.md` only if observed drift exceeds 3%.

Bake-off bench (`mise run bench:bakeoff`): not run in this PR. Once refreshed (sprint-tidy follow-up), the LAHC `soft_score_median` cells will rise to reflect the home-room and class-day-balance cost the existing plans carry. The relative ordering between `lahc`, `lahc_rr`, and `lahc_rr_kempe` may shift; that informs the ADR 0032 production-default revisit, which is already a downstream sprint item.

## Commit plan

1. `test(solver-core): pin production-weight soft_score reporting (item 41)`. Adds `solve_soft_score_under_production_weights_equals_score_solution` and `build_production_weight_problem`. The test fails (slice differs from full).
2. `fix(solver-core): report full weighted cost as solution.soft_score (item 41)`. Flips `solve.rs:249` to call `score_solution`. Updates the surrounding rustdoc and the field rustdoc on `Solution::soft_score`. Test from commit 1 turns green.
3. `chore(solver-bench): drop duplicate score_solution recompute from cpsat arm`. Replaces the bench's cpsat-side recompute with the field-read.
4. `docs: remove shipped item 41 from OPEN_THINGS`. Deletes the item, advances the next-pickup line, refreshes auto-memory.
5. (Conditional) `chore(solver-core): refresh BASELINE.md post-soft_score-fix`. Only if `mise run bench` shows >3% drift on at least one of the three fixtures.

Each commit is independently reviewable. The TDD red-green sequence (test before fix) gives the reviewer a one-glance demonstration of the bug and the fix.

## Risks

- A future test that uses `PRODUCTION_ACTIVE_WEIGHTS` and asserts a hand-computed `soft_score` value would silently pass today (slice happens to equal full when other axes are 0 on that test's plan) and flip when the plan starts exercising home-room or class-day-balance. The new regression test pins the production contract; future tests should use `score_solution` recomputes against `PRODUCTION_ACTIVE_WEIGHTS` rather than hand-computed numbers.
- LAHC's outer-loop early exit triggers on `state.soft_score == 0`. Under `PRODUCTION_ACTIVE_WEIGHTS`, that means LAHC stops when the slice is zero even if home-room or class-day-balance still carry cost. This is existing behaviour, surfaced (not introduced) by this PR. Tracked as the LAHC objective-alignment follow-up; not in scope for this PR.
- The frontend renders `soft_score` as-is in the schedule view. Post-fix, displayed values rise on plans that exercise the omitted axes. No UI test asserts a specific value; the rise is the corrected number.

## Acceptance criteria

- `mise run test:rust` green on `master + this branch`.
- `mise run lint` green.
- New test `solve_soft_score_under_production_weights_equals_score_solution` fails on `master` (verified by checking out master, applying only the test commit, running the test) and passes after the fix commit.
- `mise run bench` reports <20% wall-clock regression vs. the committed `BASELINE.md` (project-wide budget). Refresh the baseline only if drift exceeds 3%; surface in PR body either way.
- `mise run bench:bakeoff` is NOT in the PR's gate; queued as the next sprint-tidy follow-up.
- OPEN_THINGS item 41 deleted, next-pickup line advanced.
- Auto-memory `project_roadmap_status.md` refreshed.
