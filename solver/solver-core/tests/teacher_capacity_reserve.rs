//! Coverage for `Teacher.reserve_hours_per_week` (Vertretungsreserve).
//! Unit-level: the saturating subtraction on the new
//! `effective_max_hours_per_week()` accessor. Integration-level: FFD's
//! teacher-weekly-capacity gate consumes the effective max, not the raw
//! `max_hours_per_week`, so a reserved teacher places strictly fewer hours.

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    solve,
    types::{
        Lesson, Problem, Room, RoomSubjectSuitability, SchoolClass, Subject, Teacher,
        TeacherQualification, TimeBlock, ViolationKind,
    },
};
use uuid::Uuid;

fn reserve_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

#[test]
fn effective_max_hours_subtracts_reserve() {
    let cases = [
        (28u8, 4u8, 24u8),
        (28u8, 28u8, 0u8),
        (28u8, 30u8, 0u8), // saturating
        (0u8, 5u8, 0u8),
    ];
    for (max, reserve, expected) in cases {
        let t = Teacher {
            id: TeacherId(reserve_uuid(1)),
            max_hours_per_week: max,
            reserve_hours_per_week: reserve,
        };
        assert_eq!(
            t.effective_max_hours_per_week(),
            expected,
            "max={max} reserve={reserve}"
        );
    }
}

/// Build a single-teacher / one-class / two-lesson problem with abundant
/// time blocks and rooms. The lessons together require 8 teacher hours per
/// week; setting `reserve_hours_per_week` halves the effective capacity.
fn capacity_probe_problem(max: u8, reserve: u8) -> Problem {
    // 10 time blocks, two days, five positions each: enough room for both
    // 4 hour lessons to place when the teacher has 8 hours of capacity.
    let mut time_blocks = Vec::new();
    let mut tb_idx: u8 = 0;
    for day in 0u8..2 {
        for position in 0u8..5 {
            time_blocks.push(TimeBlock {
                id: TimeBlockId(reserve_uuid(100 + tb_idx)),
                day_of_week: day,
                position,
            });
            tb_idx += 1;
        }
    }

    Problem {
        time_blocks,
        teachers: vec![Teacher {
            id: TeacherId(reserve_uuid(20)),
            max_hours_per_week: max,
            reserve_hours_per_week: reserve,
        }],
        rooms: vec![
            Room {
                id: RoomId(reserve_uuid(30)),
            },
            Room {
                id: RoomId(reserve_uuid(31)),
            },
        ],
        subjects: vec![Subject {
            id: SubjectId(reserve_uuid(40)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass {
            id: SchoolClassId(reserve_uuid(50)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        }],
        lessons: vec![
            Lesson {
                id: LessonId(reserve_uuid(60)),
                school_class_ids: vec![SchoolClassId(reserve_uuid(50))],
                subject_id: SubjectId(reserve_uuid(40)),
                teacher_candidates: vec![TeacherId(reserve_uuid(20))],
                teacher_pin: Some(TeacherId(reserve_uuid(20))),
                hours_per_week: 4,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: LessonId(reserve_uuid(61)),
                school_class_ids: vec![SchoolClassId(reserve_uuid(50))],
                subject_id: SubjectId(reserve_uuid(40)),
                teacher_candidates: vec![TeacherId(reserve_uuid(20))],
                teacher_pin: Some(TeacherId(reserve_uuid(20))),
                hours_per_week: 4,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
        ],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: TeacherId(reserve_uuid(20)),
            subject_id: SubjectId(reserve_uuid(40)),
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![RoomSubjectSuitability {
            room_id: RoomId(reserve_uuid(30)),
            subject_id: SubjectId(reserve_uuid(40)),
        }],
        pinned_placements: vec![],
    }
}

#[test]
fn reserve_reduces_placement_capacity() {
    let problem_full = capacity_probe_problem(8, 0);
    let problem_reserved = capacity_probe_problem(8, 4);

    let solution_full = solve(&problem_full).expect("solve full");
    let solution_reserved = solve(&problem_reserved).expect("solve reserved");

    // Full capacity should place both 4 hour lessons (8 hours total).
    assert_eq!(
        solution_full.placements.len(),
        8,
        "full capacity should place all 8 lesson-hours"
    );
    // Reserved capacity (effective max 4) caps placements at the lesson size.
    assert!(
        solution_reserved.placements.len() <= 4,
        "expected <=4 placements with reserve=4, got {}",
        solution_reserved.placements.len()
    );
    // FFD picker should record TeacherOverCapacity for the over-capacity hours.
    assert!(
        solution_reserved
            .violations
            .iter()
            .any(|v| matches!(v.kind, ViolationKind::TeacherOverCapacity)),
        "expected TeacherOverCapacity violation, got {:?}",
        solution_reserved.violations
    );
}
