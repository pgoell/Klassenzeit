//! Regression test for the FFD-packs-two-N=1-blocks-on-one-day pattern.
//!
//! Pre-fix `rr_collect_anchors` emitted one anchor per (lesson, day) without
//! checking that the day held only one block of the lesson. When FFD packed
//! both `hours_per_week=2, preferred_block_size=1` rows of a lesson on the
//! same day (because room or teacher availability forced it), an R&R move
//! ruined both rows but only recreated one, silently dropping the other.
//! The drop was invisible because LAHC's acceptance gate only rejects on
//! `failed_recreates > 0`, and the soft score actually improves when
//! placements vanish.
//!
//! This test pins the minimal repro: one class, one teacher, one room, one
//! subject, one lesson with `hours_per_week=2, preferred_block_size=1`,
//! and a room blocked-time-blocks set that forces FFD to put both hours on
//! day 0. The post-fix invariant is that `lahc_rr` and `lahc_rr_kempe`
//! return as many placements as greedy.

use std::time::Duration;

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::solve_with_config;
use solver_core::types::{
    ConstraintWeights, Lesson, Problem, Room, RoomBlockedTime, SchoolClass, SolveConfig, Subject,
    Teacher, TeacherQualification, TimeBlock,
};
use uuid::Uuid;

fn anchor_filter_id(n: u32) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[12..16].copy_from_slice(&n.to_be_bytes());
    Uuid::from_bytes(bytes)
}

/// Minimal `Problem` where FFD must pack both hours of one lesson onto day 0.
fn build_anchor_filter_fixture() -> Problem {
    let class_a = SchoolClassId(anchor_filter_id(1));
    let teacher_a = TeacherId(anchor_filter_id(2));
    let room_a = RoomId(anchor_filter_id(3));
    let subject_a = SubjectId(anchor_filter_id(4));
    let lesson_a = LessonId(anchor_filter_id(5));

    let mut time_blocks: Vec<TimeBlock> = Vec::with_capacity(10);
    let mut tb_idx = 0u32;
    for d in 0u8..5 {
        for p in 0u8..2 {
            time_blocks.push(TimeBlock {
                id: TimeBlockId(anchor_filter_id(100 + tb_idx)),
                day_of_week: d,
                position: p,
            });
            tb_idx += 1;
        }
    }

    let room_blocked_times: Vec<RoomBlockedTime> = time_blocks
        .iter()
        .filter(|tb| tb.day_of_week != 0)
        .map(|tb| RoomBlockedTime {
            room_id: room_a,
            time_block_id: tb.id,
        })
        .collect();

    Problem {
        time_blocks,
        teachers: vec![Teacher {
            id: teacher_a,
            max_hours_per_week: 40,
        }],
        rooms: vec![Room { id: room_a }],
        subjects: vec![Subject {
            id: subject_a,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass {
            id: class_a,
            home_room_id: None,
            max_lessons_per_day: None,
        }],
        lessons: vec![Lesson {
            id: lesson_a,
            school_class_ids: vec![class_a],
            subject_id: subject_a,
            teacher_id: teacher_a,
            hours_per_week: 2,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: teacher_a,
            subject_id: subject_a,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times,
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

fn anchor_filter_weights() -> ConstraintWeights {
    ConstraintWeights {
        class_gap: 1,
        teacher_gap: 1,
        ..ConstraintWeights::default()
    }
}

#[test]
fn rr_does_not_drop_packed_block() {
    let problem = build_anchor_filter_fixture();

    let greedy = solve_with_config(
        &problem,
        &SolveConfig {
            weights: anchor_filter_weights(),
            ..SolveConfig::default()
        },
    )
    .unwrap();
    assert_eq!(
        greedy.placements.len(),
        2,
        "greedy must place both hours on day 0"
    );

    let lahc_rr = solve_with_config(
        &problem,
        &SolveConfig {
            weights: anchor_filter_weights(),
            seed: 7,
            deadline: Some(Duration::from_millis(50)),
            max_iterations: Some(50),
            lahc_rr_period: Some(1),
            lahc_kempe_period: None,
        },
    )
    .unwrap();
    assert_eq!(
        lahc_rr.placements.len(),
        greedy.placements.len(),
        "lahc_rr must not drop placements; got {} placements vs greedy {}",
        lahc_rr.placements.len(),
        greedy.placements.len(),
    );
}

#[test]
fn kempe_does_not_drop_packed_block() {
    let problem = build_anchor_filter_fixture();

    let greedy = solve_with_config(
        &problem,
        &SolveConfig {
            weights: anchor_filter_weights(),
            ..SolveConfig::default()
        },
    )
    .unwrap();

    let lahc_kempe = solve_with_config(
        &problem,
        &SolveConfig {
            weights: anchor_filter_weights(),
            seed: 7,
            deadline: Some(Duration::from_millis(50)),
            max_iterations: Some(50),
            lahc_rr_period: None,
            lahc_kempe_period: Some(1),
        },
    )
    .unwrap();
    assert_eq!(
        lahc_kempe.placements.len(),
        greedy.placements.len(),
        "lahc_kempe must not drop placements; got {} placements vs greedy {}",
        lahc_kempe.placements.len(),
        greedy.placements.len(),
    );
}
