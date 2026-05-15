# 0043: Bump `class_day_balance` weight to clear LAHC day-spread bar

- **Status:** Accepted
- **Date:** 2026-05-16

## Context

The 2026-05-15 production bake-off (`solver/solver-core/benches/BENCH_RESULTS.md`) reported `worst_spread_med = 5` for all four LAHC backends on the `dreizuegig` pinned fixture; the schedule-quality bar passes at `worst_spread <= 2`. Unpinned dropped to 3 or 4 depending on backend, still over the bar. The 4-class demo Grundschule integration test `test_grundschule_schedule_meets_quality_bar` flaked ~1-in-3 at 5000 ms on the same `MAX_DAY_LOAD_SPREAD <= 2` predicate (OPEN_THINGS item 82, routed through item 87).

The `worst_class_spread` axis in `score_solution` is non-smooth ("max over classes of max - min"). Moves that improve a non-worst class show zero canonical delta on this axis, so LAHC plateaus. The smooth gradient on the same shape (`class_day_balance`, scaled L1 from mean) carried a weight of 5 in `PRODUCTION_ACTIVE_WEIGHTS`, dominated 2:1 by `class_gap` and `teacher_gap` (each 10). The search traded day-balance for cheaper gap fixes.

## Decision

Bump `class_day_balance` weight from 5 to 20 in `PRODUCTION_ACTIVE_WEIGHTS`. The smooth scaled-L1 gradient becomes one of the strongest soft axes and pulls every class toward an even daily count. Delta paths in `lahc::try_change_move_n1`, `lahc::rr_attempt` (recreate), and `lahc::try_kempe_move` already maintain this axis incrementally; no new code paths added.

Validation: a 20-iter flake-loop on `backend/tests/scheduling/test_grundschule_schedule_quality.py::test_grundschule_schedule_meets_quality_bar` at the test's 5000 ms `lahc_rr` deadline returned 20/20 pass after the weight bump (the test's `@pytest.mark.xfail(strict=False)` decorator dropped in the same PR).

Per profile rule 9 ("Smoke bench validates fixes; production refresh refreshes data"), a separate production-budget bench refresh updates `BENCH_RESULTS.md` in a follow-up. The integration-test flake-loop is the canonical gate for item 82 closure (profile rule 10).

## Consequences

- OPEN_THINGS items 87 (dreizuegig pinned `worst_spread_med = 5`) and 82 (`test_grundschule_schedule_meets_quality_bar` flake) close in this PR.
- `test_grundschule_schedule_meets_quality_bar` runs without the xfail mark; the integration-test surface gains one more hard gate against soft-cost regressions.
- CP-SAT objective mirror (`klassenzeit_solver.cpsat`) reads `PRODUCTION_ACTIVE_WEIGHTS` through the PyO3 binding; the new value propagates automatically. CP-SAT dreizuegig 0/20 infeasibility (item 83) is unaffected (search-budget bound, not weight bound).
- Other axes (notably `home_room_miss`, item 86) may regress slightly; item 86's plateau remains open as a separate axis to revisit when the next production refresh data lands.
- `solver/solver-core/src/types.rs::production_active_weights_match_legacy_inline_literal` updated in lockstep with the const.
- `solve()` legacy entry point in `solver-core/src/solve.rs` consolidated against `PRODUCTION_ACTIVE_WEIGHTS` in a tidy-first commit so future weight changes flow through one site.

## Alternatives considered

- **Bump `max_per_class_spread` from 10 to 40-50.** Rejected: the axis is non-smooth; weight alone does not address the plateau, because two-or-more classes tied at the worst spread produce zero canonical delta on improving moves.
- **New `class_day_spread_sum` axis (sum over classes of `max - min`).** Rejected for field-cascade blast radius: per `solver/CLAUDE.md`, a new `ConstraintWeights` field is a ~15-site cascade plus CP-SAT mirror plus py stubs plus bench column.
- **Widen `solve_deadline_ms` for `dreizuegig` per ADR 0038 widening shape.** Rejected: the plateau is gradient-bound, not budget-bound; longer LAHC walks the same flat landscape.
- **Add a `class_day_spread` targeted move alongside Change + Swap + R&R + Kempe.** Rejected for blast radius; revisit if a future regression forces it.

## Anchors

- ADR 0023 (home-room weight semantics).
- ADR 0030 (CP-SAT objective mirror).
- ADR 0037 (production-default `lahc_rr`).
- ADR 0038 (per-backend deadline configuration).
- OPEN_THINGS items 87 + 82 closed by this PR.
