//! Integration tests for ProgressBeacon plumbing in LAHC.
//!
//! 1. Byte-equal determinism across a 50-seed sweep: solving with and without
//!    a beacon must produce identical `(Solution, _)` when no cancel is
//!    signalled.
//! 2. Cancel exits promptly and stamps `was_cancelled` on the result.
//! 3. The beacon's iter advances during a live solve.

use std::thread;
use std::time::Duration;

use solver_core::test_fixtures::grundschule_fixture;
use solver_core::{solve, ProgressBeacon, SolveConfig};

/// Byte-equal determinism on the deadline-only path across 50 distinct seeds.
/// Uses the lower-level `solve_with_config_stats` / `solve_with_progress`
/// Rust APIs (not the JSON entry) so we can vary `SolveConfig.seed`.
#[test]
fn beacon_does_not_perturb_determinism_across_seed_sweep() {
    let problem = grundschule_fixture();
    for seed in 0..50u64 {
        let config = SolveConfig {
            seed,
            deadline: Some(Duration::from_millis(50)),
            ..SolveConfig::default()
        };

        let (without_solution, _without_stats) =
            solve::solve_with_config_stats(&problem, &config).expect("solve without");

        let beacon = ProgressBeacon::new();
        let (with_solution, _with_stats) =
            solve::solve_with_progress(&problem, &config, &beacon).expect("solve with");

        assert_eq!(
            without_solution, with_solution,
            "beacon perturbed solution on seed {seed}"
        );
        assert!(
            !with_solution.was_cancelled,
            "uncancelled solve must report false"
        );
    }
}

/// Setting cancel_requested causes the loop to exit within a short wall-clock
/// budget and return a Solution (feasible or partial, both acceptable) with
/// `was_cancelled = true`.
#[test]
fn cancel_returns_best_so_far_promptly() {
    let problem = grundschule_fixture();
    let problem_json = serde_json::to_string(&problem).expect("encode");
    let beacon = ProgressBeacon::new();
    let beacon_observer = std::sync::Arc::clone(&beacon);

    let handle = thread::spawn(move || {
        solver_core::solve_json_with_progress(&problem_json, Some(10_000), &beacon, None, None)
    });

    thread::sleep(Duration::from_millis(50));
    beacon_observer.request_cancel();
    let start = std::time::Instant::now();
    let result_json = handle.join().expect("join").expect("solve");
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(500),
        "cancel was slow: {elapsed:?}"
    );
    assert!(
        result_json.contains("\"was_cancelled\":true"),
        "cancelled solution must carry was_cancelled=true: {result_json}"
    );
}

/// During a solve, the beacon's iter counter must reach a non-zero value.
#[test]
fn beacon_iter_advances_during_solve() {
    let problem = grundschule_fixture();
    let problem_json = serde_json::to_string(&problem).expect("encode");
    let beacon = ProgressBeacon::new();
    let beacon_observer = std::sync::Arc::clone(&beacon);

    let handle = thread::spawn(move || {
        solver_core::solve_json_with_progress(&problem_json, Some(500), &beacon, None, None)
    });

    let mut seen_progress = false;
    for _ in 0..20 {
        thread::sleep(Duration::from_millis(25));
        if beacon_observer.iter_snapshot() > 0 {
            seen_progress = true;
            break;
        }
    }
    let _ = handle.join().expect("join").expect("solve");
    assert!(
        seen_progress,
        "beacon iter never advanced during a 500ms solve"
    );
}

/// Backward-compat: a Solution JSON that predates `was_cancelled` must
/// deserialize cleanly with `was_cancelled = false`.
#[test]
fn solution_was_cancelled_defaults_to_false_on_legacy_json() {
    use solver_core::quality::QualityReport;
    use solver_core::Solution;
    let qr_json = serde_json::to_string(&QualityReport::default()).expect("encode qr");
    let legacy_json =
        format!(r#"{{"placements":[],"violations":[],"soft_score":0,"quality_report":{qr_json}}}"#);
    let parsed: Solution = serde_json::from_str(&legacy_json).expect("legacy decode");
    assert!(!parsed.was_cancelled, "missing field must default to false");
}
