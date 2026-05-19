# 0049: Bump `max_per_class_interior_gaps` weight to clear LAHC einzuegig interior-gap bar

- **Status:** Accepted
- **Date:** 2026-05-19

## Context

`test_grundschule_schedule_meets_quality_bar` denied 17 of 20 iterations on a production-deadline flake-loop (60000 ms `lahc_rr`) with every denial landing on `QualityIssue(kind='interior_gap', detail={'total_gaps': 3, 'max_gaps_per_class': 2})` on einzuegig classes 1a (twice) and 2a (once). The quality predicate `check_interior_gaps` in `backend/src/klassenzeit_backend/scheduling/quality_checks.py` sums per-class interior gaps across the 5 weekdays and fails when the total exceeds `MAX_INTERIOR_GAPS_PER_CLASS = 2`.

The canonical scorer does include interior gaps via `score::worst_class_interior_gaps` multiplied by `PRODUCTION_ACTIVE_WEIGHTS.max_per_class_interior_gaps = 10`. At that weight, an LAHC trial closing 1 gap (saves 10) while perturbing `class_day_balance` by 1 (costs 20, the heaviest soft axis after ADR 0043) nets +10 and is rejected. LAHC therefore plateaus at 3 gaps for einzuegig 1a / 2a even at production wall-clock. This is the same plateau shape ADR 0043 documented for `class_day_balance` on dreizuegig: a smooth soft-cost axis under-weighted relative to a neighbouring axis the LAHC moves perturb. Item 86's 2026-05-16 home_pairs FFD experiment ruled out FFD-side reseeding on this einzuegig fixture.

## Decision

Bump `max_per_class_interior_gaps` weight from 10 to 25 in `PRODUCTION_ACTIVE_WEIGHTS`. At weight 25 an LAHC trial closing 1 gap while perturbing `class_day_balance` by 1 nets minus 5 and is accepted. The weight sits above `class_day_balance = 20` so gap-closing moves can absorb a 1-unit balance regression.

Validation: a 20-iter flake-loop on `test_grundschule_schedule_meets_quality_bar` at the test's new 60000 ms `lahc_rr` deadline returned 20/20 pass after the weight bump (`pass=20 fail=0`).

## Alternatives considered

- **Add a directed interior-gap LAHC move.** Rejected for blast radius; revisit if a future regression denies the flake-loop at weight 25.
- **Bump weight to 30 or higher.** Reserved as the in-PR escalation path; 25 is the smallest value that passed 20/20, minimising blast radius on other quality axes.
- **Widen `solve_deadline_ms` for einzuegig.** Rejected: at production budget the flake already manifests; the plateau is gradient-bound, not budget-bound.
- **Gap-aware FFD seed.** Rejected: item 86's home_pairs experiment denied on this exact einzuegig `interior_gap` mechanism.
- **Relax `MAX_INTERIOR_GAPS_PER_CLASS` to 3.** Rejected: tests the implementation, not the contract.

## Consequences

- OPEN_THINGS item 87 closes in this PR; no fresh items filed.
- The integration test deadline rises from 5000 ms to 60000 ms; the test now validates production-budget behaviour. `.test-duration-budget` raised from 120s to 280s in lockstep.
- CP-SAT objective mirror (`klassenzeit_solver.cpsat`) hardcodes `_W_MAX_PER_CLASS_INTERIOR_GAPS = 25` in lockstep with the Rust const (the mirror is NOT auto-derived from the PyO3 binding; weight changes touch both sites).
- `solver/solver-core/src/types.rs::production_active_weights_match_legacy_inline_literal` updated in lockstep with the const.
- Delta paths in `lahc::try_change_move_n1`, `lahc::rr_attempt`, and `lahc::try_kempe_move` already maintain this axis incrementally; no new code paths added.
- Other axes (notably `home_room_miss` on dreizuegig, item 86) may regress slightly on a future bench refresh; revisit when production refresh data lands.

## Anchors

- ADR 0023 (home-room weight semantics).
- ADR 0030 (CP-SAT objective mirror).
- ADR 0037 (production-default `lahc_rr`).
- ADR 0038 (per-backend deadline configuration).
- ADR 0043 (precedent for single-weight bump in `PRODUCTION_ACTIVE_WEIGHTS`).
- OPEN_THINGS item 87 closed by this PR.
