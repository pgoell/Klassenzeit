//! Wire-format coverage for `Solution.supervision_assignments`.
//!
//! Pins the contract that the canonical solve entry point populates the new
//! `supervision_assignments` field on `Solution` for every Hofpause time block
//! whose adjacency yields at least one eligible supervisor. The downstream
//! backend / UI consumers (`supervision_assignments` table, teacher-week
//! Aufsicht badge) depend on this field being present on the wire.

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    solve_with_config,
    types::{
        Lesson, Problem, Room, RoomSubjectSuitability, SchoolClass, SolveConfig, Subject, Teacher,
        TeacherQualification, TimeBlock, TimeBlockKind,
    },
};
use uuid::Uuid;

fn ssf_tid(n: u128) -> TeacherId {
    TeacherId(Uuid::from_u128(n))
}
fn ssf_tb_id(n: u128) -> TimeBlockId {
    TimeBlockId(Uuid::from_u128(n))
}
fn ssf_rid(n: u128) -> RoomId {
    RoomId(Uuid::from_u128(n))
}
fn ssf_sid(n: u128) -> SubjectId {
    SubjectId(Uuid::from_u128(n))
}
fn ssf_cid(n: u128) -> SchoolClassId {
    SchoolClassId(Uuid::from_u128(n))
}
fn ssf_lid(n: u128) -> LessonId {
    LessonId(Uuid::from_u128(n))
}

fn ssf_empty_subject(n: u128) -> Subject {
    Subject {
        id: ssf_sid(n),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: 0,
        max_hours_per_day: 8,
    }
}

fn ssf_empty_class(n: u128) -> SchoolClass {
    SchoolClass {
        id: ssf_cid(n),
        home_room_id: None,
        max_lessons_per_day: None,
        class_teacher_id: None,
    }
}

fn ssf_teacher(n: u128) -> Teacher {
    Teacher {
        id: ssf_tid(n),
        max_hours_per_week: 40,
        reserve_hours_per_week: 0,
    }
}

#[test]
fn solve_produces_supervision_assignments_for_break_blocks() {
    // Tiny problem: one teaching slot at (day=0, position=0) and one Break at
    // (day=0, position=1). One lesson with one hour, teacher 10 is the only
    // candidate and is qualified. After greedy placement teacher 10 is the
    // unique supervisor for the break slot, so the canonical solve must emit
    // exactly one SupervisionAssignment pointing at the break slot.
    let teaching_tb = TimeBlock {
        id: ssf_tb_id(1),
        day_of_week: 0,
        position: 0,
        kind: TimeBlockKind::Lesson,
    };
    let break_tb = TimeBlock {
        id: ssf_tb_id(2),
        day_of_week: 0,
        position: 1,
        kind: TimeBlockKind::Break,
    };
    let lesson = Lesson {
        id: ssf_lid(900),
        school_class_ids: vec![ssf_cid(20)],
        subject_id: ssf_sid(50),
        teacher_candidates: vec![ssf_tid(10)],
        teacher_pin: Some(ssf_tid(10)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    };
    let problem = Problem {
        time_blocks: vec![teaching_tb, break_tb],
        teachers: vec![ssf_teacher(10), ssf_teacher(11)],
        rooms: vec![Room { id: ssf_rid(100) }],
        subjects: vec![ssf_empty_subject(50)],
        school_classes: vec![ssf_empty_class(20)],
        lessons: vec![lesson],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: ssf_tid(10),
            subject_id: ssf_sid(50),
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: Vec::<RoomSubjectSuitability>::new(),
        pinned_placements: vec![],
    };

    // Greedy-only (no LAHC); the FFD pass is enough to place the single
    // lesson and trigger the post-solve supervision finalisation.
    let config = SolveConfig::default();
    let solution = solve_with_config(&problem, &config).expect("solve succeeds");

    assert_eq!(
        solution.supervision_assignments.len(),
        1,
        "solver must emit one supervision assignment for the single Hofpause",
    );
    assert_eq!(
        solution.supervision_assignments[0].time_block_id,
        ssf_tb_id(2),
        "the assignment must target the Break time block",
    );
    assert_eq!(
        solution.supervision_assignments[0].teacher_id,
        ssf_tid(10),
        "the only adjacent + free teacher is teacher 10",
    );
}
