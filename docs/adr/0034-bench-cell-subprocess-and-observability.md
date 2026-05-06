# 0034: Bench cell-subprocess architecture and observability columns

Date: 2026-05-06

## Status

Accepted.

## Context

The solver bake-off bench (`mise run bench:bakeoff`, `solver-bench`) was a single Rust process that ran every `(fixture, backend)` cell in sequence and emitted one markdown table to `BENCH_RESULTS.md`. Production-default decisions on `Settings.solver_backend` need three additional cells per row: peak resident-set size during the cell, wall-clock to first feasible incumbent, and wall-clock to the run's final soft score. The naive shape "one bench process, `getrusage(RUSAGE_SELF).ru_maxrss` after each in-process LAHC cell" produces monotonic-cumulative numbers across cells: cell N's reported peak is `max(over cells 1..=N)`, not cell N's actual peak. Cells run in size order grundschule, zweizuegig, dreizuegig, lock_in; under monotonic-max, dreizuegig's peak hides lock_in's peak. Cross-backend RAM trade-offs become illegible.

## Decision

Reorganise `solver-bench` into a supervisor and per-cell child process via recursive self-spawn:

- Supervisor (default mode): parses CLI, spawns one `solver-bench --cell <fixture> --backend <name> --budget <d> --seeds <n>` child per cell via `std::env::current_exe()`, captures stdout, parses a `CellResult` JSON object, formats one markdown row per cell, writes the file.
- Cell-child mode (`--cell ...`): runs the seed loop for one (fixture, backend) pair, reads its own peak via `libc::getrusage(libc::RUSAGE_SELF)`, prints one `CellResult` JSON object on stdout, exits.

LAHC stats (`time_to_first_feasible_ms`, `time_to_optimal_ms`) come from a new `solver_core::solve_with_config_stats` that returns `(Solution, SolveStats)`. The existing `solve_with_config` becomes a one-line wrapper that discards stats; production callers (the no-config `solve()` entry, `solve_json_with_config`, the solver-py binding, the backend `solver_io.py`) stay byte-identical.

CP-SAT stats come from a `cp_model.CpSolverSolutionCallback` (first feasible) and `solver.WallTime()` (final at OPTIMAL); the python module reports its own peak RSS via `resource.getrusage(resource.RUSAGE_SELF)`. The output JSON gains three additive fields: `peak_rss_kb`, `time_to_first_feasible_ms`, `time_to_optimal_ms`.

The bench cell-child reads the python child's `peak_rss_kb` from its stdout JSON and takes the max across the seed loop for the cpsat row's `Peak RSS (kB)` column.

## Consequences

Positive:

- Per-cell peak RSS is honest and cross-backend comparable. cpsat's ~50 MB python footprint vs LAHC's sub-MB working set is now legible per fixture.
- LAHC's time-to-first-feasible and time-to-optimal are visible per cell. Together with the existing `Soft score (median, feasible)` column they make ADR 0032 production-default revisits and ADR 0033 deadline tunings fact-based.
- Production callers are unchanged. `solve_with_config` keeps its signature; `SolveStats` is opt-in via `solve_with_config_stats`.

Negative:

- One additional process spawn per cell (~5 ms); invisible against multi-second cell budgets.
- New `libc` dep in `solver-bench`. Deviates from solver/CLAUDE.md "no external runtime deps for solver-bench". Accepted because `libc` is foundational (Rust org-maintained) and the alternatives (`/proc/self/status` parse) are Linux-only and string-fragile.
- `ru_maxrss` units differ across OS (Linux: kilobytes; macOS: bytes). Bench normalises to kilobytes with `cfg!(target_os = "macos")` division; documented in the markdown footer.
- LAHC `time_to_optimal_ms` is the wall-clock of the last running-best improvement, not a proof-of-optimality timestamp. LAHC has no proof of optimality; the field is a lower bound on the iteration count to the final soft score. Documented on the field rustdoc.

## Alternatives considered

1. Read `/proc/self/status:VmHWM` instead of `getrusage`. Linux-only, string-fragile. Rejected.
2. Read `RUSAGE_CHILDREN` deltas around python subprocess invocations. Subtle delta semantics, harder to test than the python-self-report path. Rejected.
3. Fork the cell from inside the supervisor process. Avoids re-exec but introduces unsafe `libc::fork` and pipe-based stats transfer. Rejected.
4. Per-seed records in the cell-child JSON. Forces the supervisor to re-implement the median helpers. Rejected; aggregated `CellResult` is sufficient.

## References

- OPEN_THINGS item 30 (`docs/superpowers/OPEN_THINGS.md`).
- Spec: `docs/superpowers/specs/2026-05-06-bench-observability-columns-design.md`.
- Plan: `docs/superpowers/plans/2026-05-06-bench-observability-columns.md`.
- ADR 0029 (bake-off methodology), ADR 0030 (cpsat layout), ADR 0031 (production default), ADR 0032 (default revisit), ADR 0033 (daily caps + deadline raise).
