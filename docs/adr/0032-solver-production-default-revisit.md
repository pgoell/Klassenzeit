# 0032: Solver production-default revisit

- **Status:** Accepted
- **Date:** 2026-05-05

## Context

ADR 0031 (Accepted 2026-05-05) picked `Settings.solver_backend = "lahc_rr_kempe"` off the canonical bake-off output committed in PR #181. Three follow-up items in the active sprint program landed AFTER PR #181 and changed both the solver state under test and the harness's ability to flag silent regressions:

- Item 26 (PR #183) ported the Kempe anchor filter to `rr_collect_anchors`, fixing a class of silent placement-drop in `lahc_rr` and `lahc_rr_kempe`.
- Item 28 (PR #184) gated bake-off cell feasibility on `placements_total >= placements_expected`. Pre-fix, the harness only checked `hard_violations.is_empty()`; a silently-dropped placement therefore passed as feasible.
- Item 37 (PR #186) ported the Kempe row-keyed rollback pattern into `rr_attempt`, fixing the residual silent placement-drop that survived items 26 + 27.

ADR 0031's verdict therefore rode on partly-corrupted data: the `lahc_rr` and `lahc_rr_kempe` rows had been silently dropping placements in roughly half the seeds, and pre-item-28 the harness could not distinguish those rows from honestly-feasible ones. Item 29 closes that loop: the `BENCH_RESULTS.md` checked into this commit was regenerated from the post-item-37 solver state at canonical settings (`--budget 60s --seeds 20`, all four fixtures × all four backends).

## Decision

The production default is `Settings.solver_backend = "lahc_rr_kempe"`.

The default holds. The corrected data reinforces ADR 0031's choice rather than overturning it.

## Rationale

Decision rule (re-applied verbatim from ADR 0029 / ADR 0031):

1. Hard gate: any backend with a `0/N` cell on any fixture is rejected.
2. Tiebreak: feasibility rate, then median soft-score across feasible cells, then median total wall-clock.

Applied to the refreshed bench (full numbers in `solver/solver-core/benches/BENCH_RESULTS.md`):

| Backend | All cells 20/20? | Soft-score sum (4 fixtures) | Wall-clock | Verdict |
| --- | :-: | ---: | --- | --- |
| `lahc` | yes | 314 | deadline (60s) on every fixture | rejected on soft-score |
| `lahc_rr` | yes | 319 | deadline (60s) on every fixture | rejected on soft-score |
| `lahc_rr_kempe` | yes | 0 | deadline (60s) on every fixture | **chosen** |
| `cpsat` | yes | 9721 | 0.4s to 12.4s | rejected on soft-score, see below |

`lahc_rr_kempe` reaches the soft-score floor (sum 0 across 4 fixtures × 20 seeds) on every cell. The next-best LAHC variant is `lahc` at sum 314; `lahc_rr` is `lahc` plus noise, sum 319, neither materially better nor worse than `lahc` once dropped placements are accounted for. The tie ADR 0031 had to break (`lahc_rr` and `lahc_rr_kempe` both at sum 0 in PR #181's broken table) is gone: item 37's rollback fix unmasked the real `lahc_rr` numbers, and Kempe now wins outright on soft-score.

Items 26 + 28 + 37 each fixed silent regressions; together they shift `lahc_rr` from "tied with Kempe at sum 0" to "indistinguishable from `lahc` at sum 319." The corrected verdict is therefore stronger than ADR 0031's: Kempe was chosen as a tiebreaker before, it is now chosen as the unique soft-score winner.

## CP-SAT trade-off

CP-SAT lands at sum 9721 on the four-fixture aggregate, far behind every LAHC variant; soft-score gates it out under the rule. Looking past the rule, CP-SAT's wall-clock advantage is real and worth surfacing for a future operator considering it as a configurable fallback:

| Fixture | LAHC variants total wall-clock | `cpsat` total wall-clock | `cpsat` soft-score |
| --- | --- | --- | ---: |
| grundschule | 60.0 s | 0.4 s | 349 |
| zweizuegig | 60.0 s | 5.8 s | 2798 |
| dreizuegig | 60.0 s | 12.4 s | 5285 |
| lock_in | 60.0 s | 0.6 s | 1289 |

CP-SAT runs 5x to 150x faster on every fixture and reaches the same hard-feasibility on every cell. The cost is the soft-score gap: 349 on grundschule (one full Doppelstunde of misplaced gaps and / or one or two FÖ-late-period violations), up to 5285 on dreizuegig. In Klassenzeit's pedagogical model that gap is meaningful: zero gaps per class per day and FÖ in the late part of the day are explicit quality predicates ([ADR 0023](0023-home-room-preference.md), [ADR 0024](0024-avoid-last-period.md), [ADR 0025](0025-subject-preference-weights.md), `backend/src/klassenzeit_backend/scheduling/quality_checks.py`); a soft sum near zero is the difference between a schedule that passes those predicates and one that does not.

The rule's tiebreak ordering (soft-score before wall-clock) reflects that priority. CP-SAT remains the right choice for a deployment where wall-clock matters more than soft-score (interactive re-solving, low-resource environments) and for fixtures stiffer than the current four where LAHC variants might begin to flake on the hard gate. `KZ_SOLVER_BACKEND=cpsat` switches in one env-var; ADR 0030 records the dependency-direction decision that keeps that switch a one-env-var change.

## Consequences

- All four backends remain available behind `KZ_SOLVER_BACKEND`; switching is a one-env-var change at deploy time, no code release.
- `backend/tests/core/test_settings.py::test_solver_backend_default_is_production_choice` continues to pin `lahc_rr_kempe`. Flipping the default in a future bake-off refresh remains a one-commit change touching `core/settings.py:56` and the assertion in lockstep.
- `solver/CLAUDE.md` and `backend/CLAUDE.md` cross-reference this ADR alongside the existing ADR 0031 mention so future readers can find the corrected-data confirmation.
- ADR 0031's table is preserved in git history but stale: its `lahc_rr` soft-sum-0 row reflects pre-item-37 data and should not be cited as a reference. ADR 0032's table is the canonical post-correction record. ADR 0031 stays Accepted and is implicitly superseded only on the bench-data axis; the bigger architectural framing (four backends, decision rule, reversibility) carries over unchanged.
- Future bake-off refreshes (when a new fixture or backend lands) re-apply the rule against the refreshed table. If a future fixture distinguishes the LAHC variants in a new way, or if a structural fix unmasks more silent variation, the data picks the next winner; if Kempe regresses, the default flips back to `lahc` (since the corrected `lahc_rr` is `lahc` plus noise) or to `cpsat` (if soft-score parity arrives there).

## Reversibility

Same as ADR 0031: the default flip is one line in `backend/src/klassenzeit_backend/core/settings.py:56`. To revert: change the literal back. No data migration, no schema change, no client-side change. Operators override per-deployment via `KZ_SOLVER_BACKEND` without waiting for an ADR.
