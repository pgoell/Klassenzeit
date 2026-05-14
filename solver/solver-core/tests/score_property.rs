//! Property tests for `score_solution` and the lowest-delta greedy.

use proptest::prelude::*;
use solver_core::{
    score_solution, solve_with_config, ConstraintWeights, Lesson, LessonId, Placement, Problem,
    Room, RoomId, SchoolClass, SchoolClassId, SolveConfig, Subject, SubjectId, Teacher, TeacherId,
    TeacherQualification, TimeBlock, TimeBlockId, TimeBlockKind, PRODUCTION_ACTIVE_WEIGHTS,
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
                kind: TimeBlockKind::Lesson,
            })
        }).collect();

        let teachers: Vec<Teacher> = (0..n_teachers).map(|i| Teacher {
            id: TeacherId(id_from(u32::try_from(i).unwrap_or(0) + 2000)),
            max_hours_per_week: 30,
            reserve_hours_per_week: 0,
        }).collect();

        let rooms: Vec<Room> = (0..n_rooms).map(|i| Room {
            id: RoomId(id_from(u32::try_from(i).unwrap_or(0) + 3000)),
        }).collect();

        let subjects: Vec<Subject> = (0..n_subjects).map(|i| Subject {
            id: SubjectId(id_from(u32::try_from(i).unwrap_or(0) + 4000)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }).collect();

        let school_classes: Vec<SchoolClass> = (0..n_classes).map(|i| SchoolClass {
            id: SchoolClassId(id_from(u32::try_from(i).unwrap_or(0) + 5000)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
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
                teacher_candidates: vec![teachers[i % n_teachers].id],
                teacher_pin: Some(teachers[i % n_teachers].id),
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
        let recomputed = score_solution(&problem, &sol.placements, &w, &::std::collections::HashSet::new());
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
            time_blocks: vec![TimeBlock { id: tb_id, day_of_week: 0, position, kind: TimeBlockKind::Lesson }],
            teachers: vec![Teacher { id: teacher_id, max_hours_per_week: 10, reserve_hours_per_week: 0 }],
            rooms: vec![Room { id: room_id }],
            subjects: vec![Subject {
                id: subject_id,
                prefer_early_period: 1,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![SchoolClass { id: class_id, home_room_id: None, max_lessons_per_day: None, class_teacher_id: None }],
            lessons: vec![Lesson {
                id: lesson_id,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_candidates: vec![teacher_id],
                teacher_pin: Some(teacher_id),
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
        let placements = [Placement { lesson_id, time_block_id: tb_id, room_id, teacher_id }];
        let weights = ConstraintWeights {
            prefer_early_period: weight,
            ..ConstraintWeights::default()
        };
        prop_assert_eq!(
            score_solution(&problem, &placements, &weights, &::std::collections::HashSet::new()),
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
            time_blocks: vec![TimeBlock { id: tb_id, day_of_week: 0, position, kind: TimeBlockKind::Lesson }],
            teachers: vec![Teacher { id: teacher_id, max_hours_per_week: 10, reserve_hours_per_week: 0 }],
            rooms: vec![Room { id: room_id }],
            subjects: vec![Subject {
                id: subject_id,
                prefer_early_period: 0,
                avoid_first_period: 1,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![SchoolClass { id: class_id, home_room_id: None, max_lessons_per_day: None, class_teacher_id: None }],
            lessons: vec![Lesson {
                id: lesson_id,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_candidates: vec![teacher_id],
                teacher_pin: Some(teacher_id),
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
        let placements = [Placement { lesson_id, time_block_id: tb_id, room_id, teacher_id }];
        let weights = ConstraintWeights {
            avoid_first_period: weight,
            ..ConstraintWeights::default()
        };
        let expected = if position == 0 { weight } else { 0 };
        prop_assert_eq!(score_solution(&problem, &placements, &weights, &::std::collections::HashSet::new()), expected);
    }
}

/// Hand-built problem that exercises the `class_day_balance` axis under
/// `PRODUCTION_ACTIVE_WEIGHTS`. FFD-greedy packs the lesson's two hours
/// onto a single day (best slice score: zero class_gap), leaving the
/// second day empty. The slice score is therefore zero; the full
/// `score_solution` adds a non-zero `class_day_balance` cost. Pin: this
/// fixture is the regression for item 41.
fn build_class_day_balance_problem() -> Problem {
    let class_id = SchoolClassId(id_from(5000));
    let teacher_id = TeacherId(id_from(2000));
    let room_id = RoomId(id_from(3000));
    let subject_id = SubjectId(id_from(4000));
    let lesson_id = LessonId(id_from(6000));

    let time_blocks: Vec<TimeBlock> = (0u8..2)
        .flat_map(|d| {
            (0u8..2).map(move |p| TimeBlock {
                id: TimeBlockId(id_from(u32::from(d) * 100 + u32::from(p) + 1000)),
                day_of_week: d,
                position: p,
                kind: TimeBlockKind::Lesson,
            })
        })
        .collect();

    Problem {
        time_blocks,
        teachers: vec![Teacher {
            id: teacher_id,
            max_hours_per_week: 30,
            reserve_hours_per_week: 0,
        }],
        rooms: vec![Room { id: room_id }],
        subjects: vec![Subject {
            id: subject_id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass {
            id: class_id,
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        }],
        lessons: vec![Lesson {
            id: lesson_id,
            school_class_ids: vec![class_id],
            subject_id,
            teacher_candidates: vec![teacher_id],
            teacher_pin: Some(teacher_id),
            hours_per_week: 2,
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
    }
}

/// Item 41 contract: `solve_with_config` must report `solution.soft_score`
/// as the full weighted cost (`score_solution(problem, placements,
/// weights)`), not the LAHC running slice. Under `PRODUCTION_ACTIVE_WEIGHTS`
/// the `class_day_balance` axis is non-zero on a one-day-packed plan; on
/// master before the fix this assertion fails because the slice misses it.
#[test]
fn solve_soft_score_under_production_weights_equals_score_solution() {
    let problem = build_class_day_balance_problem();
    let cfg = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        deadline: None,
        ..SolveConfig::default()
    };
    let sol = solve_with_config(&problem, &cfg).expect("solve must succeed on the tiny fixture");
    let recomputed = score_solution(
        &problem,
        &sol.placements,
        &PRODUCTION_ACTIVE_WEIGHTS,
        &::std::collections::HashSet::new(),
    );
    assert_eq!(
        sol.soft_score, recomputed,
        "Solution.soft_score must equal score_solution(...) under PRODUCTION_ACTIVE_WEIGHTS; \
         got slice={}, full={}",
        sol.soft_score, recomputed,
    );
}

/// Item 54: FFD greedy's `try_place_block` picker must respond to
/// `weights.class_day_balance`. Solve the existing two-day class-day-balance
/// fixture once with the axis disabled and once with it enabled at the
/// production weight (5). Re-evaluate both placement sets under a balance-
/// only scorer (`class_day_balance = 1`, every other weight `0`) and assert
/// the balance-on placement set produces a strictly lower contribution.
#[test]
fn ffd_greedy_class_day_balance_weight_lowers_post_solve_class_day_balance_cost() {
    let problem = build_class_day_balance_problem();
    let cfg_off = SolveConfig {
        weights: ConstraintWeights::default(),
        deadline: None,
        ..SolveConfig::default()
    };
    let cfg_on = SolveConfig {
        weights: ConstraintWeights {
            class_day_balance: 5,
            ..ConstraintWeights::default()
        },
        deadline: None,
        ..SolveConfig::default()
    };
    let sol_off = solve_with_config(&problem, &cfg_off).expect("baseline solve");
    let sol_on = solve_with_config(&problem, &cfg_on).expect("balance-on solve");

    // Re-score both placement sets under a balance-only scorer so the
    // comparison isolates the class_day_balance contribution.
    let scorer = ConstraintWeights {
        class_day_balance: 1,
        ..ConstraintWeights::default()
    };
    let balance_off = score_solution(
        &problem,
        &sol_off.placements,
        &scorer,
        &::std::collections::HashSet::new(),
    );
    let balance_on = score_solution(
        &problem,
        &sol_on.placements,
        &scorer,
        &::std::collections::HashSet::new(),
    );
    assert!(
        balance_on < balance_off,
        "balance-on solve must produce a strictly lower class_day_balance \
         contribution; got off={balance_off} on={balance_on}"
    );
}
