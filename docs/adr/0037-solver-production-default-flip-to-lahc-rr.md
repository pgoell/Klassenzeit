# 0037: Solver production-default flip to `lahc_rr`

- **Status:** Accepted
- **Date:** 2026-05-13

## Context

ADR 0031 picked `Settings.solver_backend = "lahc_rr_kempe"` as a Kempe-superset tiebreak when `lahc_rr` and `lahc_rr_kempe` reached soft-score sum = 0 on the canonical refresh. ADR 0032 reaffirmed against post-item-37 corrected bench data. Since then the canonical objective and the algorithm have shifted on every axis the decision rule reads:

- Item 49 ranked R&R recreate rooms by home_room delta.
- Item 52 maintains `state.canonical_score` in lockstep with placements.
- Item 57 widened the canonical objective with `max_per_class_spread` and `max_per_class_interior_gaps` at production weights 10/10.
- Item 67 added the `prefer_class_teacher` axis.
- Item 76 fixed LAHC drift on `class_day_balance` dedup.
- Item 78 added the LAHC R&R rescue path for FFD-unplaced lessons under widened candidates.
- Items 79 and 80 fixed FFD picker capacity and lesson ordering for unpinned mode.

The 2026-05-12 `BENCH_RESULTS.md` refresh reflects all of the above on the pinned cross-section; the unpinned section is pre-item-78 and item 47's body documents the post-item-78 5s × 1-seed smoke for the multi-school siblings. OPEN_THINGS item 47 marked the revisit trigger SATISFIED and asked for an ADR reasoning from per-component vectors per `solver/CLAUDE.md`'s item-51 rule.

## Decision

`Settings.solver_backend = "lahc_rr"`. Flip from `lahc_rr_kempe`.

## Rationale

Decision rule (verbatim from ADR 0029 / 0031 / 0032):

1. Hard gate: any backend with a `0/N` cell on any fixture is rejected.
2. Tiebreak: feasibility rate, then median soft-score across feasible cells, then median total wall-clock.

Pinned (`BENCH_RESULTS.md`, 2026-05-12):

| Backend | grundschule | zweizuegig | dreizuegig | lock_in | Hard gate |
| --- | :-: | :-: | :-: | :-: | :-: |
| lahc | 20/20 | 20/20 | 20/20 | 20/20 | pass |
| lahc_rr | 20/20 | 20/20 | 20/20 | 20/20 | pass |
| lahc_kempe | 20/20 | 20/20 | 20/20 | 20/20 | pass |
| lahc_rr_kempe | 20/20 | 20/20 | 20/20 | 20/20 | pass |
| cpsat | 20/20 | 20/20 | **0/20** | 20/20 | **rejected** |

Unpinned (post-item-78 smoke at 5s × 1 seed for rescue-equipped backends, pre-item-78 production cells for the others, documented in OPEN_THINGS item 47):

| Backend | grundschule | zweizuegig | dreizuegig | lock_in | Hard gate |
| --- | :-: | :-: | :-: | :-: | :-: |
| lahc | 20/20 | **0/20** | **0/20** | **0/20** | **rejected** |
| lahc_rr | 20/20 | 1/1 (smoke) | 1/1 (smoke) | 1/1 (smoke) | pass (smoke) |
| lahc_kempe | 20/20 | **0/20** | **0/20** | **0/20** | **rejected** |
| lahc_rr_kempe | 20/20 | 1/1 (smoke) | 1/1 (smoke) | 1/1 (smoke) | pass (smoke) |
| cpsat | 20/20 | 18/20 | **0/20** | 20/20 | **rejected** |

Survivors after both hard-gate passes: `lahc_rr`, `lahc_rr_kempe`.

Tiebreak on median soft-score across feasible cells per-fixture, summed over the four pinned fixtures:

| Backend | grundschule | zweizuegig | dreizuegig | lock_in | Sum |
| --- | ---: | ---: | ---: | ---: | ---: |
| lahc_rr | 22 | 199 | 1133 | 315 | **1669** |
| lahc_rr_kempe | 23 | 221 | 1157 | 324 | 1725 |

`lahc_rr` is lower on every pinned fixture: grundschule by 1, zweizuegig by 22, dreizuegig by 24, lock_in by 9. Sum delta 56 (1725 minus 1669) across 80 seeds, and the per-fixture direction is consistent.

Per-component breakdown (per `solver/CLAUDE.md` item 51's per-component-vectors rule):

- `class_gap_h`, `teacher_gap_h`: both at 0 across all four pinned fixtures for every LAHC variant.
- `home_room_miss`: Kempe marginally better on dreizuegig (138 vs 139); tied elsewhere.
- `day_balance`: Kempe marginally better on dreizuegig (19 vs 20); tied elsewhere.
- Quality predicates (worst_spread, worst_home_room_ratio, total_interior_gaps, late_period_ratio): identical 14/16 across the four LAHC variants.

`lahc_rr`'s aggregate soft-score edge comes from the axes the bench table does not break out as standalone columns (subject_pref components, `max_per_class_spread`, `max_per_class_interior_gaps`). The aggregate `Soft score` column is the weighted sum under `PRODUCTION_ACTIVE_WEIGHTS`; the direction is small but consistent.

ADR 0031's tiebreak ("Kempe is a strict superset of R&R-only search") was load-bearing when both backends reached sum 0; today `lahc_rr` is decisively better on every fixture and the strict-superset argument no longer overrides the data.

## CP-SAT trade-off

CP-SAT fails the hard gate on dreizuegig under both pinned and unpinned shapes. The wall-clock advantage from ADR 0032 still holds for any operator who configures `KZ_SOLVER_BACKEND=cpsat` as a deployment override (interactive re-solving, low-resource environments, fixtures stiffer than the current four where LAHC variants might begin to flake on the hard gate). The trade-off framing carries over unchanged.

## Cost-of-widening (unpinned variant)

The unpinned cross-section captures the cost of widening teacher decision variables per ADR 0036:

- `lahc` and `lahc_kempe`: plateau at FFD-greedy stage on multi-school siblings (zweizuegig `hard_med=2`, dreizuegig `hard_med=13`, lock_in `hard_med=1`). No rescue mechanism; LAHC alone cannot lift FFD-unplaced lessons.
- `lahc_rr` and `lahc_rr_kempe` post-item-78 LAHC R&R rescue: lift the multi-school siblings to `feasibility 1/1 hard_med=0` at the 5s × 1-seed smoke shape.
- `cpsat`: recovers zweizuegig (18/20 feasibility, peak RSS 989 MB) and lock_in (20/20 feasibility, peak RSS 233 MB) at the cost of wall-clock budget overruns, but still loses dreizuegig (0/20 feasibility, peak RSS 2.36 GB).

The unpinned hard gate gives the same `{lahc_rr, lahc_rr_kempe}` survivor set as pinned; the unpinned soft-score tiebreak at production cell shape is the open question and is filed as a follow-up in OPEN_THINGS.

## Consequences

- All four backends remain available behind `KZ_SOLVER_BACKEND`; switching is a one-env-var change at deploy time, no code release.
- `backend/tests/core/test_settings.py::test_solver_backend_default_is_production_choice` pins `lahc_rr`. Flipping again in a future bake-off refresh is a one-commit change touching `core/settings.py:56` and the assertion in lockstep.
- `backend/CLAUDE.md` and `solver/CLAUDE.md` cite this ADR alongside ADR 0031 and ADR 0032 so a future reader can trace the verdict.
- ADRs 0031 and 0032 remain Accepted; this ADR supersedes them only on the bench-data axis. The decision-rule framing and reversibility envelope carry over unchanged.
- The pinned-data verdict is post-everything; the unpinned-data verdict relies on a 5s × 1-seed smoke for rescue-equipped backends. A production-refresh follow-up (filed alongside this ADR in OPEN_THINGS) closes the loop with full-shape unpinned data after Sprint 2 lands or sooner.

## Reversibility

The flip is one line in `backend/src/klassenzeit_backend/core/settings.py:56` plus its matching assertion in `backend/tests/core/test_settings.py:160`. Operators override per-deployment via `KZ_SOLVER_BACKEND` without waiting for an ADR.

## Ratification (2026-05-15 refresh)

The 2026-05-14 to 2026-05-15 production refresh (60s × 20 seeds × 4 fixtures × 5 backends × 2 pin variants) is the first refresh after PR #266 (block-aware LAHC `Change` + `Swap`), which closed the unpinned multi-school feasibility gap for all four LAHC variants. The decision rule re-runs against the refreshed data below and `lahc_rr` holds; this section captures the trace so a future reader of this ADR does not have to re-derive it from `BENCH_RESULTS.md`.

Hard gate is tied. All four LAHC variants pass 20/20 on every fixture in both pin variants; CP-SAT remains rejected on dreizuegig (0/20 pinned and unpinned). The survivor set is the same `{lahc_rr, lahc_rr_kempe}` pair the original decision reasoned over.

Median soft-score across feasible cells, pinned:

| Backend | grundschule | zweizuegig | dreizuegig | lock_in | Sum |
| --- | ---: | ---: | ---: | ---: | ---: |
| lahc_rr | 32 | 189 | 1155 | 304 | 1680 |
| lahc_rr_kempe | 32 | 184 | 1141 | 304 | **1661** |

Unpinned:

| Backend | grundschule | zweizuegig | dreizuegig | lock_in | Sum |
| --- | ---: | ---: | ---: | ---: | ---: |
| lahc_rr | 32 | 239 | 1346 | 315 | 1932 |
| lahc_rr_kempe | 32 | 236 | 1318 | 323 | **1909** |

`lahc_rr_kempe` edges `lahc_rr` by 19 pinned (-1.13%) and 23 unpinned (-1.19%) on aggregate sum. Per-fixture direction is not consistent: pinned grundschule and lock_in tie; pinned zweizuegig and dreizuegig favour Kempe by 5 and 14 respectively. Unpinned grundschule ties; unpinned zweizuegig and dreizuegig favour Kempe by 3 and 28; unpinned lock_in favours `lahc_rr` by 8 (+2.5%).

Per-component breakdown on the only fixture (dreizuegig) where the LAHC variants exit at non-zero canonical:

- `class_gap_h`, `teacher_gap_h`: 0 across all four pinned fixtures for both backends.
- `home_room_miss`: pinned dreizuegig `lahc_rr` 138 vs `lahc_rr_kempe` 139 (`lahc_rr` -1).
- `day_balance`: pinned dreizuegig `lahc_rr` 21 vs `lahc_rr_kempe` 18 (Kempe -3).
- Quality predicate sets are identical across the four pinned fixtures (`5/5`, `5/5`, `4/5`, `4/5` on both backends).

Stddev tiebreaker on pinned soft-score across fixtures: `lahc_rr_kempe` 430 vs `lahc_rr` 435 (-1.2%). Like the sum delta, this sits inside the bench's documented host-sensitivity noise.

Verdict: `lahc_rr` holds. The 1.1-1.2% aggregate edge to Kempe is inside the noise band; the per-fixture direction is mixed (`lahc_rr` wins lock_in unpinned, Kempe wins multi-school, ties elsewhere); the per-component vectors are mixed (`lahc_rr` -1 on `home_room_miss`, Kempe -3 on `day_balance`, gap-h axes tied at 0); the stddev tiebreaker is also in noise band. `solver/CLAUDE.md`'s production-default reasoning rule asks for a consistent direction across all fixtures from per-component vectors before flipping; the refresh does not satisfy that bar, and flipping the default twice in three days (2026-05-13 to 2026-05-15) on a noise-band signal would be operator-facing churn for no net quality gain.

The reversibility envelope above is unchanged: the next refresh after items 86 / 87 (LAHC dreizuegig tuning) is the natural next point to re-ask the question.
