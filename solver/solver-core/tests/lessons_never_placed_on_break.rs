//! Regression coverage for the FFD lesson placer and LAHC move generators:
//! every code path that enumerates candidate time blocks for lesson placement
//! must filter on `time_block.kind == TimeBlockKind::Lesson`. Item 3 of the
//! Hofpause supervision objective added break-kind `TimeBlock` rows to the
//! solver input, but break slots are not teaching slots; the solver must not
//! place lessons on them. The supervision pass in `solver-core/src/supervision.rs`
//! already iterates only break-kind blocks, so the symmetry is intentional.
//!
//! Fixture shape: one class, one lesson with `hours_per_week == 1`, two time
//! blocks on the same day. The break sits at the lower-position slot so the
//! default `(day, position, id)` order would place the lesson there first if
//! the kind filter is missing. The lesson must instead land on the lesson-kind
//! block at the higher position.

use std::time::Duration;

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    solve_with_config,
    types::{
        ConstraintWeights, Lesson, Problem, Room, RoomSubjectSuitability, SchoolClass, SolveConfig,
        Subject, Teacher, TeacherQualification, TimeBlock, TimeBlockKind,
    },
};
use uuid::Uuid;

fn uuid_from(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

#[test]
fn ffd_never_places_a_lesson_on_a_break_time_block() {
    let break_tb_id = TimeBlockId(uuid_from(0xB1));
    let lesson_tb_id = TimeBlockId(uuid_from(0xA1));
    let teacher_id = TeacherId(uuid_from(0x10));
    let room_id = RoomId(uuid_from(0x20));
    let subject_id = SubjectId(uuid_from(0x30));
    let class_id = SchoolClassId(uuid_from(0x40));
    let lesson_id = LessonId(uuid_from(0x50));

    let problem = Problem {
        time_blocks: vec![
            // Break sits at the LOWER position so FFD's `(day, position, id)`
            // sort would land here first without the kind filter.
            TimeBlock {
                id: break_tb_id,
                day_of_week: 0,
                position: 0,
                kind: TimeBlockKind::Break,
            },
            TimeBlock {
                id: lesson_tb_id,
                day_of_week: 0,
                position: 1,
                kind: TimeBlockKind::Lesson,
            },
        ],
        teachers: vec![Teacher {
            id: teacher_id,
            max_hours_per_week: 40,
            reserve_hours_per_week: 0,
        }],
        rooms: vec![Room { id: room_id }],
        subjects: vec![Subject {
            id: subject_id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass {
            id: class_id,
            home_room_id: None,
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
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        }],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id,
            subject_id,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: Vec::<RoomSubjectSuitability>::new(),
        pinned_placements: vec![],
        pre_first_slot_grace_minutes: 0,
    };

    let config = SolveConfig {
        deadline: Some(Duration::from_millis(50)),
        seed: 0,
        weights: ConstraintWeights::default(),
        max_iterations: None,
        lahc_rr_period: None,
        lahc_kempe_period: None,
        lahc_rr_k: 5,
        lahc_kempe_max_chain: 8,
        lahc_home_room_period: None,
    };

    let solution = solve_with_config(&problem, &config).expect("solve succeeds");

    assert_eq!(
        solution.placements.len(),
        1,
        "the single lesson must be placed exactly once; got {} placements",
        solution.placements.len(),
    );
    assert_eq!(
        solution.placements[0].time_block_id, lesson_tb_id,
        "lesson must land on the lesson-kind block, not the break slot",
    );
    assert!(
        solution
            .placements
            .iter()
            .all(|p| p.time_block_id != break_tb_id),
        "no placement may target a break-kind TimeBlock; got placements: {:?}",
        solution.placements,
    );
}
