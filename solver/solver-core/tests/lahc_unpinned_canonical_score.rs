//! Regression test for OPEN_THINGS item 76 (LAHC `canonical_score` drift on
//! the unpinned-teacher solve path). The per-iteration `debug_assert_eq!` at
//! `solver-core/src/lahc.rs:260` panics on master with `state.canonical_score`
//! drifting from `score::score_solution(...)` by the `prefer_class_teacher`
//! axis (weight 5 in PRODUCTION_ACTIVE_WEIGHTS) when teachers are unpinned and
//! the per-(class, subject) lock map gets exercised by LAHC moves.
//!
//! Uses `dreizuegig_fixture` (12 classes, 102 lessons, 294 placements):
//! `grundschule_fixture` and `zweizuegig_fixture` both early-exit LAHC at
//! `canonical_score == 0` within a few hundred iterations even with unpinned
//! teachers, so the drift surface isn't exercised. The dreizügig fixture's
//! richer teacher-qualification graph plus its per-Jahrgang Religion trios
//! keep LAHC iterating long enough for the missing-axis bug to surface.
//!
//! Gated on `max_iterations: Some(5000)` per `solver/CLAUDE.md` so the test is
//! deterministic across debug / release modes. `lahc_rr_period: Some(2)` and
//! `lahc_kempe_period: Some(2)` raise R&R and Kempe rates so the drift
//! surface is exercised within the iteration budget.

use std::collections::HashMap;
use std::time::Duration;

use solver_core::test_fixtures::dreizuegig_fixture;
use solver_core::{
    solve_with_config, Problem, SolveConfig, SubjectId, TeacherId, PRODUCTION_ACTIVE_WEIGHTS,
};

/// Mirror of `tests/picker_no_double_book.rs::unpin_teachers`. Globally-unique
/// helper name per the unique-fn-name rule in `.claude/CLAUDE.md`.
fn lahc_unpinned_test_unpin_teachers(problem: &mut Problem) {
    let mut quals_by_subject: HashMap<SubjectId, Vec<TeacherId>> = HashMap::new();
    for q in &problem.teacher_qualifications {
        quals_by_subject
            .entry(q.subject_id)
            .or_default()
            .push(q.teacher_id);
    }
    for v in quals_by_subject.values_mut() {
        v.sort_by_key(|t| t.0);
        v.dedup();
    }
    for lesson in &mut problem.lessons {
        lesson.teacher_pin = None;
        lesson.teacher_candidates = quals_by_subject
            .get(&lesson.subject_id)
            .cloned()
            .unwrap_or_default();
    }
}

#[test]
fn lahc_unpinned_dreizuegig_keeps_canonical_score_in_sync() {
    let mut problem = dreizuegig_fixture();
    lahc_unpinned_test_unpin_teachers(&mut problem);

    let config = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS.clone(),
        // Deadline must be Some(_) for `lahc::run` to engage the local-search
        // loop at all; set well above the wall-clock cost of `max_iterations`
        // iterations so `max_iterations` is the binding cap.
        deadline: Some(Duration::from_secs(60)),
        max_iterations: Some(5000),
        lahc_rr_period: Some(2),
        lahc_kempe_period: Some(2),
        ..SolveConfig::default()
    };

    // The per-iteration `debug_assert_eq!(state.canonical_score,
    // score_solution(...))` inside the LAHC loop is the implicit gate:
    // if `state.canonical_score` drifts from the true score, the solver
    // panics inside `klassenzeit_solver` (debug build) before this line
    // returns. Acceptance: `solve_with_config` returns `Ok(_)`.
    let result = solve_with_config(&problem, &config);
    assert!(
        result.is_ok(),
        "LAHC should complete without canonical_score drift; got: {result:?}",
    );
}
