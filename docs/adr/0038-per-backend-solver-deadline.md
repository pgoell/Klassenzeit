# 0038: Per-backend solver deadline configuration

- **Status:** Accepted
- **Date:** 2026-05-13

## Context

`Settings.solve_deadline_ms: int | None = 5000` (env var `KZ_SOLVE_DEADLINE_MS`) was raised to 5000 ms in ADR 0033 so hard problems get LAHC headroom while easy ones still complete in under 100 ms via the canonical-floor early-exit. The same scalar was shared across every backend in `solver_backend ∈ {"lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"}`. The four backends have materially different wall-clock characteristics: the three LAHC variants converge well at 5000 ms per ADR 0033 and ADR 0037 (the production-default flip to `lahc_rr`), but CP-SAT routinely overshoots 60 s on dreizügig (post-item-74 bench data) and 5000 ms forces it into a wedged "no solution" state on the multi-school siblings.

The active sprint's solver-driven teacher assignment widens the LAHC search space (`teacher_candidates` becomes a decision variable per ADR 0036), and CP-SAT's overshoot is structural under that widened search. Sharing one deadline across backends has three failure modes: raising the scalar to accommodate CP-SAT wastes LAHC wall-clock and slows route-handler latency, keeping it at 5000 ms wedges CP-SAT, and any future per-backend tuning is impossible without a config-shape change first.

## Decision

- Replace the scalar `solve_deadline_ms` with `solve_deadline_ms_by_backend: dict[SolverBackend, int]` on `Settings`, keyed by the same `SolverBackend` Literal type that `solver_backend` uses.
- Defaults: `{"lahc": 5000, "lahc_rr": 5000, "lahc_rr_kempe": 5000, "cpsat": 120000}`. LAHC variants inherit the ADR 0033 budget; CP-SAT gets 120 s as a first-pass refresh against the post-item-74 bench data.
- Four per-backend env vars (`KZ_SOLVE_DEADLINE_MS_LAHC`, `KZ_SOLVE_DEADLINE_MS_LAHC_RR`, `KZ_SOLVE_DEADLINE_MS_LAHC_RR_KEMPE`, `KZ_SOLVE_DEADLINE_MS_CPSAT`) override individual entries; unset entries inherit the default. The legacy single `KZ_SOLVE_DEADLINE_MS` env var is dropped without a backward-compatible alias.
- Resolution happens at the route-handler boundary in `scheduling/routes/schedule.py`: each handler reads `settings.solve_deadline_ms_by_backend[settings.solver_backend]` and threads the resulting int into `solver_io.run_solve(..., deadline_ms=...)`. `run_solve`'s signature is unchanged.
- Two pydantic model validators enforce the shape: a `@model_validator(mode="before")` reads the four env vars and merges them into the dict, surfacing invalid values as `ValidationError` naming the offending env var; a `@model_validator(mode="after")` asserts every `SolverBackend` Literal variant has a key in the resolved dict, so a future backend addition that forgets to update `_PER_BACKEND_DEADLINE_DEFAULTS` fails loudly at startup rather than raising `KeyError` deep in a route handler.

## Consequences

Operationally: `backend/.env.test` flips from one zero scalar to four per-backend zeros so every backend stays greedy-only in CI; `backend/.env.example` mirrors the new shape with documenting comments; the deploy / staging compose layer renames any reference to the old scalar to the matching per-backend var; the test-override pattern moves from `monkeypatch.setattr(app.state.settings, "solve_deadline_ms", N)` to `monkeypatch.setitem(app.state.settings.solve_deadline_ms_by_backend, "<backend>", N)`. The matching rule in `backend/CLAUDE.md` is updated to reflect the new shape.

Maintenance: a future backend addition to the `SolverBackend` Literal must be added to `_PER_BACKEND_DEADLINE_DEFAULTS` and `_PER_BACKEND_DEADLINE_ENV_VARS` in the same edit, or `_check_per_backend_deadlines_complete` surfaces a `ValidationError` at startup. The defaults choice is first-pass and per-backend re-tuning is now a one-line edit to the `default_factory`; future bench refreshes that move CP-SAT's converged budget update the default here in lockstep with the bench data.

Citation chain: ADR 0033 (raised scalar to 5000 ms), ADR 0037 (production-default backend flip to `lahc_rr`), this ADR (per-backend dict).

## Reversibility

Collapse the dict back to a scalar plus a backend-keyed constant inside `run_solve` if per-backend tuning ever proves load-bearing the wrong way. Concretely: revert the three commits that introduced the dict (settings + validators, route-handler resolution, ADR / OPEN_THINGS / CLAUDE.md update) and re-add the single `KZ_SOLVE_DEADLINE_MS` env var. Reversibility cost: same shape as the forward migration, three commits. No data migration; `solve_deadline_ms_by_backend` is a runtime-only setting.
