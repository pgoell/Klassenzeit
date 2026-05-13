//! Integration test for OPEN_THINGS item 49: the R&R recreate phase scores
//! candidate placements by `slice + home_room_penalty(room)` and picks the
//! lowest-total candidate, restoring home-room placements that the FFD
//! greedy bootstrap forced into a non-home room when the home room was
//! contended.
//!
//! Fixture shape: one class C0 with `home_room_id = R_HOME`, one lesson L1
//! with `hours_per_week = 2, preferred_block_size = 1`, three free time
//! blocks on day 0, two rooms (R_HOME, R_OTHER). With R&R firing every
//! iteration, the post-LAHC placements of L1 must both sit in R_HOME (the
//! home-room delta makes any non-home placement strictly worse on the new
//! picker). Pre-fix the recreate's lowest-`slice` pick was indifferent to
//! home-room delta, so a recreate could land L1 in R_OTHER once and stay
//! there forever (LAHC Change does not move rooms).

use std::time::Duration;

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::solve_with_config;
use solver_core::types::{
    ConstraintWeights, Lesson, Problem, Room, SchoolClass, Solution, SolveConfig, Subject, Teacher,
    TeacherQualification, TimeBlock,
};
use uuid::Uuid;

fn rr_recreate_id(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

#[test]
fn lahc_rr_recreate_picks_lowest_soft_delta() {
    let c0 = SchoolClassId(rr_recreate_id(1));
    let t1 = TeacherId(rr_recreate_id(2));
    let s_math = SubjectId(rr_recreate_id(10));
    let r_home = RoomId(rr_recreate_id(20));
    let r_other = RoomId(rr_recreate_id(21));
    let tb1 = TimeBlockId(rr_recreate_id(30));
    let tb2 = TimeBlockId(rr_recreate_id(31));
    let tb3 = TimeBlockId(rr_recreate_id(32));
    let l1 = LessonId(rr_recreate_id(40));

    let problem = Problem {
        time_blocks: vec![
            TimeBlock {
                id: tb1,
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: tb2,
                day_of_week: 0,
                position: 1,
            },
            TimeBlock {
                id: tb3,
                day_of_week: 0,
                position: 2,
            },
        ],
        teachers: vec![Teacher {
            id: t1,
            max_hours_per_week: 10,
            reserve_hours_per_week: 0,
        }],
        rooms: vec![Room { id: r_home }, Room { id: r_other }],
        school_classes: vec![SchoolClass {
            id: c0,
            home_room_id: Some(r_home),
            max_lessons_per_day: None,
            class_teacher_id: None,
        }],
        subjects: vec![Subject {
            id: s_math,
            max_hours_per_day: 4,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_early_period: 0,
            prefer_late_period: 0,
        }],
        lessons: vec![Lesson {
            id: l1,
            subject_id: s_math,
            teacher_candidates: vec![t1],
            teacher_pin: Some(t1),
            school_class_ids: vec![c0],
            hours_per_week: 2,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: t1,
            subject_id: s_math,
        }],
        room_subject_suitabilities: vec![],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        pinned_placements: vec![],
    };

    let config = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_home_room: 100,
            ..ConstraintWeights::default()
        },
        deadline: Some(Duration::from_millis(50)),
        seed: 0,
        max_iterations: Some(5_000),
        lahc_rr_period: Some(1),
        ..SolveConfig::default()
    };

    let solution: Solution = solve_with_config(&problem, &config).expect("solve");

    assert!(
        solution.violations.is_empty(),
        "expected no violations, got {:?}",
        solution.violations
    );
    assert_eq!(
        solution.placements.len(),
        2,
        "two single-period placements expected"
    );
    for p in &solution.placements {
        assert_eq!(
            p.room_id, r_home,
            "every placement should be in the home room (R_HOME, id=20). Got {:?}",
            p.room_id
        );
    }
}
