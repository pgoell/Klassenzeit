//! Asserts that the LAHC outer loop exits as soon as the incumbent reaches
//! `placements.len() == placements_expected && state.search_score_slice == 0`,
//! regardless of the configured deadline.

use std::time::{Duration, Instant};
use uuid::Uuid;

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::types::{
    Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification, TimeBlock,
};
use solver_core::{solve_with_config, SolveConfig};

fn early_exit_uuid(b: u8) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = b;
    Uuid::from_bytes(bytes)
}

fn early_exit_tb_id(d: u8, p: u8) -> TimeBlockId {
    TimeBlockId(Uuid::from_u128(((d as u128) << 64) | p as u128))
}

#[test]
fn lahc_exits_at_objective_floor_well_before_deadline() {
    // Tiny problem the FFD greedy solves to soft_score=0 and full placement
    // count. With deadline = 10s and the early-exit predicate live, the wall
    // clock on the solve should be well under 1 second.
    let class_id: SchoolClassId = SchoolClassId(early_exit_uuid(1));
    let teacher_id: TeacherId = TeacherId(early_exit_uuid(2));
    let subject_id: SubjectId = SubjectId(early_exit_uuid(3));
    let room_id: RoomId = RoomId(early_exit_uuid(4));

    let mut tbs = Vec::new();
    for d in 0..5u8 {
        for p in 0..5u8 {
            tbs.push(TimeBlock {
                id: early_exit_tb_id(d, p),
                day_of_week: d,
                position: p,
            });
        }
    }

    let problem = Problem {
        time_blocks: tbs,
        teachers: vec![Teacher {
            id: teacher_id,
            max_hours_per_week: 30,
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
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        }],
        lessons: vec![Lesson {
            id: LessonId(early_exit_uuid(5)),
            school_class_ids: vec![class_id],
            subject_id,
            teacher_candidates: vec![teacher_id],
            teacher_pin: Some(teacher_id),
            hours_per_week: 2,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id,
            subject_id,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    };

    let cfg = SolveConfig {
        deadline: Some(Duration::from_secs(10)),
        seed: 42,
        max_iterations: None,
        ..Default::default()
    };
    let started = Instant::now();
    let solution = solve_with_config(&problem, &cfg).expect("solve succeeds");
    let elapsed = started.elapsed();

    assert_eq!(solution.placements.len(), 2);
    assert_eq!(solution.soft_score, 0);
    assert!(
        elapsed < Duration::from_secs(1),
        "early exit should fire well before the 10s budget; took {:?}",
        elapsed
    );
}
