# Bake-off placement-count validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Gate bake-off bench cells on `placements.len() == sum(hours_per_week)` so a future R&R-style placement-drop regression cannot pass the harness silently.

**Architecture:** All edits live in `solver/solver-bench/src/main.rs`. The harness gains a `placements_expected_for_problem` helper, threads the `expected` count through `run_cell` and `run_cpsat_cell`, extends `CellResult` with two new fields, adds one new `Placements (median / expected)` markdown column, and tightens feasibility to `violations.is_empty() && placements.len() == expected`. CP-SAT subprocess error continue paths push `0` for placement count, mirroring the existing `u32::MAX` push for hard violations on the same row. The committed `BENCH_RESULTS.md` is not refreshed in this PR; that is item 29's job.

**Tech Stack:** Rust 1.85, `solver-core` (test fixtures, `Problem` / `Solution` types), `solver-bench` (binary crate). No new deps. Tests run via `cargo nextest run -p solver-bench`.

**Spec:** `docs/superpowers/specs/2026-05-05-bench-placement-count-validation-design.md`.

---

## File Structure

- Modify: `solver/solver-bench/src/main.rs` (helper, struct fields, threading, feasibility predicate, header / row rendering, unit tests).
- Modify: `docs/superpowers/OPEN_THINGS.md` (delete item 28, update active-sprint pickup pointer at line 9, append two follow-ups under existing `## Open solver follow-ups` section).

No other files change. The harness is self-contained.

---

## Task 1: Add `median_u64` helper plus its first caller

The bench has `median_u32` and `median_f64` but no `median_u64`. Per `solver/CLAUDE.md` "Bundle a new `pub(crate)` helper with its first caller in the same commit", we add the helper together with the field on `CellResult` so the lint passes.

**Files:**
- Modify: `solver/solver-bench/src/main.rs`

- [ ] **Step 1: Add `median_u64` next to the existing median helpers**

In `solver/solver-bench/src/main.rs`, find:

```rust
fn median_u32(values: &mut [u32]) -> u32 {
    values.sort_unstable();
    let mid = values.len() / 2;
    values[mid]
}
```

Add immediately below it:

```rust
fn median_u64(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    let mid = values.len() / 2;
    values[mid]
}
```

- [ ] **Step 2: Add `placements_total_median: u64` and `placements_expected: u64` fields to `CellResult`**

In the `struct CellResult { ... }` block, add the two fields:

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

The cascade rule: every `CellResult { ... }` literal in the file (including the unit tests at the bottom) needs the two new fields. The compiler error list will enumerate them.

- [ ] **Step 3: Populate the new fields in `run_cell`'s return value**

Find the existing `CellResult { ... }` literal at the end of `run_cell` and rewrite to:

```rust
CellResult {
    seeds,
    feasibility_count,
    hard_violations_median: median_u32(&mut hard_violations_samples),
    placements_total_median: 0,
    placements_expected: 0,
    soft_score_median: if soft_score_feasible.is_empty() {
        None
    } else {
        Some(median_u32(&mut soft_score_feasible))
    },
    ffd_ms_median: ffd_ms,
    total_ms_median: median_f64(&mut total_ms_samples),
}
```

The `0` placeholders for the new fields stand in until Task 3 wires the actual values; this is a compile-only step.

- [ ] **Step 4: Populate the new fields in `run_cpsat_cell`'s return value**

Mirror Task 1 step 3 in the `run_cpsat_cell` function. Same `0` placeholders, same shape.

- [ ] **Step 5: Update the existing `write_row_*` unit tests to populate the two new fields**

Five tests in `mod tests` construct `CellResult { ... }` literals: `write_row_emits_one_line_with_dash_for_no_feasible`, `write_row_renders_lahc_rr_backend_label`, `write_row_renders_lahc_rr_kempe_backend_label`, `write_row_renders_cpsat_backend_label`, and any others surfaced by the compiler. For each, add `placements_total_median: 0,` and `placements_expected: 0,` (the assertions stay unchanged at this point because Task 4 adds the new column rendering).

- [ ] **Step 6: Confirm everything still compiles and existing tests pass**

Run: `cargo nextest run -p solver-bench`
Expected: PASS (existing tests unchanged behaviour-wise; only field plumbing changed).

- [ ] **Step 7: Commit**

Do not commit yet; this is a compile-only step shared with Task 2 + 3 + 4 + 5 + 6 in one atomic feat commit. Skip to next task.

---

## Task 2: Write the failing helper test (red)

**Files:**
- Modify: `solver/solver-bench/src/main.rs` (add to `mod tests`)

- [ ] **Step 1: Write `placements_expected_for_problem_sums_hours_per_week`**

Use the already-imported `grundschule_fixture` so the test does not have to enumerate every `Problem` field (the cascade rule from `solver/CLAUDE.md` makes hand-built `Problem` literals fragile). The grundschule fixture has 45 expected placements (2 classes, 15 lessons summing to 45 hours).

Add to `mod tests` at the bottom of `solver/solver-bench/src/main.rs`:

```rust
#[test]
fn placements_expected_for_problem_sums_hours_per_week() {
    let problem = grundschule_fixture();
    let manual_sum: u64 = problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as u64)
        .sum();
    assert_eq!(placements_expected_for_problem(&problem), manual_sum);
    assert_eq!(placements_expected_for_problem(&problem), 45);
}
```

The first assertion guards the helper against future fixture drift (if the fixture grows, `manual_sum` stays consistent). The second assertion pins the absolute number so a regression that silently changed `hours_per_week` cast logic surfaces.

- [ ] **Step 2: Run the test; expect compile failure**

Run: `cargo nextest run -p solver-bench placements_expected_for_problem_sums_hours_per_week`
Expected: FAIL with "cannot find function `placements_expected_for_problem` in this scope".

---

## Task 3: Make the helper test pass (green)

**Files:**
- Modify: `solver/solver-bench/src/main.rs`

- [ ] **Step 1: Add `placements_expected_for_problem` at module scope**

Insert near the top of `solver/solver-bench/src/main.rs`, after the imports and before `BenchBackend`:

```rust
/// Total number of placements a fully solved problem must produce: one per (lesson, hour).
/// `placements.len() < placements_expected_for_problem(problem)` is a feasibility failure
/// even when `solution.violations` is empty (a placement drop during local search does
/// not grow the violation list). Bench harness predicate.
fn placements_expected_for_problem(problem: &Problem) -> u64 {
    problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as u64)
        .sum()
}
```

- [ ] **Step 2: Run the test; expect pass**

Run: `cargo nextest run -p solver-bench placements_expected_for_problem_sums_hours_per_week`
Expected: PASS.

---

## Task 4: Write the failing markdown row test (red)

**Files:**
- Modify: `solver/solver-bench/src/main.rs` (add to `mod tests`)

- [ ] **Step 1: Add `write_row_renders_placements_column`**

Add to `mod tests`:

```rust
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
    assert!(
        out.contains("| 196/196 |"),
        "expected `| 196/196 |` somewhere in row, got: {out}"
    );
}
```

- [ ] **Step 2: Add `write_row_renders_underflow_placement_count`**

Add to `mod tests`:

```rust
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
    assert!(
        out.contains("| 60/196 |"),
        "expected `| 60/196 |` somewhere in row, got: {out}"
    );
    assert!(
        out.contains("| 0/20 |"),
        "expected `| 0/20 |` (feasibility) somewhere in row, got: {out}"
    );
}
```

- [ ] **Step 3: Run the new tests; expect fail**

Run: `cargo nextest run -p solver-bench write_row_renders_placements_column write_row_renders_underflow_placement_count`
Expected: FAIL because `write_row` does not yet emit the new column. The assertion message will show the actual rendered row missing the `196/196` substring.

---

## Task 5: Make the markdown row tests pass (green)

**Files:**
- Modify: `solver/solver-bench/src/main.rs`

- [ ] **Step 1: Extend `write_header` with the new column**

Find the existing `write_header` body and replace the two `out.push_str` lines with:

```rust
out.push_str("| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Placements (median / expected) | Soft score (median, feasible) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) |\n");
out.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
```

The new column header reads `Placements (median / expected)`. The divider gains one `---:` (right-aligned) for the new column, lifting the total from eight to nine.

- [ ] **Step 2: Extend `write_row` to render the new column**

Replace the existing `out.push_str(&format!(...))` block with:

```rust
out.push_str(&format!(
    "| {fixture} | {backend} | {seeds} | {n}/{seeds} | {hard} | {placed}/{expected} | {soft} | {ffd:.2} | {total:.0} |\n",
    backend = backend.label(),
    seeds = cell.seeds,
    n = cell.feasibility_count,
    hard = cell.hard_violations_median,
    placed = cell.placements_total_median,
    expected = cell.placements_expected,
    ffd = cell.ffd_ms_median,
    total = cell.total_ms_median,
));
```

The new column lands between the hard-violations column and the soft-score column, mirroring the column order chosen in the spec.

- [ ] **Step 3: Run the new tests; expect pass**

Run: `cargo nextest run -p solver-bench write_row_renders_placements_column write_row_renders_underflow_placement_count`
Expected: PASS.

- [ ] **Step 4: Run the full bench unit suite; expect pass**

Run: `cargo nextest run -p solver-bench`
Expected: PASS (all eleven-or-so tests, including the four `write_row_renders_*_backend_label` tests that did not previously assert against the new column).

---

## Task 6: Wire `expected` through `run_cell`, `run_cpsat_cell`, and `main` (functional change)

**Files:**
- Modify: `solver/solver-bench/src/main.rs`

- [ ] **Step 1: Extend `run_cell` signature and body**

Change the function signature from:

```rust
fn run_cell(backend: BenchBackend, problem: &Problem, budget: Duration, seeds: u64) -> CellResult {
```

to:

```rust
fn run_cell(
    backend: BenchBackend,
    problem: &Problem,
    expected: u64,
    budget: Duration,
    seeds: u64,
) -> CellResult {
```

Inside the body, add `let mut placements_total_samples: Vec<u64> = Vec::with_capacity(seeds as usize);` next to the other sample-collection vectors.

In the per-seed loop (the `for seed in 1..=seeds` block), replace:

```rust
let solution = solve_with_config(problem, &cfg).expect("solve");
let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
let hard = solution.violations.len() as u32;
let feasible = hard == 0;
if feasible {
    feasibility_count += 1;
    soft_score_feasible.push(solution.soft_score);
}
hard_violations_samples.push(hard);
total_ms_samples.push(total_ms);
```

with:

```rust
let solution = solve_with_config(problem, &cfg).expect("solve");
let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
let hard = solution.violations.len() as u32;
let placements_total = solution.placements.len() as u64;
debug_assert!(
    placements_total <= expected,
    "placements_total ({placements_total}) > expected ({expected}); structural invariant violated",
);
let feasible = hard == 0 && placements_total == expected;
if feasible {
    feasibility_count += 1;
    soft_score_feasible.push(solution.soft_score);
}
hard_violations_samples.push(hard);
total_ms_samples.push(total_ms);
placements_total_samples.push(placements_total);
```

In the trailing `CellResult { ... }` literal, replace the two placeholder `0`s with the real values:

```rust
placements_total_median: median_u64(&mut placements_total_samples),
placements_expected: expected,
```

- [ ] **Step 2: Extend `run_cpsat_cell` signature and body**

Mirror Task 6 step 1 in `run_cpsat_cell`. Add `expected: u64` as the third parameter (before `budget`). Add `let mut placements_total_samples: Vec<u64> = Vec::with_capacity(seeds as usize);` next to the other sample vectors.

In the per-seed loop, replace the existing block ending at `total_ms_samples.push(total_ms);` with the placements-aware version. Critically, every `continue` path on subprocess error must push `0` to `placements_total_samples`:

```rust
let solution_json = match result {
    Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
    Ok(o) => {
        eprintln!(
            "cpsat subprocess non-zero exit (seed={seed}): {}",
            String::from_utf8_lossy(&o.stderr)
        );
        hard_violations_samples.push(u32::MAX);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(0);
        continue;
    }
    Err(e) => {
        eprintln!("cpsat subprocess error (seed={seed}): {e}");
        hard_violations_samples.push(u32::MAX);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(0);
        continue;
    }
};
let solution: solver_core::Solution = match serde_json::from_str(&solution_json) {
    Ok(s) => s,
    Err(e) => {
        eprintln!("cpsat parse error (seed={seed}): {e}");
        hard_violations_samples.push(u32::MAX);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(0);
        continue;
    }
};
let hard = solution.violations.len() as u32;
let placements_total = solution.placements.len() as u64;
debug_assert!(
    placements_total <= expected,
    "cpsat placements_total ({placements_total}) > expected ({expected}); structural invariant violated",
);
let feasible = hard == 0 && placements_total == expected;
if feasible {
    feasibility_count += 1;
    let soft = solver_core::score_solution(
        problem,
        &solution.placements,
        &solver_core::PRODUCTION_ACTIVE_WEIGHTS,
    );
    soft_score_feasible.push(soft);
}
hard_violations_samples.push(hard);
total_ms_samples.push(total_ms);
placements_total_samples.push(placements_total);
```

In the trailing `CellResult { ... }` literal, replace the two placeholder `0`s with:

```rust
placements_total_median: median_u64(&mut placements_total_samples),
placements_expected: expected,
```

- [ ] **Step 3: Update the dispatch wrapper**

Find the early return at the top of `run_cell`:

```rust
if let BenchBackend::CpSat = backend {
    return run_cpsat_cell(problem, budget, seeds);
}
```

and replace with:

```rust
if let BenchBackend::CpSat = backend {
    return run_cpsat_cell(problem, expected, budget, seeds);
}
```

- [ ] **Step 4: Compute `expected` in `main` and thread into the call**

Find the inner fixture loop in `main`:

```rust
for (name, build) in FIXTURES {
    if !args.fixtures.iter().any(|f| f == name) {
        continue;
    }
    let problem = build();
    for backend in &backends {
        eprintln!("cell start: {} / {}", name, backend.label());
        let cell = run_cell(*backend, &problem, args.budget, args.seeds);
        eprintln!(
            "cell done: {} / {} feasibility {}/{} hard_med={} soft_med={} total_ms_med={:.0}",
            name,
            backend.label(),
            cell.feasibility_count,
            cell.seeds,
            cell.hard_violations_median,
            cell.soft_score_median
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".to_string()),
            cell.total_ms_median,
        );
        write_row(&mut markdown, name, *backend, &cell);
    }
}
```

Replace with:

```rust
for (name, build) in FIXTURES {
    if !args.fixtures.iter().any(|f| f == name) {
        continue;
    }
    let problem = build();
    let expected = placements_expected_for_problem(&problem);
    for backend in &backends {
        eprintln!("cell start: {} / {}", name, backend.label());
        let cell = run_cell(*backend, &problem, expected, args.budget, args.seeds);
        eprintln!(
            "cell done: {} / {} feasibility {}/{} hard_med={} placements_med={}/{} soft_med={} total_ms_med={:.0}",
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
        );
        write_row(&mut markdown, name, *backend, &cell);
    }
}
```

- [ ] **Step 5: Run the full unit suite again**

Run: `cargo nextest run -p solver-bench`
Expected: PASS. The two new fields in `CellResult` are now populated by real data; the unit tests still construct synthetic `CellResult` literals so their expected values stay byte-identical.

- [ ] **Step 6: Run `mise run lint` and confirm clippy `-D warnings` passes**

Run: `mise run lint`
Expected: PASS. The new helper has its first caller in the same commit (Task 6 step 4 wires it in), so the `dead_code` lint does not trip.

---

## Task 7: Dev-loop bench refresh as receipt (manual)

**Files:**
- No persisted file changes; the receipt is captured for the PR body.

- [ ] **Step 1: Run the bake-off bench at dev-loop settings**

Run: `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule --out /tmp/bench-receipt.md`
Expected: completes in well under a minute and writes the markdown to `/tmp/bench-receipt.md`.

- [ ] **Step 2: Read the receipt**

Run: `cat /tmp/bench-receipt.md`
Expected: the markdown table contains the new column. For each of the four backends on the grundschule row, `Placements (median / expected)` should read `45/45` (45 placements is the grundschule fixture's expected total). If any cell shows underflow at production settings, item 29's investigation surfaces it; for grundschule at 5s/4-seeds we expect all four to read `45/45`.

- [ ] **Step 3: Capture the table for the PR body**

Copy the markdown table out of `/tmp/bench-receipt.md` for inclusion in the PR description's "Receipt" section.

---

## Task 8: Update OPEN_THINGS.md

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Delete item 28 entry from the active sprint**

Open `docs/superpowers/OPEN_THINGS.md`, find the section `### Bench prevention phase` and the item 28 paragraph (a single bullet starting `28. **Add placement-count validation to the bake-off.**`). Delete that entry plus the empty line that separates it from item 29. The `### Bench prevention phase` heading stays; item 29 becomes its sole entry until item 29 also ships.

- [ ] **Step 2: Update the active-sprint pickup pointer at line 9**

Find the line near the top (currently around line 9) reading "Next pickup: P0 item 28 (placement-count validation in the bake-off harness). Items 26 and 27 (the R&R anchor filter and its property-test guards) shipped in PR #TBD."

Replace the first sentence with: "Next pickup: P0 item 29 (refresh `BENCH_RESULTS.md` against the corrected feasibility gate, then write ADR 0032)."

Leave the rest of the paragraph intact.

- [ ] **Step 3: Append two follow-ups to `## Open solver follow-ups`**

Find the `## Open solver follow-ups` heading. After the last entry in that section (currently item 23, `Promote KEMPE_MAX_CHAIN to SolveConfig.lahc_kempe_max_chain`), append two new entries. Use the next-available numbers (continue the sequence; check the file for the highest number in use):

```markdown
35. **Per-cell placement count distribution in the bake-off, not just median.** `[P2]` Bake-off bench reports median placement count per `(fixture, backend)` cell. Bimodal cells (50% of seeds drop placements, 50% do not) render cleanly in the median while half the runs are broken. Land only if item 29's refresh data shows a cell where `placements_total_median == placements_expected` while `feasibility_count < seeds`; the symptom is the median masking the reality. Mitigation: render an additional `min/max` per cell or a `feasibility-only median` when feasibility < seeds.

36. **Promote `placements_expected_for_problem` to `pub fn` in `solver-core`.** `[P2]` Today the helper lives in `solver/solver-bench/src/main.rs` because the bench is the only caller. If a Python harness in `klassenzeit_solver` (e.g. a CP-SAT-side validator) wants to double-check expected placement count without re-implementing the sum, promote the helper to `pub fn solver_core::placements_expected(problem: &Problem) -> u64` and bind through `solver-py`.
```

- [ ] **Step 4: Verify the file still parses cleanly**

Run: `head -100 docs/superpowers/OPEN_THINGS.md`
Expected: the active sprint pointer reads the new "next pickup" sentence; item 28 is gone from the Bench prevention phase; the rest of the file is intact.

---

## Task 9: Atomic feat commit

**Files:**
- All modifications from Tasks 1, 3, 4 (test additions), 5, 6 land in one commit.

- [ ] **Step 1: Stage the bench file**

Run: `git add solver/solver-bench/src/main.rs`

- [ ] **Step 2: Create the feat commit**

Use a HEREDOC commit message:

```
git commit -m "$(cat <<'EOF'
feat(solver-bench): gate cell feasibility on placement count

Bake-off cells now require both `violations.is_empty()` and
`placements.len() == sum(hours_per_week)`. Adds a
`Placements (median / expected)` column to BENCH_RESULTS.md so a
future placement-drop regression cannot pass the harness silently.

R&R can drop 60+ percent of placements without growing `violations`
(the violation list is the FFD-time view; LAHC moves do not append to
it on silent drops). Items 26 and 27 fixed the underlying drop in
`rr_collect_anchors`; this commit closes the prevention hole that hid
the bug from the bench.

OPEN_THINGS item 28.
EOF
)"
```

- [ ] **Step 3: Stage and commit the OPEN_THINGS update**

Run: `git add docs/superpowers/OPEN_THINGS.md`
Run:

```
git commit -m "$(cat <<'EOF'
docs: close OPEN_THINGS item 28 + queue placement-count follow-ups

Item 28 ships in this PR; item 29 (production bench refresh + ADR 0032)
becomes the active-sprint pickup. Two conditional follow-ups appended
to `Open solver follow-ups` for whether bench data eventually demands a
distribution column or a `pub fn` placements_expected in solver-core.
EOF
)"
```

---

## Task 10: Pre-push validation

**Files:**
- None; this is a pre-push gate.

- [ ] **Step 1: Run the full Rust test suite**

Run: `mise run test:rust`
Expected: PASS.

- [ ] **Step 2: Run `mise run lint`**

Run: `mise run lint`
Expected: PASS.

- [ ] **Step 3: Push (lefthook runs the full pre-push suite)**

Run: `mise exec -- git push -u origin feat/bench-placement-count-validation`
Expected: lefthook runs `cargo nextest run --workspace`, `uv run pytest`, and the frontend Vitest suite, all green; push completes.

---

## Self-Review

Spec coverage:
- Helper at module scope: Task 3.
- `CellResult` extension: Task 1.
- `median_u64`: Task 1.
- Threading `expected` through `run_cell` / `run_cpsat_cell` / `main`: Task 6.
- New `Placements (median / expected)` markdown column: Task 5.
- CP-SAT subprocess error continue paths push `0`: Task 6 step 2.
- `debug_assert!` on overshoot: Task 6 step 1 + step 2.
- Three new unit tests (`placements_expected_for_problem_sums_hours_per_week`, `write_row_renders_placements_column`, `write_row_renders_underflow_placement_count`): Tasks 2 + 4.
- Update existing `write_row_*` tests to populate new fields: Task 1 step 5.
- Dev-loop receipt: Task 7.
- OPEN_THINGS item 28 deletion + active-sprint pickup pointer + two follow-ups: Task 8.
- `BENCH_RESULTS.md` not refreshed: deliberate per spec; deferred to item 29.

Placeholder scan: no "TBD" or "implement later" markers remain. Every code step shows the actual code to write or the actual line to replace.

Type consistency: `placements_total_median: u64` and `placements_expected: u64` used consistently across `CellResult`, `run_cell`, `run_cpsat_cell`, the new median helper (`median_u64`), and the unit tests. `expected: u64` is the parameter name in both run-cell variants. `placements_total_samples: Vec<u64>` is the sample-collection vector.

Plan looks complete. Item 29 is the next pickup once this lands.
