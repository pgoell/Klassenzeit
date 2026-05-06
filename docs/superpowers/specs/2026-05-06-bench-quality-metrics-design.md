# Schedule-quality metrics in the bake-off output (active sprint, item 31)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Observability phase: item 31.
**Goal.** `solver/solver-core/benches/BENCH_RESULTS.md` carries 12 columns after item 30: feasibility, hard violations, placements, soft score, FFD wall-clock, total wall-clock, peak RSS, time to first feasible, time to optimal. None of the 12 evaluate the *quality* of the produced schedule along the predicates that `backend/src/klassenzeit_backend/scheduling/quality_checks.py` asserts: median interior gaps per class per day, max daily spread per class, home-room ratio per class, late-period FÖ ratio. A bake-off cell with feasibility 20/20 today can hide a schedule that violates the quality bar (one class at spread=5 hidden by ten classes at spread=1, median home-room ratio of 0.42, etc.). Picking a production winner from such cells is unsafe. This spec adds four numeric quality columns plus a composite `Quality (pass / 4)` column to BENCH_RESULTS.md so a winner cannot be picked on hard-feasibility alone.

**Non-goal.** Not folding any predicate into the LAHC softscore (rejected in brainstorm Q1: softscore stays continuous to keep LAHC's delta scoring honest, predicates stay pass/fail thresholds layered on top). Not running `mise run bench:bakeoff` at production cell shape (`--budget 60s --seeds 20`, ~4.5 h); the production refresh is queued as item 42 in the sprint-tidy phase, blocked on item 15. Not adding `Subject.name`, `Subject.exempt_from_home_room`, or any other Problem-cascading field. Not refactoring `backend/.../quality_checks.py`; the Python implementation operates on persisted ORM rows and stays as-is. Not extending the bench with `room_hop` or `day_too_long` columns (rejected in brainstorm Q5: `room_hop` is a hard-constraint already validated post-condition; `day_too_long` is well covered by the existing `prefer_early_period` softscore axis).

## Context

The OPEN_THINGS item 31 bullet sketches the four predicates and asks whether they should be part of the softscore. The brainstorm (`/tmp/kz-brainstorm/brainstorm.md` for this run) refined the sketch into a buildable design. Key refinements:

- Softscore vs predicates is a kind difference, not a degree difference. Softscore is a continuous gradient LAHC's delta scoring climbs down. Predicates are pass/fail thresholds at a quality bar. A median softscore cell hides distribution shape (one class at spread=5 plus ten classes at spread=1 has the same median softscore as eleven classes at spread=2). The predicate cell surfaces the broken class. Folding predicates into the softscore would either need percentile-style penalties (alien to LAHC delta scoring) or per-class hard-cap weights that grow exponentially (destabilises LAHC). Brainstorm Q1.
- The evaluator lives in `solver-bench`, not `solver-core`. Predicates have no production caller; only the bench compares them across backends. `solver-core/CLAUDE.md` says "no I/O beyond what callers pass in" and "the scheduling algorithm, constraint model, and typed errors live here" — quality predicates over a finished schedule are neither. Keeping them in `solver-bench/src/quality.rs` avoids polluting `solver-core`'s public surface and dodges the "Adding a field cascades to ~15 fixture sites" tax. Brainstorm Q2.
- The four predicates are: `worst_class_day_spread` (max over classes of `max_lessons_in_day - min_lessons_in_day` across the school week), `worst_home_room_ratio` (min over classes of `non_exempt_home_room_hits / non_exempt_placements`), `total_interior_gaps` (sum over (class, day) of `last_position - first_position + 1 - count`), `late_period_ratio` (median position of late-preferred subjects' placements, normalised to `[0.0, 1.0]` via `position / max_position_per_day`). Thresholds match the Python test verbatim: `max_spread <= 2`, `min_home_room_ratio >= 0.6`, `max_total_interior_gaps <= 2`, `late_period_ratio >= 0.5`. Brainstorm Q3.
- Late-period FÖ identification uses the existing `Subject.prefer_late_period > 0` axis as the proxy. No `Subject.name` field needed. Today every Rust-side fixture subject has `prefer_late_period: 0` (per OPEN_THINGS item 12, the seed value was reverted to a no-op in PR #171, and the Rust `test_fixtures.rs` mirrors that). Render the predicate as `n/a` in the column when no subject in the fixture has the axis enabled, rather than a misleading `1.00` (vacuous truth). When item 12 lands and the seed sets `prefer_late_period=5` for FÖ, the bench fixture mirror gets the same value, the cell becomes a real ratio, and the xfail in OPEN_THINGS item 14 (median FÖ position bar) is unblocked at the same moment. The composite `Quality (pass / 4)` count treats `n/a` as passing. Brainstorm Q4 + Q7.
- Home-room exempt subjects derive from `Problem.room_subject_suitabilities` (Q6.B). For each `(class, subject)`, the exempt set is computed as: if the subject has any `RoomSubjectSuitability` row and the class's `home_room_id` is *not* in the subject's suitable rooms, the subject is exempt for that class. This handles SP / KU / MU on Grundschule (the gym, Werkraum, Musikraum suitabilities exclude `1a`'s home room) and degrades gracefully on fixtures with different exemption shapes. The Python predicate's hand-encoded `{SP, KU, MU}` set is a special case of this rule.
- Per-cell rendering uses the *median over feasible seeds* for each of the four numeric columns. Mirrors `soft_score_median` shape. Infeasible cells render `-` for the four columns (same as today's `soft_score_median`). The composite `Quality (pass / 4)` column counts how many of the four predicates pass on the median values, rendered as `n/4` (e.g. `4/4`, `3/4`).

The brainstorm also closed the wire-shape question: extend `CellResult` with five additive fields rather than nest a sub-struct. Keeps the bench JSON envelope flat and the markdown render code linear.

`BENCH_RESULTS.md` itself: this PR ships the column *shape* (header, render rule, render tests) but does *not* run the bake-off to refresh the data cells. The four numeric columns will appear with whatever values the existing low-budget shape demo (committed by item 30) produces; the production refresh that fills them at production cell shape is item 42. This decoupling matches item 30's shape.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 31. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run). Companion ADR: none. The bench column shape is operational, not architectural; ADR 0029 (bake-off methodology) and ADR 0034 (cell subprocess) are referenced in the spec for context but not amended.

## Scope

**In scope.**

- New module `solver/solver-bench/src/quality.rs`. Public surface:
    - `pub struct QualityReport { pub worst_spread: u32, pub worst_home_room_ratio: Option<f64>, pub total_interior_gaps: u32, pub late_period_ratio: Option<f64> }`. `Option<f64>` for both ratios so `None` distinguishes `n/a` (no relevant placements / no late-preferred subject) from a real `0.0`.
    - `pub const QUALITY_MAX_SPREAD: u32 = 2;`
    - `pub const QUALITY_MIN_HOME_ROOM_RATIO: f64 = 0.6;`
    - `pub const QUALITY_MAX_INTERIOR_GAPS: u32 = 2;`
    - `pub const QUALITY_MIN_LATE_PERIOD_RATIO: f64 = 0.5;`
    - `pub fn evaluate_quality(problem: &Problem, solution: &Solution) -> QualityReport`. Pure function; reads `problem.school_classes`, `problem.room_subject_suitabilities`, `problem.subjects`, `problem.lessons`, and `solution.placements` (each placement is a `solver_core::Placement`). Returns the four metrics; never panics; treats empty placement vectors gracefully (worst_spread=0, ratios=None, gaps=0).
    - `pub fn quality_pass_count(report: &QualityReport) -> u32`. Returns the number of the four predicates that pass on the configured thresholds. `None` ratios count as passing (vacuous truth). Returns a value in `0..=4`.
- `solver/solver-bench/src/main.rs` extends `CellResult` with five additive fields (all `Option`-typed for the same reason `soft_score_median` is `Option`):
    - `worst_spread_median: Option<u32>`
    - `worst_home_room_ratio_median: Option<f64>`
    - `total_interior_gaps_median: Option<u32>`
    - `late_period_ratio_median: Option<f64>`
    - `quality_pass_count_median: Option<u32>`
- LAHC cell-child (`run_lahc_cell`):
    - After each per-seed `solve_with_config_stats(...)`, when `feasible == true`, call `quality::evaluate_quality(problem, &solution)`, push to per-feasible-seed sample vectors.
    - Aggregate at end: median of `worst_spread`, median of `worst_home_room_ratio` (filtering `None` from the median set), median of `total_interior_gaps`, median of `late_period_ratio` (filtering `None`), median of `quality_pass_count`.
    - When `feasibility_count == 0`, all five fields are `None`.
- cpsat cell-child (`run_cpsat_cell`):
    - Same pattern but the python child currently emits only `placements`, `violations`, `soft_score`, `peak_rss_kb`, `time_to_first_feasible_ms`, `time_to_optimal_ms`. The Rust side already deserialises `placements: serde_json::Value`. Convert to `Vec<Placement>` per seed via a small helper, then pass to `quality::evaluate_quality(problem, &solution)`. Same per-seed accumulation.
- Markdown table (`write_header`, `write_row`):
    - Five new columns appended to the right of the existing 12: `Worst spread (median)`, `Worst home-room ratio (median)`, `Total interior gaps (median)`, `Late-period ratio (median)`, `Quality (pass / 4)`.
    - Render rule: each numeric column renders the median value or `-` when `None`; the `Quality (pass / 4)` column renders `n/4` (e.g. `4/4`) or `-` when `None`.
    - Footer block grows one paragraph documenting the four predicates, the thresholds (constants from `quality.rs`), and the `n/a`-as-pass convention for `late_period_ratio` when no fixture subject has `prefer_late_period > 0`.
- `solver/solver-bench/tests/end_to_end.rs` extends the existing markdown-shape assertion: the produced markdown contains the five new column headers.
- Inline tests in `solver/solver-bench/src/quality.rs#tests`:
    - `worst_class_day_spread_returns_zero_for_balanced_schedule`
    - `worst_class_day_spread_picks_largest_class`
    - `worst_home_room_ratio_excludes_subjects_unsuitable_for_home_room`
    - `worst_home_room_ratio_returns_none_when_all_subjects_exempt`
    - `total_interior_gaps_counts_only_holes_inside_first_last_window`
    - `late_period_ratio_returns_none_when_no_subject_prefers_late`
    - `late_period_ratio_normalises_position_against_max_per_day`
    - `quality_pass_count_treats_none_ratios_as_pass`
    - `quality_pass_count_grundschule_fixture_passes_all_four`
- Inline tests in `solver/solver-bench/src/main.rs#tests` (extending the existing block):
    - `cell_result_round_trips_through_json` extended to cover the five new fields.
    - `write_header_includes_five_quality_columns`
    - `write_row_renders_quality_columns`
    - `write_row_renders_dash_when_no_feasible_seed` extended to assert the five new columns are also `-`.
- OPEN_THINGS:
    - Delete item 31 from the active sprint observability phase.
    - Update the active-sprint preamble's "next pickup" line to item 32 (test realism phase: solvability test mirroring the production route flow).
    - Add a follow-up under the active sprint's tidy phase: "Refresh `BENCH_RESULTS.md` to surface real quality cells once item 12 lands. The column shape is in place from item 31; until item 12 sets `prefer_late_period=5` for FÖ in the seed and the bench fixture mirrors it, the late-period column will render `n/a` on every cell."
    - Add a follow-up under the active sprint's tidy phase: "Promote `room_hop` and `day_too_long` to bench columns if a future bench refresh shows non-zero counts. Today both are 0 across all fixtures (`room_hop` is a hard constraint, `day_too_long` is well covered by `prefer_early_period`); landing them now would add columns that report only zeros."
- Auto-memory `project_roadmap_status.md` refresh: item 31 shipped; next pickup is item 32. Update the description field too.
- `solver/CLAUDE.md` addendum: one bullet under "Bench workflow" pointing to `solver-bench/src/quality.rs` as the schedule-quality evaluator and noting the deliberate Python/Rust divergence (Python evaluator runs on persisted ORM rows; Rust evaluator runs on in-memory `Solution` and infers exempt subjects from `room_subject_suitabilities`).

**Out of scope.**

- Schedule-quality predicate refactor on the Python side (`backend/.../quality_checks.py`). Cross-language parity test rejected in brainstorm Q9; the two implementations are designed to drift.
- ADR. Bench column shape is operational; existing ADRs 0029 and 0034 cover the architecture this PR sits on top of. The spec references them but does not amend either.
- Folding any predicate into the LAHC softscore (rejected in brainstorm Q1).
- `Subject.name` or `Subject.exempt_from_home_room` (rejected in brainstorm Q4 / Q6).
- Production-cell-shape `BENCH_RESULTS.md` refresh (item 42; blocked on item 15).
- `room_hop` / `day_too_long` bench columns (deferred to OPEN_THINGS follow-ups).
- `quality_pass_count`-as-rate column (`n/seeds` instead of median over seeds). The median is more defensible at the bake-off's seed counts; rate is a fifth column that adds little.
- Surfacing per-class breakdown in BENCH_RESULTS.md. Per-cell aggregates only; per-class shape is `solver-trace`-territory.

## Code change

`solver/solver-bench/src/quality.rs` (new file):

```rust
//! Schedule-quality predicates for bake-off cells.
//!
//! Mirrors the predicates `backend/src/klassenzeit_backend/scheduling/quality_checks.py`
//! enforces in the demo Grundschule integration test. The Python and Rust
//! implementations are intentionally separate: the Python version operates on
//! persisted ORM rows with a hand-supplied exempt-subjects set; the Rust
//! version operates on the in-memory `Solution` and infers exempt subjects
//! from `Problem.room_subject_suitabilities`. Cross-language parity is not
//! a contract; the two are designed to drift around their respective inputs.

use std::collections::{HashMap, HashSet};

use solver_core::types::{Problem, Solution, RoomId, SchoolClassId, SubjectId};

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
/// are pure functions over `Problem` + `Solution`; `None` on either ratio
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

/// Pure function over `Problem` + `Solution`. See module rustdoc for the
/// per-predicate semantics. Never panics; treats empty placements gracefully.
pub fn evaluate_quality(problem: &Problem, solution: &Solution) -> QualityReport {
    let positions_per_day = positions_per_day_index(problem);
    let max_position_per_day = max_position_per_day_index(&positions_per_day);
    let exempt = exempt_subjects_per_class(problem);
    let home_rooms: HashMap<SchoolClassId, RoomId> = problem
        .school_classes
        .iter()
        .filter_map(|c| c.home_room_id.map(|r| (c.id, r)))
        .collect();

    QualityReport {
        worst_spread: worst_class_day_spread(problem, solution),
        worst_home_room_ratio: worst_home_room_ratio(problem, solution, &home_rooms, &exempt),
        total_interior_gaps: total_interior_gaps(problem, solution),
        late_period_ratio: late_period_ratio(problem, solution, &max_position_per_day),
    }
}

/// Returns the count (0..=4) of predicates that pass at the configured
/// thresholds. `None` ratios count as passing (vacuous truth).
pub fn quality_pass_count(r: &QualityReport) -> u32 {
    let mut n = 0;
    if r.worst_spread <= QUALITY_MAX_SPREAD { n += 1; }
    if r.worst_home_room_ratio.map_or(true, |v| v >= QUALITY_MIN_HOME_ROOM_RATIO) { n += 1; }
    if r.total_interior_gaps <= QUALITY_MAX_INTERIOR_GAPS { n += 1; }
    if r.late_period_ratio.map_or(true, |v| v >= QUALITY_MIN_LATE_PERIOD_RATIO) { n += 1; }
    n
}

// ... helper fns and #[cfg(test)] mod tests below ...
```

`solver/solver-bench/src/main.rs` extends `CellResult`:

```rust
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CellResult {
    // ... existing fields ...
    worst_spread_median: Option<u32>,
    worst_home_room_ratio_median: Option<f64>,
    total_interior_gaps_median: Option<u32>,
    late_period_ratio_median: Option<f64>,
    quality_pass_count_median: Option<u32>,
}
```

`run_lahc_cell` adds per-seed accumulators that fill only on feasible seeds:

```rust
let mut quality_reports: Vec<QualityReport> = Vec::with_capacity(seeds as usize);
// ... inside the seed loop, after the existing feasibility branch:
if feasible {
    // ... existing accumulators ...
    quality_reports.push(quality::evaluate_quality(problem, &solution));
}
// ... at end of fn:
let (worst_spread_median, worst_home_room_ratio_median,
     total_interior_gaps_median, late_period_ratio_median,
     quality_pass_count_median) = aggregate_quality_medians(&quality_reports);
```

`run_cpsat_cell`: same shape after `parsed`. Need a small helper that converts the `serde_json::Value`-typed `placements` (today untyped) into `Vec<Placement>` per seed for the quality call. The cpsat python emits `placements: list[dict]`; the existing in-memory `Solution::placements: Vec<Placement>` shape matches.

`write_header` adds the five new columns:

```rust
out.push_str(
    "| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Placements (median / expected) | Soft score (median, feasible) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) | Peak RSS (kB) | Time to first feasible (ms, median) | Time to optimal (ms, median) | Worst spread (median) | Worst home-room ratio (median) | Total interior gaps (median) | Late-period ratio (median) | Quality (pass / 4) |\n",
);
out.push_str(
    "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
);
```

`write_row` formats them. Each numeric column renders the median value or `-` when `None`. The `Quality (pass / 4)` column renders `n/4` (e.g. `4/4`) or `-` when `None`. Match the `Option`-pattern already used for `soft_score_median`, `time_to_first_feasible_ms_median`, `time_to_optimal_ms_median`.

`write_footer` adds one paragraph:

```text
Quality columns (rightmost five): per-cell median across feasible seeds. Predicates pass at:
worst spread <= 2, worst home-room ratio >= 0.6, total interior gaps <= 2, late-period ratio >= 0.5.
Late-period ratio is the median normalised position (`position / max_position_per_day`) of all
placements of subjects with `Subject.prefer_late_period > 0`; `n/a` when no fixture subject has
the axis enabled, and `n/a` counts as pass for the composite Quality column. Home-room ratio
exempts subjects whose `room_subject_suitabilities` exclude the class's `home_room_id` (the
gym / Werkraum / Musikraum on Grundschule). Mirrors `quality_checks.py` predicates by intent;
implementations are intentionally separate (Python operates on persisted ORM rows, Rust on the
in-memory `Solution`).
```

`solver/solver-bench/Cargo.toml`: no new deps. The existing `solver-core`, `serde`, `serde_json`, `libc` cover the new module.

## Test changes

Inline tests on `quality.rs` (sketched in the In-scope section, expanded here).

`worst_home_room_ratio_excludes_subjects_unsuitable_for_home_room`: builds a small `Problem` with one class (home_room=R1), three subjects (S1 with no suitability rows; S2 with suitability {R1, R2}; S3 with suitability {R2}), and three placements (one per subject, all in R1 except S3 in R2). Asserts `worst_home_room_ratio == Some(2.0/2.0) == Some(1.0)` because S3 is exempt for the class (R1 not in S3's suitable rooms).

`quality_pass_count_grundschule_fixture_passes_all_four`: builds the `solver_core::test_fixtures::grundschule_fixture()`, calls `solve_with_config(&problem, &SolveConfig { weights: PRODUCTION_ACTIVE_WEIGHTS, deadline: None, ..default() })` (greedy-only for determinism per `solver/CLAUDE.md`'s "Pin solver-core unit tests to greedy" rule), evaluates the resulting `Solution`, asserts `quality_pass_count(&report) == 4`. This is both a regression test on the predicate logic and a sanity check that Grundschule passes the bar today (greedy alone might not; LAHC is needed for some axes per `test_grundschule_schedule_meets_quality_bar`'s xfail). Acceptable to assert `>= 3` if greedy alone fails one predicate; tighten later when item 12-14 land. The test stays in `quality.rs`'s inline `#[cfg(test)] mod tests` so the bench's `cargo nextest run -p solver-bench --bin solver-bench` covers it.

`solver/solver-bench/tests/end_to_end.rs` extends the existing assertions:

```rust
assert!(body.contains("Worst spread (median)"));
assert!(body.contains("Worst home-room ratio (median)"));
assert!(body.contains("Total interior gaps (median)"));
assert!(body.contains("Late-period ratio (median)"));
assert!(body.contains("Quality (pass / 4)"));
```

The existing smoke spawns `cargo run -p solver-bench -- --budget 200ms --seeds 1 --fixtures grundschule --out /tmp/...`. The new assertions ride on the same invocation.

`solver/solver-bench/src/main.rs#tests`: extend `cell_result_round_trips_through_json` to cover the five new fields. Extend `write_row_renders_observability_columns` rename or add `write_row_renders_quality_columns` that constructs a `CellResult` with `worst_spread_median = Some(2)`, `worst_home_room_ratio_median = Some(0.75)`, `total_interior_gaps_median = Some(1)`, `late_period_ratio_median = Some(0.6)`, `quality_pass_count_median = Some(4)` and asserts each renders correctly. Extend `write_row_renders_dash_when_no_feasible_seed` to include the five new fields all `None`.

Add a new test `write_row_renders_quality_pass_as_n_slash_four`:

```rust
#[test]
fn write_row_renders_quality_pass_as_n_slash_four() {
    let mut cell = make_feasible_cell(); // helper
    cell.quality_pass_count_median = Some(3);
    let mut out = String::new();
    write_row(&mut out, "grundschule", BenchBackend::LahcRrKempe, &cell);
    assert!(out.contains("| 3/4 |"), "missing 3/4: {out}");
}
```

No property test added; the predicates are deterministic functions over `Problem` + `Solution`, well covered by hand-built unit fixtures. A proptest would mostly re-derive what the unit tests already pin.

No Python tests touched; the cpsat output JSON is unchanged. The Rust side already deserialises `placements: serde_json::Value` from the cpsat child, so no wire-format change.

No backend or frontend test touched.

## Bench impact

Criterion bench (`mise run bench`):

- `solve_with_config_stats` is unchanged. `solver-bench` does not feed criterion. Quality evaluator is bench-only and never runs from the criterion bench. No expected drift.

Bake-off bench (`mise run bench:bakeoff`):

- Per-seed quality compute is O(P + classes × 5) where P is total placements. For dreizügige (C=12, P=294), per-seed cost is ~1500 add/compare ops — well under 1 ms. Total per refresh: 4 fixtures × 4 backends × 20 seeds = 320 cells × <1 ms = under 1 second across the full multi-hour refresh. Negligible.
- Cell-child JSON envelope grows by five fields. Bytes: ~150 per cell. Total per refresh: <50 kB across all cells. Negligible.
- Markdown output grows from 12 to 17 columns. Wall-clock to render is unchanged. Visual width grows by ~80 characters per row.

## Commit plan

1. `test(solver-bench): unit tests for quality.rs predicates and CellResult quality fields (item 31)`. Adds `solver-bench/src/quality.rs` with `pub` types and constants but `unimplemented!()` bodies; adds the inline tests; extends `CellResult` with five `Option`-typed fields plus the markdown-render tests. Compiles green; tests fail at the `unimplemented!()` panics. Documents the failure in the commit body. (Per `backend/CLAUDE.md`'s "Land a stub module typed signature, body `raise NotImplementedError(...)`" pattern, adapted for Rust.)
2. `feat(solver-bench): quality.rs predicate evaluator (item 31)`. Implements `worst_class_day_spread`, `worst_home_room_ratio`, `total_interior_gaps`, `late_period_ratio`, `evaluate_quality`, `quality_pass_count`. Tests from step 1 turn green.
3. `feat(solver-bench): per-seed quality accumulation in cell-child + median aggregate (item 31)`. Wires `quality::evaluate_quality` into `run_lahc_cell` and `run_cpsat_cell`. Adds `aggregate_quality_medians` helper. Populates the five new `CellResult` fields. The cpsat path needs a small `placements_value_to_scheduled_lessons(&serde_json::Value) -> Vec<Placement>` helper; ship inline.
4. `feat(solver-bench): render five quality columns in BENCH_RESULTS.md (item 31)`. Extends `write_header`, `write_row`, `write_footer`. End-to-end smoke from `solver-bench/tests/end_to_end.rs` extended to cover the new headers.
5. `docs: open-things sweep after item 31 ships`. Removes item 31 from the active sprint; advances "next pickup" to item 32; adds two follow-ups under the active sprint's tidy phase (post-item-12 late-period column refresh; potential `room_hop` / `day_too_long` columns); refreshes auto-memory `project_roadmap_status.md` (body and description field both).
6. `docs(claude): point at solver-bench/src/quality.rs as the bench-side quality evaluator`. One bullet under "Bench workflow" in `solver/CLAUDE.md` documenting the deliberate Python/Rust divergence.

Steps 1-4 follow TDD red-green: step 1 lands the red, step 2 turns the predicate tests green, step 3 turns the cell-aggregation tests green, step 4 turns the markdown-render tests green. Each commit is independently buildable + lintable + passing pre-push tests. (Step 1's compilation passes; the `unimplemented!()` panics fire only when the new tests run, which the next commit fixes.) Steps 5-6 are docs-only.

Per the autopilot workflow: spec + plan land in `docs:` commits before the test/feat sequence (autopilot steps 3 and 4). Settings tweaks from `fewer-permission-prompts`, autopilot.md improvements, and CLAUDE.md edits land on the same feature branch.

## Risks

- **The `n/a`-as-pass convention for late-period ratio is subtle.** A reader looking at the Quality column today sees `4/4` on every feasible cell because the late-period predicate is vacuously satisfied. When OPEN_THINGS item 12 lands, the same column may drop to `3/4` on some cells without any code change in this PR. Mitigation: footer paragraph documents the convention; spec body explains it; OPEN_THINGS follow-up item carries the cross-reference.
- **The home-room exemption derivation may misfire on a future fixture.** If a fixture adds `room_subject_suitabilities` rows that *include* the class's home room for a subject the design intends to exempt (e.g., MU's suitability erroneously listing the class home room alongside Musikraum), the bench evaluator counts MU placements toward the home-room ratio. Mitigation: the unit test `worst_home_room_ratio_excludes_subjects_unsuitable_for_home_room` pins the exemption logic; a regression test on the `grundschule_fixture` asserts the inferred exempt set matches the Python `{SP, KU, MU}` set.
- **Cross-language drift between Python `quality_checks.py` and Rust `quality.rs`.** The two are designed to drift but a divergence on the same `(problem, solution)` pair could confuse a reviewer comparing the bench output to the Python integration test. Mitigation: spec body and the `quality.rs` module rustdoc both call this out; no parity test attempted (rejected in brainstorm Q9).
- **PR ships columns without refreshing the data.** A reviewer might expect `mise run bench:bakeoff` to run as part of this PR. The PR body and OPEN_THINGS item 42 both explain: data refresh is item 42, blocked on item 15.
- **15+ `Problem { ... }` literal cascade?** No. `quality.rs` reads `Problem` but does not extend it. `CellResult` adds five fields; only constructed in two sites (`run_lahc_cell`, `run_cpsat_cell`) plus the unit tests. No cascade.
- **Markdown table width.** Going from 12 to 17 columns; rendered table is wider than the GitHub PR diff viewer comfortably shows. One-time cost; future column additions should batch or pause. Documented in the brainstorm.
- **`solve_with_config` greedy-only on grundschule may fail one predicate.** The `quality_pass_count_grundschule_fixture_passes_all_four` test may need to assert `>= 3` if greedy alone produces a schedule with `worst_spread = 3` (LAHC needed for class_day_balance optimisation). Mitigation: assert `>= 3` and document the greedy gap in a code comment; the bench's actual output uses LAHC and reports the real number.
- **`HashMap<SchoolClassId, RoomId>` from `home_rooms` may be empty on a fixture with no `home_room_id` set on any class.** `worst_home_room_ratio` returns `None` in that case; the predicate counts as passing. Documented.

## Acceptance criteria

- `mise run test:rust` green on the branch.
- `mise run test:py` green on the branch (no Python changes; smoke test).
- `mise run lint` green.
- `cargo nextest run -p solver-bench` green.
- New tests fail at HEAD~N (where N is the number of feat commits) and pass at HEAD.
- `mise run bench:bakeoff -- --budget 200ms --seeds 1 --fixtures grundschule --out /tmp/...` produces a markdown table with all 17 column headers and at least one numeric value in each new column for the grundschule/lahc row.
- BENCH_RESULTS.md is *not* refreshed in this PR (no production-cell-shape run); the existing low-budget shape demo carries over with the five new columns appended (the columns will read whatever the `mise run bench:bakeoff -- --budget 5s --seeds 4` shape demo produces, which is acceptable).
- OPEN_THINGS item 31 deleted; "next pickup" line advanced to item 32; two follow-ups added under the active sprint's tidy phase.
- Auto-memory `project_roadmap_status.md` body and description field both refreshed.
- `solver/CLAUDE.md` carries one bullet pointing at `solver-bench/src/quality.rs` as the bench-side quality evaluator.
- No ADR added or amended.
