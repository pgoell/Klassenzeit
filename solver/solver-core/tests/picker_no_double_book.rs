// solver-core/tests/picker_no_double_book.rs
//
// Item 74 regression: the FFD greedy picker must not produce a
// double-booking placement under widened teacher candidates. Exercises
// the canonical grundschule fixture with teacher_candidates expanded
// to the deduplicated set of subject-qualified teachers (mirrors the
// bench's --teacher-pins=off shape from solver-bench/src/main.rs::
// unpin_teachers_in_problem). Asserts the solve succeeds without
// validate_no_double_booking firing.

use solver_core::types::PinnedPlacement;
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

/// Item 77 regression. Two pinned single-hour lessons from different
/// classes share `teacher_candidates[0]` under unpinned mode. The
/// backend's per-class `/schedule` flow passes prior-class placements as
/// `pinned_placements` without `teacher_id` populated, and the pre-solve
/// seed sites (validate_pins, seed_greedy_state_from_pins) stamp the pin
/// with `lesson.assigned_teacher_id()`, which falls back to
/// `teacher_candidates[0]` when `teacher_pin` is None. Both seed
/// Placements end up with the same pseudo-teacher at the same time
/// block but in different rooms, and `validate_no_double_booking`
/// rejects the output with `input: double-booking`.
#[test]
fn ffd_with_pins_carrying_no_teacher_id_does_not_collapse_to_static_fallback() {
    let mut problem = grundschule_fixture();
    unpin_teachers(&mut problem);

    // Pick the two single-hour lessons (one per class) that share
    // `teacher_candidates[0]` after unpinning. The grundschule fixture's
    // `hours_per_class[c_idx][s_idx]` table puts a single-hour FÖ lesson
    // (subject index 5) on every class; after `unpin_teachers` both FÖ
    // lessons inherit the same sorted teacher_candidates from their
    // shared subject_id qualifications.
    let fö_lessons: Vec<_> = problem
        .lessons
        .iter()
        .filter(|l| l.hours_per_week == 1 && l.preferred_block_size == 1)
        .filter(|l| l.school_class_ids.len() == 1)
        .filter(|l| {
            problem
                .lessons
                .iter()
                .any(|other| other.id != l.id && other.subject_id == l.subject_id)
        })
        .take(2)
        .map(|l| (l.id, l.school_class_ids[0], l.subject_id))
        .collect();
    assert_eq!(
        fö_lessons.len(),
        2,
        "need two single-hour lessons from different classes sharing a subject"
    );
    let (l1_id, _c1, s1_id) = fö_lessons[0];
    let (l2_id, _c2, s2_id) = fö_lessons[1];
    assert_eq!(s1_id, s2_id, "the two lessons must share a subject");

    // Pin both at TB_0 (Mon period 0) in different rooms. Different
    // classes + different rooms → no class / room conflict at pin-time;
    // validate_pins accepts both. The teacher field on the seed
    // Placement is what trips the validator.
    let tb_0 = problem.time_blocks[0].id;
    let room_1 = problem.rooms[1].id;
    let room_2 = problem.rooms[2].id;
    problem.pinned_placements.push(PinnedPlacement {
        lesson_id: l1_id,
        time_block_id: tb_0,
        room_id: room_1,
    });
    problem.pinned_placements.push(PinnedPlacement {
        lesson_id: l2_id,
        time_block_id: tb_0,
        room_id: room_2,
    });

    let cfg = SolveConfig {
        deadline: None,
        ..SolveConfig::default()
    };

    let solution = solve_with_config(&problem, &cfg).expect(
        "solve_with_config must succeed; validate_no_double_booking must not \
         false-positive on pinned placements that share teacher_candidates[0] \
         under unpinned mode",
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
