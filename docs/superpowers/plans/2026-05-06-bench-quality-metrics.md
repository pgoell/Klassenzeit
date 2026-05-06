# Bench Schedule-Quality Metrics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add four schedule-quality metric columns plus a composite `Quality (pass / 4)` column to `solver/solver-core/benches/BENCH_RESULTS.md` so a winner cannot be picked on hard-feasibility alone (active-sprint OPEN_THINGS item 31).

**Architecture:** New `solver/solver-bench/src/quality.rs` module with four pure predicates over `&Problem` + `&Solution`: `worst_class_day_spread`, `worst_home_room_ratio` (exempts subjects whose `room_subject_suitabilities` exclude the class's `home_room_id`), `total_interior_gaps`, `late_period_ratio` (uses `Subject.prefer_late_period > 0` as the late-preferred proxy; renders `n/a` when no fixture subject has the axis enabled). Cell-child accumulates per-feasible-seed `QualityReport`s and aggregates medians; supervisor renders five new markdown columns. No changes to `solver-core`, no changes to `solver-py` Python, no changes to backend.

**Tech Stack:** Rust 2021 (`solver-bench` crate); `solver-core` types (`Problem`, `Solution`, `Placement`, `RoomId`, `SchoolClassId`, `SubjectId`); existing `serde_json` deserialisation for the cpsat path; existing `cargo nextest` test runner via `mise run test:rust`. No new workspace dependencies.

**Spec:** `docs/superpowers/specs/2026-05-06-bench-quality-metrics-design.md`. **Brainstorm:** `/tmp/kz-brainstorm/brainstorm.md` (this run).

---

## File Structure

- **Create:** `solver/solver-bench/src/quality.rs`. Module-private helpers + pub `QualityReport`, four pub `QUALITY_*` thresholds, `pub fn evaluate_quality`, `pub fn quality_pass_count`. Inline `#[cfg(test)] mod tests`.
- **Modify:** `solver/solver-bench/src/main.rs`. Declare `mod quality;` at top. Extend `CellResult` with five fields. Extend `run_lahc_cell` and `run_cpsat_cell` to accumulate per-seed `QualityReport`s and emit medians. Extend `write_header`, `write_row`, `write_footer`. Extend the cell-done `eprintln!`. Add small helper `aggregate_quality_medians`. Extend the existing `#[cfg(test)] mod tests`.
- **Modify:** `solver/solver-bench/tests/end_to_end.rs`. Extend the existing test's assertions to cover the five new column headers.
- **Modify:** `docs/superpowers/OPEN_THINGS.md`. Delete item 31 from the observability phase. Update the active-sprint preamble. Add two follow-ups under sprint-tidy phase.
- **Modify:** `solver/CLAUDE.md`. Add one bullet under "Bench workflow" pointing at `solver-bench/src/quality.rs`.
- **Modify:** `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`. Body and `description` field both refreshed.

---

## Task 1: Spec + plan land first (autopilot step prereq)

**Files:**
- Create: `docs/superpowers/specs/2026-05-06-bench-quality-metrics-design.md` (already done in a prior step)
- Create: `docs/superpowers/plans/2026-05-06-bench-quality-metrics.md` (this file)

- [ ] **Step 1: Commit the spec and plan together**

```bash
git add docs/superpowers/specs/2026-05-06-bench-quality-metrics-design.md docs/superpowers/plans/2026-05-06-bench-quality-metrics.md
git commit -m "docs: bench schedule-quality metrics spec + plan (item 31)"
```

The pre-commit hook runs lint; docs-only commit should clear.

---

## Task 2: Quality module skeleton + red unit tests

**Files:**
- Create: `solver/solver-bench/src/quality.rs`
- Modify: `solver/solver-bench/src/main.rs` (add `mod quality;` at top; extend `CellResult` with five `Option`-typed fields; touch up the two existing render tests so the `CellResult` literal still compiles)

This task lands the typed surface plus the inline tests. Bodies are `unimplemented!()` so tests fail at panic. Compilation passes; pre-push will reject if we attempt to push standalone, but the autopilot pushes only at the end of step 7.

- [ ] **Step 1: Write the new module**

Create `solver/solver-bench/src/quality.rs`:

```rust
//! Schedule-quality predicates for bake-off cells.
//!
//! Mirrors the predicates `backend/src/klassenzeit_backend/scheduling/quality_checks.py`
//! enforces in the demo Grundschule integration test. The Python and Rust
//! implementations are intentionally separate: the Python version operates on
//! persisted ORM rows with a hand-supplied exempt-subjects set; the Rust
//! version operates on the in-memory [`Solution`] and infers exempt subjects
//! from [`Problem::room_subject_suitabilities`]. Cross-language parity is not
//! a contract; the two are designed to drift around their respective inputs.

use std::collections::{HashMap, HashSet};

use solver_core::{Placement, Problem, RoomId, SchoolClassId, Solution, SubjectId};

/// Threshold: a class's daily-load spread (max - min across the school week)
/// must not exceed this for the spread predicate to pass. Mirrors the Python
/// test's `check_class_day_balance(max_spread=2)`.
pub const QUALITY_MAX_SPREAD: u32 = 2;

/// Threshold: a class's non-exempt home-room hit rate must meet or exceed this.
/// Mirrors the Python test's `check_home_room_ratio(min_ratio=0.6, ...)`.
pub const QUALITY_MIN_HOME_ROOM_RATIO: f64 = 0.6;

/// Threshold: total interior gaps summed across (class, day) partitions must
/// not exceed this. Mirrors the Python test's
/// `check_interior_gaps(max_gaps_per_class=2)`.
pub const QUALITY_MAX_INTERIOR_GAPS: u32 = 2;

/// Threshold: median normalised position of placements of late-preferred
/// subjects must meet or exceed this (0.5 = latter half of the day).
/// Borrowed from OPEN_THINGS item 14's xfail bar.
pub const QUALITY_MIN_LATE_PERIOD_RATIO: f64 = 0.5;

/// Per-cell quality summary returned by [`evaluate_quality`]. All four metrics
/// are pure functions over [`Problem`] + [`Solution`]; `None` on either ratio
/// means "no relevant placements to evaluate" and counts as a pass for the
/// composite predicate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QualityReport {
    /// Max over classes of `max_lessons_in_day - min_lessons_in_day` across
    /// `day_of_week ∈ 0..5`. Empty schedule returns 0.
    pub worst_spread: u32,
    /// Min over classes of `non_exempt_home_room_hits / non_exempt_placements`.
    /// `None` when no class has any non-exempt placements (e.g. fixture has
    /// no `home_room_id` set on any class).
    pub worst_home_room_ratio: Option<f64>,
    /// Sum over `(class, day)` partitions of `last_position - first_position + 1 - count`.
    pub total_interior_gaps: u32,
    /// Median across all placements of late-preferred subjects of
    /// `position / max_position_per_day(day_of_week)`. `None` when no
    /// subject has `prefer_late_period > 0` or no such placements exist.
    pub late_period_ratio: Option<f64>,
}

/// Pure function over [`Problem`] + [`Solution`]. See module rustdoc for the
/// per-predicate semantics. Never panics; treats empty placements gracefully.
pub fn evaluate_quality(_problem: &Problem, _solution: &Solution) -> QualityReport {
    unimplemented!("Task 3 implements")
}

/// Returns the count (0..=4) of predicates that pass at the configured
/// thresholds. `None` ratios count as passing (vacuous truth).
pub fn quality_pass_count(report: &QualityReport) -> u32 {
    let mut n = 0;
    if report.worst_spread <= QUALITY_MAX_SPREAD {
        n += 1;
    }
    if report
        .worst_home_room_ratio
        .map_or(true, |v| v >= QUALITY_MIN_HOME_ROOM_RATIO)
    {
        n += 1;
    }
    if report.total_interior_gaps <= QUALITY_MAX_INTERIOR_GAPS {
        n += 1;
    }
    if report
        .late_period_ratio
        .map_or(true, |v| v >= QUALITY_MIN_LATE_PERIOD_RATIO)
    {
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use solver_core::test_fixtures::grundschule_fixture;
    use solver_core::types::SolveConfig;
    use solver_core::{solve_with_config, ConstraintWeights, PRODUCTION_ACTIVE_WEIGHTS};

    #[test]
    fn quality_pass_count_treats_none_ratios_as_pass() {
        let report = QualityReport {
            worst_spread: 0,
            worst_home_room_ratio: None,
            total_interior_gaps: 0,
            late_period_ratio: None,
        };
        assert_eq!(quality_pass_count(&report), 4);
    }

    #[test]
    fn quality_pass_count_counts_each_failing_predicate() {
        let report = QualityReport {
            worst_spread: 5,                       // fail
            worst_home_room_ratio: Some(0.3),      // fail
            total_interior_gaps: 10,               // fail
            late_period_ratio: Some(0.2),          // fail
        };
        assert_eq!(quality_pass_count(&report), 0);

        let report = QualityReport {
            worst_spread: 2,                       // pass
            worst_home_room_ratio: Some(0.7),      // pass
            total_interior_gaps: 0,                // pass
            late_period_ratio: Some(0.4),          // fail
        };
        assert_eq!(quality_pass_count(&report), 3);
    }

    #[test]
    fn quality_report_default_passes_every_predicate() {
        let report = QualityReport::default();
        assert_eq!(quality_pass_count(&report), 4);
    }

    #[test]
    fn evaluate_quality_grundschule_fixture_passes_three_or_four_predicates() {
        // Greedy-only solve per solver/CLAUDE.md: pin solver-core unit tests
        // to greedy when wall-clock cost matters. The bench's actual output
        // uses LAHC and reports the real number; this unit test checks the
        // predicate plumbing on a real fixture without paying LAHC's budget.
        let problem = grundschule_fixture();
        let cfg = SolveConfig {
            weights: PRODUCTION_ACTIVE_WEIGHTS.clone(),
            deadline: None,
            ..SolveConfig::default()
        };
        let solution = solve_with_config(&problem, &cfg).expect("solve");
        let report = evaluate_quality(&problem, &solution);
        let n = quality_pass_count(&report);
        assert!(
            n >= 3,
            "expected at least 3 of 4 predicates to pass on grundschule greedy: {report:?}",
        );
    }
}
```

- [ ] **Step 2: Wire the module into main.rs and extend CellResult**

Modify `solver/solver-bench/src/main.rs`. At the top of the file (immediately after the `//! ADR: ...` doc-comment block), add:

```rust
mod quality;
```

Find the existing `CellResult` struct definition. Append five fields:

```rust
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CellResult {
    seeds: u64,
    feasibility_count: u64,
    hard_violations_median: u32,
    placements_total_median: u64,
    placements_expected: u64,
    soft_score_median: Option<u32>,
    ffd_ms_median: f64,
    total_ms_median: f64,
    peak_kb: u64,
    time_to_first_feasible_ms_median: Option<f64>,
    time_to_optimal_ms_median: Option<f64>,
    worst_spread_median: Option<u32>,
    worst_home_room_ratio_median: Option<f64>,
    total_interior_gaps_median: Option<u32>,
    late_period_ratio_median: Option<f64>,
    quality_pass_count_median: Option<u32>,
}
```

Find every `CellResult { ... }` literal in `main.rs` (currently three: end of `run_lahc_cell`, end of `run_cpsat_cell`, and three inside `#[cfg(test)] mod tests`). Add the five new fields with `None` / `0` defaults so the file still compiles. The cell-functions get real values in Task 4.

- [ ] **Step 3: Extend the inline render tests with red assertions**

Inside `#[cfg(test)] mod tests` in `solver/solver-bench/src/main.rs`, find `cell_result_round_trips_through_json`. Extend the constructed `CellResult` literal to cover the five new fields and assert they round-trip:

```rust
#[test]
fn cell_result_round_trips_through_json() {
    let cell = CellResult {
        seeds: 4,
        feasibility_count: 4,
        hard_violations_median: 0,
        placements_total_median: 45,
        placements_expected: 45,
        soft_score_median: Some(15),
        ffd_ms_median: 0.5,
        total_ms_median: 60000.0,
        peak_kb: 12345,
        time_to_first_feasible_ms_median: Some(2.5),
        time_to_optimal_ms_median: Some(40.0),
        worst_spread_median: Some(2),
        worst_home_room_ratio_median: Some(0.75),
        total_interior_gaps_median: Some(1),
        late_period_ratio_median: Some(0.6),
        quality_pass_count_median: Some(4),
    };
    let s = serde_json::to_string(&cell).unwrap();
    let back: CellResult = serde_json::from_str(&s).unwrap();
    assert_eq!(back, cell);
}
```

Add three new tests in the same module:

```rust
#[test]
fn write_header_includes_five_quality_columns() {
    let mut out = String::new();
    write_header(&mut out);
    assert!(out.contains("Worst spread (median)"), "missing worst-spread header: {out}");
    assert!(out.contains("Worst home-room ratio (median)"), "missing home-room header: {out}");
    assert!(out.contains("Total interior gaps (median)"), "missing gaps header: {out}");
    assert!(out.contains("Late-period ratio (median)"), "missing late-period header: {out}");
    assert!(out.contains("Quality (pass / 4)"), "missing quality column header: {out}");
}

#[test]
fn write_row_renders_quality_columns() {
    let cell = CellResult {
        seeds: 20,
        feasibility_count: 20,
        hard_violations_median: 0,
        placements_total_median: 45,
        placements_expected: 45,
        soft_score_median: Some(0),
        ffd_ms_median: 0.13,
        total_ms_median: 60000.0,
        peak_kb: 49152,
        time_to_first_feasible_ms_median: Some(0.4),
        time_to_optimal_ms_median: Some(1500.0),
        worst_spread_median: Some(2),
        worst_home_room_ratio_median: Some(0.75),
        total_interior_gaps_median: Some(1),
        late_period_ratio_median: Some(0.6),
        quality_pass_count_median: Some(4),
    };
    let mut out = String::new();
    write_row(&mut out, "grundschule", BenchBackend::LahcRrKempe, &cell);
    assert!(out.contains("| 2 |"), "missing worst spread: {out}");
    assert!(out.contains("| 0.75 |"), "missing home-room ratio: {out}");
    assert!(out.contains("| 1 |"), "missing interior gaps: {out}");
    assert!(out.contains("| 0.60 |") || out.contains("| 0.6 |"), "missing late-period: {out}");
    assert!(out.contains("| 4/4 |"), "missing quality pass count: {out}");
}

#[test]
fn write_row_renders_dash_when_quality_fields_are_none() {
    let cell = CellResult {
        seeds: 20,
        feasibility_count: 0,
        hard_violations_median: 1,
        placements_total_median: 0,
        placements_expected: 0,
        soft_score_median: None,
        ffd_ms_median: 0.05,
        total_ms_median: 60050.0,
        peak_kb: 49152,
        time_to_first_feasible_ms_median: None,
        time_to_optimal_ms_median: None,
        worst_spread_median: None,
        worst_home_room_ratio_median: None,
        total_interior_gaps_median: None,
        late_period_ratio_median: None,
        quality_pass_count_median: None,
    };
    let mut out = String::new();
    write_row(&mut out, "grundschule", BenchBackend::Lahc, &cell);
    // Five dash-cells appended at the right end (worst spread, home-room
    // ratio, interior gaps, late-period, quality).
    let dash_count = out.matches("| - |").count();
    assert!(dash_count >= 5, "expected at least 5 dashes for quality cells: {out}");
}
```

Update the existing `write_row_renders_dash_when_no_feasible_seed` test: extend its `CellResult` literal with the five new fields all `None`. The existing dash-count assertion still holds (it asserts `out.contains("| - |")`).

- [ ] **Step 4: Run the new and existing tests; verify the four new tests fail**

```bash
cargo nextest run -p solver-bench --bin solver-bench --no-tests=pass
```

Expected:
- `quality::tests::quality_pass_count_treats_none_ratios_as_pass` PASS (no `unimplemented!()` on this path)
- `quality::tests::quality_pass_count_counts_each_failing_predicate` PASS
- `quality::tests::quality_report_default_passes_every_predicate` PASS
- `quality::tests::evaluate_quality_grundschule_fixture_passes_three_or_four_predicates` FAIL with `unimplemented!("Task 3 implements")`
- `tests::cell_result_round_trips_through_json` PASS (round-trip works on the extended struct)
- `tests::write_header_includes_five_quality_columns` FAIL (no new headers yet)
- `tests::write_row_renders_quality_columns` FAIL (no new render rules yet)
- `tests::write_row_renders_dash_when_quality_fields_are_none` may PASS or FAIL depending on whether row already prints `| - |` cells; if it FAILs, that's expected (the new fields aren't rendered yet).

Also: `cargo nextest run -p solver-bench --test end_to_end` should still pass (no new column header assertions yet).

Lint:

```bash
cargo clippy -p solver-bench --all-targets -- -D warnings
```

The new module's `unimplemented!()` may trip `clippy::missing_panics_doc` — add `# Panics` to `evaluate_quality`'s rustdoc only if clippy complains:

```rust
/// # Panics
///
/// Panics in Task 2's stub form; Task 3 fills in the body.
```

Remove the panics-doc block when Task 3 lands.

- [ ] **Step 5: Commit the red**

```bash
git add solver/solver-bench/src/quality.rs solver/solver-bench/src/main.rs
git commit -m "test(solver-bench): unit tests for quality.rs predicates and CellResult quality fields (item 31)"
```

Commit body explains: stub bodies (`unimplemented!()`) fail at runtime; Task 3 fills them in. Per `solver/CLAUDE.md`'s "Bundle a new `pub(crate)` helper with its first caller in the same commit" rule, the public surface is bundled with its inline-test caller in this commit, satisfying clippy's `dead_code` lint.

---

## Task 3: Implement the four predicates

**Files:**
- Modify: `solver/solver-bench/src/quality.rs` (replace the four `unimplemented!()` bodies with real implementations + helpers)

- [ ] **Step 1: Implement worst_class_day_spread**

Inside `quality.rs`, append (above the `#[cfg(test)] mod tests` block) the helper:

```rust
fn worst_class_day_spread(_problem: &Problem, solution: &Solution) -> u32 {
    // Day axis runs 0..5 (Mon-Fri); SchoolClass.day_of_week assumed in that
    // range per `solver-core/src/types.rs::TimeBlock::day_of_week`.
    let mut counts: HashMap<SchoolClassId, [u32; 5]> = HashMap::new();
    let tb_day: HashMap<_, _> = _problem
        .time_blocks
        .iter()
        .map(|tb| (tb.id, tb.day_of_week))
        .collect();
    let lesson_classes: HashMap<_, _> = _problem
        .lessons
        .iter()
        .map(|l| (l.id, &l.school_class_ids))
        .collect();
    for placement in &solution.placements {
        let day = match tb_day.get(&placement.time_block_id).copied() {
            Some(d) if d < 5 => d as usize,
            _ => continue,
        };
        let classes = match lesson_classes.get(&placement.lesson_id).copied() {
            Some(c) => c,
            None => continue,
        };
        for class_id in classes {
            counts.entry(*class_id).or_insert([0; 5])[day] += 1;
        }
    }
    counts
        .values()
        .map(|per_day| per_day.iter().max().copied().unwrap_or(0)
            - per_day.iter().min().copied().unwrap_or(0))
        .max()
        .unwrap_or(0)
}
```

- [ ] **Step 2: Implement total_interior_gaps**

Append:

```rust
fn total_interior_gaps(problem: &Problem, solution: &Solution) -> u32 {
    let tb_meta: HashMap<_, _> = problem
        .time_blocks
        .iter()
        .map(|tb| (tb.id, (tb.day_of_week, tb.position)))
        .collect();
    let lesson_classes: HashMap<_, _> = problem
        .lessons
        .iter()
        .map(|l| (l.id, &l.school_class_ids))
        .collect();
    let mut positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    for placement in &solution.placements {
        let (day, pos) = match tb_meta.get(&placement.time_block_id).copied() {
            Some(p) => p,
            None => continue,
        };
        let classes = match lesson_classes.get(&placement.lesson_id).copied() {
            Some(c) => c,
            None => continue,
        };
        for class_id in classes {
            positions.entry((*class_id, day)).or_default().push(pos);
        }
    }
    let mut total = 0u32;
    for ps in positions.values_mut() {
        ps.sort_unstable();
        ps.dedup();
        if let (Some(&first), Some(&last)) = (ps.first(), ps.last()) {
            let span = (last - first + 1) as u32;
            let gaps = span.saturating_sub(ps.len() as u32);
            total = total.saturating_add(gaps);
        }
    }
    total
}
```

- [ ] **Step 3: Implement worst_home_room_ratio with exempt-subject inference**

Append:

```rust
fn worst_home_room_ratio(
    problem: &Problem,
    solution: &Solution,
    home_rooms: &HashMap<SchoolClassId, RoomId>,
) -> Option<f64> {
    // Exempt set per (class, subject): if the subject has any
    // room_subject_suitabilities row and the class's home_room_id is NOT in
    // that subject's suitable rooms, the subject is exempt for that class.
    let mut suitable_rooms_per_subject: HashMap<SubjectId, HashSet<RoomId>> = HashMap::new();
    for s in &problem.room_subject_suitabilities {
        suitable_rooms_per_subject
            .entry(s.subject_id)
            .or_default()
            .insert(s.room_id);
    }

    let lesson_meta: HashMap<_, _> = problem
        .lessons
        .iter()
        .map(|l| (l.id, (l.subject_id, &l.school_class_ids)))
        .collect();

    // (class_id, subject_id) -> exempt?
    let mut exempt: HashMap<(SchoolClassId, SubjectId), bool> = HashMap::new();
    for class in &problem.school_classes {
        let home = match home_rooms.get(&class.id).copied() {
            Some(r) => r,
            None => continue,
        };
        for subject in &problem.subjects {
            let is_exempt = match suitable_rooms_per_subject.get(&subject.id) {
                Some(rooms) => !rooms.contains(&home),
                None => false,
            };
            exempt.insert((class.id, subject.id), is_exempt);
        }
    }

    // (class_id) -> (hits, total)
    let mut counts: HashMap<SchoolClassId, (u32, u32)> = HashMap::new();
    for placement in &solution.placements {
        let (subject_id, classes) = match lesson_meta.get(&placement.lesson_id).copied() {
            Some(m) => m,
            None => continue,
        };
        for class_id in classes {
            let home = match home_rooms.get(class_id).copied() {
                Some(r) => r,
                None => continue,
            };
            if exempt.get(&(*class_id, subject_id)).copied().unwrap_or(false) {
                continue;
            }
            let entry = counts.entry(*class_id).or_insert((0, 0));
            entry.1 += 1;
            if placement.room_id == home {
                entry.0 += 1;
            }
        }
    }

    let ratios: Vec<f64> = counts
        .values()
        .filter(|(_, total)| *total > 0)
        .map(|(hits, total)| f64::from(*hits) / f64::from(*total))
        .collect();
    if ratios.is_empty() {
        return None;
    }
    Some(ratios.iter().copied().fold(f64::INFINITY, f64::min))
}
```

- [ ] **Step 4: Implement late_period_ratio**

Append:

```rust
fn late_period_ratio(problem: &Problem, solution: &Solution) -> Option<f64> {
    let late_subjects: HashSet<SubjectId> = problem
        .subjects
        .iter()
        .filter(|s| s.prefer_late_period > 0)
        .map(|s| s.id)
        .collect();
    if late_subjects.is_empty() {
        return None;
    }

    let mut max_pos_per_day: HashMap<u8, u8> = HashMap::new();
    for tb in &problem.time_blocks {
        let entry = max_pos_per_day.entry(tb.day_of_week).or_insert(0);
        if tb.position > *entry {
            *entry = tb.position;
        }
    }

    let tb_meta: HashMap<_, _> = problem
        .time_blocks
        .iter()
        .map(|tb| (tb.id, (tb.day_of_week, tb.position)))
        .collect();
    let lesson_subject: HashMap<_, _> = problem
        .lessons
        .iter()
        .map(|l| (l.id, l.subject_id))
        .collect();

    let mut ratios: Vec<f64> = Vec::new();
    for placement in &solution.placements {
        let subject_id = match lesson_subject.get(&placement.lesson_id).copied() {
            Some(s) => s,
            None => continue,
        };
        if !late_subjects.contains(&subject_id) {
            continue;
        }
        let (day, pos) = match tb_meta.get(&placement.time_block_id).copied() {
            Some(m) => m,
            None => continue,
        };
        let max_pos = max_pos_per_day.get(&day).copied().unwrap_or(0);
        if max_pos == 0 {
            continue;
        }
        ratios.push(f64::from(pos) / f64::from(max_pos));
    }
    if ratios.is_empty() {
        return None;
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(ratios[ratios.len() / 2])
}
```

- [ ] **Step 5: Wire `evaluate_quality` to the four helpers**

Replace the `unimplemented!()` body:

```rust
pub fn evaluate_quality(problem: &Problem, solution: &Solution) -> QualityReport {
    let home_rooms: HashMap<SchoolClassId, RoomId> = problem
        .school_classes
        .iter()
        .filter_map(|c| c.home_room_id.map(|r| (c.id, r)))
        .collect();
    QualityReport {
        worst_spread: worst_class_day_spread(problem, solution),
        worst_home_room_ratio: worst_home_room_ratio(problem, solution, &home_rooms),
        total_interior_gaps: total_interior_gaps(problem, solution),
        late_period_ratio: late_period_ratio(problem, solution),
    }
}
```

Remove the `# Panics` rustdoc block from Task 2 if it was added.

- [ ] **Step 6: Add helper-targeted unit tests**

Inside `#[cfg(test)] mod tests`, append targeted tests on the helpers (the existing `evaluate_quality_grundschule_fixture_passes_three_or_four_predicates` test will turn green at this step too; the helpers need their own cases for shrinking debug):

```rust
use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TimeBlockId};
use solver_core::types::{Lesson, Placement as CorePlacement, Problem, RoomSubjectSuitability,
    SchoolClass, Solution, Subject, TimeBlock};
use uuid::Uuid;

fn fixture_uuid(n: u128) -> Uuid {
    Uuid::from_u128(0x10000000_0000_0000_0000_000000000000_u128 | n)
}

fn empty_problem() -> Problem {
    Problem {
        time_blocks: vec![],
        teachers: vec![],
        rooms: vec![],
        subjects: vec![],
        school_classes: vec![],
        lessons: vec![],
        teacher_qualifications: vec![],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

#[test]
fn worst_class_day_spread_returns_zero_for_empty_schedule() {
    let problem = empty_problem();
    let solution = Solution { placements: vec![], violations: vec![], soft_score: 0 };
    assert_eq!(worst_class_day_spread(&problem, &solution), 0);
}

#[test]
fn total_interior_gaps_counts_only_holes_inside_first_last_window() {
    // Class C1 places at (day=0, pos=[0, 2, 3]) -> first=0, last=3, span=4,
    // count=3, gaps=1.
    let class_id = SchoolClassId(fixture_uuid(1));
    let subject_id = SubjectId(fixture_uuid(2));
    let lesson_id = LessonId(fixture_uuid(3));
    let room_id = RoomId(fixture_uuid(4));
    let tb_ids: Vec<TimeBlockId> = (0..4).map(|i| TimeBlockId(fixture_uuid(10 + i))).collect();
    let problem = Problem {
        time_blocks: tb_ids
            .iter()
            .enumerate()
            .map(|(i, id)| TimeBlock { id: *id, day_of_week: 0, position: i as u8 })
            .collect(),
        subjects: vec![Subject {
            id: subject_id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass {
            id: class_id,
            home_room_id: None,
            max_lessons_per_day: None,
        }],
        lessons: vec![Lesson {
            id: lesson_id,
            school_class_ids: vec![class_id],
            subject_id,
            teacher_id: solver_core::TeacherId(fixture_uuid(99)),
            hours_per_week: 3,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        ..empty_problem()
    };
    let solution = Solution {
        placements: vec![
            CorePlacement { lesson_id, time_block_id: tb_ids[0], room_id },
            CorePlacement { lesson_id, time_block_id: tb_ids[2], room_id },
            CorePlacement { lesson_id, time_block_id: tb_ids[3], room_id },
        ],
        violations: vec![],
        soft_score: 0,
    };
    assert_eq!(total_interior_gaps(&problem, &solution), 1);
}

#[test]
fn worst_home_room_ratio_excludes_subjects_unsuitable_for_home_room() {
    // Class C1 home_room=R1. Subjects: S1 (no suitability rows; not exempt),
    // S2 (suitable for R1 only; not exempt), S3 (suitable for R2 only;
    // exempt for class with home_room=R1). Placements: S1 at R1 (hit),
    // S2 at R1 (hit), S3 at R2 (exempt, ignored). Expected ratio = 2/2 = 1.0.
    let class_id = SchoolClassId(fixture_uuid(1));
    let r1 = RoomId(fixture_uuid(20));
    let r2 = RoomId(fixture_uuid(21));
    let s1 = SubjectId(fixture_uuid(30));
    let s2 = SubjectId(fixture_uuid(31));
    let s3 = SubjectId(fixture_uuid(32));
    let l1 = LessonId(fixture_uuid(40));
    let l2 = LessonId(fixture_uuid(41));
    let l3 = LessonId(fixture_uuid(42));
    let tb1 = TimeBlockId(fixture_uuid(50));
    let tb2 = TimeBlockId(fixture_uuid(51));
    let tb3 = TimeBlockId(fixture_uuid(52));
    let teacher = solver_core::TeacherId(fixture_uuid(99));
    let make_subject = |id| Subject {
        id,
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: 0,
        max_hours_per_day: 8,
    };
    let make_lesson = |id, sid| Lesson {
        id,
        school_class_ids: vec![class_id],
        subject_id: sid,
        teacher_id: teacher,
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    };
    let problem = Problem {
        time_blocks: vec![
            TimeBlock { id: tb1, day_of_week: 0, position: 0 },
            TimeBlock { id: tb2, day_of_week: 0, position: 1 },
            TimeBlock { id: tb3, day_of_week: 0, position: 2 },
        ],
        subjects: vec![make_subject(s1), make_subject(s2), make_subject(s3)],
        school_classes: vec![SchoolClass { id: class_id, home_room_id: Some(r1), max_lessons_per_day: None }],
        lessons: vec![make_lesson(l1, s1), make_lesson(l2, s2), make_lesson(l3, s3)],
        room_subject_suitabilities: vec![
            RoomSubjectSuitability { room_id: r1, subject_id: s2 },
            RoomSubjectSuitability { room_id: r2, subject_id: s3 },
        ],
        ..empty_problem()
    };
    let home_rooms: HashMap<_, _> = std::iter::once((class_id, r1)).collect();
    let solution = Solution {
        placements: vec![
            CorePlacement { lesson_id: l1, time_block_id: tb1, room_id: r1 },
            CorePlacement { lesson_id: l2, time_block_id: tb2, room_id: r1 },
            CorePlacement { lesson_id: l3, time_block_id: tb3, room_id: r2 },
        ],
        violations: vec![],
        soft_score: 0,
    };
    let ratio = worst_home_room_ratio(&problem, &solution, &home_rooms);
    assert_eq!(ratio, Some(1.0));
}

#[test]
fn worst_home_room_ratio_returns_none_when_no_class_has_home_room() {
    let problem = empty_problem();
    let solution = Solution { placements: vec![], violations: vec![], soft_score: 0 };
    assert_eq!(worst_home_room_ratio(&problem, &solution, &HashMap::new()), None);
}

#[test]
fn late_period_ratio_returns_none_when_no_subject_prefers_late() {
    let problem = empty_problem();
    let solution = Solution { placements: vec![], violations: vec![], soft_score: 0 };
    assert_eq!(late_period_ratio(&problem, &solution), None);
}

#[test]
fn late_period_ratio_normalises_position_against_max_per_day() {
    let class_id = SchoolClassId(fixture_uuid(1));
    let subject_id = SubjectId(fixture_uuid(2));
    let lesson_id = LessonId(fixture_uuid(3));
    let room_id = RoomId(fixture_uuid(4));
    let tb_ids: Vec<TimeBlockId> = (0..4).map(|i| TimeBlockId(fixture_uuid(10 + i))).collect();
    let teacher = solver_core::TeacherId(fixture_uuid(99));
    let problem = Problem {
        time_blocks: tb_ids
            .iter()
            .enumerate()
            .map(|(i, id)| TimeBlock { id: *id, day_of_week: 0, position: i as u8 })
            .collect(),
        subjects: vec![Subject {
            id: subject_id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 5,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass { id: class_id, home_room_id: None, max_lessons_per_day: None }],
        lessons: vec![Lesson {
            id: lesson_id,
            school_class_ids: vec![class_id],
            subject_id,
            teacher_id: teacher,
            hours_per_week: 3,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        ..empty_problem()
    };
    // Place at positions [0, 2, 3]; max_position_per_day = 3.
    // Normalised ratios: 0/3, 2/3, 3/3 -> sorted [0.0, 0.666, 1.0]; median = 0.666.
    let solution = Solution {
        placements: vec![
            CorePlacement { lesson_id, time_block_id: tb_ids[0], room_id },
            CorePlacement { lesson_id, time_block_id: tb_ids[2], room_id },
            CorePlacement { lesson_id, time_block_id: tb_ids[3], room_id },
        ],
        violations: vec![],
        soft_score: 0,
    };
    let ratio = late_period_ratio(&problem, &solution).expect("late ratio");
    assert!((ratio - 2.0 / 3.0).abs() < 1e-9, "got {ratio}");
}
```

If `solver-core` does not re-export `Lesson`, `Placement` (as the in-memory placement; the test uses `CorePlacement` alias to avoid colliding with the bench's row-renderer fictionalisation if any), `RoomSubjectSuitability`, `Subject`, `SchoolClass`, `Solution`, `TimeBlock`, adjust the `use` paths. The grep run before this plan confirms `solver_core::types::{Lesson, Placement, Problem, RoomSubjectSuitability, SchoolClass, Solution, Subject, TimeBlock}` are public; `solver_core` re-exports them at the crate root. `solver-core::ids` exposes the newtype id constructors.

Add the `uuid = "1"` test-side dep to `solver-bench/Cargo.toml` if not present:

```bash
grep -n '^uuid\b' /home/pascal/Code/Klassenzeit/solver/solver-bench/Cargo.toml || echo "uuid dep missing"
```

If missing, append under a new `[dev-dependencies]` block in `solver/solver-bench/Cargo.toml`:

```toml
[dev-dependencies]
uuid = { workspace = true }
```

Verify `uuid` is in the workspace `[workspace.dependencies]`:

```bash
grep -n '^uuid\b' /home/pascal/Code/Klassenzeit/Cargo.toml
```

If absent there too, add to workspace deps with the version solver-core uses (typically `uuid = { version = "1", features = ["v4", "serde"] }`).

- [ ] **Step 7: Run all unit tests; verify the helper tests pass plus the grundschule fixture test**

```bash
cargo nextest run -p solver-bench --bin solver-bench --no-tests=pass
```

Expected: every test in `quality::tests` passes. The three render tests from Task 2 still fail.

```bash
cargo clippy -p solver-bench --all-targets -- -D warnings
```

Expected: green. Common nits: `unused_imports` if a `use` was added speculatively; `clippy::needless_collect` on the ratios collection (the `f64::min` fold needs a Vec when chained); these are easily fixed inline.

- [ ] **Step 8: Commit**

```bash
git add solver/solver-bench/src/quality.rs solver/solver-bench/Cargo.toml /home/pascal/Code/Klassenzeit/Cargo.toml
git commit -m "feat(solver-bench): quality.rs predicate evaluator (item 31)"
```

Adjust `git add` if `Cargo.toml` files were not modified.

---

## Task 4: Wire per-seed quality accumulation into both cell-children

**Files:**
- Modify: `solver/solver-bench/src/main.rs`

- [ ] **Step 1: Extend run_lahc_cell to accumulate QualityReports**

Inside `run_lahc_cell`, find the per-seed accumulation block and the `if feasible { ... }` branch. Add a `Vec<quality::QualityReport>` accumulator and push on the feasible branch:

```rust
let mut quality_reports: Vec<quality::QualityReport> = Vec::with_capacity(seeds as usize);

for seed in 1..=seeds {
    // ... existing config / solve ...
    if feasible {
        feasibility_count += 1;
        soft_score_feasible.push(solution.soft_score);
        if let Some(t) = stats.time_to_first_feasible_ms {
            ttf_feasible.push(t);
        }
        if let Some(t) = stats.time_to_optimal_ms {
            tto_feasible.push(t);
        }
        quality_reports.push(quality::evaluate_quality(problem, &solution));
    }
    // ... existing accumulators ...
}

let (
    worst_spread_median,
    worst_home_room_ratio_median,
    total_interior_gaps_median,
    late_period_ratio_median,
    quality_pass_count_median,
) = aggregate_quality_medians(&quality_reports);
```

Set the five new `CellResult` fields from the tuple returned by `aggregate_quality_medians`.

- [ ] **Step 2: Extend run_cpsat_cell symmetrically**

Inside `run_cpsat_cell`, the cpsat python child returns a JSON object whose `placements` field is `serde_json::Value`. Convert it on the feasible branch before pushing:

```rust
if feasible {
    // ... existing accumulators ...
    let placements: Vec<solver_core::Placement> =
        serde_json::from_value(parsed.placements.clone())
            .expect("cpsat placements deserialise into Vec<Placement>");
    let solution = solver_core::Solution {
        placements,
        violations: vec![],
        soft_score: parsed.soft_score,
    };
    quality_reports.push(quality::evaluate_quality(problem, &solution));
}
```

The wrapping `solver_core::Solution { ... }` mirrors what the cpsat python emits semantically. `Solution` is constructed only here in the bench; if `solver-core` does not export it directly, use `solver_core::types::Solution`.

Same `aggregate_quality_medians` call at the end. The cpsat `CellResult` literal gets the same five fields populated.

- [ ] **Step 3: Implement aggregate_quality_medians**

Append to `main.rs` (next to the existing `median_*` helpers):

```rust
fn aggregate_quality_medians(
    reports: &[quality::QualityReport],
) -> (Option<u32>, Option<f64>, Option<u32>, Option<f64>, Option<u32>) {
    if reports.is_empty() {
        return (None, None, None, None, None);
    }
    let mut spreads: Vec<u32> = reports.iter().map(|r| r.worst_spread).collect();
    let worst_spread = Some(median_u32(&mut spreads));

    let mut home_room_ratios: Vec<f64> = reports
        .iter()
        .filter_map(|r| r.worst_home_room_ratio)
        .collect();
    let worst_home_room_ratio = if home_room_ratios.is_empty() {
        None
    } else {
        Some(median_f64(&mut home_room_ratios))
    };

    let mut gaps: Vec<u32> = reports.iter().map(|r| r.total_interior_gaps).collect();
    let total_interior_gaps = Some(median_u32(&mut gaps));

    let mut late: Vec<f64> = reports.iter().filter_map(|r| r.late_period_ratio).collect();
    let late_period_ratio = if late.is_empty() {
        None
    } else {
        Some(median_f64(&mut late))
    };

    let mut pass_counts: Vec<u32> = reports
        .iter()
        .map(|r| quality::quality_pass_count(r))
        .collect();
    let quality_pass_count = Some(median_u32(&mut pass_counts));

    (
        worst_spread,
        worst_home_room_ratio,
        total_interior_gaps,
        late_period_ratio,
        quality_pass_count,
    )
}
```

- [ ] **Step 4: Run unit tests**

```bash
cargo nextest run -p solver-bench --bin solver-bench --no-tests=pass
```

Expected: every quality test passes; the inline render tests still fail (those need Task 5's header / row changes).

```bash
cargo clippy -p solver-bench --all-targets -- -D warnings
```

Expected: green.

- [ ] **Step 5: Commit**

```bash
git add solver/solver-bench/src/main.rs
git commit -m "feat(solver-bench): per-seed quality accumulation in cell-child + median aggregate (item 31)"
```

---

## Task 5: Render the five new columns + extended footer

**Files:**
- Modify: `solver/solver-bench/src/main.rs` (write_header, write_row, write_footer, the per-cell `eprintln!`)

- [ ] **Step 1: Extend write_header**

Replace the existing `write_header`:

```rust
fn write_header(out: &mut String) {
    out.push_str("# Solver bake-off feasibility bench\n\n");
    out.push_str("<!-- Regenerated by `mise run bench:bakeoff`. Do not hand-edit. -->\n\n");
    out.push_str(
        "| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Placements (median / expected) | Soft score (median, feasible) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) | Peak RSS (kB) | Time to first feasible (ms, median) | Time to optimal (ms, median) | Worst spread (median) | Worst home-room ratio (median) | Total interior gaps (median) | Late-period ratio (median) | Quality (pass / 4) |\n",
    );
    out.push_str(
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
}
```

- [ ] **Step 2: Extend write_row**

Replace `write_row`:

```rust
fn write_row(out: &mut String, fixture: &str, backend: BenchBackend, cell: &CellResult) {
    let soft = match cell.soft_score_median {
        Some(s) => s.to_string(),
        None => "-".to_string(),
    };
    let ttf = match cell.time_to_first_feasible_ms_median {
        Some(v) => format!("{v:.0}"),
        None => "-".to_string(),
    };
    let tto = match cell.time_to_optimal_ms_median {
        Some(v) => format!("{v:.0}"),
        None => "-".to_string(),
    };
    let worst_spread = match cell.worst_spread_median {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let worst_home = match cell.worst_home_room_ratio_median {
        Some(v) => format!("{v:.2}"),
        None => "-".to_string(),
    };
    let gaps = match cell.total_interior_gaps_median {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let late = match cell.late_period_ratio_median {
        Some(v) => format!("{v:.2}"),
        None => "-".to_string(),
    };
    let quality = match cell.quality_pass_count_median {
        Some(v) => format!("{v}/4"),
        None => "-".to_string(),
    };
    out.push_str(&format!(
        "| {fixture} | {backend} | {seeds} | {n}/{seeds} | {hard} | {placed}/{expected} | {soft} | {ffd:.2} | {total:.0} | {peak} | {ttf} | {tto} | {worst_spread} | {worst_home} | {gaps} | {late} | {quality} |\n",
        backend = backend.label(),
        seeds = cell.seeds,
        n = cell.feasibility_count,
        hard = cell.hard_violations_median,
        placed = cell.placements_total_median,
        expected = cell.placements_expected,
        ffd = cell.ffd_ms_median,
        total = cell.total_ms_median,
        peak = cell.peak_kb,
    ));
}
```

- [ ] **Step 3: Extend write_footer**

After the existing footer paragraphs, before the final `See ...` line, append:

```rust
out.push_str(
    "Quality columns (rightmost five): per-cell median across feasible seeds. Predicates pass at\n",
);
out.push_str(
    "worst spread <= 2, worst home-room ratio >= 0.6, total interior gaps <= 2, late-period ratio >= 0.5.\n",
);
out.push_str(
    "Late-period ratio is the median normalised position (`position / max_position_per_day`) of all\n",
);
out.push_str(
    "placements of subjects with `Subject.prefer_late_period > 0`; rendered as `-` when no fixture\n",
);
out.push_str(
    "subject has the axis enabled, and that case counts as pass for the composite Quality column.\n",
);
out.push_str(
    "Home-room ratio exempts subjects whose `room_subject_suitabilities` exclude the class's\n",
);
out.push_str(
    "`home_room_id` (e.g. gym / Werkraum / Musikraum on Grundschule). Mirrors `quality_checks.py`\n",
);
out.push_str(
    "predicates by intent; implementations are intentionally separate (Python operates on persisted\n",
);
out.push_str(
    "ORM rows, Rust on the in-memory `Solution`).\n\n",
);
```

- [ ] **Step 4: Extend the per-cell `eprintln!` log line**

Find the `eprintln!("cell done: ...")` block in `run_supervisor`. Append the five new fields:

```rust
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
    cell.soft_score_median.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string()),
    cell.total_ms_median,
    cell.peak_kb,
    cell.time_to_first_feasible_ms_median.map(|v| format!("{:.0}", v)).unwrap_or_else(|| "-".to_string()),
    cell.time_to_optimal_ms_median.map(|v| format!("{:.0}", v)).unwrap_or_else(|| "-".to_string()),
    cell.worst_spread_median.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
    cell.worst_home_room_ratio_median.map(|v| format!("{v:.2}")).unwrap_or_else(|| "-".to_string()),
    cell.total_interior_gaps_median.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
    cell.late_period_ratio_median.map(|v| format!("{v:.2}")).unwrap_or_else(|| "-".to_string()),
    cell.quality_pass_count_median.map(|v| format!("{v}/4")).unwrap_or_else(|| "-".to_string()),
);
```

- [ ] **Step 5: Run inline render tests**

```bash
cargo nextest run -p solver-bench --bin solver-bench --no-tests=pass
```

Expected: every test passes. Specifically:
- `tests::write_header_includes_five_quality_columns` PASS
- `tests::write_row_renders_quality_columns` PASS (`| 0.75 |` for home-room, `| 0.60 |` for late-period, `| 4/4 |` for quality)
- `tests::write_row_renders_dash_when_quality_fields_are_none` PASS

Lint:

```bash
cargo clippy -p solver-bench --all-targets -- -D warnings
```

Expected: green.

- [ ] **Step 6: Extend the end-to-end smoke**

Modify `solver/solver-bench/tests/end_to_end.rs`. Add to the existing `supervisor_emits_three_new_columns_in_markdown` test (rename it to `supervisor_emits_observability_and_quality_columns`):

```rust
assert!(body.contains("Worst spread (median)"), "missing worst-spread header: {body}");
assert!(body.contains("Worst home-room ratio (median)"), "missing home-room header: {body}");
assert!(body.contains("Total interior gaps (median)"), "missing gaps header: {body}");
assert!(body.contains("Late-period ratio (median)"), "missing late-period header: {body}");
assert!(body.contains("Quality (pass / 4)"), "missing quality column header: {body}");
```

Run:

```bash
cargo nextest run -p solver-bench --test end_to_end
```

Expected: PASS. The supervisor smoke spawns a fast shape demo (`--budget 200ms --seeds 1 --fixtures grundschule`); the new column headers appear in the output.

- [ ] **Step 7: Commit**

```bash
git add solver/solver-bench/src/main.rs solver/solver-bench/tests/end_to_end.rs
git commit -m "feat(solver-bench): render five quality columns in BENCH_RESULTS.md (item 31)"
```

---

## Task 6: Sweep OPEN_THINGS, autopilot.md improvements (if any), CLAUDE.md, auto-memory

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`
- Modify: `solver/CLAUDE.md`
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`
- Possibly modify: `.claude/commands/autopilot.md` (only if a workflow learning surfaced)

This task happens at autopilot step 6 (after the implementation). The plan ordering here lets the implementing agent treat it as the closing task.

- [ ] **Step 1: Update OPEN_THINGS.md**

Open `docs/superpowers/OPEN_THINGS.md`. Find the active-sprint `## Active sprint program: Solver feasibility correctness + observability` block.

In the active-sprint preamble (around line 9): change the "Next pickup: P0 item 31 ..." line to advance to item 32 (test realism phase: solvability test mirroring the production route flow). Mention item 31 shipped today (2026-05-06) with the date and what shipped: "Item 31 shipped on 2026-05-06: five quality columns plus pass-count in BENCH_RESULTS.md (worst spread, worst home-room ratio, total interior gaps, late-period ratio, quality pass count); evaluator lives in `solver-bench/src/quality.rs`; thresholds match the Python integration test verbatim."

In the `### Observability phase` block (around line 19): delete the entire item 31 paragraph.

In the `### Sprint-tidy phase` block: append two follow-up items at the end (numbered after the existing items):

- "Refresh `BENCH_RESULTS.md` once item 12 lands. Item 31 added the late-period column, but every fixture currently has `prefer_late_period=0` (the seed-side value was reverted in PR #171, tracked as item 12). Once item 12 sets `prefer_late_period=5` for FÖ in the seed AND the bench fixture mirrors it, the late-period column will report a real ratio per cell. Promote the OPEN_THINGS item 14 xfail removal to active when the cell shows ratio >= 0.5 on grundschule. `[P1]`"
- "Promote `room_hop` and `day_too_long` to bench columns if a future bench refresh shows non-zero counts. Today both are 0 across all fixtures (`room_hop` is a hard constraint validated by `validate_no_room_hopping`; `day_too_long` is well covered by `prefer_early_period`). Adding columns now would only report zeros. `[P2]`"

The preamble's `Goal:` paragraph (around line 11) — no change needed. The four-axis goal still applies; item 31's columns deliver axis (b) "judge produced plans on schedule quality (gaps, spread, home-room ratio) not just hard-violation counts."

If a duplicate "Note: ..." inline annotation on item 31 was carried over from the working tree, it is removed by the deletion above.

- [ ] **Step 2: Update solver/CLAUDE.md**

Open `solver/CLAUDE.md`. Find the `## Bench workflow` section. Append one bullet:

```markdown
- **Schedule-quality predicates live in `solver-bench/src/quality.rs`.** Pure functions over `&Problem` + `&Solution`; mirror the predicates `backend/.../quality_checks.py` enforces in `test_grundschule_schedule_meets_quality_bar`. Cross-language parity is intentionally not a contract: Python operates on persisted ORM rows with a hand-supplied exempt set; Rust operates on the in-memory `Solution` and infers exempt subjects from `Problem.room_subject_suitabilities`. Bench `CellResult` carries five median fields (`worst_spread_median`, `worst_home_room_ratio_median`, `total_interior_gaps_median`, `late_period_ratio_median`, `quality_pass_count_median`); `BENCH_RESULTS.md` renders a `Quality (pass / 4)` column. Late-period predicate uses `Subject.prefer_late_period > 0` as the proxy for "FÖ-shaped" (today the proxy is empty on every fixture pending OPEN_THINGS item 12). Thresholds in `quality.rs::QUALITY_*` constants match the Python test verbatim.
```

- [ ] **Step 3: Refresh auto-memory**

Read `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`.

Update the `description:` YAML field to reflect the new next-pickup. Example new value: "Active sprint solver correctness + observability. Items 30 + 31 shipped (ADR 0034, bench quality columns). Next pickup item 32 (test realism: solvability via production route flow)."

Update the body to reflect:
- Item 31 shipped on 2026-05-06 (PR #TBD, will be filled in step 7).
- Next pickup is item 32 (test realism phase).
- Two new follow-ups under the sprint-tidy phase.

Update `MEMORY.md` only if the roadmap-status entry's title / link changed; the body update doesn't require a MEMORY.md touch.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/OPEN_THINGS.md solver/CLAUDE.md /home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md
git commit -m "docs: open-things sweep and CLAUDE.md pointer after item 31 ships"
```

If the working-tree's pre-existing OPEN_THINGS edit (the unrelated item 42 trim from before this branch was created) is still in the diff, include it in this commit; the trim is consistent with an OPEN_THINGS sweep and the change is so small it does not deserve its own commit.

- [ ] **Step 5: Run skills for the autopilot step 6 closing pass**

Per `.claude/commands/autopilot.md` step 6, run the three closing skills via the `Skill` tool: `claude-md-management:revise-claude-md`, `claude-md-management:claude-md-improver`, `fewer-permission-prompts`. Apply edits each skill proposes directly (autonomous mode); commit them on the same feature branch. Skills may want to touch `solver/CLAUDE.md`, `backend/CLAUDE.md`, `frontend/CLAUDE.md`, `.claude/CLAUDE.md`, or `.claude/settings.json`. Each skill's edits commit under the appropriate Conventional Commits type (`docs(claude):`, `chore(settings):`).

If a skill is unavailable in the current environment, note it in the end-of-turn summary and continue.

---

## Self-Review

**Spec coverage:**

- Spec In-scope: `solver-bench/src/quality.rs` module → Tasks 2 (skeleton) + 3 (impl).
- Spec In-scope: `CellResult` extension with five fields → Tasks 2 + 4.
- Spec In-scope: per-seed accumulation in `run_lahc_cell` and `run_cpsat_cell` → Task 4.
- Spec In-scope: markdown header / row / footer → Task 5.
- Spec In-scope: end-to-end smoke extended → Task 5.
- Spec In-scope: inline tests on each predicate, the round-trip serde test, the markdown render tests → Tasks 2 + 3.
- Spec In-scope: OPEN_THINGS sweep, auto-memory refresh, solver/CLAUDE.md addendum → Task 6.
- Spec out of scope: `quality_checks.py` refactor — no task; correct.
- Spec out of scope: production-bakeoff data refresh — no task; correct.
- Spec out of scope: ADR — no task; correct.

**Placeholder scan:** No "TBD", "implement later", "similar to Task N". Code blocks include the full body for every step that asks the agent to write code. The `Cargo.toml` `dev-dependencies` step references the actual workspace dep style.

**Type consistency:**
- `QualityReport` field names match across Tasks 2, 3, 4, and 6: `worst_spread`, `worst_home_room_ratio`, `total_interior_gaps`, `late_period_ratio`.
- `CellResult` field names match: `worst_spread_median`, `worst_home_room_ratio_median`, `total_interior_gaps_median`, `late_period_ratio_median`, `quality_pass_count_median`.
- `aggregate_quality_medians` returns a 5-tuple in the same order as the `CellResult` field append order.
- Test fixtures construct `Lesson` with full field set including `lesson_group_id: None` and `allowed_room_ids: None` (verified against `solver-core/src/types.rs::Lesson`).
- `worst_home_room_ratio` signature in Task 3 takes `&HashMap<SchoolClassId, RoomId>` for home_rooms; `evaluate_quality` builds the same shape from `problem.school_classes` and passes it down.

**Gaps:** None identified. Plan implements every In-scope spec requirement; no out-of-scope item is implemented.

---

## Execution

This plan is executed by the autopilot workflow (`.claude/commands/autopilot.md` step 5). Per autopilot's required-skill table: invoke `superpowers:test-driven-development`, then `superpowers:subagent-driven-development`, then dispatch each plan task to a fresh `general-purpose` subagent.

The tasks share state in `solver/solver-bench/src/main.rs`: dispatch sequentially (one agent at a time, waiting for each to return). Tasks 2 + 3 + 4 + 5 all touch `main.rs` or `quality.rs`. Task 6 touches `OPEN_THINGS.md`, `solver/CLAUDE.md`, and the auto-memory file; safe to dispatch after Task 5 returns.

Plan task to commit-message-prefix mapping:

| Task | Commit prefix |
| --- | --- |
| 1   | `docs:` |
| 2   | `test(solver-bench):` |
| 3   | `feat(solver-bench):` |
| 4   | `feat(solver-bench):` |
| 5   | `feat(solver-bench):` |
| 6   | `docs:` (plus one `docs(claude):` and one `chore(settings):` if step 5 surfaces edits) |
