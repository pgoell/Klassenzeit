// solver-core/tests/picker_no_double_book.rs
//
// Item 74 regression: the FFD greedy picker must not produce a
// double-booking placement under widened teacher candidates. Exercises
// the canonical grundschule fixture with teacher_candidates expanded
// to the deduplicated set of subject-qualified teachers (mirrors the
// bench's --teacher-pins=off shape from solver-bench/src/main.rs::
// unpin_teachers_in_problem). Asserts the solve succeeds without
// validate_no_double_booking firing.

use solver_core::{
    solve_with_config, test_fixtures::grundschule_fixture, Problem, SolveConfig, SubjectId,
    TeacherId,
};
use std::collections::HashMap;

/// Mirror of `solver-bench/src/main.rs::unpin_teachers_in_problem`.
/// Clears `teacher_pin` on every lesson and widens `teacher_candidates`
/// to the dedup'd, TeacherId-sorted list of teachers qualified for the
/// lesson's subject (drawn from `Problem.teacher_qualifications`).
fn unpin_teachers(problem: &mut Problem) {
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
fn ffd_greedy_on_grundschule_unpinned_does_not_double_book() {
    let mut problem = grundschule_fixture();
    unpin_teachers(&mut problem);

    let cfg = SolveConfig {
        deadline: None,
        ..SolveConfig::default()
    };

    let solution = solve_with_config(&problem, &cfg).expect(
        "solve_with_config must succeed; the post-condition validators \
                 (including validate_no_double_booking) must not fire under \
                 widened teacher candidates",
    );

    assert!(
        solution.placements.iter().all(|p| {
            problem
                .lessons
                .iter()
                .find(|l| l.id == p.lesson_id)
                .is_some_and(|l| l.teacher_candidates.contains(&p.teacher_id))
        }),
        "every Placement.teacher_id must be in the corresponding Lesson.teacher_candidates"
    );
}
