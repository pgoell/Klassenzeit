//! Property test pinning the contract that
//! `quality_report(...).weighted_score == score_solution(...)` for any
//! `(Problem, placements, weights)` triple. Generator shape mirrors
//! `lahc_property::lahc_small_problem` but draws non-zero subject-axis
//! weights and an optional home_room_id so the per-axis subtotals get
//! exercised; the property still holds even when the data underlying an
//! axis is zero.

use proptest::prelude::*;
use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::score::score_solution;
use solver_core::types::{
    ConstraintWeights, Lesson, Placement, Problem, Room, SchoolClass, Solution, SolveConfig,
    Subject, Teacher, TeacherQualification, TimeBlock, TimeBlockKind,
};
use solver_core::{quality_report, solve_with_config};
use uuid::Uuid;

fn quality_property_id_from(n: u32) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[12..16].copy_from_slice(&n.to_be_bytes());
    Uuid::from_bytes(bytes)
}

prop_compose! {
    fn quality_small_problem()(
        n_classes in 1usize..=3,
        n_teachers in 1usize..=4,
        n_rooms in 2usize..=3,
        n_days in 1u8..=3,
        slots_per_day in 2u8..=5,
        preferred_block_size in 1u8..=2u8,
        prefer_early in 0u32..=2,
        avoid_first in 0u32..=2,
        avoid_last in 0u32..=2,
        prefer_late in 0u32..=2,
        set_home_room in proptest::bool::ANY,
        // Per-class flags ("set class_teacher_id?") + teacher index;
        // 50%-truthy bool by proptest::bool::ANY. Vec is sized to the
        // upper bound of n_classes so any drawn n_classes can index it.
        class_teacher_seeds in proptest::collection::vec(
            (proptest::bool::ANY, 0usize..=10),
            3,
        ),
    ) -> Problem {
        let subject_a = SubjectId(quality_property_id_from(1));
        let subjects = vec![Subject {
            id: subject_a,
            prefer_early_period: prefer_early,
            avoid_first_period: avoid_first,
            avoid_last_period: avoid_last,
            prefer_late_period: prefer_late,
            max_hours_per_day: 8,
        }];

        let teachers: Vec<Teacher> = (0..n_teachers)
            .map(|i| Teacher {
                id: TeacherId(quality_property_id_from(1000 + i as u32)),
                max_hours_per_week: 40,
                reserve_hours_per_week: 0,
            })
            .collect();
        let teacher_qualifications: Vec<TeacherQualification> = teachers
            .iter()
            .map(|t| TeacherQualification {
                teacher_id: t.id,
                subject_id: subject_a,
            })
            .collect();

        let rooms: Vec<Room> = (0..n_rooms)
            .map(|i| Room {
                id: RoomId(quality_property_id_from(3000 + i as u32)),
            })
            .collect();

        let school_classes: Vec<SchoolClass> = (0..n_classes)
            .map(|i| {
                let (set_kt, kt_idx) = class_teacher_seeds[i];
                let class_teacher_id = if set_kt {
                    Some(teachers[kt_idx % teachers.len()].id)
                } else {
                    None
                };
                SchoolClass {
                    id: SchoolClassId(quality_property_id_from(2000 + i as u32)),
                    home_room_id: if set_home_room { Some(rooms[0].id) } else { None },
                    max_lessons_per_day: None,
                    class_teacher_id,
                }
            })
            .collect();

        let mut time_blocks: Vec<TimeBlock> = Vec::new();
        let mut tb_idx = 0u32;
        for d in 0..n_days {
            for p in 0..slots_per_day {
                time_blocks.push(TimeBlock {
                    id: TimeBlockId(quality_property_id_from(4000 + tb_idx)),
                    day_of_week: d,
                    position: p,
                    kind: TimeBlockKind::Lesson,
                });
                tb_idx += 1;
            }
        }

        let lessons: Vec<Lesson> = school_classes
            .iter()
            .enumerate()
            .map(|(i, sc)| {
                let hours = if preferred_block_size == 2 {
                    2u8 + 2 * ((i as u8) % 2)
                } else {
                    2u8 + ((i as u8) % 3)
                };
                Lesson {
                    id: LessonId(quality_property_id_from(5000 + i as u32)),
                    school_class_ids: vec![sc.id],
                    subject_id: subject_a,
                    teacher_candidates: vec![teachers[i % teachers.len()].id],
                    teacher_pin: Some(teachers[i % teachers.len()].id),
                    hours_per_week: hours,
                    preferred_block_size,
                    lesson_group_id: None,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                }
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
            pinned_placements: vec![],
        }
    }
}

prop_compose! {
    fn quality_random_weights()(
        class_gap in 0u32..=10,
        teacher_gap in 0u32..=10,
        prefer_early_period in 0u32..=5,
        avoid_first_period in 0u32..=5,
        avoid_last_period in 0u32..=5,
        prefer_late_period in 0u32..=5,
        prefer_home_room in 0u32..=10,
        class_day_balance in 0u32..=10,
        prefer_class_teacher in 0u32..=20,
        // Item 57 Task 4: per-class worst-case axes are now populated into
        // QualityReport and folded into weighted_score, so the property
        // test exercises them with non-zero weights. The 0..=10 / 0..=5
        // shapes match the sibling axes.
        max_per_class_spread in 0u32..=10,
        max_per_class_interior_gaps in 0u32..=5,
    ) -> ConstraintWeights {
        ConstraintWeights {
            class_gap,
            teacher_gap,
            prefer_early_period,
            avoid_first_period,
            avoid_last_period,
            prefer_late_period,
            prefer_home_room,
            class_day_balance,
            prefer_class_teacher,
            max_per_class_spread,
            max_per_class_interior_gaps,
            supervision_spread: 0,
            soft_pin_miss: 0,
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        .. ProptestConfig::default()
    })]

    #[test]
    fn quality_report_weighted_score_matches_score_solution(
        problem in quality_small_problem(),
        weights in quality_random_weights(),
    ) {
        // Run greedy solve to get realistic placements + violations.
        let cfg = SolveConfig {
            weights: weights.clone(),
            deadline: None,
            ..SolveConfig::default()
        };
        let solution: Solution = solve_with_config(&problem, &cfg).expect("greedy solve");
        let placements: &[Placement] = &solution.placements;

        let report = quality_report(&problem, placements, &solution.violations, &weights);
        let expected = score_solution(
            &problem,
            placements,
            &weights,
            &::std::collections::HashSet::new(),
        );
        prop_assert_eq!(report.weighted_score, expected);
        prop_assert_eq!(
            report.class_gap_hours_by_class.values().copied().sum::<u32>(),
            report.class_gap_hours,
            "class_gap_hours_by_class sum invariant"
        );
        prop_assert_eq!(
            report.teacher_gap_hours_by_teacher.values().copied().sum::<u32>(),
            report.teacher_gap_hours,
            "teacher_gap_hours_by_teacher sum invariant"
        );
        prop_assert_eq!(
            report.home_room_misses_by_class.values().copied().sum::<u32>(),
            report.home_room_misses,
            "home_room_misses_by_class sum invariant"
        );
        prop_assert_eq!(
            report.class_day_balance_cost_by_class.values().copied().sum::<u32>(),
            report.class_day_balance_cost,
            "class_day_balance_cost_by_class sum invariant"
        );
        // Skip-zero invariant: no entry holds value 0.
        for (_, v) in report.class_gap_hours_by_class.iter() { prop_assert_ne!(*v, 0); }
        for (_, v) in report.teacher_gap_hours_by_teacher.iter() { prop_assert_ne!(*v, 0); }
        for (_, v) in report.home_room_misses_by_class.iter() { prop_assert_ne!(*v, 0); }
        for (_, v) in report.class_day_balance_cost_by_class.iter() { prop_assert_ne!(*v, 0); }
    }
}
