//! Schedule-quality predicates for bake-off cells.
//!
//! Mirrors the predicates `backend/src/klassenzeit_backend/scheduling/quality_checks.py`
//! enforces in the demo Grundschule integration test. The Python and Rust
//! implementations are intentionally separate: the Python version operates on
//! persisted ORM rows with a hand-supplied exempt-subjects set; the Rust
//! version operates on the in-memory [`Solution`] and infers exempt subjects
//! from [`Problem::room_subject_suitabilities`]. Cross-language parity is not
//! a contract; the two are designed to drift around their respective inputs.

use solver_core::{Problem, Solution};

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
///
/// # Panics
///
/// Panics in Task 2's stub form; Task 3 fills in the body.
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
        .is_none_or(|v| v >= QUALITY_MIN_HOME_ROOM_RATIO)
    {
        n += 1;
    }
    if report.total_interior_gaps <= QUALITY_MAX_INTERIOR_GAPS {
        n += 1;
    }
    if report
        .late_period_ratio
        .is_none_or(|v| v >= QUALITY_MIN_LATE_PERIOD_RATIO)
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
    use solver_core::{solve_with_config, PRODUCTION_ACTIVE_WEIGHTS};

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
            worst_spread: 5,                  // fail
            worst_home_room_ratio: Some(0.3), // fail
            total_interior_gaps: 10,          // fail
            late_period_ratio: Some(0.2),     // fail
        };
        assert_eq!(quality_pass_count(&report), 0);

        let report = QualityReport {
            worst_spread: 2,                  // pass
            worst_home_room_ratio: Some(0.7), // pass
            total_interior_gaps: 0,           // pass
            late_period_ratio: Some(0.4),     // fail
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
