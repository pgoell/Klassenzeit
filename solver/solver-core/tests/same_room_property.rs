//! Hard-constraint property: for every solved problem, no `(class, day, subject)`
//! triple has placements in two different rooms.
//!
//! Two scenarios are covered:
//! 1. A targeted minimal fixture that forces a room hop without the constraint
//!    (low-id room blocked at position 0 so FFD must use the high-id room
//!    there; at position 1 the low-id room is free again and the FFD greedy
//!    would naturally pick it). The constraint forces the second placement to
//!    stay in the high-id room.
//! 2. A Grundschule-shaped fixture mirroring `tests/grundschule_smoke.rs`,
//!    asserted under non-trivial soft weights so the LAHC pass also runs.

use std::collections::HashMap;

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    solve_with_config,
    test_fixtures::ffd_lock_in_grundschule,
    types::{
        ConstraintWeights, Lesson, Problem, Room, RoomBlockedTime, RoomSubjectSuitability,
        SchoolClass, Solution, SolveConfig, Subject, Teacher, TeacherQualification, TimeBlock,
    },
};
use uuid::Uuid;

fn same_room_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

/// Assert no `(class, day, subject)` triple uses more than one room across the
/// solved placements.
fn assert_no_room_hopping(problem: &Problem, solution: &Solution) {
    let mut groups: HashMap<(SchoolClassId, u8, SubjectId), RoomId> = HashMap::new();
    for placement in &solution.placements {
        let lesson = problem
            .lessons
            .iter()
            .find(|l| l.id == placement.lesson_id)
            .expect("placement lesson exists");
        let tb = problem
            .time_blocks
            .iter()
            .find(|t| t.id == placement.time_block_id)
            .expect("placement time_block exists");
        for class in &lesson.school_class_ids {
            let key = (*class, tb.day_of_week, lesson.subject_id);
            let entry = groups.entry(key).or_insert(placement.room_id);
            assert_eq!(
                *entry, placement.room_id,
                "room hop within day for class {:?} day {} subject {:?}: rooms {:?} and {:?}",
                class, tb.day_of_week, lesson.subject_id, entry, placement.room_id
            );
        }
    }
}

/// Minimal fixture: one class, two hours of one subject on day 0, two rooms
/// where the lowest-id room is blocked at position 0 only. Without the
/// constraint, FFD would place hour 0 in the high-id room (r1) then hour 1 in
/// the low-id room (r0) on the same day, producing a room hop.
fn forced_hop_problem() -> Problem {
    let r0 = RoomId(same_room_uuid(30));
    let r1 = RoomId(same_room_uuid(31));
    let class = SchoolClassId(same_room_uuid(50));
    let subject = SubjectId(same_room_uuid(40));
    let teacher = TeacherId(same_room_uuid(20));
    let lesson = LessonId(same_room_uuid(60));
    let tb0 = TimeBlockId(same_room_uuid(10));
    let tb1 = TimeBlockId(same_room_uuid(11));

    Problem {
        time_blocks: vec![
            TimeBlock {
                id: tb0,
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: tb1,
                day_of_week: 0,
                position: 1,
            },
        ],
        teachers: vec![Teacher {
            id: teacher,
            max_hours_per_week: 10,
        }],
        rooms: vec![Room { id: r0 }, Room { id: r1 }],
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
        lessons: vec![Lesson {
            id: lesson,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 2,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: teacher,
            subject_id: subject,
        }],
        teacher_blocked_times: vec![],
        // Only r0 is blocked at position 0; r0 is the lowest-id room so FFD
        // would pick it everywhere else by default.
        room_blocked_times: vec![RoomBlockedTime {
            room_id: r0,
            time_block_id: tb0,
        }],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

#[test]
fn no_room_hopping_within_day_for_one_subject_minimal_forced_hop() {
    let problem = forced_hop_problem();
    let config = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 1,
            teacher_gap: 1,
            ..ConstraintWeights::default()
        },
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve");
    // Both hours must be placed (the constraint sticks them in the same room
    // rather than dropping a placement; r1 is feasible at both positions).
    assert_eq!(
        solution.placements.len(),
        2,
        "expected both hours placed; got {:?}",
        solution.placements
    );
    assert_no_room_hopping(&problem, &solution);
    // All Deutsch placements must share the high-id room (r1) since r0 is
    // blocked at position 0.
    let r1 = RoomId(same_room_uuid(31));
    for placement in &solution.placements {
        assert_eq!(
            placement.room_id, r1,
            "expected r1 for both placements; got {:?}",
            placement
        );
    }
}

/// Grundschule-shaped Problem mirroring `tests/grundschule_smoke.rs`. Two
/// classes, 5 weekdays × 5 periods, 5 rooms with the gym restricted to Sport.
fn same_room_grundschule() -> Problem {
    let time_blocks: Vec<TimeBlock> = (0..25)
        .map(|i| TimeBlock {
            id: TimeBlockId(same_room_uuid(100 + i)),
            day_of_week: i / 5,
            position: i % 5,
        })
        .collect();

    let teachers: Vec<Teacher> = (0..8)
        .map(|i| Teacher {
            id: TeacherId(same_room_uuid(30 + i)),
            max_hours_per_week: 28,
        })
        .collect();

    let rooms: Vec<Room> = (0..5)
        .map(|i| Room {
            id: RoomId(same_room_uuid(50 + i)),
        })
        .collect();

    let subject_ids: Vec<SubjectId> = (0..8).map(|i| SubjectId(same_room_uuid(60 + i))).collect();
    let subjects: Vec<Subject> = subject_ids
        .iter()
        .map(|id| Subject {
            id: *id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        })
        .collect();

    let classes: Vec<SchoolClass> = (0..2)
        .map(|i| SchoolClass {
            id: SchoolClassId(same_room_uuid(70 + i)),
            home_room_id: Some(RoomId(same_room_uuid(50 + i))),
            max_lessons_per_day: None,
        })
        .collect();

    // Same Stundentafeln as grundschule_smoke.rs.
    let hours_per_class: [[u8; 8]; 2] = [[6, 5, 2, 0, 2, 1, 2, 3], [5, 5, 4, 2, 2, 1, 2, 3]];

    let mut lessons = Vec::new();
    let mut quals = Vec::new();
    let mut lesson_idx = 0u8;
    for (c_idx, class) in classes.iter().enumerate() {
        for (s_idx, subject) in subjects.iter().enumerate() {
            let hours = hours_per_class[c_idx][s_idx];
            if hours == 0 {
                continue;
            }
            let teacher = &teachers[(c_idx * 4 + s_idx) % teachers.len()];
            lessons.push(Lesson {
                id: LessonId(same_room_uuid(200 + lesson_idx)),
                school_class_ids: vec![class.id],
                subject_id: subject.id,
                teacher_candidates: vec![teacher.id],
                teacher_pin: Some(teacher.id),
                hours_per_week: hours,
                preferred_block_size: 1,
                lesson_group_id: None,
            });
            lesson_idx += 1;
            quals.push(TeacherQualification {
                teacher_id: teacher.id,
                subject_id: subject.id,
            });
        }
    }

    let sport_subject = subject_ids[7];
    let gym = rooms[4].id;
    let suits: Vec<RoomSubjectSuitability> = vec![RoomSubjectSuitability {
        room_id: gym,
        subject_id: sport_subject,
    }];

    Problem {
        time_blocks,
        teachers,
        rooms,
        subjects,
        school_classes: classes,
        lessons,
        teacher_qualifications: quals,
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: suits,
        pinned_placements: vec![],
    }
}

#[test]
fn no_room_hopping_within_day_for_one_subject_demo_grundschule() {
    let problem = same_room_grundschule();
    // Non-trivial weights including prefer_home_room so the LAHC pass exercises
    // moves that could re-introduce room hops.
    let config = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        },
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve");
    assert_no_room_hopping(&problem, &solution);
}

/// Asserts the FFD lock-in failure mode the active sprint's diagnostic phase
/// surfaced is now closed: a Grundschule-shaped Problem produces zero hard
/// violations under greedy-only solving with the production active-default
/// weights. The fixture pins teachers per the builder's docstring and removes
/// Klassenraum 4a from the academic pool (`room_blocked_times`) for every slot
/// to simulate the real-world "renovation week" scenario.
///
/// Path A (same-room-aware FFD eligibility) replaced the per-lesson
/// `free_blocks * suitable_rooms` metric with a `(day, room)` slot-pair
/// counter. The new metric correctly identifies 4a-FÖ as more constrained
/// than the old metric did and FFD now places it before sibling lessons can
/// claim the academic-suitable Klassenräume that 4a-FÖ needs.
#[test]
fn ffd_does_not_lock_in_on_demo_grundschule() {
    let problem = ffd_lock_in_grundschule();
    let config = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_early_period: 1,
            avoid_first_period: 1,
            prefer_home_room: 5,
            avoid_last_period: 1,
            prefer_late_period: 1,
            class_day_balance: 5,
        },
        deadline: None, // greedy only; Path A's contribution is at the FFD layer.
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve");
    assert!(
        solution.violations.is_empty(),
        "expected zero violations after Path A; got {:?}",
        solution.violations
    );
    let total_hours: u32 = problem
        .lessons
        .iter()
        .map(|l| u32::from(l.hours_per_week))
        .sum();
    assert_eq!(
        solution.placements.len() as u32,
        total_hours,
        "expected every hour placed; got {} of {}",
        solution.placements.len(),
        total_hours
    );
}
