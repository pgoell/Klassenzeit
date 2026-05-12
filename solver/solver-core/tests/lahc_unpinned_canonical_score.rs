//! Regression test for OPEN_THINGS item 76 (LAHC `canonical_score` drift on
//! the unpinned-teacher solve path). The per-iteration `debug_assert_eq!` at
//! `solver-core/src/lahc.rs:260` panics on master with `state.canonical_score`
//! drifting from `score::score_solution(...)` when teachers are unpinned and
//! the per-(class, subject) lock map gets exercised by LAHC moves. The
//! second drift mechanism (closed atomically with this test) is Kempe's
//! `kempe_snapshot_pre_score` reading `lesson.assigned_teacher_id()` (pin
//! shorthand) instead of `lesson_teacher_in_state(state, lesson)` (the
//! lock-map teacher that `kempe_apply_block` actually writes), so the
//! teacher-gap partition snapshotted is the wrong one and
//! `kempe_post_score_delta` misses every change to the real teacher's
//! `(teacher, day)` gap count. Same shape applies to
//! `running_slice_from_placements` (called by R&R after recreate), which
//! must also read `p.teacher_id` rather than the static pin.
//!
//! Uses `dreizuegig_fixture` (12 classes, 102 lessons, 294 placements):
//! `grundschule_fixture` and `zweizuegig_fixture` both early-exit LAHC at
//! `canonical_score == 0` within a few hundred iterations even with unpinned
//! teachers, so the drift surface isn't exercised. The dreizügig fixture's
//! richer teacher-qualification graph plus its per-Jahrgang Religion trios
//! keep LAHC iterating long enough for the missing-axis bug to surface.
//!
//! Gated on `max_iterations: Some(10_000)` per `solver/CLAUDE.md` so the test
//! is deterministic across debug / release modes. `lahc_rr_period: Some(2)`
//! and `lahc_kempe_period: Some(2)` raise R&R and Kempe rates so the drift
//! surface is exercised within the iteration budget. Each fixture's
//! `school_classes` get a `class_teacher_id` assigned post-build so the
//! `prefer_class_teacher` axis (weight 5 in `PRODUCTION_ACTIVE_WEIGHTS`) is
//! exercised, mirroring the Hessen Grundschule's Klassenlehrer-per-Klasse
//! rule the backend seed honours. The Rust fixtures default to
//! `class_teacher_id: None` so without this assignment the `prefer_class_teacher`
//! component is always 0 and the second drift mechanism (Kempe's
//! `canonical_delta` omits the axis) cannot fire.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use solver_core::test_fixtures::{dreizuegig_fixture, grundschule_fixture};
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

/// Assign every class a `class_teacher_id` chosen from the teachers
/// qualified for at least one of the class's own lessons' subjects.
/// Lowest-`TeacherId.0` pick keeps the assignment deterministic. Skips
/// classes whose candidate set is empty (no lesson references them, so
/// the prefer_class_teacher axis cannot apply anyway). The Hessen
/// Grundschule rule is "Klassenlehrer is qualified for several core
/// subjects of their own class"; this helper simplifies to "any
/// qualified teacher for any subject the class learns".
fn lahc_unpinned_test_assign_class_teachers(problem: &mut Problem) {
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
fn lahc_unpinned_dreizuegig_keeps_canonical_score_in_sync() {
    let mut problem = dreizuegig_fixture();
    lahc_unpinned_test_unpin_teachers(&mut problem);
    lahc_unpinned_test_assign_class_teachers(&mut problem);

    // Item 57: zero out the new per-class worst-case axes; Task 2 wires the
    // LAHC delta arithmetic through. Task 1 only adds them to score_solution
    // / ConstraintWeights, so the per-iteration debug_assert_eq! gate would
    // otherwise fire on every iteration that mutates per-class counts. The
    // test's intent (canonical_score stays in sync under unpinned mode) is
    // preserved against the pre-item-57 axes.
    let mut weights = PRODUCTION_ACTIVE_WEIGHTS.clone();
    weights.max_per_class_spread = 0;
    weights.max_per_class_interior_gaps = 0;
    let config = SolveConfig {
        weights,
        // Deadline must be Some(_) for `lahc::run` to engage the local-search
        // loop at all; set well above the wall-clock cost of `max_iterations`
        // iterations so `max_iterations` is the binding cap.
        deadline: Some(Duration::from_secs(60)),
        max_iterations: Some(10_000),
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

#[test]
fn lahc_unpinned_einzuegig_with_klassenlehrer_keeps_canonical_score_in_sync() {
    let mut problem = grundschule_fixture();
    lahc_unpinned_test_unpin_teachers(&mut problem);
    lahc_unpinned_test_assign_class_teachers(&mut problem);

    // Item 57: see note on `lahc_unpinned_dreizuegig_...` above.
    let mut weights = PRODUCTION_ACTIVE_WEIGHTS.clone();
    weights.max_per_class_spread = 0;
    weights.max_per_class_interior_gaps = 0;
    let config = SolveConfig {
        weights,
        deadline: Some(Duration::from_secs(60)),
        max_iterations: Some(10_000),
        lahc_rr_period: Some(2),
        lahc_kempe_period: Some(2),
        ..SolveConfig::default()
    };

    let result = solve_with_config(&problem, &config);
    assert!(
        result.is_ok(),
        "LAHC should complete without canonical_score drift; got: {result:?}",
    );
}
