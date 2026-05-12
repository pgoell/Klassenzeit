//! Regression test for OPEN_THINGS item 78 (LAHC R&R rescue mode for
//! FFD-unplaced lessons). Under widened teacher candidates (ADR 0036
//! unpinned mode), every LAHC variant on master plateaus at the FFD-greedy
//! stage on multi-school fixtures: zweizuegig shows `hard_med=2`,
//! dreizuegig `hard_med=13`, with violations of kind `no_free_time_block`
//! and `teacher_over_capacity` (FFD-unplaced lessons whose missing hours
//! the post-condition validators surface). Today's `rr_attempt` only
//! ruins-and-recreates anchors already on the placements vector, so
//! FFD-unplaced lessons never enter the anchor set and the post-FFD
//! plateau is structurally unreachable from local search.
//!
//! These two tests fail on master and pass once `rr_rescue_attempt` ships
//! in `solver/solver-core/src/lahc.rs`. The rescue extends each R&R
//! iteration so that when any lesson is under-placed, the move ruins one
//! same-class anchor and tries to place the under-placed lesson into the
//! freed window. Acceptance is feasibility-only: the under-placed lesson
//! must come back fully placed AND the ruined block must fully recreate.
//!
//! Gated on `deadline: Some(7_500ms), lahc_rr_period: Some(5), seed: 42`
//! to keep the test deterministic and bound the wall-clock cost in CI.
//! The deadline must be `Some(_)` for `lahc::run` to engage the LAHC loop
//! at all; the higher rescue-frequency `lahc_rr_period: 5` (vs the
//! production default 25) raises rescue iteration density inside the
//! budget so multi-school fixtures can reach feasibility. The 7500 ms
//! budget gives ~50% headroom over the production 5000 ms default; pre-
//! item-57 the dreizuegig rescue hugged 5.014 s in isolation, and the
//! canonical-objective widening (item 57) raised per-iteration cost just
//! enough that parallel-pressure CI runs lost one placement under the
//! tighter budget.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use solver_core::test_fixtures::{dreizuegig_fixture, zweizuegig_fixture};
use solver_core::{
    solve_with_config, Problem, SolveConfig, SubjectId, TeacherId, PRODUCTION_ACTIVE_WEIGHTS,
};

/// Mirror of `tests/picker_no_double_book.rs::unpin_teachers` and
/// `tests/lahc_unpinned_canonical_score.rs::lahc_unpinned_test_unpin_teachers`.
/// Globally-unique helper name per the unique-fn-name rule.
fn lahc_rescue_test_unpin_teachers(problem: &mut Problem) {
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

/// Mirror of `tests/lahc_unpinned_canonical_score.rs::lahc_unpinned_test_assign_class_teachers`.
/// Globally-unique helper name per the unique-fn-name rule. Required so the
/// `prefer_class_teacher` cost axis is exercised under unpinned mode,
/// matching the Hessen Klassenlehrer convention the backend seed honours.
fn lahc_rescue_test_assign_class_teachers(problem: &mut Problem) {
    let mut subjects_per_class: HashMap<_, HashSet<SubjectId>> = HashMap::new();
    for lesson in &problem.lessons {
        for class_id in &lesson.school_class_ids {
            subjects_per_class
                .entry(*class_id)
                .or_default()
                .insert(lesson.subject_id);
        }
    }
    let mut qualified_for_subject: HashMap<SubjectId, HashSet<TeacherId>> = HashMap::new();
    for q in &problem.teacher_qualifications {
        qualified_for_subject
            .entry(q.subject_id)
            .or_default()
            .insert(q.teacher_id);
    }
    for class in &mut problem.school_classes {
        let Some(subjects) = subjects_per_class.get(&class.id) else {
            continue;
        };
        let mut candidates: HashSet<TeacherId> = HashSet::new();
        for sid in subjects {
            if let Some(qs) = qualified_for_subject.get(sid) {
                for t in qs {
                    candidates.insert(*t);
                }
            }
        }
        let mut sorted: Vec<TeacherId> = candidates.into_iter().collect();
        sorted.sort_unstable_by_key(|t| t.0);
        if let Some(klt) = sorted.first().copied() {
            class.class_teacher_id = Some(klt);
        }
    }
}

#[test]
fn lahc_rescue_zweizuegig_unpinned_reaches_feasibility() {
    let mut problem = zweizuegig_fixture();
    lahc_rescue_test_unpin_teachers(&mut problem);
    lahc_rescue_test_assign_class_teachers(&mut problem);

    let placements_expected: usize = problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as usize)
        .sum();

    let config = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS.clone(),
        deadline: Some(Duration::from_millis(7_500)),
        lahc_rr_period: Some(5),
        seed: 42,
        ..SolveConfig::default()
    };

    let solution = solve_with_config(&problem, &config)
        .expect("solver should not return Err(Error::Input) on zweizuegig unpinned");
    assert_eq!(
        solution.placements.len(),
        placements_expected,
        "rescue should reach full placement on zweizuegig unpinned; got {} of {}",
        solution.placements.len(),
        placements_expected,
    );
    assert!(
        solution.violations.is_empty(),
        "rescue should produce zero hard violations on zweizuegig unpinned; got: {:?}",
        solution.violations,
    );
}

#[test]
fn lahc_rescue_dreizuegig_unpinned_reaches_feasibility() {
    let mut problem = dreizuegig_fixture();
    lahc_rescue_test_unpin_teachers(&mut problem);
    lahc_rescue_test_assign_class_teachers(&mut problem);

    let placements_expected: usize = problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as usize)
        .sum();

    let config = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS.clone(),
        deadline: Some(Duration::from_millis(7_500)),
        lahc_rr_period: Some(5),
        seed: 42,
        ..SolveConfig::default()
    };

    let solution = solve_with_config(&problem, &config)
        .expect("solver should not return Err(Error::Input) on dreizuegig unpinned");
    assert_eq!(
        solution.placements.len(),
        placements_expected,
        "rescue should reach full placement on dreizuegig unpinned; got {} of {}",
        solution.placements.len(),
        placements_expected,
    );
    assert!(
        solution.violations.is_empty(),
        "rescue should produce zero hard violations on dreizuegig unpinned; got: {:?}",
        solution.violations,
    );
}
