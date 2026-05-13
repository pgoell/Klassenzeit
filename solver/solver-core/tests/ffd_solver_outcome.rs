//! Integration test: FFD ordering changes the placement outcome on a fixture
//! that input-Vec order cannot solve. Lives in `tests/` (not inline) because
//! the assertion is at the public `solve` boundary, not at `ffd_order`.

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    solve, solve_with_config,
    types::{
        Lesson, Problem, Room, RoomSubjectSuitability, SchoolClass, SolveConfig, Subject, Teacher,
        TeacherQualification, TimeBlock,
    },
    PRODUCTION_ACTIVE_WEIGHTS,
};
use uuid::Uuid;

fn ffd_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

/// 1 time block, 2 rooms (R_general suits all, R_special suits only SP), 2
/// classes, 2 lessons. Input Vec lists the more permissive SP lesson first;
/// greedy first-fit takes R_general for SP, leaving DE with no suitable
/// room. FFD orders DE (1 suitable room) before SP (2 suitable rooms), so DE
/// takes R_general and SP falls back to R_special.
fn pessimal_input_problem() -> Problem {
    Problem {
        time_blocks: vec![TimeBlock {
            id: TimeBlockId(ffd_uuid(10)),
            day_of_week: 0,
            position: 0,
        }],
        teachers: vec![
            Teacher {
                id: TeacherId(ffd_uuid(20)),
                max_hours_per_week: 5,
                reserve_hours_per_week: 0,
            },
            Teacher {
                id: TeacherId(ffd_uuid(21)),
                max_hours_per_week: 5,
                reserve_hours_per_week: 0,
            },
        ],
        rooms: vec![
            Room {
                id: RoomId(ffd_uuid(30)),
            },
            Room {
                id: RoomId(ffd_uuid(31)),
            },
        ],
        subjects: vec![
            Subject {
                id: SubjectId(ffd_uuid(40)),
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            },
            Subject {
                id: SubjectId(ffd_uuid(41)),
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            },
        ],
        school_classes: vec![
            SchoolClass {
                id: SchoolClassId(ffd_uuid(50)),
                home_room_id: None,
                max_lessons_per_day: None,
                class_teacher_id: None,
            },
            SchoolClass {
                id: SchoolClassId(ffd_uuid(51)),
                home_room_id: None,
                max_lessons_per_day: None,
                class_teacher_id: None,
            },
        ],
        lessons: vec![
            Lesson {
                id: LessonId(ffd_uuid(61)),
                school_class_ids: vec![SchoolClassId(ffd_uuid(51))],
                subject_id: SubjectId(ffd_uuid(41)),
                teacher_candidates: vec![TeacherId(ffd_uuid(21))],
                teacher_pin: Some(TeacherId(ffd_uuid(21))),
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: LessonId(ffd_uuid(60)),
                school_class_ids: vec![SchoolClassId(ffd_uuid(50))],
                subject_id: SubjectId(ffd_uuid(40)),
                teacher_candidates: vec![TeacherId(ffd_uuid(20))],
                teacher_pin: Some(TeacherId(ffd_uuid(20))),
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
        ],
        teacher_qualifications: vec![
            TeacherQualification {
                teacher_id: TeacherId(ffd_uuid(20)),
                subject_id: SubjectId(ffd_uuid(40)),
            },
            TeacherQualification {
                teacher_id: TeacherId(ffd_uuid(21)),
                subject_id: SubjectId(ffd_uuid(41)),
            },
        ],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![RoomSubjectSuitability {
            room_id: RoomId(ffd_uuid(31)),
            subject_id: SubjectId(ffd_uuid(41)),
        }],
        pinned_placements: vec![],
    }
}

#[test]
fn ffd_solve_places_a_lesson_that_input_order_leaves_unplaced() {
    let problem = pessimal_input_problem();
    let solution = solve(&problem).expect("solve must not return Err");
    assert_eq!(solution.placements.len(), 2, "both lessons should place");
    assert!(
        solution.violations.is_empty(),
        "FFD should produce zero violations on this fixture"
    );
}

#[test]
fn ffd_solve_active_default_weights_match_explicit() {
    // `solve()` uses `PRODUCTION_ACTIVE_WEIGHTS` plus a 200 ms LAHC deadline.
    // The fixture has one time-block, so LAHC has no legal Change move and
    // the placements coincide with the greedy result. This test asserts the
    // structural equivalence: greedy with production-active weights and no
    // deadline produces the same Solution as `solve()` on this LAHC-
    // degenerate fixture.
    let problem = pessimal_input_problem();
    let s_default = solve(&problem).expect("solve");
    let greedy_cfg = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        ..SolveConfig::default()
    };
    let s_explicit = solve_with_config(&problem, &greedy_cfg).expect("solve_with_config");
    assert_eq!(s_default, s_explicit);
}
