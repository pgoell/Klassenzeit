//! Integration test: FFD and LAHC honour `Problem.pre_first_slot_grace_minutes`.
//!
//! When the school's pre-first-slot grace covers the lesson's `pre_buffer_minutes`,
//! a buffered lesson is legal at day-position 0. When the grace is too small (or
//! zero), the day-edge rejection holds and the solver leaves the lesson unplaced.
//!
//! ADR 0044 amendment.

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::solve_with_config;
use solver_core::types::{
    Lesson, Problem, Room, SchoolClass, SolveConfig, Subject, Teacher, TeacherQualification,
    TimeBlock, TimeBlockKind, PRODUCTION_ACTIVE_WEIGHTS,
};
use uuid::Uuid;

#[test]
fn ffd_places_buffered_lesson_at_pos0_under_grace() {
    let problem = single_buffered_lesson_problem(/* grace */ 15, /* pre */ 15);
    let config = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        deadline: None,
        ..Default::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve returns Ok");
    assert_eq!(
        solution.placements.len(),
        1,
        "expected one placement under grace; got {:?}",
        solution.placements
    );
    assert_eq!(
        solution.placements[0].time_block_id,
        problem.time_blocks[0].id
    );
}

#[test]
fn ffd_rejects_buffered_lesson_at_pos0_without_grace() {
    let problem = single_buffered_lesson_problem(/* grace */ 0, /* pre */ 15);
    let config = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        deadline: None,
        ..Default::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve returns Ok");
    assert!(
        solution.placements.is_empty(),
        "expected no placement at pos=0 without grace; got {:?}",
        solution.placements
    );
}

fn single_buffered_lesson_problem(grace: u8, pre: u8) -> Problem {
    let class_id = SchoolClassId(Uuid::new_v4());
    let teacher_id = TeacherId(Uuid::new_v4());
    let subject_id = SubjectId(Uuid::new_v4());
    let room_id = RoomId(Uuid::new_v4());
    let tb_id = TimeBlockId(Uuid::new_v4());
    let lesson_id = LessonId(Uuid::new_v4());
    Problem {
        time_blocks: vec![TimeBlock {
            id: tb_id,
            day_of_week: 0,
            position: 0,
            kind: TimeBlockKind::Lesson,
        }],
        teachers: vec![Teacher {
            id: teacher_id,
            max_hours_per_week: 10,
            reserve_hours_per_week: 0,
        }],
        rooms: vec![Room { id: room_id }],
        subjects: vec![Subject {
            id: subject_id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 2,
        }],
        school_classes: vec![SchoolClass {
            id: class_id,
            home_room_id: Some(room_id),
            max_lessons_per_day: None,
            class_teacher_id: None,
        }],
        lessons: vec![Lesson {
            id: lesson_id,
            school_class_ids: vec![class_id],
            subject_id,
            teacher_candidates: vec![teacher_id],
            teacher_pin: Some(teacher_id),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
            pre_buffer_minutes: pre,
            post_buffer_minutes: 0,
        }],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id,
            subject_id,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
        pre_first_slot_grace_minutes: grace,
    }
}
