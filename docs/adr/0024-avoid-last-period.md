# 0024: Avoid-last-period soft constraint

- **Status:** Accepted
- **Date:** 2026-05-01

## Context

Sprint item 8 (algorithm phase, P1) of the "Realer Schulalltag" sprint. The previous algorithm-phase work (PR-9c, ADR 0017) introduced two subject-level pedagogy axes: `prefer_early_periods` (linear in `tb.position`) and `avoid_first_period` (binary at `tb.position == 0`). The mirror at the other end of the day was deliberately deferred. The dreizuegige seed and the Hessen Grundschule pedagogy reference both call for "Hauptfächer früh, also nicht in der letzten Stunde": Mathematik and Deutsch should avoid the last period of the day, where end-of-day fatigue is highest. The existing avoid-first axis covers the wakeup-cold edge but not this one.

## Decision

1. **`avoid_last_period: bool` on `Subject`.** Additive Alembic migration, server-side default `FALSE`, NOT NULL. Wire format additive end-to-end (Pydantic, Zod, OpenAPI, solver JSON via `#[serde(default)]`).
2. **`avoid_last_period: u32` axis on `ConstraintWeights`.** Per-placement binary penalty: a placement at `tb.position == max_position_for_day` for that placement's `day_of_week` contributes `weights.avoid_last_period` once. The active-default `solve()` weight is `1`, alongside the existing five axes.
3. **Per-day max-position lookup.** `score_solution`, the lowest-delta greedy in `solve_with_config`, and the LAHC Change-move delta path each build a `HashMap<u8, u8>` from `problem.time_blocks` once per call (folding `max(position)` per `day_of_week`) and pass the per-placement value into `subject_preference_score`. The function gains a `max_position_for_day: u8` parameter; allocation-free.
4. **Demo seeds mark Mathematik and Deutsch as avoid-last.** Same two Hauptfächer the seed already marks `prefer_early_periods=True`. The flag carries through all three demo fixtures (Grundschule, zweizuegig, dreizuegig) via the shared `_SubjectSpec` table.

## Alternatives considered

- **Single global max position.** Simpler, but wrong for asymmetric Hessen schedules where Halbtag days end earlier than Ganztag days. Per-day max captures the actual user-meaningful "last period of *this* day" semantics.
- **Inline avoid-last logic in `score_solution` only.** Splits the per-placement axis logic across two sites (avoid_first inside `subject_preference_score`, avoid_last in the score loop), making the LAHC delta path's symmetric old-vs-new score call awkward. Threading `max_position_for_day` through the helper keeps all three axes unified.
- **Bundle with sprint item 9 (configurable per-subject weights, P2).** Risks dragging a P1 over a sprint boundary; the OPEN_THINGS rule "structural and behavioural changes never ship in the same commit" reinforces keeping each axis its own additive change.

## Consequences

Easier: parity with `avoid_first_period`, no new public-API reshape required, BASELINE.md refresh is optional (per-placement scoring is `O(placements)` with hoisted lookups, well inside the 20% sprint budget; the home-room PR confirmed the same shape needs no refresh). Harder: every literal `Subject { ... }` and `ConstraintWeights { ... }` test fixture across solver-core, solver-py tests, and benches gains one new field; future axis additions compound the maintenance cost. Revisit once sprint item 9 (configurable weights) lands and per-axis weights become a real user knob.
