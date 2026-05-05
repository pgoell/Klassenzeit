//! Regression tests for per-day caps (Subject.max_hours_per_day and
//! SchoolClass.max_lessons_per_day) added in items 38 + 39.

use std::collections::HashMap;
use uuid::Uuid;

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::types::{
    Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification, TimeBlock,
};
use solver_core::{solve_with_config, SolveConfig};

fn caps_uuid(b: u8) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = b;
    Uuid::from_bytes(bytes)
}

fn caps_tb_id(d: u8, p: u8) -> TimeBlockId {
    TimeBlockId(Uuid::from_u128(((d as u128) << 64) | p as u128))
}

#[test]
fn caps_subject_hours_per_class_per_day_to_two_by_default() {
    // One class, one subject (cap default 2), one teacher, one room, 5 days x 5 positions.
    // Lesson with hours_per_week=4 preferred_block_size=1: 4 placements that must
    // distribute across at least 2 days under the cap.
    let class_id: SchoolClassId = SchoolClassId(caps_uuid(1));
    let teacher_id: TeacherId = TeacherId(caps_uuid(2));
    let subject_id: SubjectId = SubjectId(caps_uuid(3));
    let room_id: RoomId = RoomId(caps_uuid(4));

    let mut tbs = Vec::new();
    for d in 0..5u8 {
        for p in 0..5u8 {
            tbs.push(TimeBlock {
                id: caps_tb_id(d, p),
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
        }],
        lessons: vec![Lesson {
            id: LessonId(caps_uuid(5)),
            school_class_ids: vec![class_id],
            subject_id,
            teacher_id,
            hours_per_week: 4,
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

    let solution = solve_with_config(&problem, &SolveConfig::default()).expect("greedy succeeds");

    let mut count_by_day: HashMap<u8, u32> = HashMap::new();
    let tb_lookup: HashMap<_, _> = problem.time_blocks.iter().map(|t| (t.id, t)).collect();
    for p in &solution.placements {
        let day = tb_lookup[&p.time_block_id].day_of_week;
        *count_by_day.entry(day).or_default() += 1;
    }
    for (day, count) in &count_by_day {
        assert!(
            *count <= 2,
            "day {day} has {count} hours of subject; cap is 2"
        );
    }
    assert_eq!(
        solution.placements.len(),
        4,
        "all 4 hours should be placed across days"
    );
}

#[test]
fn caps_total_lessons_per_class_per_day_when_set() {
    // One class with max_lessons_per_day=4, daily time-blocks of 6 positions,
    // weekly hours = 25 (forced spillover to a 5th day).
    // Force 5 single-hour lessons across 5 different subjects so subject cap
    // does not interfere; the class cap is the binding constraint.
    let class_id: SchoolClassId = SchoolClassId(caps_uuid(1));
    let teacher_id: TeacherId = TeacherId(caps_uuid(2));
    let room_id: RoomId = RoomId(caps_uuid(4));

    let mut tbs = Vec::new();
    for d in 0..5u8 {
        for p in 0..6u8 {
            tbs.push(TimeBlock {
                id: caps_tb_id(d, p),
                day_of_week: d,
                position: p,
            });
        }
    }

    // 5 subjects, each with max_hours_per_day=5 to remove subject-cap interference.
    let subjects: Vec<Subject> = (0..5u8)
        .map(|i| Subject {
            id: SubjectId(caps_uuid(10 + i)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 5,
        })
        .collect();

    // 5 lessons, each hours_per_week=5 preferred_block_size=1.
    let lessons: Vec<Lesson> = (0..5u8)
        .map(|i| Lesson {
            id: LessonId(caps_uuid(20 + i)),
            school_class_ids: vec![class_id],
            subject_id: subjects[i as usize].id,
            teacher_id,
            hours_per_week: 5,
            preferred_block_size: 1,
            lesson_group_id: None,
        })
        .collect();

    let teacher_quals: Vec<TeacherQualification> = subjects
        .iter()
        .map(|s| TeacherQualification {
            teacher_id,
            subject_id: s.id,
        })
        .collect();

    let problem = Problem {
        time_blocks: tbs,
        teachers: vec![Teacher {
            id: teacher_id,
            max_hours_per_week: 30,
        }],
        rooms: vec![Room { id: room_id }],
        subjects,
        school_classes: vec![SchoolClass {
            id: class_id,
            home_room_id: None,
            max_lessons_per_day: Some(4),
        }],
        lessons,
        teacher_qualifications: teacher_quals,
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    };

    let solution = solve_with_config(&problem, &SolveConfig::default()).expect("greedy succeeds");

    let mut count_by_day: HashMap<u8, u32> = HashMap::new();
    let tb_lookup: HashMap<_, _> = problem.time_blocks.iter().map(|t| (t.id, t)).collect();
    for p in &solution.placements {
        let day = tb_lookup[&p.time_block_id].day_of_week;
        *count_by_day.entry(day).or_default() += 1;
    }
    for (day, count) in &count_by_day {
        assert!(*count <= 4, "day {day} has {count} lessons; cap is 4");
    }
}

#[test]
fn caps_kempe_solve_under_production_caps_smoke() {
    // Smoke test: the dreizuegig fixture reshaped to match production-school
    // caps (`Subject.max_hours_per_day = 2` everywhere) and run with the
    // production move config (R&R + Kempe) plus production weights. The
    // post-condition `validate_daily_caps` inside `solve_with_config`
    // panics on any (class, day, subject) cap violation. This exists as a
    // forward regression guard for the move-path cap pruning; the existing
    // fixtures hide such bugs because cap = 8 is rarely binding, but new
    // LAHC moves added to the loop must keep this green.
    let mut problem = solver_core::test_fixtures::dreizuegig_fixture();
    for subject in &mut problem.subjects {
        subject.max_hours_per_day = 2;
    }

    for seed in 0..10u64 {
        let cfg = SolveConfig {
            seed,
            deadline: None,
            max_iterations: Some(5_000),
            weights: solver_core::PRODUCTION_ACTIVE_WEIGHTS,
            lahc_rr_period: Some(25),
            lahc_kempe_period: Some(23),
        };
        solve_with_config(&problem, &cfg).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    }
}
