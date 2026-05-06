# Bench supervisor resilience to per-cell panics spec (active sprint, item 46)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Sprint-tidy phase: item 46.
**Goal.** Patch the `solver-bench` supervisor at `solver/solver-bench/src/main.rs:239-244` so a per-cell-child failure (panic, non-zero exit, JSON parse error, spawn error) is logged to stderr, rendered as a placeholder row in the markdown output, and followed by the next cell. The supervisor exits 0 if at least one queued cell succeeded; exits non-zero only when every queued cell failed; CLI parse errors keep their existing `ExitCode::from(2)`. ADR 0034 (cell-subprocess architecture) chose per-cell isolation specifically so one cell's panic could not cancel the rest of the run, but the supervisor still fails-fast on any non-zero cell-child exit. This PR is the missing follow-through.

**Non-goal.** No bake-off rerun (item 42 is a separate PR, blocked on this one). No production refresh of `BENCH_RESULTS.md`. No new `cell-child` flags or output JSON fields (a panicking child cannot reliably emit structured output before going down). No refactor of `spawn_cell` beyond what's needed to call it from a function pointer / closure. No ADR (ADR 0034 already records the principle).

## Context

`solver-bench` runs as supervisor + per-cell-child via recursive self-spawn (ADR 0034). The supervisor parses CLI flags, iterates `(fixture, backend)` pairs, spawns one `solver-bench --cell <fixture> --backend <name> --budget <d> --seeds <n>` child per cell, captures stdout, parses a `CellResult` JSON object, formats one markdown row per cell, writes the file. `spawn_cell` returns `Result<CellResult, String>`: `Ok` on a clean child exit with parseable stdout, `Err(reason)` on any of: spawn error (`spawn cell: ...`), wait error (`wait cell: ...`), non-zero child exit (`cell exited with ...`), stdout UTF-8 decode error (`cell stdout utf-8: ...`), or JSON parse error (`cell JSON: ...`).

Today the supervisor's loop (`solver-bench/src/main.rs:233-286`) handles `Err` by `eprintln!`-ing the reason and immediately `return ExitCode::FAILURE`-ing. The markdown buffer is never flushed because the post-loop `fs::write(&args.out, &markdown)` (line 290) is unreachable on the early return. So the operator gets stderr, no markdown, no record of which cells did succeed.

Concrete failure shape that motivated the item: under the production refresh of `BENCH_RESULTS.md` (item 42, `--budget 60s --seeds 20`), the `grundschule / lahc_rr_kempe` cell tripped a `validate_no_double_booking` panic in the cell-child (item 45's bug). The cell-child exited non-zero, the supervisor early-returned `FAILURE`, and the partial markdown for the 2 cells that had completed was lost. Item 45 has since shipped (commits 4cb853b, 0393fbf), but the supervisor remains a fragile single-point-of-failure for any future single-cell regression. ADR 0034's per-cell-process isolation is undermined whenever the supervisor short-circuits on one child's exit code.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 46. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Refactor the cell-loop body inside `run_supervisor` to a pure function `render_cells(plan, runner, &mut markdown) -> usize` where:
    - `plan` is an iterator of `(fixture_name, BenchBackend)` pairs to render in order.
    - `runner` is a closure or `FnMut` that takes `(fixture_name, BenchBackend)` and returns `Result<CellResult, String>`. Production wires it to a closure over `spawn_cell(&exe, fixture, backend, args.budget, args.seeds)`. The test wires a stubbed runner.
    - The function appends one markdown row per pair to `markdown`: `write_row(...)` on `Ok`, a new `write_error_row(...)` on `Err`. On `Err` it also `eprintln!`s the cell error before continuing (so live operator output retains the diagnostic).
    - Returns the count of `Ok` cells.
- Add `write_error_row(out, fixture, backend, reason)` next to `write_row`. Renders 17 columns: real fixture and backend names, all other cells set to `-` except the Feasibility cell, which renders the literal string `panic`. The `reason` string is unused in the markdown row itself (it's already gone to stderr); the function takes it as an argument so a future change can swap the marker for the reason without touching the call site signature.
- Update `run_supervisor` to call `render_cells` with the production runner. Replace the existing early-return-on-error with the new resilient flow. After the loop, derive the exit code: `if cells_attempted == 0 { SUCCESS } else if successes >= 1 { SUCCESS } else { FAILURE }`. CLI parse errors and `current_exe()` failures keep their existing exit codes (`from(2)` and `FAILURE` respectively).
- Update the markdown footer copy at `write_footer` to document the `panic` token: one short sentence describing what the marker means, so a reader does not need to grep the source. Place between the existing quality-columns paragraph and the methodology pointer.
- Add an inline unit test `supervisor_renders_panic_row_and_continues_on_cell_error` to `#[cfg(test)] mod tests` in `solver-bench/src/main.rs`. Synthetic plan of three pairs; runner returns `Ok(CellResult)` for the first and third, `Err("synthetic-panic")` for the second; assert the rendered markdown contains rows for the surviving pairs (real `placements_total_median` token), the literal `| panic |` marker for the failed pair, and the returned success count is `2`.
- Add a second inline unit test `supervisor_returns_zero_successes_when_every_cell_panics` to confirm the success-count derivation when every cell errors. The runner returns `Err` for every pair; assert success count is `0`. Exit-code derivation is exercised indirectly via this count (the `if successes >= 1` branch is one match away).
- Update auto-memory `project_roadmap_status.md` to reflect item 46 shipped and surface item 42 (production-shape `BENCH_RESULTS.md` refresh) as the unblocked next pickup.
- Delete OPEN_THINGS item 46. Update item 42's "Blocked on item 46" pointer to "unblocked" or remove the line.

**Out of scope.**

- Item 42 (production-shape `BENCH_RESULTS.md` refresh). Separate P1; lands as a follow-up `bench(solver-core): refresh BENCH_RESULTS.md` PR after this one.
- Item 44 (BENCH refresh after item 12 lands). Tracked separately.
- Cell-child changes. The cell-child stays bit-identical; the resilient flow only reads the existing `Result<CellResult, String>` shape from `spawn_cell`.
- Restructured exit codes for partial failure (e.g., a new exit code 1 = "all failed", 3 = "partial"). The OPEN_THINGS guidance is "exits 0 if every cell either succeeded or was reported, exits non-zero only when no cells succeeded"; matching that contract avoids ratcheting a CI-visible signal.
- New ADR. ADR 0034 already records the cell-subprocess principle; this PR honours its consequences. Cite the ADR in the PR body.
- Integration test in `solver-bench/tests/`. The unit test covers the supervisor's resilient logic without needing a real subprocess; the existing `tests/end_to_end.rs` continues to cover the happy-path cell-spawn flow.
- Bench refresh of `BASELINE.md`. The supervisor change is wall-clock-neutral (no inner loop changes; one extra branch per cell, taken at most once per cell on `Err`).

## Implementation shape

### Function extraction

Today (truncated):

```rust
fn run_supervisor(raw: Vec<String>) -> ExitCode {
    let args = ...;
    let mut markdown = String::new();
    write_header(&mut markdown);

    let exe = ...;

    for (name, _build) in FIXTURES {
        if !args.fixtures.iter().any(|f| f == name) { continue; }
        for backend in &BenchBackend::ALL {
            eprintln!("cell start: {} / {}", name, backend.label());
            let cell = match spawn_cell(&exe, name, *backend, args.budget, args.seeds) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("cell error: {} / {}: {e}", name, backend.label());
                    return ExitCode::FAILURE;
                }
            };
            eprintln!("cell done: ...");
            write_row(&mut markdown, name, *backend, &cell);
        }
    }
    write_footer(&mut markdown);
    if let Err(e) = fs::write(&args.out, &markdown) { ...; return FAILURE; }
    ExitCode::SUCCESS
}
```

After:

```rust
fn run_supervisor(raw: Vec<String>) -> ExitCode {
    let args = ...;
    let mut markdown = String::new();
    write_header(&mut markdown);

    let exe = ...;

    let plan: Vec<(&'static str, BenchBackend)> = FIXTURES
        .iter()
        .filter(|(name, _)| args.fixtures.iter().any(|f| f == name))
        .flat_map(|(name, _)| BenchBackend::ALL.iter().map(move |b| (*name, *b)))
        .collect();
    let cells_attempted = plan.len();

    let mut runner = |name: &str, backend: BenchBackend| {
        spawn_cell(&exe, name, backend, args.budget, args.seeds)
    };
    let successes = render_cells(plan.iter().copied(), &mut runner, &mut markdown);

    write_footer(&mut markdown);
    if let Err(e) = fs::write(&args.out, &markdown) { ...; return FAILURE; }
    eprintln!("wrote {:?}", args.out);

    if cells_attempted == 0 || successes >= 1 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

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
                eprintln!("cell done: ... (existing format string)");
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

The two-character "&str" lifetime fits because `FIXTURES` is `&[(&'static str, fn() -> Problem)]`. Producing `&'static str` from the iterator avoids an allocation per pair.

`render_cells`'s `runner` parameter is `&mut F` rather than `F` so the closure can capture `&exe` mutably (it does not need to, but `FnMut` is the most permissive bound). Type inference resolves the closure to `FnMut`.

### Error-row rendering

```rust
fn write_error_row(out: &mut String, fixture: &str, backend: BenchBackend, _reason: &str) {
    out.push_str(&format!(
        "| {fixture} | {backend} | - | panic | - | - | - | - | - | - | - | - | - | - | - | - | - |\n",
        backend = backend.label(),
    ));
}
```

Column count: 17 (matches `write_row`'s 17 columns and `write_header`'s separator row). The `panic` token sits in the Feasibility column (position 4); every other numeric / status column renders `-`.

The `_reason` argument is captured for forward compatibility (a future change might want to render the reason inline) but is unused in the body. Naming it `_reason` keeps clippy quiet without adding `#[allow(unused)]`.

### Footer addition

Insert between the quality-columns paragraph (ending with `composite Quality column.`) and the home-room paragraph (starting with `Home-room ratio exempts ...`):

```text
Cells whose subprocess fails (panic, non-zero exit, JSON parse error) render as `panic` in the
Feasibility column with `-` in every other numeric column. The supervisor logs the underlying
reason to stderr and continues to the next cell.
```

One short paragraph; no markdown shape ratchet beyond what already exists in the footer.

## Test plan

**Synthetic-runner unit tests (`solver-bench/src/main.rs`, in `#[cfg(test)] mod tests`).**

```rust
fn synthetic_cell(seeds: u64, feasibility_count: u64) -> CellResult {
    CellResult {
        seeds,
        feasibility_count,
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
            Ok(synthetic_cell(20, 20))
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
    assert_eq!(panic_row_count, 2, "every plan entry should render a panic row");
}
```

The exact "cell done" format string is preserved from the existing supervisor body; tests assert structural tokens (the leading `| fixture | backend | seeds | n/seeds |` prefix) rather than the entire row, which depends on the synthetic CellResult's other fields. The tests use `BenchBackend::ALL` variants directly without the `BenchBackend::label()` indirection so a refactor of label strings would surface in the test rather than silently passing.

**Acceptance:**

- Both new unit tests pass (`cargo nextest run -p solver-bench --bin solver-bench`).
- `mise run test:rust` passes end-to-end (covers the existing `tests/end_to_end.rs` happy-path integration test, which proves the refactor preserved the wire shape under a real cell-spawn).
- `mise run lint` passes (no new clippy warnings; the unused `_reason` parameter is named with a leading underscore to silence `unused_variables`).
- Markdown footer documents the `panic` token (covered by a third inline test that asserts `write_footer` includes the new sentence).

## Risks

- **Exit-code change visible to CI / dev-loop callers.** Today a partial-failure run exits non-zero; after this change it exits 0 if at least one cell succeeded. Any external tooling that gated on "supervisor exit 0 = all-cells-clean" needs to switch to "scan markdown for `panic` token". The repo's own `mise run bench:bakeoff` is a developer command, not a CI gate; the staging deploy workflow does not invoke the bench. Acceptable trade-off.
- **`panic` token collision.** No existing column renders the literal `panic` string (Feasibility renders `n/seeds` as digits); no cell name or backend label uses it. Future backend names should avoid `panic`. Document in `solver/CLAUDE.md` if this changes.
- **Footer text drift.** A future column addition that re-orders the footer paragraphs could orphan the new resilience paragraph. Keep the resilience paragraph adjacent to the quality / methodology paragraphs; the new test pins its presence.
- **Determinism.** `render_cells` iterates the plan in given order. The plan is built from `FIXTURES.iter().filter(...).flat_map(BenchBackend::ALL)`, which preserves the existing `(grundschule, zweizuegig, dreizuegig, lock_in)` x `(lahc, lahc_rr, lahc_rr_kempe, cpsat)` ordering. No determinism regression.

## Commit shape

Single `fix(solver-bench): supervisor renders panic placeholder and continues on cell-child failure (item 46)` commit containing:

- The `render_cells` extraction in `solver-bench/src/main.rs`.
- The `write_error_row` helper in `solver-bench/src/main.rs`.
- The `run_supervisor` rewire including the new exit-code derivation.
- The footer text addition in `write_footer`.
- The two new inline unit tests plus the third test that pins the footer copy.
- OPEN_THINGS item 46 deletion plus item 42 unblock.
- Auto-memory `project_roadmap_status.md` advance.
