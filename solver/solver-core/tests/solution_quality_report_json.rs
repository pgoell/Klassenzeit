//! End-to-end test: `Solution.quality_report` survives JSON roundtrip and
//! matches both `Solution.soft_score` and the freestanding
//! `quality_report_json` adapter that `klassenzeit_solver.cpsat` consumes.
//! Item 58.

use solver_core::{
    json::{quality_report_json, solve_json_with_config},
    quality::QualityReport,
    test_fixtures::grundschule_fixture,
    types::Solution,
};

#[test]
fn solution_quality_report_survives_json_roundtrip_and_matches_soft_score() {
    let problem = grundschule_fixture();
    let problem_json = serde_json::to_string(&problem).unwrap();

    // deadline_ms=None skips LAHC entirely (greedy-only); the parity invariant
    // and roundtrip behaviour are independent of the LAHC pass. Keeping it None
    // makes the test fast and deterministic.
    let solution_json = solve_json_with_config(&problem_json, None, None, None).unwrap();

    let solution: Solution = serde_json::from_str(&solution_json).unwrap();
    assert_eq!(
        solution.quality_report.weighted_score, solution.soft_score,
        "soft_score must equal quality_report.weighted_score post-roundtrip"
    );

    // Cross-check against the freestanding quality_report_json adapter that
    // cpsat.py consumes; both paths must produce the same QualityReport for
    // the same (problem, placements, violations) triple.
    let placements_json = serde_json::to_string(&solution.placements).unwrap();
    let violations_json = serde_json::to_string(&solution.violations).unwrap();
    let report_json =
        quality_report_json(&problem_json, &placements_json, &violations_json).unwrap();
    let report_via_adapter: QualityReport = serde_json::from_str(&report_json).unwrap();
    assert_eq!(
        report_via_adapter, solution.quality_report,
        "freestanding quality_report_json must equal the attached Solution.quality_report"
    );
}
