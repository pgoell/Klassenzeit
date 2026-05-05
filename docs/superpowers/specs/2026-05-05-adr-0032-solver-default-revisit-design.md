# Solver production-default revisit (ADR 0032 + corrected bench data) spec (active sprint, item 29)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Bench prevention phase: item 29.
**Goal.** Refresh `solver/solver-core/benches/BENCH_RESULTS.md` against the post-item-37 solver state, re-apply the ADR 0029 / ADR 0031 decision rule against the corrected data, and write ADR 0032 with the corrected verdict. If the verdict flips the production default, update `core/settings.py` and `tests/core/test_settings.py` in lockstep.

**Non-goal.** No new bench columns (`peak_memory_kb`, `time_to_first_feasible_ms`, `time_to_optimal_ms` stay queued behind item 30). No schedule-quality metrics in bake-off output (item 31). No backend-aware `solve_deadline_ms` (item 34, blocked on this item per the OPEN_THINGS note). No new bake-off backends or fixtures. No changes to the bake-off harness itself; item 28's placement-count gate stays as is.

## Context

ADR 0031 (`docs/adr/0031-solver-production-default.md`, Accepted 2026-05-05) picked `Settings.solver_backend = "lahc_rr_kempe"` as the production default off the canonical bench output committed in PR #181. The decision rule there:

1. Hard gate: any backend with a `0/N` cell on any fixture is rejected.
2. Tiebreak: feasibility rate, then median soft-score across feasible cells, then median total wall-clock.

PR #181's table showed every backend at 80/80 cells, `lahc_rr` and `lahc_rr_kempe` tied at soft-score sum = 0, ADR 0031 broke the tie toward `lahc_rr_kempe` because Kempe is a strict superset of R&R-only search.

That bench output is now known to be wrong. Items 26+27 (R&R anchor filter + property tests, PR #183), 28 (placement-count gate in the bake-off harness, PR #184), and 37 (R&R row-keyed rollback, PR #186) all landed AFTER PR #181:

- Item 26 surfaced a silent placement-drop in `rr_collect_anchors` that affected `lahc_rr` and `lahc_rr_kempe` rows on `grundschule`, `zweizuegig`, and `dreizuegig` fixtures.
- Item 28 added a per-cell `placements_total < expected` gate to the bake-off harness; PRE-fix it could not detect the drop because the harness only checked `hard_violations.is_empty()`.
- Item 37 fixed the residual silent placement-drop in `rr_attempt`'s rollback path that survived items 26 + 27.

The dev-loop receipt at `--budget 5s --seeds 4 --fixtures grundschule` post-item-37 already shows `lahc_rr` and `lahc_rr_kempe` at `placements_med=45/45 feasibility 4/4` (matching `lahc`); zweizügig sanity at the same budget shows `lahc_rr_kempe` at `196/196 soft_med=0`. This is the corrected solver state. The canonical bench at production settings (`--budget 60s --seeds 20`, all fixtures × all backends) has not yet been re-run. ADR 0031's verdict therefore rides on partly-corrupted data.

Item 29 closes that loop: re-run the canonical bench, write ADR 0032 with whatever the corrected data says, flip the production default in lockstep with the assertion pin if the verdict changes.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 29. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run). Reproduces on master tip e9897fa.

## Scope

**In scope.**

- Re-run `mise run bench:bakeoff` at canonical settings (`--budget 60s --seeds 20`, all four fixtures × all four backends; default args satisfy this). Wall-clock approximately 4.5 hours on the AMD Ryzen 7 3700X recording host. Replaces `solver/solver-core/benches/BENCH_RESULTS.md` in place (the harness writes the file directly).
- Run `mise run solver:rebuild` BEFORE the bench so the cpsat backend's `score_solution_json` Python binding sees the post-item-37 solver state. Bench-binding rule per `solver/CLAUDE.md`: cpsat invokes `python3 -m klassenzeit_solver.cpsat` as a subprocess, and `klassenzeit_solver` is a maturin-built editable wheel. Pure-Rust LAHC backends do not need the rebuild step but the LAHC plus cpsat columns share one bench run.
- Write `docs/adr/0032-solver-production-default-revisit.md` covering: (1) context (PR #181 data was rendered stale by items 26 / 28 / 37 landing after it), (2) the corrected bench table excerpt, (3) verbatim re-application of the ADR 0029 decision rule, (4) verdict (hold or flip), (5) consequences (settings change if flip; CLAUDE.md cross-reference updates either way). Title format: `# 0032: Solver production-default revisit` (colon, no em-dash, per the recent `.claude/CLAUDE.md` ADR rule).
- If verdict flips: update `backend/src/klassenzeit_backend/core/settings.py:56` (the `solver_backend` Literal default), update `backend/tests/core/test_settings.py::test_solver_backend_default_is_production_choice`'s assertion, update `solver/CLAUDE.md`'s "Production default per ADR 0031" cross-reference, update `backend/CLAUDE.md`'s `KZ_SOLVER_BACKEND` paragraph's "default `lahc_rr_kempe` per ADR 0031" cross-reference. All four sites in the same commit (atomic lockstep change).
- If verdict holds: still cross-reference ADR 0032 from `solver/CLAUDE.md` and `backend/CLAUDE.md` so future readers find the corrected-data confirmation, but leave the default and the assertion untouched.
- `docs/adr/README.md` index updated with the ADR 0032 entry.
- `docs/superpowers/OPEN_THINGS.md`: delete item 29; the active-sprint header's "next pickup" line repoints to item 30 (the next P0 in the observability phase).
- `solver/CLAUDE.md`'s bench paragraph adds a one-line cross-reference to ADR 0032 in the "Production default per ADR 0031" sentence.
- PR body excerpts the corrected vs. PR #181 table to make the regression magnitude visible to reviewers.

**Out of scope.**

- New bench columns (`peak_memory_kb`, `time_to_first_feasible_ms`, `time_to_optimal_ms`). Item 30; the verdict rule rides on existing columns only.
- Schedule-quality metrics in bake-off output. Item 31.
- Solvability test mirroring the production route flow. Item 32.
- Backend-aware `solve_deadline_ms` (`Settings.solve_deadline_ms_by_backend`). Item 34. The OPEN_THINGS note on item 34 says it should land "only after item 29 if `cpsat` becomes a real production option"; this PR explicitly does not promote cpsat unless the verdict rule does so.
- `collect_pinned_placements` filter intent. Item 33; backend tidy phase.
- Re-recording `solver/solver-core/benches/BASELINE.md` (criterion bench). The criterion bench is unchanged by items 26 / 28 / 37 (R&R is not in BASELINE.md per `solver/CLAUDE.md`'s "Two artifacts, two purposes" rule). Algorithm-phase regression budget continues to ride on the existing committed file.
- Refactor of `bake-off`'s output rendering. The committed harness at `solver-bench/src/main.rs` writes the markdown table directly; no shape change wanted.

## Decision rule (verbatim re-application)

The rule below is copied from ADR 0029 §"Decision rule" / ADR 0031 §"Rationale". Re-applied without modification against the corrected `BENCH_RESULTS.md`. ADR 0029's `peak_memory_kb` etc. were planned columns; the rule today operates only on the columns that exist:

1. **Hard gate.** Any backend with a `0/N` cell on any fixture is rejected.
2. **Tiebreak 1.** Higher feasibility rate (count of `N/N` cells across all fixtures) wins.
3. **Tiebreak 2.** Lower soft-score sum across feasible cells wins.
4. **Tiebreak 3.** Lower median total wall-clock across feasible cells wins. Wall-clock is host-sensitive; an in-noise tie at this level falls back to the most-recently-shipped backend (ADR 0031's `lahc_rr_kempe`).

Two outcomes are realistic:

- **Hold.** Items 26 / 28 / 37 fixed silent placement drops without changing which backend wins on soft-score; ADR 0031's tie at soft-score = 0 between `lahc_rr` and `lahc_rr_kempe` reproduces, Kempe holds the tie-break for the same superset-of-search reason.
- **Flip toward `lahc`.** The corrected R&R rows now match `lahc`'s placement count but the bake-off shows `lahc` reaches soft-score = 0 too at canonical budget. The tiebreak then collapses to wall-clock; if `lahc` lands materially faster than R&R variants on whole-school fixtures, ADR 0032 flips the default.
- **Flip toward `cpsat`.** Unlikely because PR #181's table showed cpsat at soft-score sum 9721 vs. zero for the LAHC variants; this PR does not expect that to change.

The verdict is what the rule says, not what we predict before running the bench. The "Hold" outcome is filed first in this list because items 26 / 28 / 37 fix bugs that masked LAHC backends' real numbers; once unmasked, all three LAHC variants are likely to converge on soft-score = 0 at 60s budget, leaving the wall-clock tiebreak as the discriminator.

## Failure modes

- **Bench errors midway.** Subprocess crash, OOM, transient. The wrapper captures stderr; on non-zero exit, inspect the log, re-run after fixing the cause. Partial output is overwritten by a fresh run; no need to manually clean up.
- **0/N cell on a backend.** That is a real outcome the rule consumes; the backend is gated out of the verdict. Do not regenerate to chase green numbers; the data is the data. ADR 0032 documents the gated cell with the date and host so a future refresh can cross-check.
- **Bench wall-clock balloons past the autopilot session's budget.** Bench is a background process; the autopilot session writes the spec, plan, and ADR skeleton in parallel and stalls at the table-fill step until the bench completes. If the user halts the run, the partial log identifies which fixture / backend started showing trouble and the spec / plan / ADR skeleton are committed independently so no work is lost.
- **In-noise wall-clock tie.** Feasibility and soft-score columns are host-stable per `solver/CLAUDE.md`; the wall-clock column is host-sensitive. If the only meaningful discriminator left is wall-clock, ADR 0032 footnotes the recording host and falls back to the most-recently-shipped backend (`lahc_rr_kempe`) per the rule's stability bias.
- **Stale `klassenzeit_solver` editable wheel.** Mitigated by the `mise run solver:rebuild` step that runs before the bench; cpsat then sees the post-item-37 binding.

## Test plan

- `mise run lint` green (covers ruff, ty, vulture, clippy, machete, cargo fmt, biome, actionlint, the CLAUDE.md drift gates, and `scripts/check_unique_fns.py`).
- `mise run test` green (Rust + Python + frontend). Specifically: `backend/tests/core/test_settings.py::test_solver_backend_default_is_production_choice` passes; if the verdict flips, the assertion's expected value changes and the new value passes.
- Spot-check the regenerated `BENCH_RESULTS.md`: every cell shows `feasibility = N/N` for a non-rejected backend; the host footer's date and rustc line update; the table includes the cpsat row.
- The PR body cites the regenerated table and ADR 0032 verdict; the spec and plan are linked from the PR body.

## Reversibility

- ADRs are immutable per `docs/adr/README.md`. ADR 0032 is the new canonical decision; ADR 0031 stays unchanged but is implicitly superseded.
- Default-flip reversibility: same as ADR 0031's footer. One line in `core/settings.py` plus the matching assertion. No data migration, no schema change, no client-side change. The frontend has no backend-default coupling; the dev / staging / prod environments override via `KZ_SOLVER_BACKEND` if a different backend is needed before a future ADR 0033.
- Bench data: `solver/solver-core/benches/BENCH_RESULTS.md` regenerates on demand via `mise run bench:bakeoff`. The committed file is replaceable; git history retains every prior version for cross-reference.
