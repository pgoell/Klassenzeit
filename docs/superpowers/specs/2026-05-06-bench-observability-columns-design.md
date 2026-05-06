# Add `peak_memory_kb`, `time_to_first_feasible_ms`, `time_to_optimal_ms` columns to BENCH_RESULTS.md (active sprint, item 30)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Observability phase: item 30.
**Goal.** `solver/solver-core/benches/BENCH_RESULTS.md` carries 9 columns today, all about feasibility (`Feasibility`, `Hard violations`, `Placements (median / expected)`) and quality (`Soft score`) plus two wall-clock columns (`FFD wall-clock`, `Total wall-clock`). Production-default decisions (ADR 0032 picked `lahc_rr_kempe`; ADR 0033 raised `solve_deadline_ms` from 200 ms to 5000 ms) need three additional cells per cell: peak resident-set size during the cell, wall-clock to first feasible incumbent, and wall-clock to the run's final soft score. Add those three columns and the supporting probes in `solver-core` and `klassenzeit_solver.cpsat`.

**Non-goal.** Not aligning the LAHC inner-loop optimisation objective with the full cost (item 41 closed reporting; the loop still optimises the slice). Not adding schedule-quality columns (median gaps, max spread, home-room ratio, late-period FÖ ratio); that is item 31. Not refreshing `BENCH_RESULTS.md` at production cell shape (`--budget 60s --seeds 20`, ~4.5 h wall-clock); a low-budget shape demo lands with the tooling, the production refresh is queued as item 42 in the sprint-tidy phase. No backend, frontend, or solver-py public-API change beyond the additive cpsat output JSON fields.

## Context

The OPEN_THINGS item 30 bullet sketches three columns and how to capture each. The brainstorm (`/tmp/kz-brainstorm/brainstorm.md` for this run) refined the sketch into a buildable design. Key refinements:

- The naive shape "one bench process, `RUSAGE_SELF.ru_maxrss` after each in-process LAHC cell" produces monotonic-cumulative numbers across cells; cell N's reported peak is `max(over cells 1..=N)`, not cell N's actual peak. Cells run in the order grundschule (small), zweizuegig (medium), dreizuegig (large), lock_in (small); under monotonic-max, dreizuegig's peak hides lock_in's peak. To get honest per-cell numbers every cell must run in a fresh process.
- The honest path is recursive-self: the existing `solver-bench` binary becomes a supervisor that spawns one cell-child per `(fixture, backend)` pair via `Command::new(env!("CARGO_BIN_EXE_solver-bench"))` (or `std::env::current_exe()` for the runtime binary). Cell-child runs the seed loop in-process for LAHC, spawns python per seed for cpsat, reads its own peak via `getrusage(RUSAGE_SELF).ru_maxrss` at exit, and emits a single `CellResult`-shaped JSON object on stdout.
- The cpsat python child reports its own peak via `resource.getrusage(resource.RUSAGE_SELF).ru_maxrss` immediately before `sys.stdout.write(...)`. The bench cell-child reads it back from the JSON and takes the max across the seed loop. Symmetric with how LAHC's cell-child reports its own peak; avoids `RUSAGE_CHILDREN` delta orchestration on the Rust side.
- LAHC `time_to_first_feasible_ms`: the existing LAHC outer-loop predicate at `solver-core/src/lahc.rs:171` (`state.soft_score == 0 && placements.len() == placements_expected`) drives early-exit. The probe slots in next to it: record `start.elapsed()` to `stats.time_to_first_feasible_ms` the first iteration the predicate is true. If the FFD greedy is already feasible at LAHC entry (placements.len() == placements_expected, no violations), the probe records `Some(0.0)` immediately. If LAHC never reaches feasibility (R&R cannot rescue today; tracked as item 20), the probe stays `None`.
- LAHC `time_to_optimal_ms`: the LAHC loop never declares OPTIMAL; the running-best soft score stops improving at some iteration. Track the wall-clock of the last accepted move that improved the running-best `state.soft_score`. The probe sits at every accept site in `lahc.rs::run` (Change move at `try_change_move`, R&R at `rr_attempt`, Kempe at `kempe_attempt`). When `state.soft_score < running_best`, update both `running_best` and `stats.time_to_optimal_ms = Some(start.elapsed())`.
- cpsat `time_to_first_feasible_ms` and `time_to_optimal_ms`: `cp_model.CpSolverSolutionCallback` fires on every improving feasible incumbent. First fire's `solver.WallTime()` is `time_to_first_feasible_ms`. CP-SAT's objective is `Minimize(0)` (per ADR 0030), so any feasible solution is also optimal in the model; the callback's first invocation establishes both ttf and tto. When status is OPTIMAL, set `time_to_optimal_ms = solver.WallTime() * 1000.0`. When status is FEASIBLE without OPTIMAL or INFEASIBLE/UNKNOWN, set `time_to_optimal_ms = None`.

The brainstorm also closed the wire-shape question: rather than break `solve_with_config`'s signature, add a new internal `solve_with_config_stats(problem, config) -> Result<(Solution, SolveStats), Error>`. Existing `solve_with_config` becomes a one-line wrapper that calls the stats variant and discards the stats. `SolveStats { time_to_first_feasible_ms: Option<f64>, time_to_optimal_ms: Option<f64> }` is the new public type. Bench imports `solver_core::solve_with_config_stats`; production callers (the `solve()` no-config entry, `solve_json_with_config`, the solver-py binding, and the backend `solver_io.py`) stay byte-identical.

`BENCH_RESULTS.md` itself: low-budget refresh in this PR (`--budget 5s --seeds 4`, ~5 min) so the committed file demonstrates the new column shape; production refresh queued as item 42. The footer gains a "shape demo" addendum that the next regeneration overwrites.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 30. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run). Companion ADR: `docs/adr/0034-bench-cell-subprocess-and-observability.md` (load-bearing because it changes the bench architecture from single-process to supervisor + cell-child).

## Scope

**In scope.**

- New public Rust API in `solver-core`:
    - `pub struct SolveStats { pub time_to_first_feasible_ms: Option<f64>, pub time_to_optimal_ms: Option<f64> }` in `solver-core/src/types.rs`. `Default` derive, all fields `None`.
    - `pub fn solve_with_config_stats(problem: &Problem, config: &SolveConfig) -> Result<(Solution, SolveStats), Error>` in `solver-core/src/solve.rs`. Returns the existing `Solution` plus the new stats.
    - `pub fn solve_with_config(problem: &Problem, config: &SolveConfig) -> Result<Solution, Error>` keeps its signature; body becomes `solve_with_config_stats(problem, config).map(|(s, _)| s)`.
    - `pub use SolveStats;` re-export at the crate root (`solver-core/src/lib.rs`).
- LAHC probe in `solver-core/src/lahc.rs::run`:
    - New parameter `stats: &mut SolveStats`.
    - At entry, if `placements.len() == placements_expected && state.soft_score == 0` (FFD already feasible), set `stats.time_to_first_feasible_ms = Some(0.0)`.
    - In the per-iteration tail (after `lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.soft_score`):
        - If `stats.time_to_first_feasible_ms.is_none()` and the predicate at line 171 holds, set `stats.time_to_first_feasible_ms = Some(start.elapsed().as_secs_f64() * 1000.0)`.
        - Track `running_best: u32` initialised to `state.soft_score` at function entry. After the per-iteration update of `state.soft_score`, if `state.soft_score < running_best`, set `running_best = state.soft_score; stats.time_to_optimal_ms = Some(start.elapsed().as_secs_f64() * 1000.0)`.
    - The early-exit predicate at line 171 stays unchanged.
- LAHC running-best initialisation: `solve_with_config_stats` initialises `stats.time_to_optimal_ms = Some(0.0)` if FFD greedy already produced a feasible solution with `state.soft_score == 0`. Mirror with `time_to_first_feasible_ms`.
- New Python module surface in `klassenzeit_solver.cpsat`:
    - `_FirstSolutionCallback(cp_model.CpSolverSolutionCallback)` records `solver.WallTime()` to `self.first_ms` on first invocation.
    - `solve_cpsat_json(...)` passes the callback to `solver.solve(model, callback)`.
    - Output JSON gains three additive fields:
        - `peak_rss_kb: int` (always present, from `resource.getrusage(resource.RUSAGE_SELF).ru_maxrss`; on macOS divided by 1024 to normalise; on Linux already kilobytes).
        - `time_to_first_feasible_ms: float | null` (set when status is OPTIMAL or FEASIBLE).
        - `time_to_optimal_ms: float | null` (set only when status is OPTIMAL).
- New `solver-bench` architecture:
    - Add `libc = "0.2"` workspace dep; add `serde = { workspace = true, features = ["derive"] }`. Both bumped at workspace root if not already pinned.
    - `CellResult` derives `Serialize, Deserialize`. New fields: `peak_kb: u64`, `time_to_first_feasible_ms_median: Option<f64>`, `time_to_optimal_ms_median: Option<f64>`.
    - Two CLI surfaces share `main`:
        - Supervisor (default): parses existing CLI, spawns one cell-child per `(fixture, backend)` cell via `std::env::current_exe()`, captures stdout, parses `CellResult`, formats markdown row.
        - Cell-child (`--cell <fixture> --backend <backend>` plus `--budget`/`--seeds`): runs the seed loop, computes `CellResult`, reads its own `getrusage(RUSAGE_SELF).ru_maxrss`, attaches as `peak_kb`, prints the JSON to stdout, exits.
    - LAHC cell: cell-child uses `solve_with_config_stats`; tracks per-seed `time_to_first_feasible_ms` and `time_to_optimal_ms`, computes median over feasible seeds.
    - cpsat cell: cell-child parses the python child's JSON output for `peak_rss_kb`, `time_to_first_feasible_ms`, `time_to_optimal_ms` per seed; computes max for `peak_kb` and median over feasible seeds for the two timing columns.
- Markdown table refresh:
    - `write_header` adds three columns to the right: `Peak RSS (kB)`, `Time to first feasible (ms, median)`, `Time to optimal (ms, median)`.
    - `write_row` formats them: peak as integer kB, ttf/tto as `{:.0}` ms or `-` when None.
    - `write_footer` gains a one-line note explaining the cell-subprocess architecture and the units (Linux kB, macOS bytes-then-normalised).
- Tests:
    - `solver-bench/src/main.rs#tests`: `parse_args_reads_cell_mode_flags`, `write_row_renders_observability_columns`, `cell_result_round_trips_json`, `write_header_includes_three_new_columns`.
    - `solver-bench/tests/end_to_end.rs`: spawns `cargo run -p solver-bench -- --budget 200ms --seeds 1 --fixtures grundschule --out /tmp/kz-bench-test-<nonce>.md` and asserts the resulting markdown contains the three new column headers and at least one numeric value in each new column for the grundschule/lahc row.
    - `solver-core/tests/lahc_property.rs`: `lahc_stats_ttf_le_tto_le_total` proptest. Generates a small problem, runs `solve_with_config_stats`, asserts `ttf <= tto <= total_ms` whenever both ttf and tto are `Some`.
    - `solver-core/src/solve.rs#tests`: `solve_with_config_stats_returns_zero_ttf_when_greedy_is_feasible`, `solve_with_config_stats_returns_none_when_unfeasible`, `solve_with_config_stats_records_running_best_improvement`.
    - `solver/solver-py/tests/test_cpsat.py`: `test_solve_cpsat_json_emits_observability_fields_when_optimal`, `test_solve_cpsat_json_omits_tto_when_not_optimal`.
- Low-budget bench refresh: `mise run bench:bakeoff -- --budget 5s --seeds 4` produces a 12-column `BENCH_RESULTS.md` shape demo. Footer addendum: `_Shape demo at low budget/seeds; production refresh queued as OPEN_THINGS item 42._`. Commit alongside the tooling.
- ADR `docs/adr/0034-bench-cell-subprocess-and-observability.md`. Records:
    - The supervisor + cell-child architecture and its monotonic-max motivation.
    - The libc dep deviation from solver/CLAUDE.md "no external runtime deps for solver-bench".
    - The Linux/macOS `ru_maxrss` unit normalisation.
    - The choice to keep `solve_with_config` byte-identical and route stats through a sibling fn.
- OPEN_THINGS:
    - Delete item 30 from the active sprint observability phase.
    - Update the active-sprint preamble's "next pickup" line to item 31 (schedule-quality metrics).
    - Refine item 42's text to note the new columns are in place; the production refresh now produces 12 columns.
    - Leave item 19 (historical reference) unchanged.
- solver/CLAUDE.md addendum:
    - One bullet under "Bench workflow" recording the supervisor + cell-child architecture and the libc dep.
- Auto-memory `project_roadmap_status.md` refresh: item 30 shipped, next pickup is item 31.

**Out of scope.**

- Schedule-quality columns (item 31). Separate scope; needs Python-Rust glue for `quality_checks.py`.
- Production-budget bench refresh (`--budget 60s --seeds 20`, ~4.5 h). Item 42, sprint-tidy phase.
- Reading `RUSAGE_CHILDREN` deltas around python subprocess invocations. The python self-report path covers the cpsat peak; `RUSAGE_CHILDREN` adds nothing.
- Per-seed records in the cell-child JSON. The supervisor only consumes the aggregated `CellResult`; per-seed records would force the supervisor to re-implement the median helpers already inside the cell-child.
- Schedule-quality predicate failures flagged in `BENCH_RESULTS.md`. Item 31.
- `solve_json_with_config` / `klassenzeit_solver.solve_json_with_config` exposing stats to Python. cpsat handles its own stats; LAHC stats stay internal to `solver-bench`.

## Code change

`solver/solver-core/src/types.rs` (additive at the bottom of the public types section):

```rust
/// Optional timing probes produced by [`crate::solve_with_config_stats`].
/// Populated by the LAHC loop and the FFD greedy entry-check; consumers
/// (today: `solver-bench`) median or aggregate across seed runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SolveStats {
    /// Wall-clock from `solve_with_config_stats` entry to first feasible
    /// incumbent. `Some(0.0)` when FFD greedy is already feasible at LAHC
    /// entry. `None` when the run never reaches feasibility.
    pub time_to_first_feasible_ms: Option<f64>,
    /// Wall-clock from `solve_with_config_stats` entry to the last
    /// running-best improvement. `Some(0.0)` when FFD greedy is already at
    /// `state.soft_score == 0` and feasible. `None` when no LAHC iteration
    /// improved the running-best (or LAHC was not run because deadline is
    /// `None`).
    pub time_to_optimal_ms: Option<f64>,
}
```

`solver/solver-core/src/solve.rs` (signature split):

```rust
pub fn solve_with_config(problem: &Problem, config: &SolveConfig) -> Result<Solution, Error> {
    solve_with_config_stats(problem, config).map(|(sol, _)| sol)
}

pub fn solve_with_config_stats(
    problem: &Problem,
    config: &SolveConfig,
) -> Result<(Solution, SolveStats), Error> {
    let mut stats = SolveStats::default();
    let solve_start = std::time::Instant::now();
    // ... existing body, with `&mut stats` threaded into crate::lahc::run ...
    // After greedy, before LAHC:
    let placements_expected: usize = problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as usize)
        .sum();
    if solution.violations.is_empty() && solution.placements.len() == placements_expected {
        stats.time_to_first_feasible_ms = Some(0.0);
        if state.soft_score == 0 {
            stats.time_to_optimal_ms = Some(0.0);
        }
    }
    crate::lahc::run(
        problem,
        &idx,
        config,
        &mut solution.placements,
        &mut state,
        &pinned,
        &class_max_lessons_per_day,
        &mut stats,            // new
        solve_start,           // new
    );
    // ... existing post-LAHC validators ...
    solution.soft_score = score::score_solution(...);
    Ok((solution, stats))
}
```

`solver/solver-core/src/lahc.rs::run` signature gains `stats: &mut SolveStats, solve_start: std::time::Instant`. Inside the loop, replace the existing `start = Instant::now()` with `solve_start` so all wall-clock samples share one origin. Add the running-best track:

```rust
let mut running_best = state.soft_score;
if running_best == 0 && placements.len() == placements_expected {
    if stats.time_to_first_feasible_ms.is_none() {
        stats.time_to_first_feasible_ms = Some(0.0);
    }
    if stats.time_to_optimal_ms.is_none() {
        stats.time_to_optimal_ms = Some(0.0);
    }
}
// ... existing loop body ...
iter += 1;
lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.soft_score;
if stats.time_to_first_feasible_ms.is_none()
    && state.soft_score == 0
    && placements.len() == placements_expected {
    stats.time_to_first_feasible_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
}
if state.soft_score < running_best {
    running_best = state.soft_score;
    stats.time_to_optimal_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
}
if state.soft_score == 0 && placements.len() == placements_expected {
    break;
}
```

`solver/solver-core/src/lib.rs` adds `pub use types::SolveStats;` and `pub use solve::solve_with_config_stats;` next to the existing re-exports.

`solver/solver-py/python/klassenzeit_solver/cpsat.py`:

```python
import resource

class _FirstSolutionCallback(cp_model.CpSolverSolutionCallback):
    def __init__(self) -> None:
        super().__init__()
        self.first_ms: float | None = None

    def on_solution_callback(self) -> None:
        if self.first_ms is None:
            self.first_ms = self.WallTime() * 1000.0


def solve_cpsat_json(
    problem_json: str,
    deadline_ms: int | None,
    seed: int = 1,
) -> str:
    # ... existing body up to solver.solve(model) ...
    callback = _FirstSolutionCallback()
    status = solver.solve(model, callback)
    peak_rss_kb = _read_peak_rss_kb()
    if status in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        placements = _extract_placements(solver, anchor_vars, meta)
        soft_score = score_solution_json(problem_json, json.dumps(placements))
        ttf = callback.first_ms
        tto = solver.WallTime() * 1000.0 if status == cp_model.OPTIMAL else None
        return json.dumps({
            "placements": placements,
            "violations": [],
            "soft_score": int(soft_score),
            "peak_rss_kb": peak_rss_kb,
            "time_to_first_feasible_ms": ttf,
            "time_to_optimal_ms": tto,
        })
    if status in (cp_model.INFEASIBLE, cp_model.UNKNOWN):
        # ... existing violations build ...
        return json.dumps({
            "placements": [],
            "violations": violations,
            "soft_score": 0,
            "peak_rss_kb": peak_rss_kb,
            "time_to_first_feasible_ms": None,
            "time_to_optimal_ms": None,
        })
    # ... existing MODEL_INVALID / unexpected branches ...

def _read_peak_rss_kb() -> int:
    raw = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return raw // 1024 if sys.platform == "darwin" else raw
```

`solver/solver-bench/Cargo.toml` gains `libc = "0.2"` and the existing `serde_json` plus a new `serde = { workspace = true, features = ["derive"] }`. The workspace `Cargo.toml` may need to add `libc` to `[workspace.dependencies]`; check at implementation time.

`solver/solver-bench/src/main.rs` reorganises into:

```rust
fn main() -> ExitCode {
    let raw: Vec<String> = env::args().skip(1).collect();
    if matches!(raw.first().map(|s| s.as_str()), Some("--cell")) {
        return run_cell_child(raw);
    }
    run_supervisor(raw)
}
```

`run_cell_child` parses `--cell <fixture> --backend <backend> --budget <d> --seeds <n>`, runs the existing seed loop (now via `solve_with_config_stats` for LAHC; via the existing python subprocess shape for cpsat with the new JSON parser), reads its own peak via `libc::getrusage(libc::RUSAGE_SELF, ...)`, prints the JSON `CellResult`, exits.

`run_supervisor` parses `--budget`/`--seeds`/`--fixtures`/`--out`, walks `(fixture, backend)` cells, spawns `Command::new(env::current_exe().unwrap()).args([...])`, reads stdout, parses `CellResult` via `serde_json::from_str`, formats the markdown row with the three new columns, writes the file.

Existing `run_cell` and `run_cpsat_cell` move into the cell-child path. The supervisor no longer calls them directly.

## Test changes

Inline tests (`solver/solver-bench/src/main.rs#tests`):

```rust
#[test]
fn parse_cell_args_reads_fixture_backend_budget_seeds() { /* ... */ }

#[test]
fn write_row_renders_observability_columns() {
    let cell = CellResult {
        // ... existing fields ...
        peak_kb: 49152,
        time_to_first_feasible_ms_median: Some(0.4),
        time_to_optimal_ms_median: Some(1500.0),
    };
    let mut out = String::new();
    write_row(&mut out, "grundschule", BenchBackend::Lahc, &cell);
    assert!(out.contains("| 49152 |"));
    assert!(out.contains("| 0 |") || out.contains("| 0.4 |"));
    assert!(out.contains("| 1500 |"));
}

#[test]
fn cell_result_round_trips_through_json() { /* ... */ }

#[test]
fn write_header_includes_three_new_columns() { /* ... */ }
```

Integration test (`solver/solver-bench/tests/end_to_end.rs`):

```rust
#[test]
fn supervisor_emits_three_new_columns_in_markdown() {
    let out = std::env::temp_dir().join(format!("kz-bench-{}.md", std::process::id()));
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget", "200ms",
            "--seeds", "1",
            "--fixtures", "grundschule",
            "--out", out.to_str().unwrap(),
        ])
        .status()
        .expect("spawn supervisor");
    assert!(status.success());
    let body = std::fs::read_to_string(&out).expect("read markdown");
    assert!(body.contains("Peak RSS (kB)"));
    assert!(body.contains("Time to first feasible"));
    assert!(body.contains("Time to optimal"));
    let _ = std::fs::remove_file(&out);
}
```

Property test (`solver/solver-core/tests/lahc_property.rs`):

```rust
proptest! {
    #[test]
    fn lahc_stats_ttf_le_tto_le_total(problem in lahc_small_problem(), seed in 0u64..1024) {
        let cfg = SolveConfig {
            weights: ConstraintWeights { class_gap: 1, teacher_gap: 1, ..Default::default() },
            seed,
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2000),
            ..SolveConfig::default()
        };
        let start = std::time::Instant::now();
        let (_sol, stats) = solve_with_config_stats(&problem, &cfg).expect("solve");
        let total_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let (Some(ttf), Some(tto)) = (stats.time_to_first_feasible_ms, stats.time_to_optimal_ms) {
            prop_assert!(ttf <= tto + 1e-6, "ttf {} > tto {}", ttf, tto);
            prop_assert!(tto <= total_ms + 50.0, "tto {} > total+50ms {}", tto, total_ms + 50.0);
        }
    }
}
```

(50ms slack absorbs the gap between `solve_with_config_stats` entry and the outer test's `Instant::now()`.)

Inline tests (`solver/solver-core/src/solve.rs#tests`): three small unit tests for the FFD-feasible-immediately, LAHC-improves, and never-feasible cases.

Python tests (`solver/solver-py/tests/test_cpsat.py`):

```python
def test_solve_cpsat_json_emits_observability_fields_when_optimal():
    problem = _small_feasible_problem()
    out = json.loads(solve_cpsat_json(json.dumps(problem), deadline_ms=5000, seed=1))
    assert "peak_rss_kb" in out and isinstance(out["peak_rss_kb"], int) and out["peak_rss_kb"] > 0
    assert isinstance(out["time_to_first_feasible_ms"], float) and out["time_to_first_feasible_ms"] >= 0.0
    assert isinstance(out["time_to_optimal_ms"], float) and out["time_to_optimal_ms"] >= 0.0
    assert out["time_to_first_feasible_ms"] <= out["time_to_optimal_ms"] + 1e-6


def test_solve_cpsat_json_omits_tto_when_not_optimal():
    problem = _infeasible_problem()
    out = json.loads(solve_cpsat_json(json.dumps(problem), deadline_ms=200, seed=1))
    assert out["peak_rss_kb"] > 0
    assert out["time_to_first_feasible_ms"] is None
    assert out["time_to_optimal_ms"] is None
```

`_small_feasible_problem` and `_infeasible_problem` are small fixtures; reuse existing helpers in `test_cpsat.py` if available.

## Bench impact

Criterion bench (`mise run bench`):

- `solve_with_config_stats` is what the bench harness now calls; `solve_with_config` is unchanged. The criterion bench (`solver-core/benches/solver_fixtures.rs`) calls `solve_with_config` directly per OPEN_THINGS item 15; its hot path is unchanged.
- LAHC's hot path gains: one `Option::is_none()` check per iteration plus one `Instant::elapsed` call per feasibility transition (at most once per solve) and one `Instant::elapsed` call per running-best improvement (at most a few hundred per solve, bounded by O(distinct soft scores in the run)). Cost per solve is well under 100 µs total.
- Expected criterion drift: <1% on grundschule, <1% on zweizuegig, <1% on dreizuegig. Refresh `BASELINE.md` only if observed drift exceeds 3%. Note: the criterion bench is currently blocked on item 15, so a `mise run bench:record` cannot complete end-to-end. Verify drift on the partial output before the abort.

Bake-off bench (`mise run bench:bakeoff`):

- Cell-child subprocess spawn adds ~5 ms per cell (cargo-run overhead is amortised by `cargo run --release` reusing the cached binary; subsequent invocations reuse the binary). Total overhead per refresh: 4 fixtures × 4 backends × 5 ms = 80 ms, invisible against the multi-hour cell budget.
- Cell-child startup and JSON serialise at end add another ~5 ms per cell. Same bound.
- Production refresh wall-clock unchanged at ~4.5 h.

## Commit plan

1. `feat(solver-core): SolveStats with ttf/tto probes via solve_with_config_stats (item 30)`. Adds `SolveStats` to `types.rs`, `solve_with_config_stats` to `solve.rs`, threads stats through `lahc.rs::run`. Inline tests for the three FFD/LAHC stats cases. Property test in `lahc_property.rs`. `solve_with_config` becomes a one-liner over the stats variant; existing tests pass unchanged.
2. `feat(klassenzeit_solver): cpsat reports peak_rss_kb, ttf, tto in output JSON (item 30)`. Adds `_FirstSolutionCallback`, `_read_peak_rss_kb`, three new output fields. Python tests for the OPTIMAL and INFEASIBLE branches.
3. `feat(solver-bench): subprocess-per-cell mode + observability columns (item 30)`. Adds `libc` dep and `serde` derive feature. Splits `main` into supervisor and cell-child paths. `CellResult` derives serde and gains three fields. Markdown header and row formatter add three columns. Inline unit tests and end-to-end smoke.
4. `chore(solver): low-budget BENCH_RESULTS.md shape demo with new columns (item 30)`. Runs `mise run bench:bakeoff -- --budget 5s --seeds 4` locally; commits the resulting 12-column file with a footer addendum. Production refresh queued as item 42.
5. `docs(adr): 0034 bench cell-subprocess and observability`. New ADR plus index entry in `docs/adr/README.md`.
6. `docs: shrink OPEN_THINGS active sprint after item 30 ships`. Removes item 30 from the observability phase; advances the next-pickup line; refines item 42's text; refreshes auto-memory.
7. `docs(claude): document bench supervisor + cell-child architecture`. One bullet under "Bench workflow" in `solver/CLAUDE.md`. Also covers the libc dep deviation.

Each commit is independently buildable + lintable + passing pre-push tests. Steps 1-3 each contain TDD red-green-refactor (test first, implementation second; the test's failure mode is documented in the commit body).

## Risks

- **`ru_maxrss` unit drift Linux vs macOS.** Linux returns kilobytes; macOS returns bytes. Solver-bench's recorded host today is Linux; if a contributor runs the bench on macOS, the cell-child's libc-side normalisation handles it (`if cfg!(target_os = "macos") { ru_maxrss / 1024 } else { ru_maxrss }`). Documented in the ADR and the footer.
- **`solver.WallTime()` semantics in callback.** CP-SAT exposes `WallTime()` from inside `OnSolutionCallback`; our test asserts `time_to_first_feasible_ms >= 0.0 and <= deadline_ms`. If OR-Tools changes the callback contract in a future release (renames `WallTime`, returns nan), the test catches it.
- **Subprocess overhead skewing wall-clock columns.** Single-digit ms per cell, invisible against 60 s production cell budget. Verified by the end-to-end smoke timing.
- **Recursive self-spawn discovering its own binary.** `std::env::current_exe()` returns the running binary's path; `cargo run -p solver-bench` resolves to `target/release/solver-bench`. The `mise run bench:bakeoff` task wraps `cargo run` in `uv run`; the spawned cell-children inherit the venv via env vars. Verified by end-to-end smoke.
- **Deviation from solver/CLAUDE.md "no external runtime deps for solver-bench".** Adding `libc` dep. Justification: `libc` is foundational (Rust org-maintained crate, not third-party). Alternatives are `/proc/self/status` parse (Linux-only) or shelling out to `time -v` (fragile). Documented in the CLAUDE.md update.
- **`SolveStats` cascades into the 15+ `Problem { ... }` literal sites?** No: `SolveStats` is a return-only type, never constructed inline by callers. The cascade rule applies to `Problem`, `Subject`, `ConstraintWeights` only.
- **LAHC `time_to_optimal_ms` reports the time of the LAST improvement, not the FIRST proof of optimality.** LAHC has no proof of optimality; `time_to_optimal_ms` is a lower bound on the actual optimisation cost. Documented in the field rustdoc and in the ADR.
- **Property test seed sensitivity.** The proptest invariant `ttf <= tto <= total_ms` may flake under rare seeds where ttf and tto are recorded within microseconds of each other; the 1e-6 tolerance plus the 50 ms total-slack absorbs this. The 5x128 local sweep policy from solver/CLAUDE.md applies before commit.

## Acceptance criteria

- `mise run test:rust` green on the branch.
- `mise run test:py` green on the branch (covers the new cpsat tests).
- `mise run lint` green.
- New tests pass on the branch and fail on master (verified by checking out master, applying only the test commit, running the test suite for the relevant crate).
- `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule --out /tmp/...` produces a 12-column markdown table with the three new columns populated for the grundschule/lahc cell.
- `BENCH_RESULTS.md` committed with the new column shape and the shape-demo footer addendum.
- ADR 0034 committed and indexed in `docs/adr/README.md`.
- OPEN_THINGS item 30 deleted; next-pickup line advanced to item 31.
- Auto-memory `project_roadmap_status.md` refreshed; description field too.
- `solver/CLAUDE.md` carries the bench-supervisor architecture bullet and the libc dep note.
