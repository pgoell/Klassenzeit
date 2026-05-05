//! Regression test for the R&R rollback row-keyed fix (active sprint item 37).
//!
//! Pre-fix, `rr_rollback` used `placements.iter().position(|p| p.lesson_id == lesson_id)`
//! to locate a recreated lesson's row, then ruined the WHOLE day at that
//! placement. For a lesson with multiple blocks across different days, the
//! "first" placement was usually one of the lesson's untouched original blocks,
//! and the rollback dropped that block instead of undoing the recreate. The
//! grundschule fixture surfaces this bug visibly (`lahc_rr` drops to 19/45 at
//! 5 s budget). This test pins the rule that R&R can never reduce placement
//! count below FFD greedy on a hand-built multi-block-across-days problem.

use std::time::Duration;

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::solve_with_config;
use solver_core::types::{
    ConstraintWeights, Lesson, Problem, Room, SchoolClass, SolveConfig, Subject, Teacher,
    TeacherQualification, TimeBlock,
};
use uuid::Uuid;

fn rr_rollback_uuid(n: u32) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[12..16].copy_from_slice(&n.to_be_bytes());
    Uuid::from_bytes(bytes)
}

fn rr_rollback_problem() -> Problem {
    // 5 days × 5 positions = 25 time blocks. One class, one teacher, three
    // rooms (so the lesson is room-feasible across days). Lesson L has
    // hours_per_week=5, preferred_block_size=1, so FFD places one row per
    // weekday. Three filler lessons with hours_per_week=1 keep R&R's
    // shuffle non-trivial without crowding the schedule.
    let subject = SubjectId(rr_rollback_uuid(1));
    let teacher = TeacherId(rr_rollback_uuid(1000));
    let class = SchoolClassId(rr_rollback_uuid(2000));
    let room_a = RoomId(rr_rollback_uuid(3000));
    let room_b = RoomId(rr_rollback_uuid(3001));
    let room_c = RoomId(rr_rollback_uuid(3002));
    let mut time_blocks: Vec<TimeBlock> = Vec::with_capacity(25);
    let mut tb_idx: u32 = 0;
    for d in 0..5u8 {
        for p in 0..5u8 {
            time_blocks.push(TimeBlock {
                id: TimeBlockId(rr_rollback_uuid(4000 + tb_idx)),
                day_of_week: d,
                position: p,
            });
            tb_idx += 1;
        }
    }
    let lesson_multi = LessonId(rr_rollback_uuid(5000));
    let filler_a = LessonId(rr_rollback_uuid(5001));
    let filler_b = LessonId(rr_rollback_uuid(5002));
    let filler_c = LessonId(rr_rollback_uuid(5003));
    Problem {
        time_blocks,
        teachers: vec![Teacher {
            id: teacher,
            max_hours_per_week: 28,
        }],
        rooms: vec![
            Room { id: room_a },
            Room { id: room_b },
            Room { id: room_c },
        ],
        subjects: vec![Subject {
            id: subject,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass {
            id: class,
            home_room_id: None,
            max_lessons_per_day: None,
        }],
        lessons: vec![
            Lesson {
                id: lesson_multi,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 5,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: filler_a,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: filler_b,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: filler_c,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
        ],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: teacher,
            subject_id: subject,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

fn rr_rollback_weights() -> ConstraintWeights {
    ConstraintWeights {
        class_gap: 1,
        teacher_gap: 1,
        ..ConstraintWeights::default()
    }
}

#[test]
fn lahc_rr_preserves_placements_on_multi_block_across_days() {
    let problem = rr_rollback_problem();
    let greedy = solve_with_config(
        &problem,
        &SolveConfig {
            weights: rr_rollback_weights(),
            ..SolveConfig::default()
        },
    )
    .expect("greedy solve");
    for seed in 1u64..=8 {
        let lahc_rr = solve_with_config(
            &problem,
            &SolveConfig {
                weights: rr_rollback_weights(),
                seed,
                deadline: Some(Duration::from_millis(50)),
                lahc_rr_period: Some(5),
                ..SolveConfig::default()
            },
        )
        .expect("lahc_rr solve");
        assert_eq!(
            lahc_rr.placements.len(),
            greedy.placements.len(),
            "seed {seed}: lahc_rr dropped placements ({} < greedy {})",
            lahc_rr.placements.len(),
            greedy.placements.len(),
        );
    }
}
