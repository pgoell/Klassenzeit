# Bench supervisor resilience to per-cell panics implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Patch `solver-bench`'s supervisor so a per-cell-child failure logs to stderr, renders a `panic` placeholder row in the markdown output, and continues to the next cell. Supervisor exits 0 if at least one cell succeeded; non-zero only when every queued cell failed.

**Architecture:** Extract the cell-loop body inside `run_supervisor` to a pure function `render_cells(plan, runner, &mut markdown) -> usize` that takes a closure `runner` for the per-cell `Result<CellResult, String>`. Production wires the runner to `spawn_cell`; tests wire a synthetic runner that returns `Ok` / `Err` deterministically. New `write_error_row` helper renders a 17-column row with `panic` in the Feasibility cell and `-` everywhere else. Supervisor's exit-code derivation switches from "first error => FAILURE" to "zero successes after all cells attempted => FAILURE".

**Tech Stack:** Rust binary crate `solver-bench`, inline `#[cfg(test)] mod tests` in `src/main.rs`, `cargo nextest` for execution.

---

## File Structure

- **Modify**: `solver/solver-bench/src/main.rs` (single file, all changes)
    - Add `render_cells` (new pure function near the existing `run_supervisor`)
    - Add `write_error_row` (new helper next to `write_row`)
    - Update `run_supervisor` to call `render_cells` and derive exit code from the success count
    - Update `write_footer` to add one paragraph describing the `panic` token
    - Add three inline tests in `mod tests`: panic-row-and-continue, all-cells-panic, footer-includes-panic-doc
- **Modify**: `docs/superpowers/OPEN_THINGS.md` (delete item 46, mark item 42 unblocked)
- **Modify**: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md` (advance the "next pickup" pointer)

No new files. Single commit lands the implementation, test, footer addition, OPEN_THINGS bookkeeping, and auto-memory update.

---

## Task 1: Implementation, tests, and bookkeeping

**Files:**
- Modify: `solver/solver-bench/src/main.rs` (across `run_supervisor`, the new `render_cells` and `write_error_row`, `write_footer`, and the `mod tests` block)
- Modify: `docs/superpowers/OPEN_THINGS.md`
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`

**Steps:**

- [ ] **Step 1: Write the failing test for the panic-row + continue behaviour.**

Append to `#[cfg(test)] mod tests` in `solver/solver-bench/src/main.rs` (after the existing tests, before the closing brace). The test assumes `render_cells` exists with signature `fn render_cells<I, F>(plan: I, runner: &mut F, markdown: &mut String) -> usize where I: IntoIterator<Item = (&'static str, BenchBackend)>, F: FnMut(&str, BenchBackend) -> Result<CellResult, String>`.

```rust
fn synthetic_cell_for_resilience_tests(seeds: u64) -> CellResult {
    CellResult {
        seeds,
        feasibility_count: seeds,
        hard_violations_median: 0,
        placements_total_median: 45,
        placements_expected: 45,
        soft_score_median: Some(0),
        ffd_ms_median: 0.13,
        total_ms_median: 60_000.0,
        peak_kb: 49_152,
        time_to_first_feasible_ms_median: Some(1.0),
        time_to_optimal_ms_median: Some(2.0),
        worst_spread_median: None,
        worst_home_room_ratio_median: None,
        total_interior_gaps_median: None,
        late_period_ratio_median: None,
        quality_pass_count_median: None,
    }
}

#[test]
fn supervisor_renders_panic_row_and_continues_on_cell_error() {
    let plan = vec![
        ("grundschule", BenchBackend::Lahc),
        ("grundschule", BenchBackend::LahcRr),
        ("grundschule", BenchBackend::LahcRrKempe),
    ];
    let mut runner = |_name: &str, backend: BenchBackend| -> Result<CellResult, String> {
        if matches!(backend, BenchBackend::LahcRr) {
            Err("synthetic-panic: cell exited with non-zero".to_string())
        } else {
            Ok(synthetic_cell_for_resilience_tests(20))
        }
    };
    let mut markdown = String::new();
    let successes = render_cells(plan.into_iter(), &mut runner, &mut markdown);
    assert_eq!(successes, 2, "two cells should have succeeded");
    assert!(
        markdown.contains("| grundschule | lahc | 20 | 20/20 |"),
        "missing surviving lahc row: {markdown}"
    );
    assert!(
        markdown.contains("| grundschule | lahc_rr_kempe | 20 | 20/20 |"),
        "missing surviving lahc_rr_kempe row: {markdown}"
    );
    assert!(
        markdown.contains("| grundschule | lahc_rr | - | panic |"),
        "missing panic placeholder for failed cell: {markdown}"
    );
}

#[test]
fn supervisor_returns_zero_successes_when_every_cell_panics() {
    let plan = vec![
        ("grundschule", BenchBackend::Lahc),
        ("grundschule", BenchBackend::LahcRr),
    ];
    let mut runner = |_name: &str, _backend: BenchBackend| -> Result<CellResult, String> {
        Err("everything is on fire".to_string())
    };
    let mut markdown = String::new();
    let successes = render_cells(plan.into_iter(), &mut runner, &mut markdown);
    assert_eq!(successes, 0);
    let panic_row_count = markdown.matches("| panic |").count();
    assert_eq!(
        panic_row_count, 2,
        "every plan entry should render a panic row: {markdown}"
    );
}

#[test]
fn write_footer_documents_panic_token() {
    let mut out = String::new();
    write_footer(&mut out);
    assert!(
        out.contains("panic"),
        "footer must document the panic token: {out}"
    );
    assert!(
        out.contains("Feasibility"),
        "footer must reference the Feasibility column: {out}"
    );
}
```

- [ ] **Step 2: Run the new tests and verify they fail.**

Run: `cargo nextest run -p solver-bench --bin solver-bench supervisor_renders_panic_row supervisor_returns_zero_successes write_footer_documents_panic_token`

Expected: compile errors (missing `render_cells`, missing `write_footer` panic-token text), or build error referring to `render_cells` not in scope.

- [ ] **Step 3: Add `write_error_row` next to `write_row` in `solver/solver-bench/src/main.rs`.**

Insert immediately after the existing `write_row` function:

```rust
fn write_error_row(out: &mut String, fixture: &str, backend: BenchBackend, _reason: &str) {
    out.push_str(&format!(
        "| {fixture} | {backend} | - | panic | - | - | - | - | - | - | - | - | - | - | - | - | - |\n",
        backend = backend.label(),
    ));
}
```

The `_reason` argument is captured for forward compatibility but not rendered in the row (the diagnostic already went to stderr). The leading underscore silences `unused_variables`.

Column count check: 17 pipe-separated cells matching the 17 columns declared in `write_header` (Fixture, Backend, Seeds, Feasibility, Hard violations, Placements, Soft score, FFD wall-clock, Total wall-clock, Peak RSS, Time to first feasible, Time to optimal, Worst spread, Worst home-room ratio, Total interior gaps, Late-period ratio, Quality).

- [ ] **Step 4: Add `render_cells` near the existing `spawn_cell` function in `solver/solver-bench/src/main.rs`.**

Insert immediately after `spawn_cell` (which ends around line 331):

```rust
fn render_cells<I, F>(plan: I, runner: &mut F, markdown: &mut String) -> usize
where
    I: IntoIterator<Item = (&'static str, BenchBackend)>,
    F: FnMut(&str, BenchBackend) -> Result<CellResult, String>,
{
    let mut successes = 0usize;
    for (name, backend) in plan {
        eprintln!("cell start: {} / {}", name, backend.label());
        match runner(name, backend) {
            Ok(cell) => {
                eprintln!(
                    "cell done: {} / {} feasibility {}/{} hard_med={} placements_med={}/{} \
                     soft_med={} total_ms_med={:.0} peak_kb={} ttf_med={} tto_med={} \
                     worst_spread_med={} worst_home_med={} gaps_med={} late_med={} quality_med={}",
                    name,
                    backend.label(),
                    cell.feasibility_count,
                    cell.seeds,
                    cell.hard_violations_median,
                    cell.placements_total_median,
                    cell.placements_expected,
                    cell.soft_score_median
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    cell.total_ms_median,
                    cell.peak_kb,
                    cell.time_to_first_feasible_ms_median
                        .map(|v| format!("{:.0}", v))
                        .unwrap_or_else(|| "-".to_string()),
                    cell.time_to_optimal_ms_median
                        .map(|v| format!("{:.0}", v))
                        .unwrap_or_else(|| "-".to_string()),
                    cell.worst_spread_median
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    cell.worst_home_room_ratio_median
                        .map(|v| format!("{v:.2}"))
                        .unwrap_or_else(|| "-".to_string()),
                    cell.total_interior_gaps_median
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    cell.late_period_ratio_median
                        .map(|v| format!("{v:.2}"))
                        .unwrap_or_else(|| "-".to_string()),
                    cell.quality_pass_count_median
                        .map(|v| format!("{v}/4"))
                        .unwrap_or_else(|| "-".to_string()),
                );
                write_row(markdown, name, backend, &cell);
                successes += 1;
            }
            Err(reason) => {
                eprintln!("cell error: {} / {}: {reason}", name, backend.label());
                write_error_row(markdown, name, backend, &reason);
            }
        }
    }
    successes
}
```

The `eprintln!` for the success branch is the existing format string from `run_supervisor` lifted verbatim, so the live operator output remains byte-identical on the happy path.

- [ ] **Step 5: Rewire `run_supervisor` in `solver/solver-bench/src/main.rs`.**

Replace the body from `for (name, _build) in FIXTURES { ... }` (line 233) through the end of the inner success log (line 286 inclusive of `write_row(...)`) with:

```rust
let plan: Vec<(&'static str, BenchBackend)> = FIXTURES
    .iter()
    .filter(|(name, _)| args.fixtures.iter().any(|f| f == name))
    .flat_map(|(name, _)| BenchBackend::ALL.iter().map(move |b| (*name, *b)))
    .collect();
let cells_attempted = plan.len();

let mut runner = |name: &str, backend: BenchBackend| -> Result<CellResult, String> {
    spawn_cell(&exe, name, backend, args.budget, args.seeds)
};
let successes = render_cells(plan.into_iter(), &mut runner, &mut markdown);
```

Then replace the existing tail (the `write_footer`, `fs::write`, and final `ExitCode::SUCCESS`) with:

```rust
write_footer(&mut markdown);

if let Err(e) = fs::write(&args.out, &markdown) {
    eprintln!("solver-bench: failed to write {:?}: {e}", args.out);
    return ExitCode::FAILURE;
}
eprintln!("wrote {:?}", args.out);

if cells_attempted == 0 || successes >= 1 {
    ExitCode::SUCCESS
} else {
    ExitCode::FAILURE
}
```

The `cells_attempted == 0` short-circuit preserves today's behaviour for an empty fixtures queue (writes an empty markdown table, exits 0).

- [ ] **Step 6: Update `write_footer` in `solver/solver-bench/src/main.rs` to document the `panic` token.**

Insert this paragraph between the current "Late-period ratio is the median ... composite Quality column." paragraph (which ends with the line containing `subject has the axis enabled, and that case counts as pass for the composite Quality column.\n",`) and the "Home-room ratio exempts ..." paragraph.

Append two `out.push_str(...)` calls, mirroring the existing footer style:

```rust
out.push_str(
    "Cells whose subprocess fails (panic, non-zero exit, JSON parse error) render `panic` in the\n",
);
out.push_str(
    "Feasibility column with `-` in every other numeric column. The supervisor logs the underlying\n",
);
out.push_str("reason to stderr and continues to the next cell.\n\n");
```

- [ ] **Step 7: Run the unit tests and verify they pass.**

Run: `cargo nextest run -p solver-bench --bin solver-bench`

Expected: all existing tests + the three new tests pass. Total count: existing tests (`parse_duration_accepts_seconds_and_milliseconds`, `parse_supervisor_args_reads_all_flags`, `parse_supervisor_args_rejects_unknown_flag`, `parse_cell_args_reads_fixture_backend_budget_seeds`, `median_u32_returns_middle_value`, `write_header_includes_three_new_columns`, `write_row_renders_observability_columns`, `write_row_renders_dash_when_no_feasible_seed`, `cell_result_round_trips_through_json`, `write_header_includes_five_quality_columns`, `write_row_renders_quality_columns`, `write_row_renders_dash_when_quality_fields_are_none`, `placements_expected_for_problem_sums_hours_per_week`, `cpsat_subprocess_command_args_match_module_invocation`) plus the three new ones (`supervisor_renders_panic_row_and_continues_on_cell_error`, `supervisor_returns_zero_successes_when_every_cell_panics`, `write_footer_documents_panic_token`) = 17 tests, all green.

- [ ] **Step 8: Run the integration smoke and verify the happy-path is preserved.**

Run: `cargo nextest run -p solver-bench --test end_to_end`

Expected: `supervisor_emits_observability_and_quality_columns` passes. This proves the refactor preserved the wire shape and column headers under a real cell-spawn.

- [ ] **Step 9: Run lint to check for clippy / format issues.**

Run: `mise run lint:rust`

Expected: clean. If clippy flags `clippy::needless_pass_by_value` on `render_cells`'s `&mut F` runner or anywhere else, fix inline by adjusting the bound. The unused `_reason` param uses leading-underscore convention; `cargo machete` should not flag any deps because no deps are added.

- [ ] **Step 10: Update `docs/superpowers/OPEN_THINGS.md`.**

Two edits:

1. Delete item 46 entirely (lines 13-15 in the current file, including the `### Sprint-tidy phase` heading if item 46 is the only entry under it; otherwise just delete the item paragraph, keep the heading and the rest of the section).
2. Edit item 42's "Blocked on item 46" reference. Find the substring `**Blocked on item 46** (supervisor resilience to cell-child panics) so a production-budget run survives any single-cell panic and produces partial markdown on its own.` in item 42 and replace it with `Item 46 (supervisor resilience) shipped, so a production-budget run now survives any single-cell panic and produces partial markdown.`. Item 42 stays a P1 active-sprint item (no longer blocked).

- [ ] **Step 11: Update auto-memory `project_roadmap_status.md`.**

Read the current memory file and update both the YAML frontmatter `description` field and the body to reflect:

- Item 46 shipped on 2026-05-06.
- Next pickup: P1 item 42 (production-shape `BENCH_RESULTS.md` refresh, now unblocked) or P1 item 34 (backend-aware deadline), whichever the body's prior pointer was on. If it was on item 34 (per the OPEN_THINGS active-sprint Next pickup line), keep that and surface item 42 as a parallel-track candidate in the body.

Do not invent shipped items; the body should advance only what was actually completed in this PR.

- [ ] **Step 12: Verify the resulting markdown is parseable.**

Run a sanity check on the synthetic test output by eyeballing a panic row. After the tests pass, the synthetic plan generates output that should look like:

```
| grundschule | lahc | 20 | 20/20 | 0 | 45/45 | 0 | 0.13 | 60000 | 49152 | 1 | 2 | - | - | - | - | - |
| grundschule | lahc_rr | - | panic | - | - | - | - | - | - | - | - | - | - | - | - | - |
| grundschule | lahc_rr_kempe | 20 | 20/20 | 0 | 45/45 | 0 | 0.13 | 60000 | 49152 | 1 | 2 | - | - | - | - | - |
```

17 pipe-separated cells per row. No further action; this is informational only.

- [ ] **Step 13: Stage and commit (main session, after subagent returns).**

```bash
git add solver/solver-bench/src/main.rs docs/superpowers/OPEN_THINGS.md
git add /home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md
git commit -m "fix(solver-bench): supervisor renders panic placeholder and continues on cell-child failure (item 46)"
```

The auto-memory file lives under `~/.claude/...`, outside the repo. It does not need to be (and cannot be) committed to the repo; the `git add` for the memory file path is a no-op via git but the file is updated in place. Drop the third `git add` line if it errors. Final commit contains the supervisor changes + OPEN_THINGS bookkeeping only.

---

## Self-Review

**Spec coverage:**
- Refactor `run_supervisor` to extract `render_cells`: Steps 4-5.
- Add `write_error_row`: Step 3.
- Update exit-code derivation: Step 5.
- Update markdown footer: Step 6.
- Add three inline unit tests: Steps 1-2 (red), 7 (green).
- Update auto-memory: Step 11.
- Delete OPEN_THINGS item 46 + unblock item 42: Step 10.
- Single commit: Step 13.

**Placeholder scan:** None. All steps include either exact code or exact commands.

**Type consistency:** `render_cells` signature and tests align (`(&'static str, BenchBackend)` pairs, `FnMut(&str, BenchBackend) -> Result<CellResult, String>`). `write_error_row` signature matches its call site in `render_cells`. Synthetic helper name `synthetic_cell_for_resilience_tests` is unique per the unique-function-name rule (will not collide with any other helper in the codebase).
