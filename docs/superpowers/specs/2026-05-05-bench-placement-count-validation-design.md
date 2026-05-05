# Bake-off placement-count validation spec (active sprint, item 28)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Bench prevention phase: item 28.
**Goal.** Close the prevention hole that hid the R&R silent placement-drop bug. After this PR, the bake-off harness cannot report `feasibility 20/20 soft=0` while a backend silently dropped placements.

**Non-goal.** No bench refresh against production settings (deferred to item 29). No ADR (item 29's verdict refresh ships the next ADR). No new observability columns (peak RAM, time-to-first-feasible, time-to-optimal are item 30). No schedule-quality metrics (item 31). No production-route solvability tests (item 32). No `solver-core` API change.

## Context

`solver/solver-bench/src/main.rs:212` calls a cell feasible iff `solution.violations.len() == 0`; it never checks `solution.placements.len()`. R&R can drop placements without growing `violations` because the violation list is the FFD-time view of failure (items 26 / 27 fixed the underlying drop in `rr_collect_anchors`, but the harness blind spot remains). The bench column then reports `feasibility 20/20 soft=0` because absent placements pay no constraint cost; the soft score collapses to zero as the schedule empties out.

Items 26 and 27 fixed the underlying drop in `solver-core/src/lahc.rs::rr_collect_anchors` and added two property tests guarding the invariant. Item 28 is the prevention guard: even if a future ruin-style move re-introduces a similar drop, the bake-off harness must catch it instead of writing it into a misleading markdown row.

The dev-DB zweizügig receipt that surfaced the bug: greedy alone returns 191 of 196 placements; pre-fix `lahc_rr` and `lahc_rr_kempe` return 68 of 196; `cpsat` at a 5 second budget returns 196 of 196. The committed `BENCH_RESULTS.md` (see `solver/solver-core/benches/BENCH_RESULTS.md`) shows pre-fix `lahc_rr` and `lahc_rr_kempe` at `feasibility 20/20 soft 0` on every fixture, which is the receipt that the harness happily wrote a misleading row off the broken signal. ADR 0031 then picked `lahc_rr_kempe` as production default off that signal.

OPEN_THINGS item 28 prescribes the fix: add `placements_total` and `placements_expected` columns to `BENCH_RESULTS.md`; treat `placements_total < placements_expected` as a feasibility failure; compute `placements_expected = sum(l.hours_per_week for l in problem.lessons)`. This spec follows that prescription with a single tweak motivated by Q2 in the brainstorm: the two pieces of information render as one column (`Placements (median / expected)`) instead of two, because the `expected` value is per-fixture and identical across the four backend rows; one column reads more honestly and is narrower in the markdown table.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 28 (Bench prevention phase). Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Extend `solver/solver-bench/src/main.rs` so each cell evaluates feasibility as `violations.is_empty() && placements.len() == placements_expected`. `placements_expected = problem.lessons.iter().map(|l| l.hours_per_week as u64).sum::<u64>()`, computed once per fixture in `main` and threaded into `run_cell` (and `run_cpsat_cell`) as a `u64` parameter.
- Add a `placements_total_median: u64` field to `CellResult` and a parallel `placements_total_samples: Vec<u64>` collection inside both `run_cell` and `run_cpsat_cell`. CP-SAT subprocess error continue paths push `0` (the honest "no placements observed") to keep the median computation aligned with the existing `u32::MAX` push for hard violations on the same row.
- Extend `write_header` and `write_row` to render one new column `Placements (median / expected)` between the existing `Soft score (median, feasible)` and `FFD wall-clock (ms, median)` columns. Format: `{median}/{expected}`, right-aligned via the existing `---:` markdown header, e.g. `196/196` for a healthy cell or `60/196` for a regression.
- Plumb `placements_expected` into `write_row` (it's already in scope per the per-fixture loop in `main`).
- Add unit tests to `solver-bench/src/main.rs::tests`: (a) `placements_expected_for_problem_sums_hours_per_week` (a hand-built `Problem` with two lessons, `hours_per_week=2` and `hours_per_week=4`, asserts the helper returns 6); (b) `cell_with_too_few_placements_is_not_feasible` (constructs a `CellResult` with `feasibility_count=0` and `placements_total_median=60`, exercises `write_row` for `expected=196`, asserts the rendered row contains `| 60/196 |`); (c) `cell_with_full_placements_is_feasible` (renders `| 196/196 |` for a healthy cell). The existing `write_row_emits_one_line_with_dash_for_no_feasible` family extends to assert the new column shape on the no-feasible row.
- Update `docs/superpowers/OPEN_THINGS.md`: delete item 28 (it ships with this PR); update the active-sprint pickup pointer at line 9 to point at item 29 as the next pickup. Add the two conditional follow-ups from brainstorm Q10 ("compute placement count distribution per cell, not just median" and "expose `placements_expected` as `pub fn` in `solver-core`") at the bottom of the existing `## Open solver follow-ups` section (the section already collects bake-off-program-deferred items, which is the right home for these).

**Out of scope.**

- Refresh of `solver/solver-core/benches/BENCH_RESULTS.md` against production settings (`--budget 60s --seeds 20`). The full refresh is item 29's responsibility plus ADR 0032; this PR's receipt is a dev-loop refresh (`--budget 5s --seeds 4 --fixtures grundschule --out /tmp/bench-receipt.md`) cited in the PR body.
- ADR 0032. Deferred to item 29 once `BENCH_RESULTS.md` carries honest numbers under the new gate.
- Promoting `placements_expected` to a `pub fn` in `solver-core`. Brainstorm Q4 deferred this; the helper lives in `solver-bench/src/main.rs` until a second caller materialises.
- Any change to `solver-core` (no API change, no doc-comment churn), `solver-py`, the backend, or the frontend.
- Per-iteration probes for `time_to_first_feasible_ms`. Tracked separately as item 30.
- Schedule-quality predicates. Tracked separately as item 31.

## Helper and feasibility predicate

The helper:

```rust
fn placements_expected_for_problem(problem: &Problem) -> u64 {
    problem.lessons.iter().map(|l| l.hours_per_week as u64).sum()
}
```

`u64` matches the cast used in the existing wall-clock and seed counters; lifting `u8 hours_per_week` to `u64` once per lesson is sub-microsecond on the dreizügig fixture (102 lessons). The function lives at module scope in `solver-bench/src/main.rs`; per brainstorm Q4 the helper is bundled with its caller in this PR and not promoted to a `pub fn` in `solver-core`.

The seed loop in `run_cell` becomes:

```rust
let solution = solve_with_config(problem, &cfg).expect("solve");
let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
let hard = solution.violations.len() as u32;
let placements_total = solution.placements.len() as u64;
debug_assert!(placements_total <= expected,
    "placements_total ({placements_total}) > expected ({expected}); structural invariant violated");
let feasible = hard == 0 && placements_total == expected;
if feasible {
    feasibility_count += 1;
    soft_score_feasible.push(solution.soft_score);
}
hard_violations_samples.push(hard);
total_ms_samples.push(total_ms);
placements_total_samples.push(placements_total);
```

`expected` is the third parameter of `run_cell` (after `problem` and `budget`) and is consumed both inside the loop and to populate `CellResult.placements_expected` for the row writer. The `debug_assert!` per brainstorm Q1 catches structural invariant breakage in dev / CI but compiles out of the release bench binary, so the production wall-clock is unchanged.

`run_cpsat_cell` mirrors the same shape. On the subprocess-error continue paths (`Ok(o)` non-zero exit, `Err(e)`, parse error) the harness pushes `0` to `placements_total_samples` per brainstorm Q5; the existing `u32::MAX` push for `hard_violations_samples` already screams on the same row, so the `0/expected` rendering is the secondary signal.

## Markdown rendering

The header gains one column. From:

```
| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Soft score (median, feasible) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) |
```

to:

```
| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Placements (median / expected) | Soft score (median, feasible) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) |
```

`write_row` renders the new column as `{median}/{expected}` (e.g. `196/196`, `60/196`, `0/196`). The header divider gains one `---:` to keep right-alignment. The footer is unchanged.

The placement column lands between `Hard violations (median)` and `Soft score (median, feasible)` because that's where the human reading the row scans for "how broken is this cell?", with hard violations and placement underflow as the two failure modes; soft score sits one column to the right because it's only meaningful for feasible cells.

## CellResult shape

```rust
struct CellResult {
    seeds: u64,
    feasibility_count: u64,
    hard_violations_median: u32,
    placements_total_median: u64,
    placements_expected: u64,
    soft_score_median: Option<u32>,
    ffd_ms_median: f64,
    total_ms_median: f64,
}
```

`placements_expected` is on the cell (not just the row writer) so the row writer's signature stays `write_row(&mut String, &str, BenchBackend, &CellResult)`. The two new fields cost 16 bytes per cell; with 16 cells the total memory footprint of the run is unchanged at the ms scale.

`median_u64` is a new helper sibling to the existing `median_u32` and `median_f64`, same shape (sort_unstable, return middle). Per brainstorm Q9 the helper lands in the same commit.

## Helper threading in `main`

The fixture loop becomes:

```rust
for (name, build) in FIXTURES {
    if !args.fixtures.iter().any(|f| f == name) { continue; }
    let problem = build();
    let expected = placements_expected_for_problem(&problem);
    for backend in &backends {
        eprintln!("cell start: {} / {}", name, backend.label());
        let cell = run_cell(*backend, &problem, expected, args.budget, args.seeds);
        eprintln!(
            "cell done: {} / {} feasibility {}/{} hard_med={} placements_med={}/{} soft_med={} total_ms_med={:.0}",
            name, backend.label(),
            cell.feasibility_count, cell.seeds,
            cell.hard_violations_median,
            cell.placements_total_median, cell.placements_expected,
            cell.soft_score_median.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string()),
            cell.total_ms_median,
        );
        write_row(&mut markdown, name, *backend, &cell);
    }
}
```

The progress eprintln gains the placement counts, mirroring the new column in the markdown so a streaming bench refresh is legible from the terminal.

## Unit tests

Added to `solver-bench/src/main.rs::tests`:

```rust
#[test]
fn placements_expected_for_problem_sums_hours_per_week() {
    // Hand-built Problem with two lessons; helper returns hours_per_week sum.
}

#[test]
fn write_row_renders_placements_column() {
    let cell = CellResult {
        seeds: 20,
        feasibility_count: 20,
        hard_violations_median: 0,
        placements_total_median: 196,
        placements_expected: 196,
        soft_score_median: Some(10),
        ffd_ms_median: 1.0,
        total_ms_median: 60100.0,
    };
    let mut out = String::new();
    write_row(&mut out, "zweizuegig", BenchBackend::LahcRr, &cell);
    assert!(out.contains("| 196/196 |"));
}

#[test]
fn write_row_renders_underflow_placement_count() {
    let cell = CellResult {
        seeds: 20,
        feasibility_count: 0,
        hard_violations_median: 0,
        placements_total_median: 60,
        placements_expected: 196,
        soft_score_median: None,
        ffd_ms_median: 0.5,
        total_ms_median: 60000.0,
    };
    let mut out = String::new();
    write_row(&mut out, "zweizuegig", BenchBackend::LahcRr, &cell);
    assert!(out.contains("| 60/196 |"));
    assert!(out.contains("| 0/20 |"));
}
```

The existing `write_row_emits_one_line_with_dash_for_no_feasible` test is updated to populate the two new fields (`placements_total_median: 0`, `placements_expected: 45`) and to assert the new column renders correctly. The four `write_row_renders_*_backend_label` tests likewise gain populated fields; the assertions don't change, only the construction.

The `placements_expected_for_problem` test constructs a minimal `Problem` value: one teacher, one room, one subject, one school class, two lessons (`hours_per_week=2` and `hours_per_week=4`), empty `time_blocks` / `teacher_qualifications` / `*_blocked_times` / `room_subject_suitabilities` / `pinned_placements`. The harness only reads `problem.lessons[*].hours_per_week`; nothing else needs to be valid for this test. We construct via `Problem { lessons: vec![..], ..Problem::default() }` if `Default` is available, else inline the empty Vecs (the `solver/CLAUDE.md` cascade rule warns about new Problem fields, so we follow whichever shape the existing tests use; `solve.rs::tests` has examples).

## Acceptance criteria

- All existing `solver-bench` tests pass: `cargo nextest run -p solver-bench`.
- The three new unit tests pass alongside the updated existing ones.
- `mise run lint` is green (clippy `-D warnings`, ruff, ty, biome, machete, fmt). Particular attention to `cargo clippy --workspace --all-targets -- -D warnings` because the new helper plus its single caller is the canonical bundle-helper-with-caller pattern from `solver/CLAUDE.md`.
- Pre-push runs full test suite cleanly via `mise exec -- git push`.
- Dev-loop receipt cited in PR body: `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule --out /tmp/bench-receipt.md` produces a markdown table whose `Placements (median / expected)` column renders `45/45` for grundschule across all four backends. (Grundschule is small enough that 5 seconds × 4 seeds is honest and runs in well under a minute.)
- The committed `BENCH_RESULTS.md` is unchanged in this PR. Item 29 lands the production refresh in its own PR.

## Risks

1. **Median bias on bimodal cells.** If a backend regresses such that 50 % of seeds drop placements and 50 % do not, the median renders cleanly while the cell is half broken. Brainstorm Q10 deferred a "distribution column" follow-up; the symptom is detectable in this PR via the hard-violations median for the cell (which would also be bimodal) and in item 29's refresh data, so the median is not silently wrong, just noisy on the edge case.
2. **CP-SAT subprocess noise.** A flaky subprocess that intermittently fails would push `0` for placement count and skew the median low. Mitigation: the existing `u32::MAX` push for hard violations on the same row already encodes the failure; the median row reads consistently across both columns. Item 29 will surface any chronic CP-SAT flakiness in the refresh data.
3. **Behaviour drift between unit tests and production refresh.** The unit tests construct `Problem` and `CellResult` values directly; if the field cascade rule (`solver/CLAUDE.md`) lands a new field on `Problem` mid-PR, the unit tests need updating. Mitigation: this PR does not add Problem fields, and the `Problem` value the new test constructs only relies on `lessons`. The risk is a future PR's responsibility, not this one's.

## Plan

The implementation splits cleanly into ordered chunks per `superpowers:test-driven-development`:

1. Red: write the three new unit tests (`placements_expected_for_problem_sums_hours_per_week`, `write_row_renders_placements_column`, `write_row_renders_underflow_placement_count`); confirm they fail to compile (helper does not exist; `CellResult` lacks the two new fields).
2. Red-to-green: extend `CellResult` with `placements_total_median: u64` and `placements_expected: u64`; add `median_u64`; write `placements_expected_for_problem`. Update existing `write_row_*` tests to populate the two new fields (compile-only; assertions unchanged). Confirm the three new tests fail at the assertion (`write_row` does not yet render the column).
3. Green: extend `write_header` and `write_row` to render the new column; thread `expected` through `run_cell` / `run_cpsat_cell` / `main`; add the placement gate to feasibility; push samples on the cpsat error continue paths. All tests pass.
4. Refactor: read the diff, tighten doc comments on `placements_expected_for_problem`, run `mise run lint` and `mise run test:rust`.
5. Receipt: dev-loop bench refresh against grundschule for the PR body. Confirm the new column renders.
6. Docs: delete OPEN_THINGS item 28; update the active-sprint pickup pointer; append the two conditional follow-ups under `## Open solver-correctness follow-ups`. Commit.

The plan-document version expands these into checkbox tasks per `superpowers:writing-plans`.
