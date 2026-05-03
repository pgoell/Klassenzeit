//! Property tests for `score_solution` and the lowest-delta greedy.

use proptest::prelude::*;
use solver_core::{
    score_solution, solve_with_config, ConstraintWeights, Lesson, LessonId, Placement, Problem,
    Room, RoomId, SchoolClass, SchoolClassId, SolveConfig, Subject, SubjectId, Teacher, TeacherId,
    TeacherQualification, TimeBlock, TimeBlockId,
};
use uuid::Uuid;

fn id_from(n: u32) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[12..16].copy_from_slice(&n.to_be_bytes());
    Uuid::from_bytes(bytes)
}

prop_compose! {
    fn small_problem()(
        n_classes in 1usize..=3,
        n_teachers in 1usize..=4,
        n_rooms in 1usize..=3,
        n_subjects in 1usize..=3,
        n_days in 1u8..=3,
        periods_per_day in 2u8..=5,
        lesson_specs in prop::collection::vec((0usize..3, 0usize..3, 1u8..=3), 1..=12),
    ) -> Problem {
        let time_blocks: Vec<TimeBlock> = (0..n_days).flat_map(|d| {
            (0..periods_per_day).map(move |p| TimeBlock {
                id: TimeBlockId(id_from(u32::from(d) * 100 + u32::from(p) + 1000)),
                day_of_week: d,
                position: p,
            })
        }).collect();

        let teachers: Vec<Teacher> = (0..n_teachers).map(|i| Teacher {
            id: TeacherId(id_from(u32::try_from(i).unwrap_or(0) + 2000)),
            max_hours_per_week: 30,
        }).collect();

        let rooms: Vec<Room> = (0..n_rooms).map(|i| Room {
            id: RoomId(id_from(u32::try_from(i).unwrap_or(0) + 3000)),
        }).collect();

        let subjects: Vec<Subject> = (0..n_subjects).map(|i| Subject {
            id: SubjectId(id_from(u32::try_from(i).unwrap_or(0) + 4000)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
        }).collect();

        let school_classes: Vec<SchoolClass> = (0..n_classes).map(|i| SchoolClass {
            id: SchoolClassId(id_from(u32::try_from(i).unwrap_or(0) + 5000)),
            home_room_id: None,
        }).collect();

        let teacher_qualifications: Vec<TeacherQualification> = teachers.iter()
            .flat_map(|t| subjects.iter().map(move |s| TeacherQualification {
                teacher_id: t.id,
                subject_id: s.id,
            }))
            .collect();

        let lessons: Vec<Lesson> = lesson_specs.iter().enumerate().filter_map(|(i, &(ci, si, h))| {
            if ci >= n_classes || si >= n_subjects {
                return None;
            }
            Some(Lesson {
                id: LessonId(id_from(u32::try_from(i).unwrap_or(0) + 6000)),
                school_class_ids: vec![school_classes[ci].id],
                subject_id: subjects[si].id,
                teacher_id: teachers[i % n_teachers].id,
                hours_per_week: h,
                preferred_block_size: 1,
                lesson_group_id: None,
            })
        }).collect();

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
    fn weights()(class_gap in 0u32..=10, teacher_gap in 0u32..=10) -> ConstraintWeights {
        ConstraintWeights { class_gap, teacher_gap, ..ConstraintWeights::default() }
    }
}

proptest! {
    /// The standalone scorer must equal the in-loop running total.
    #[test]
    fn solve_soft_score_equals_score_solution(problem in small_problem(), w in weights()) {
        let cfg = SolveConfig { weights: w.clone(), ..SolveConfig::default() };
        let Ok(sol) = solve_with_config(&problem, &cfg) else { return Ok(()) };
        let recomputed = score_solution(&problem, &sol.placements, &w);
        prop_assert_eq!(sol.soft_score, recomputed);
    }

    /// Two solver invocations on the same problem and weights produce the
    /// same triple. Catches HashMap-iteration leaks and other hidden
    /// non-determinism.
    #[test]
    fn solve_is_deterministic(problem in small_problem(), w in weights()) {
        let cfg = SolveConfig { weights: w, ..SolveConfig::default() };
        let Ok(s1) = solve_with_config(&problem, &cfg) else { return Ok(()) };
        let Ok(s2) = solve_with_config(&problem, &cfg) else { return Ok(()) };
        prop_assert_eq!(s1.placements, s2.placements);
        prop_assert_eq!(s1.violations, s2.violations);
        prop_assert_eq!(s1.soft_score, s2.soft_score);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// score_solution scales linearly in tb.position for a single
    /// prefer_early_period placement when only that weight is non-zero.
    #[test]
    fn property_score_solution_linear_in_position_for_prefer_early(
        position in 0u8..7,
        weight in 1u32..10,
    ) {
        let subject_id = SubjectId(Uuid::from_u128(0xAA));
        let lesson_id = LessonId(Uuid::from_u128(0xBB));
        let class_id = SchoolClassId(Uuid::from_u128(0xCC));
        let teacher_id = TeacherId(Uuid::from_u128(0xDD));
        let room_id = RoomId(Uuid::from_u128(0xEE));
        let tb_id = TimeBlockId(Uuid::from_u128(0xFF));
        let problem = Problem {
            time_blocks: vec![TimeBlock { id: tb_id, day_of_week: 0, position }],
            teachers: vec![Teacher { id: teacher_id, max_hours_per_week: 10 }],
            rooms: vec![Room { id: room_id }],
            subjects: vec![Subject {
                id: subject_id,
                prefer_early_period: 1,
                avoid_first_period: 0,
                avoid_last_period: 0,
            }],
            school_classes: vec![SchoolClass { id: class_id, home_room_id: None }],
            lessons: vec![Lesson {
                id: lesson_id,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_id,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification { teacher_id, subject_id }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let placements = [Placement { lesson_id, time_block_id: tb_id, room_id }];
        let weights = ConstraintWeights {
            prefer_early_period: weight,
            ..ConstraintWeights::default()
        };
        prop_assert_eq!(
            score_solution(&problem, &placements, &weights),
            u32::from(position) * weight
        );
    }

    /// score_solution returns weight at position 0 and 0 elsewhere for an
    /// avoid_first_period subject when only that weight is non-zero.
    #[test]
    fn property_score_solution_avoid_first_only_at_position_zero(
        position in 0u8..7,
        weight in 1u32..10,
    ) {
        let subject_id = SubjectId(Uuid::from_u128(0xAA));
        let lesson_id = LessonId(Uuid::from_u128(0xBB));
        let class_id = SchoolClassId(Uuid::from_u128(0xCC));
        let teacher_id = TeacherId(Uuid::from_u128(0xDD));
        let room_id = RoomId(Uuid::from_u128(0xEE));
        let tb_id = TimeBlockId(Uuid::from_u128(0xFF));
        let problem = Problem {
            time_blocks: vec![TimeBlock { id: tb_id, day_of_week: 0, position }],
            teachers: vec![Teacher { id: teacher_id, max_hours_per_week: 10 }],
            rooms: vec![Room { id: room_id }],
            subjects: vec![Subject {
                id: subject_id,
                prefer_early_period: 0,
                avoid_first_period: 1,
                avoid_last_period: 0,
            }],
            school_classes: vec![SchoolClass { id: class_id, home_room_id: None }],
            lessons: vec![Lesson {
                id: lesson_id,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_id,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification { teacher_id, subject_id }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let placements = [Placement { lesson_id, time_block_id: tb_id, room_id }];
        let weights = ConstraintWeights {
            avoid_first_period: weight,
            ..ConstraintWeights::default()
        };
        let expected = if position == 0 { weight } else { 0 };
        prop_assert_eq!(score_solution(&problem, &placements, &weights), expected);
    }
}
