// solver-core/tests/picker_no_double_book.rs
//
// Item 74 regression: the FFD greedy picker must not produce a
// double-booking placement under widened teacher candidates. Exercises
// the canonical grundschule fixture with teacher_candidates expanded
// to the deduplicated set of subject-qualified teachers (mirrors the
// bench's --teacher-pins=off shape from solver-bench/src/main.rs::
// unpin_teachers_in_problem). Asserts the solve succeeds without
// validate_no_double_booking firing.

use solver_core::types::{PinKind, PinnedPlacement};
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
/// classes share `teacher_candidates[0]` under unpinned mode but were
/// taught by DIFFERENT teachers in their respective prior solves. The
/// pre-solve seed sites (`validate_pins`,
/// `seed_greedy_state_from_pins`) must consume `pin.teacher_id`
/// (production wire format post-item-77) rather than collapse to
/// `lesson.assigned_teacher_id()` (which returns `teacher_candidates[0]`
/// for both lessons and false-positives
/// `validate_no_double_booking`).
#[test]
fn ffd_with_pins_carrying_teacher_id_routes_through_seed_placement() {
    let mut problem = grundschule_fixture();
    unpin_teachers(&mut problem);

    // Pick two single-hour lessons (one per class) that share a subject
    // after unpinning. The grundschule fixture's
    // `hours_per_class[c_idx][s_idx]` table puts a single-hour FÖ
    // lesson on every class; the two FÖ lessons share `teacher_candidates`.
    let pin_lessons: Vec<_> = problem
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
        .map(|l| (l.id, l.school_class_ids[0], l.teacher_candidates.clone()))
        .collect();
    assert_eq!(
        pin_lessons.len(),
        2,
        "need two single-hour lessons from different classes sharing a subject"
    );
    let (l1_id, _c1, l1_candidates) = &pin_lessons[0];
    let (l2_id, _c2, l2_candidates) = &pin_lessons[1];
    assert!(
        l1_candidates.len() >= 2,
        "unpin_teachers must widen teacher_candidates so distinct picks exist"
    );
    assert_eq!(
        l1_candidates, l2_candidates,
        "the two lessons must share teacher_candidates (same subject under unpin_teachers)"
    );

    // Stamp DIFFERENT teachers from the shared candidate list onto the
    // two pins. Production-faithful: a prior solve picked t_a for
    // lesson 1 and t_b for lesson 2; the backend's
    // `collect_pins_outside_class` passes `teacher_id` through from
    // `ScheduledLesson.teacher_id`.
    let t_a = l1_candidates[0];
    let t_b = l1_candidates[1];

    let tb_0 = problem.time_blocks[0].id;
    let room_1 = problem.rooms[1].id;
    let room_2 = problem.rooms[2].id;
    problem.pinned_placements.push(PinnedPlacement {
        lesson_id: *l1_id,
        time_block_id: tb_0,
        room_id: room_1,
        teacher_id: Some(t_a),
        kind: PinKind::Hard,
    });
    problem.pinned_placements.push(PinnedPlacement {
        lesson_id: *l2_id,
        time_block_id: tb_0,
        room_id: room_2,
        teacher_id: Some(t_b),
        kind: PinKind::Hard,
    });

    let cfg = SolveConfig {
        deadline: None,
        ..SolveConfig::default()
    };

    let solution = solve_with_config(&problem, &cfg).expect(
        "solve_with_config must succeed; validate_no_double_booking must not \
         false-positive on pinned placements whose pin.teacher_id values are \
         distinct",
    );

    // The two pin placements come out with the teachers the pin
    // requested (proving the seed reads `pin.teacher_id`, not
    // `lesson.assigned_teacher_id()`).
    let pin_placements: Vec<_> = solution
        .placements
        .iter()
        .filter(|p| p.lesson_id == *l1_id || p.lesson_id == *l2_id)
        .collect();
    assert_eq!(pin_placements.len(), 2);
    for p in pin_placements {
        let expected = if p.lesson_id == *l1_id { t_a } else { t_b };
        assert_eq!(
            p.teacher_id, expected,
            "pin's teacher_id must thread through to the placement"
        );
    }

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
