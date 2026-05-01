//! Property tests for the LAHC local-search loop. Reuses the same problem
//! generator shape as `score_property.rs` so the bounds stay consistent.

use std::time::Duration;

use proptest::prelude::*;
use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::types::{
    ConstraintWeights, Lesson, Problem, Room, SchoolClass, SolveConfig, Subject, Teacher,
    TeacherQualification, TimeBlock,
};
use solver_core::{score_solution, solve_with_config};
use uuid::Uuid;

fn lahc_weights() -> ConstraintWeights {
    ConstraintWeights {
        class_gap: 1,
        teacher_gap: 1,
        ..ConstraintWeights::default()
    }
}

fn lahc_id_from(n: u32) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[12..16].copy_from_slice(&n.to_be_bytes());
    Uuid::from_bytes(bytes)
}

prop_compose! {
    fn lahc_small_problem()(
        n_classes in 1usize..=3,
        n_teachers in 1usize..=4,
        n_rooms in 1usize..=3,
        n_days in 1u8..=3,
        slots_per_day in 2u8..=5,
    ) -> Problem {
        let subject_a = SubjectId(lahc_id_from(1));
        let subjects = vec![Subject { id: subject_a, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0 }];

        let teachers: Vec<Teacher> = (0..n_teachers)
            .map(|i| Teacher {
                id: TeacherId(lahc_id_from(1000 + i as u32)),
                max_hours_per_week: 40,
            })
            .collect();
        let teacher_qualifications: Vec<TeacherQualification> = teachers
            .iter()
            .map(|t| TeacherQualification {
                teacher_id: t.id,
                subject_id: subject_a,
            })
            .collect();

        let school_classes: Vec<SchoolClass> = (0..n_classes)
            .map(|i| SchoolClass {
                id: SchoolClassId(lahc_id_from(2000 + i as u32)),
                home_room_id: None,
            })
            .collect();

        let rooms: Vec<Room> = (0..n_rooms)
            .map(|i| Room {
                id: RoomId(lahc_id_from(3000 + i as u32)),
            })
            .collect();

        let mut time_blocks: Vec<TimeBlock> = Vec::new();
        let mut tb_idx = 0u32;
        for d in 0..n_days {
            for p in 0..slots_per_day {
                time_blocks.push(TimeBlock {
                    id: TimeBlockId(lahc_id_from(4000 + tb_idx)),
                    day_of_week: d,
                    position: p,
                });
                tb_idx += 1;
            }
        }

        let lessons: Vec<Lesson> = school_classes
            .iter()
            .enumerate()
            .map(|(i, sc)| Lesson {
                id: LessonId(lahc_id_from(5000 + i as u32)),
                school_class_ids: vec![sc.id],
                subject_id: subject_a,
                teacher_id: teachers[i % teachers.len()].id,
                hours_per_week: 2,
                preferred_block_size: 1,
                lesson_group_id: None,
            })
            .collect();

        Problem {
            time_blocks,
            teachers,
            rooms,
            subjects,
            school_classes,
            lessons,
            teacher_qualifications,
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        .. ProptestConfig::default()
    })]

    #[test]
    fn lahc_never_increases_score(p in lahc_small_problem()) {
        let greedy = solve_with_config(&p, &SolveConfig {
            weights: lahc_weights(),
            ..SolveConfig::default()
        }).unwrap();
        let lahc = solve_with_config(&p, &SolveConfig {
            weights: lahc_weights(),
            deadline: Some(Duration::from_millis(20)),
            seed: 42,
            ..SolveConfig::default()
        }).unwrap();
        prop_assert!(lahc.soft_score <= greedy.soft_score);
    }

    #[test]
    fn lahc_deterministic_under_seed_and_iter_cap(p in lahc_small_problem()) {
        let cfg = SolveConfig {
            weights: lahc_weights(),
            seed: 42,
            deadline: Some(Duration::from_secs(60)),
            max_iterations: Some(200),
        };
        let a = solve_with_config(&p, &cfg).unwrap();
        let b = solve_with_config(&p, &cfg).unwrap();
        prop_assert_eq!(a, b);
    }

    #[test]
    fn lahc_does_not_add_violations(p in lahc_small_problem()) {
        let greedy = solve_with_config(&p, &SolveConfig {
            weights: lahc_weights(),
            ..SolveConfig::default()
        }).unwrap();
        let lahc = solve_with_config(&p, &SolveConfig {
            weights: lahc_weights(),
            deadline: Some(Duration::from_millis(20)),
            seed: 7,
            ..SolveConfig::default()
        }).unwrap();
        prop_assert_eq!(greedy.violations.len(), lahc.violations.len());
    }

    #[test]
    fn lahc_running_score_matches_recompute(p in lahc_small_problem()) {
        let lahc = solve_with_config(&p, &SolveConfig {
            weights: lahc_weights(),
            deadline: Some(Duration::from_millis(20)),
            seed: 11,
            ..SolveConfig::default()
        }).unwrap();
        let recomputed = score_solution(&p, &lahc.placements, &lahc_weights());
        prop_assert_eq!(lahc.soft_score, recomputed);
    }
}
