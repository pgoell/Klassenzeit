//! Property tests for the greedy solver's hard-constraint invariants.

mod common;

use std::collections::{HashMap, HashSet};

use proptest::prelude::*;
use solver_core::{
    ids::{RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    solve_with_config,
    types::{ConstraintWeights, Problem, Solution, SolveConfig, ViolationKind},
};

use common::feasible_problem;

/// Greedy-only `SolveConfig` (no LAHC pass). The hard-constraint and
/// byte-determinism properties belong to greedy; LAHC determinism is covered
/// by `lahc_property.rs` under a paired `(seed, max_iterations)` cap.
fn greedy_cfg() -> SolveConfig {
    SolveConfig {
        weights: ConstraintWeights {
            class_gap: 1,
            teacher_gap: 1,
            ..ConstraintWeights::default()
        },
        ..SolveConfig::default()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn all_placements_are_feasible(
        classes in 1u8..=4,
        teachers in 2u8..=6,
        rooms in 2u8..=5,
        blocks in 15u8..=25,
        subjects in 2u8..=4,
        hours in 1u8..=3,
    ) {
        let p = feasible_problem(classes, teachers, rooms, blocks, subjects, hours);
        let s = solve_with_config(&p, &greedy_cfg()).unwrap();
        assert_every_placement_is_feasible_and_no_double_booking(&p, &s);
        assert_teacher_hours_respected(&p, &s);
        assert_total_hours_accounted_for(&p, &s);
    }

    #[test]
    fn output_is_byte_deterministic(
        classes in 1u8..=4,
        teachers in 2u8..=6,
        rooms in 2u8..=5,
        blocks in 15u8..=25,
        subjects in 2u8..=4,
        hours in 1u8..=3,
    ) {
        let p = feasible_problem(classes, teachers, rooms, blocks, subjects, hours);
        let a = serde_json::to_string(&solve_with_config(&p, &greedy_cfg()).unwrap()).unwrap();
        let b = serde_json::to_string(&solve_with_config(&p, &greedy_cfg()).unwrap()).unwrap();
        assert_eq!(a, b, "same input must produce byte-identical output");
    }
}

fn assert_every_placement_is_feasible_and_no_double_booking(p: &Problem, s: &Solution) {
    let qualifications: HashSet<(TeacherId, SubjectId)> = p
        .teacher_qualifications
        .iter()
        .map(|q| (q.teacher_id, q.subject_id))
        .collect();
    let teacher_blocked: HashSet<(TeacherId, TimeBlockId)> = p
        .teacher_blocked_times
        .iter()
        .map(|b| (b.teacher_id, b.time_block_id))
        .collect();
    let room_blocked: HashSet<(RoomId, TimeBlockId)> = p
        .room_blocked_times
        .iter()
        .map(|b| (b.room_id, b.time_block_id))
        .collect();
    let mut room_suit: HashMap<RoomId, HashSet<SubjectId>> = HashMap::new();
    for s in &p.room_subject_suitabilities {
        room_suit.entry(s.room_id).or_default().insert(s.subject_id);
    }

    let lesson_by_id: HashMap<_, _> = p.lessons.iter().map(|l| (l.id, l)).collect();

    let mut teacher_slot: HashSet<(TeacherId, TimeBlockId)> = HashSet::new();
    let mut class_slot: HashSet<(SchoolClassId, TimeBlockId)> = HashSet::new();
    let mut room_slot: HashSet<(RoomId, TimeBlockId)> = HashSet::new();

    for pl in &s.placements {
        let lesson = lesson_by_id.get(&pl.lesson_id).unwrap();
        assert!(qualifications.contains(&(lesson.teacher_id, lesson.subject_id)));
        assert!(!teacher_blocked.contains(&(lesson.teacher_id, pl.time_block_id)));
        assert!(!room_blocked.contains(&(pl.room_id, pl.time_block_id)));
        match room_suit.get(&pl.room_id) {
            None => {}
            Some(set) => assert!(set.contains(&lesson.subject_id)),
        }
        assert!(
            teacher_slot.insert((lesson.teacher_id, pl.time_block_id)),
            "teacher double-book"
        );
        for class_id in &lesson.school_class_ids {
            assert!(
                class_slot.insert((*class_id, pl.time_block_id)),
                "class double-book"
            );
        }
        assert!(
            room_slot.insert((pl.room_id, pl.time_block_id)),
            "room double-book"
        );
    }
}

fn assert_teacher_hours_respected(p: &Problem, s: &Solution) {
    let teacher_of: HashMap<_, _> = p.lessons.iter().map(|l| (l.id, l.teacher_id)).collect();
    let teacher_max: HashMap<_, _> = p
        .teachers
        .iter()
        .map(|t| (t.id, t.max_hours_per_week))
        .collect();
    let mut hours: HashMap<TeacherId, u32> = HashMap::new();
    for pl in &s.placements {
        *hours
            .entry(*teacher_of.get(&pl.lesson_id).unwrap())
            .or_insert(0) += 1;
    }
    for (tid, h) in hours {
        assert!(h <= u32::from(*teacher_max.get(&tid).unwrap()));
    }
}

fn assert_total_hours_accounted_for(p: &Problem, s: &Solution) {
    let total_required: u32 = p.lessons.iter().map(|l| u32::from(l.hours_per_week)).sum();
    let placed: u32 = s.placements.len() as u32;
    let unplaced_hour_violations = s
        .violations
        .iter()
        .filter(|v| {
            matches!(
                v.kind,
                ViolationKind::NoQualifiedTeacher
                    | ViolationKind::TeacherOverCapacity
                    | ViolationKind::NoFreeTimeBlock
                    | ViolationKind::NoSuitableRoom
            )
        })
        .count() as u32;
    assert_eq!(placed + unplaced_hour_violations, total_required);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn block_lessons_place_n_consecutive_same_day_same_room(seed in 0u64..16u64) {
        use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
        use solver_core::types::{
            Lesson, Placement, Problem, Room, SchoolClass, Subject, Teacher,
            TeacherQualification, TimeBlock,
        };
        use uuid::Uuid;

        // Single class / single teacher / single room / 7-position day.
        // Three lessons: length-1 (h=1), Doppelstunde (n=2, h=2),
        // Doppelstunde (n=2, h=4). Total hours: 1 + 2 + 4 = 7. Capacity matches.
        let day_blocks: Vec<TimeBlock> = (0u8..7)
            .map(|pos| TimeBlock {
                id: TimeBlockId(Uuid::from_bytes([100 + pos; 16])),
                day_of_week: 0,
                position: pos,
            })
            .collect();
        let teacher = Teacher {
            id: TeacherId(Uuid::from_bytes([20; 16])),
            max_hours_per_week: 10,
        };
        let room = Room {
            id: RoomId(Uuid::from_bytes([30; 16])),
        };
        let subject = Subject {
            id: SubjectId(Uuid::from_bytes([40; 16])),
            prefer_early_periods: false,
            avoid_first_period: false,
            avoid_last_period: false,
        };
        let class = SchoolClass {
            id: SchoolClassId(Uuid::from_bytes([50; 16])),
            home_room_id: None,
        };
        let qual = TeacherQualification {
            teacher_id: teacher.id,
            subject_id: subject.id,
        };
        let lessons = vec![
            Lesson {
                id: LessonId(Uuid::from_bytes([60; 16])),
                school_class_ids: vec![class.id],
                subject_id: subject.id,
                teacher_id: teacher.id,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: LessonId(Uuid::from_bytes([61; 16])),
                school_class_ids: vec![class.id],
                subject_id: subject.id,
                teacher_id: teacher.id,
                hours_per_week: 2,
                preferred_block_size: 2,
                lesson_group_id: None,
            },
            Lesson {
                id: LessonId(Uuid::from_bytes([62; 16])),
                school_class_ids: vec![class.id],
                subject_id: subject.id,
                teacher_id: teacher.id,
                hours_per_week: 4,
                preferred_block_size: 2,
                lesson_group_id: None,
            },
        ];
        let block_lesson_ids: HashSet<LessonId> = lessons
            .iter()
            .filter(|l| l.preferred_block_size > 1)
            .map(|l| l.id)
            .collect();

        let problem = Problem {
            time_blocks: day_blocks,
            teachers: vec![teacher],
            rooms: vec![room],
            subjects: vec![subject],
            school_classes: vec![class],
            lessons,
            teacher_qualifications: vec![qual],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
        };

        let s = solve_with_config(
            &problem,
            &SolveConfig { seed, ..SolveConfig::default() },
        ).unwrap();

        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> = problem
            .time_blocks
            .iter()
            .map(|tb| (tb.id, tb))
            .collect();
        let mut by_lesson: HashMap<LessonId, Vec<&Placement>> = HashMap::new();
        for p in &s.placements {
            by_lesson.entry(p.lesson_id).or_default().push(p);
        }

        for lesson_id in block_lesson_ids {
            let placements = match by_lesson.get(&lesson_id) {
                Some(v) => v,
                None => continue, // unplaced block: violation case, not contradicted by this property
            };
            let lesson = problem
                .lessons
                .iter()
                .find(|l| l.id == lesson_id)
                .unwrap();
            let n = lesson.preferred_block_size as usize;
            let mut by_window: HashMap<(u8, RoomId), Vec<u8>> = HashMap::new();
            for p in placements {
                let tb = tb_lookup[&p.time_block_id];
                by_window
                    .entry((tb.day_of_week, p.room_id))
                    .or_default()
                    .push(tb.position);
            }
            let total: usize = by_window.values().map(|v| v.len()).sum();
            prop_assert_eq!(total, lesson.hours_per_week as usize);
            for (_, mut positions) in by_window {
                prop_assert_eq!(positions.len() % n, 0);
                positions.sort();
                for chunk in positions.chunks(n) {
                    prop_assert_eq!(chunk.len(), n);
                    for k in 1..n {
                        prop_assert_eq!(chunk[k], chunk[0] + k as u8);
                    }
                }
            }
        }
    }
}
