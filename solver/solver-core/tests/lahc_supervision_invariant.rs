//! Regression test: LAHC must keep `state.canonical_score ==
//! score_solution(...)` at every iteration tail when break-kind TimeBlocks
//! cause `weights.supervision_spread` to contribute a non-zero summand to
//! `score_solution`.
//!
//! Tasks 1-3 of the supervision-objective sprint wired
//! `weights.supervision_spread * compute_supervision_spread(...)` into
//! `score_solution` but did not extend the LAHC Change-move's per-iteration
//! delta arithmetic, so `state.canonical_score` drifts from
//! `score_solution(...)` by the supervision contribution. The
//! per-iteration `debug_assert_eq!(state.canonical_score,
//! score_solution(...))` inside the LAHC loop (around `lahc.rs:320`) panics
//! whenever an accepted Change move changes the supervision spread.
//!
//! The fix recomputes the canonical score from the full scorer at each
//! Change-move accept site. Accepts are 1-3 orders of magnitude rarer than
//! proposals, so the extra O(N) supervision pass per accept is negligible.
//!
//! The fixture mutates `grundschule_fixture` to mark several of the
//! existing TimeBlocks as `Break` (so FFD does not need to invent new
//! positions while the break adjacency surface is non-trivial). LAHC then
//! moves lessons across days, the supervision spread changes from
//! iteration to iteration, and the canonical-score invariant fires on
//! master.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use solver_core::test_fixtures::dreizuegig_fixture;
use solver_core::{
    ids::{SubjectId, TeacherId},
    score::score_solution,
    solve_with_config,
    types::{Problem, SolveConfig, TimeBlockKind},
    PRODUCTION_ACTIVE_WEIGHTS,
};

/// Replace every TimeBlock at `position == 2` with `TimeBlockKind::Break`,
/// removing any pinned placements / lessons whose teacher_pin still points
/// at that block. The grundschule fixture has 5 days x 5 positions; making
/// position 2 a break leaves four lesson positions per day (0, 1, 3, 4) and
/// puts breaks adjacent to lessons on both sides.
fn supinv_mark_position_two_as_break(problem: &mut Problem) {
    for tb in &mut problem.time_blocks {
        if tb.position == 2 {
            tb.kind = TimeBlockKind::Break;
        }
    }
}

/// Mirror `tests/lahc_unpinned_canonical_score.rs::lahc_unpinned_test_unpin_teachers`.
/// LAHC's Change move only moves placements; teacher pins keep
/// `state.class_subject_teacher` quiescent. Unpinning teachers exercises
/// the search surface more thoroughly so the assert fires within the
/// short deadline.
fn supinv_unpin_teachers(problem: &mut Problem) {
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

/// Match `lahc_unpinned_test_assign_class_teachers` shape: every class
/// gets a `class_teacher_id` picked from a teacher qualified for at least
/// one of the class's subjects (lowest TeacherId tiebreak).
fn supinv_assign_class_teachers(problem: &mut Problem) {
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
fn lahc_canonical_score_invariant_holds_with_break_blocks() {
    let mut problem = dreizuegig_fixture();
    supinv_mark_position_two_as_break(&mut problem);
    supinv_unpin_teachers(&mut problem);
    supinv_assign_class_teachers(&mut problem);

    let config = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS.clone(),
        // Bounded iteration count keeps the test deterministic across debug
        // / release modes. Leave `lahc_rr_period` and `lahc_kempe_period`
        // unset so every iteration is a Change attempt: Change is the only
        // move type whose canonical-score maintenance goes through the
        // per-move delta path (R&R and Kempe full-recompute via
        // `score_solution` already). The per-iteration `debug_assert_eq!`
        // at the LAHC iteration tail is the implicit gate.
        deadline: Some(Duration::from_secs(60)),
        max_iterations: Some(10_000),
        ..SolveConfig::default()
    };

    let solution = solve_with_config(&problem, &config)
        .expect("LAHC must complete without canonical_score drift");

    // Sanity check: the post-solve soft_score must match a fresh
    // score_solution call. On master (pre-fix) the per-iteration assert
    // panics before this line, so the assert below only runs after the
    // fix lands; treat it as a belt-and-braces check.
    let recomputed = score_solution(&problem, &solution.placements, &PRODUCTION_ACTIVE_WEIGHTS);
    assert_eq!(
        solution.soft_score, recomputed,
        "post-solve soft_score must match score_solution",
    );
}
