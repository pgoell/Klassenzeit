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
            lahc_rr_k: 5,
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
            lahc_rr_k: 5,
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

#[test]
fn default_lahc_rr_k_is_five() {
    assert_eq!(SolveConfig::default().lahc_rr_k, 5);
}

/// Build a fixture surfacing >= 8 R&R-eligible (lesson, day) anchors. Ten
/// independent lessons, each `hours_per_week=1, preferred_block_size=1`, all
/// for one class, share one room, each has a distinct teacher so non-overlap
/// is trivially satisfied. Two days with five positions each leaves enough
/// slot room for greedy to land all ten placements; every (lesson, day)
/// pair greedy lands becomes an R&R anchor.
fn build_many_anchors_fixture() -> Problem {
    let class_a = SchoolClassId(anchor_filter_id(1001));
    let room_a = RoomId(anchor_filter_id(1002));
    let subject_a = SubjectId(anchor_filter_id(1003));

    let mut time_blocks: Vec<TimeBlock> = Vec::with_capacity(10);
    let mut tb_idx = 0u32;
    for d in 0u8..2 {
        for p in 0u8..5 {
            time_blocks.push(TimeBlock {
                id: TimeBlockId(anchor_filter_id(2000 + tb_idx)),
                day_of_week: d,
                position: p,
            });
            tb_idx += 1;
        }
    }

    let lesson_count: u32 = 10;
    let mut teachers = Vec::with_capacity(lesson_count as usize);
    let mut lessons = Vec::with_capacity(lesson_count as usize);
    let mut quals = Vec::with_capacity(lesson_count as usize);
    for n in 0..lesson_count {
        let teacher_id = TeacherId(anchor_filter_id(3000 + n));
        let lesson_id = LessonId(anchor_filter_id(4000 + n));
        teachers.push(Teacher {
            id: teacher_id,
            max_hours_per_week: 40,
        });
        quals.push(TeacherQualification {
            teacher_id,
            subject_id: subject_a,
        });
        lessons.push(Lesson {
            id: lesson_id,
            school_class_ids: vec![class_a],
            subject_id: subject_a,
            teacher_id,
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
    }

    Problem {
        time_blocks,
        teachers,
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
        lessons,
        teacher_qualifications: quals,
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

fn rr_anchor_filter_count_touched(
    before: &[solver_core::types::Placement],
    after: &[solver_core::types::Placement],
) -> usize {
    use std::collections::HashSet;
    let pre: HashSet<(LessonId, TimeBlockId, RoomId)> = before
        .iter()
        .map(|p| (p.lesson_id, p.time_block_id, p.room_id))
        .collect();
    after
        .iter()
        .filter(|p| !pre.contains(&(p.lesson_id, p.time_block_id, p.room_id)))
        .count()
}

#[test]
fn rr_attempt_clamps_chosen_count_to_lahc_rr_k() {
    // Greedy returns 10 single-hour placements; every (lesson, day) is an
    // R&R-eligible anchor. With max_iterations=1 and lahc_rr_period=Some(1),
    // exactly one R&R move runs. The number of placements that differ
    // between greedy-only and post-LAHC is bounded above by `lahc_rr_k`
    // because R&R ruins at most K (lesson, day) blocks, and with
    // preferred_block_size=1 each block is one placement. R&R may pick
    // fewer than K (rejection rolls back; recreate to same slot counts as
    // zero touched), so the assertion is one-sided.
    let problem = build_many_anchors_fixture();

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
        10,
        "greedy must place all ten lessons"
    );

    for (k, expected_max) in [(3u32, 3usize), (8u32, 8usize)] {
        let lahc = solve_with_config(
            &problem,
            &SolveConfig {
                weights: anchor_filter_weights(),
                seed: 1,
                deadline: Some(Duration::from_millis(50)),
                max_iterations: Some(1),
                lahc_rr_period: Some(1),
                lahc_kempe_period: None,
                lahc_rr_k: k,
            },
        )
        .unwrap();
        let touched = rr_anchor_filter_count_touched(&greedy.placements, &lahc.placements);
        assert!(
            touched <= expected_max,
            "lahc_rr_k={k}: expected at most {expected_max} touched placements, got {touched}",
        );
    }
}
