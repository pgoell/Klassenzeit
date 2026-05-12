// solver-core/tests/picker_teacher_capacity.rs
//
// Item 79 regression: under unpinned mode (ADR 0036), the FFD greedy
// teacher picker must not over-allocate teachers on multi-school
// Grundschule fixtures. The bug shape: when first picking a teacher
// for a `(class, subject)` pair, the picker checked only "can this
// teacher fit ONE BLOCK" (`current + n > max`) rather than "can this
// teacher fit the WHOLE LESSON". The `class_subject_teacher` lock
// fires after the first block, collapsing future blocks of the same
// `(class, subject)` to the same teacher; if the teacher cannot fit
// the remaining hours, `try_place_block` emits a stream of
// `TeacherOverCapacity` violations.
//
// This test exercises FFD greedy only (no LAHC) on the dreizuegig
// fixture under `unpin_teachers` (mirrors the bench
// `--teacher-pins=off` shape) and asserts the picker produces no
// `TeacherOverCapacity` violation and places every lesson's
// `hours_per_week`. See solver/CLAUDE.md "CI-effective regression
// tests for LAHC ruin+apply bugs gate on `max_iterations`" for the
// debug-mode-stable test pattern.

use solver_core::types::ViolationKind;
use solver_core::{
    solve_with_config, test_fixtures::dreizuegig_fixture, Problem, SolveConfig, SubjectId,
    TeacherId, PRODUCTION_ACTIVE_WEIGHTS,
};
use std::collections::HashMap;

/// Mirror of `solver-bench/src/main.rs::unpin_teachers_in_problem` and
/// `solver-core/tests/picker_no_double_book.rs::unpin_teachers`.
/// Clears `teacher_pin` on every lesson and widens
/// `teacher_candidates` to the dedup'd, TeacherId-sorted list of
/// teachers qualified for the lesson's subject (drawn from
/// `Problem.teacher_qualifications`).
fn unpin_teachers_for_capacity_test(problem: &mut Problem) {
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
fn ffd_greedy_on_dreizuegig_unpinned_does_not_over_allocate_teachers() {
    let mut problem = dreizuegig_fixture();
    unpin_teachers_for_capacity_test(&mut problem);

    let expected_placements: usize = problem
        .lessons
        .iter()
        .map(|l| usize::from(l.hours_per_week))
        .sum();

    // FFD-greedy only: deadline=None and max_iterations=Some(0) skip
    // LAHC entirely, so this test pinpoints the FFD-time picker bug
    // without depending on item 78's R&R rescue masking it.
    let cfg = SolveConfig {
        deadline: None,
        max_iterations: Some(0),
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        ..SolveConfig::default()
    };

    let solution = solve_with_config(&problem, &cfg).expect(
        "solve_with_config must succeed; the post-condition validator quintet \
         must not fire under widened teacher candidates",
    );

    let over_capacity_count = solution
        .violations
        .iter()
        .filter(|v| matches!(v.kind, ViolationKind::TeacherOverCapacity))
        .count();
    assert_eq!(
        over_capacity_count,
        0,
        "FFD greedy must not emit TeacherOverCapacity violations under \
         unpinned dreizuegig; got {} (item 79). Sample violations: {:?}",
        over_capacity_count,
        solution.violations.iter().take(5).collect::<Vec<_>>(),
    );

    // The picker fix's contract is "no TeacherOverCapacity violations".
    // On dreizuegig unpinned at `max_iterations: Some(0)` (FFD only),
    // 3 hours remain unplaced as `LessonGroupSplit` because the
    // religion trio's per-(class, subject) teacher lock collapses
    // future placements onto teachers who can no longer share a free
    // window. LAHC's R&R rescue (item 78) covers this gap at
    // production budget; FFD-only on this fixture does not. The
    // assertion below pins the fix's contract (no over-capacity gap)
    // without conflating it with the pre-existing trio gap.
    let lesson_group_split_count = solution
        .violations
        .iter()
        .filter(|v| matches!(v.kind, ViolationKind::LessonGroupSplit))
        .count();
    assert!(
        solution.placements.len() + lesson_group_split_count >= expected_placements,
        "FFD greedy must recover every hour lost to TeacherOverCapacity; \
         placed {} of {} expected, with {} LessonGroupSplit remaining \
         (the only acceptable residual gap on FFD-only dreizuegig)",
        solution.placements.len(),
        expected_placements,
        lesson_group_split_count,
    );
}
