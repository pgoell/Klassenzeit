//! Score-integration coverage for `ConstraintWeights.supervision_spread`.
//!
//! Pins two contracts:
//!
//! 1. `PRODUCTION_ACTIVE_WEIGHTS.supervision_spread == 5` so cross-backend
//!    bench cells (LAHC, CP-SAT) compare on the same supervision-load axis.
//! 2. `score_solution` adds `weights.supervision_spread * spread` (saturating)
//!    to its canonical total. The asymmetric fixture below earns one teacher
//!    two supervisions and another zero, so spread = 2 and the soft-cost
//!    delta between `supervision_spread = 5` and `supervision_spread = 0`
//!    is exactly 10.

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    score::score_solution,
    types::{
        ConstraintWeights, Lesson, Placement, Problem, Room, RoomSubjectSuitability, SchoolClass,
        Subject, Teacher, TeacherQualification, TimeBlock, TimeBlockKind,
    },
    PRODUCTION_ACTIVE_WEIGHTS,
};
use uuid::Uuid;

fn supscore_tid(n: u128) -> TeacherId {
    TeacherId(Uuid::from_u128(n))
}
fn supscore_tb_id(n: u128) -> TimeBlockId {
    TimeBlockId(Uuid::from_u128(n))
}
fn supscore_rid(n: u128) -> RoomId {
    RoomId(Uuid::from_u128(n))
}
fn supscore_sid(n: u128) -> SubjectId {
    SubjectId(Uuid::from_u128(n))
}
fn supscore_cid(n: u128) -> SchoolClassId {
    SchoolClassId(Uuid::from_u128(n))
}
fn supscore_lid(n: u128) -> LessonId {
    LessonId(Uuid::from_u128(n))
}

fn supscore_empty_subject(n: u128) -> Subject {
    Subject {
        id: supscore_sid(n),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: 0,
        max_hours_per_day: 8,
    }
}

fn supscore_empty_class(n: u128) -> SchoolClass {
    SchoolClass {
        id: supscore_cid(n),
        home_room_id: None,
        max_lessons_per_day: None,
        class_teacher_id: None,
    }
}

fn supscore_teacher(n: u128) -> Teacher {
    Teacher {
        id: supscore_tid(n),
        max_hours_per_week: 40,
        reserve_hours_per_week: 0,
    }
}

fn supscore_lesson_tb(id: u128, day: u8, position: u8, kind: TimeBlockKind) -> TimeBlock {
    TimeBlock {
        id: supscore_tb_id(id),
        day_of_week: day,
        position,
        kind,
    }
}

fn supscore_placement(lesson: u128, time_block: u128, room: u128, teacher_n: u128) -> Placement {
    Placement {
        lesson_id: supscore_lid(lesson),
        time_block_id: supscore_tb_id(time_block),
        room_id: supscore_rid(room),
        teacher_id: supscore_tid(teacher_n),
    }
}

fn supscore_lesson_for(id: u128, class: u128, subject: u128, teacher_n: u128) -> Lesson {
    Lesson {
        id: supscore_lid(id),
        school_class_ids: vec![supscore_cid(class)],
        subject_id: supscore_sid(subject),
        teacher_candidates: vec![supscore_tid(teacher_n)],
        teacher_pin: Some(supscore_tid(teacher_n)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    }
}

/// Asymmetric supervision fixture: day 0 with teaching at positions 2, 4, 5, 7
/// (all by teacher 10) and breaks at positions 3 and 6. Teacher 10 is the only
/// candidate adjacent to either break, so the greedy assigns both supervisions
/// to teacher 10. Teacher 11 has zero placements, no adjacency, no
/// supervisions. Spread over the supervising pool is `max - min = 2 - 0 = 2`
/// (the supervising pool contains only teacher 10 by the implementation's
/// "supervising_counts.filter(c > 0)" rule, so the pool is `{2}` and spread =
/// 0). To force a non-zero spread we need both teachers to supervise; reshape
/// the fixture so teacher 11 supervises one break and teacher 10 supervises
/// two. That makes spread = 1.
///
/// Reshape: positions 0..=7 on day 0. Teacher 10 teaches at 0, 2, 4, 6 (so
/// adjacent to breaks at 1, 3, 5, 7). Teacher 11 teaches at 4 only (adjacent
/// to break 3 and break 5). Breaks: 1, 3, 5. Eligible per break:
///
/// - break 1: teacher 10 (adj 0 + 2). Teacher 11 not adjacent.
/// - break 3: teacher 10 (adj 2 + 4) and teacher 11 (adj 4).
/// - break 5: teacher 10 (adj 4 + 6) and teacher 11 (adj 4).
///
/// Greedy iterates breaks in (day, position) order:
///
/// 1. break 1 -> teacher 10 (only eligible). counts: {10: 1}
/// 2. break 3 -> teacher 11 (count 0 < 1). counts: {10: 1, 11: 1}
/// 3. break 5 -> tie 10 vs 11 at count 1; smaller TeacherId wins -> teacher
///    10. counts: {10: 2, 11: 1}.
///
/// Supervising pool = {2, 1}; spread = max - min = 2 - 1 = 1. Expected
/// score-delta between supervision_spread=5 and =0 is 5 * 1 = 5.
fn asymmetric_problem_and_placements() -> (Problem, Vec<Placement>) {
    let time_blocks = vec![
        supscore_lesson_tb(1, 0, 0, TimeBlockKind::Lesson),
        supscore_lesson_tb(2, 0, 1, TimeBlockKind::Break),
        supscore_lesson_tb(3, 0, 2, TimeBlockKind::Lesson),
        supscore_lesson_tb(4, 0, 3, TimeBlockKind::Break),
        supscore_lesson_tb(5, 0, 4, TimeBlockKind::Lesson),
        supscore_lesson_tb(6, 0, 5, TimeBlockKind::Break),
        supscore_lesson_tb(7, 0, 6, TimeBlockKind::Lesson),
    ];
    // Five lessons: four by teacher 10 (at positions 0, 2, 4, 6) and one by
    // teacher 11 (at position 4, same time block as teacher 10's lesson but
    // a different class so no double-booking).
    let lessons = vec![
        supscore_lesson_for(900, 20, 50, 10),
        supscore_lesson_for(901, 20, 50, 10),
        supscore_lesson_for(902, 20, 50, 10),
        supscore_lesson_for(903, 20, 50, 10),
        supscore_lesson_for(904, 21, 50, 11),
    ];
    let teacher_qualifications = vec![
        TeacherQualification {
            teacher_id: supscore_tid(10),
            subject_id: supscore_sid(50),
        },
        TeacherQualification {
            teacher_id: supscore_tid(11),
            subject_id: supscore_sid(50),
        },
    ];
    let problem = Problem {
        time_blocks,
        teachers: vec![supscore_teacher(10), supscore_teacher(11)],
        rooms: vec![
            Room {
                id: supscore_rid(100),
            },
            Room {
                id: supscore_rid(101),
            },
        ],
        subjects: vec![supscore_empty_subject(50)],
        school_classes: vec![supscore_empty_class(20), supscore_empty_class(21)],
        lessons,
        teacher_qualifications,
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: Vec::<RoomSubjectSuitability>::new(),
        pinned_placements: vec![],
    };
    let placements = vec![
        supscore_placement(900, 1, 100, 10), // teacher 10 at position 0
        supscore_placement(901, 3, 100, 10), // teacher 10 at position 2
        supscore_placement(902, 5, 100, 10), // teacher 10 at position 4
        supscore_placement(903, 7, 100, 10), // teacher 10 at position 6
        supscore_placement(904, 5, 101, 11), // teacher 11 at position 4 (different class + room)
    ];
    (problem, placements)
}

#[test]
fn production_active_weights_has_supervision_spread_five() {
    assert_eq!(PRODUCTION_ACTIVE_WEIGHTS.supervision_spread, 5);
}

#[test]
fn score_solution_includes_supervision_spread_penalty() {
    let (problem, placements) = asymmetric_problem_and_placements();

    // Sanity-check the fixture's spread directly so a failing score delta
    // points at the score wiring, not the fixture.
    let spread = solver_core::supervision::compute_supervision_spread(&problem, &placements);
    assert_eq!(spread, 1, "asymmetric fixture should yield spread = 1");

    // Two weight sets: identical except for supervision_spread (5 vs 0). The
    // canonical-score delta must equal `5 * spread`.
    let weights_on = ConstraintWeights {
        supervision_spread: 5,
        ..ConstraintWeights::default()
    };
    let weights_off = ConstraintWeights {
        supervision_spread: 0,
        ..ConstraintWeights::default()
    };

    let score_on = score_solution(
        &problem,
        &placements,
        &weights_on,
        &::std::collections::HashSet::new(),
    );
    let score_off = score_solution(
        &problem,
        &placements,
        &weights_off,
        &::std::collections::HashSet::new(),
    );

    assert_eq!(
        score_on - score_off,
        5 * spread,
        "score_solution must add weights.supervision_spread * spread to the canonical total"
    );
}
