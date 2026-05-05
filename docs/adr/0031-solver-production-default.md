# 0031: Solver production-default backend

- **Status:** Accepted
- **Date:** 2026-05-05

## Context

ADR 0029 framed the solver feasibility bake-off as a four-candidate comparison; Sprints 1 to 4 shipped the four backends (`lahc`, `lahc_rr`, `lahc_rr_kempe`, `cpsat`). The Sprint 4 commit (PR #179) shipped a smoke `BENCH_RESULTS.md` at `--budget 30s --seeds 5`; the production default needed the canonical bench shape (`--budget 60s --seeds 20` per ADR 0029) before it could be picked.

The smoke bench reported `cpsat` as 0/5 on the dreizuegige fixture and the Sprint 4 follow-up was filed as a "is this a budget or model issue?" investigation. The investigation isolated the cause: a CP-SAT model-encoding bug in `_emit_non_overlap` made every multi-class lesson group (per-Jahrgang Religion trio: 3 lessons each spanning the same 3 classes) infeasible by construction. The fix landed earlier in this PR (`fix(solver-py): cpsat class non-overlap dedups multi-class lesson groups`); the canonical refresh after the fix shows every cell at 20/20.

## Decision

The production default is `Settings.solver_backend = "lahc_rr_kempe"`.

## Rationale

The decision rule (per spec `docs/superpowers/specs/2026-05-04-solver-bakeoff-followup-design.md`):

1. Hard gate: any backend with a `0/N` cell on any fixture is rejected. Reliability is the bake-off's first-order goal.
2. Tiebreak: feasibility rate, then median soft-score across feasible cells, then median total wall-clock.

Applied to the canonical refresh:

| Backend | 80/80? | Soft-score sum | Wall-clock | Verdict |
| --- | :-: | ---: | --- | --- |
| `lahc` | yes | 314 | deadline (60s) | rejected on soft-score |
| `lahc_rr` | yes | 0 | deadline (60s) | tied with winner |
| `lahc_rr_kempe` | yes | 0 | deadline (60s) | **chosen** |
| `cpsat` | yes | 9721 | 0.4 to 12.5s | rejected on soft-score |

The tie between `lahc_rr` and `lahc_rr_kempe` resolves to `lahc_rr_kempe` because Kempe is a strict superset of R&R-only search (Sprint 3 composed Kempe atop R&R; both periods coprime so each iter picks at most one move type). The bench shows zero cost on current fixtures; the broader search is the safer pick for Beyond-Grundschule fixtures (Sek I, Gymnasium, Gesamtschule) where constraints are stiffer.

## Dreizuegige CP-SAT investigation

Smoke bench (Sprint 4, ADR 0030) reported 0/5 on dreizuegig at 30s budget. Probing CP-SAT directly with longer budgets (5s, 30s) showed `status = INFEASIBLE` returned in roughly 1.4 s wall-clock, NOT a timeout. The Religion-trio fixture shape (3 multi-class lessons co-placed via `lesson_group_id`) made each shared class accumulate `sum = 3` against `class_non_overlap.sum <= 1`. The lesson-group co-placement constraint forces all members equal, so the model was infeasible by construction. The fix in `_emit_non_overlap` (this PR) deduplicates by `(class_id, lesson_group_id)`. Post-fix, dreizuegige cpsat is 5/5 at 60s on 5 seeds (median 11.85s wall-clock) and 20/20 at the canonical refresh (median 12.54s wall-clock).

## Consequences

- All four backends remain available behind `KZ_SOLVER_BACKEND`. Switching is a one-env-var change at deploy time, no code release.
- `backend/tests/core/test_settings.py` pins the chosen default as a regression guard; flipping the default in a future bake-off refresh is a one-commit change touching `core/settings.py` and the assertion in lockstep.
- `solver/CLAUDE.md`'s bench paragraph drops the obsolete "venv pre-activation required" sentence and points at this ADR for the production-default rationale.
- Future bake-off refreshes (when a new fixture or backend lands) re-apply the rule against the refreshed table; if a future fixture distinguishes `lahc_rr` from `lahc_rr_kempe`, the data picks a winner; if Kempe regresses, the default flips back to `lahc_rr`.

## Reversibility

The default flip is one line in `backend/src/klassenzeit_backend/core/settings.py:56`. To revert: change the literal back. No data migration, no schema change.
