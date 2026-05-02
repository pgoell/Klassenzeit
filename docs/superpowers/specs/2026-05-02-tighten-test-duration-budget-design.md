# Tighten `.test-duration-budget` from 600 s to 120 s

**Date:** 2026-05-02
**Status:** Design approved (autopilot autonomous mode).

## Context

PR-2 (#152, "perf(test): profile and shrink the 20-minute backend pytest CI job") shipped on 2026-04-30. It dropped the Python pytest wall-clock from ~1215 s to ~25 s by rewriting `test_solve_json_releases_gil` and caching the Alembic migration via a Postgres template DB. The wall-clock gate (`.test-duration-budget`) was set generously at 600 s while the new floor settled.

Eight master runs have landed since (#153 through #160). Wall-clock per master Test job, in chronological order: 25, 23, 25, 25, 25, 25, 26, 27 s. Mean ~25 s, max 27 s, std ~1 s. The floor is stable.

OPEN_THINGS row under "CI / repo automation":

> **Tighten `.test-duration-budget`.** PR-2 (2026-04-30) set the budget at 600 s as a generous floor against the pre-PR 1215 s baseline. The first post-merge CI run came in at 27 s for the Python test step. Lower the budget to ~120 s in a single-line PR once two or three more master runs confirm the new floor; repeat after later changes shift the floor again.

The "two or three more master runs" precondition is satisfied (eight have landed). This PR closes the first half of the directive.

## Goal

One commit, one PR:

1. `.test-duration-budget`: `600` to `120`.
2. `docs/superpowers/OPEN_THINGS.md`: shorten the "Tighten `.test-duration-budget`" row to its forward-looking half ("repeat after later changes shift the floor"), so the recurring follow-up stays visible without the now-completed pre-condition text.

## Choice of 120 s

`120 s = ~4.4x` the observed master-run max (27 s). Tolerates GitHub-runner noise (postgres image-pull jitter, runner-allocation cold cache) and one PR's worth of new test work, while still tripping the gate on a category-scale regression (e.g., a feature lands a batch of slow seed-solvability tests collectively adding ~100+ s). Tighter values (60 s, 90 s) invite ratchet-bump churn on every test-heavy PR; looser values (180 s+) silently tolerate a 5x+ regression and defeat the gate.

The gate semantic stays "regression alarm, not deadline": the budget ceils at a multiple of the actual floor, mirroring the `.coverage-baseline` ratchet shape (which floors at the actual baseline, with an absolute floor of 80 percent below it).

## Non-goals

- **No script for automated ratcheting.** A "median of last N master runs to budget" helper is speculative; revisit if manual ratcheting becomes rework.
- **No ADR.** A budget number is operational config; ADR shape is reserved for load-bearing decisions.
- **No bump to `.coverage-baseline` or related ratchets.** Out of scope.

## Verification

- `pre-push` lefthook runs `uv run pytest`; local wall-clock typically lands well under 120 s on the dev machine, so the gate never trips on a clean local push.
- CI's "Check Python test wall-clock budget" step in `ci.yml` reads the new value on the next master run and reports `Pytest wall-clock: ~25s (budget: 120s)`, well below the new ceiling.
- No new test or workflow change is needed; the gate already exists.
