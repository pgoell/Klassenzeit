//! Property tests for the LAHC local-search loop. Reuses the same problem
//! generator shape as `score_property.rs` so the bounds stay consistent.

use std::collections::HashMap;
use std::time::Duration;

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::types::{
    ConstraintWeights, Lesson, PinnedPlacement, Problem, Room, SchoolClass, Solution, SolveConfig,
    Subject, Teacher, TeacherQualification, TimeBlock,
};
use solver_core::{score_solution, solve_with_config, solve_with_config_stats};
use uuid::Uuid;

/// Assert that the solution does not violate any subject's per-day hour cap.
fn assert_subject_cap_conformance(
    problem: &Problem,
    solution: &Solution,
) -> Result<(), TestCaseError> {
    let tb_lookup: HashMap<_, _> = problem.time_blocks.iter().map(|t| (t.id, t)).collect();
    let subject_lookup: HashMap<_, _> = problem.subjects.iter().map(|s| (s.id, s)).collect();
    let lesson_lookup: HashMap<_, _> = problem.lessons.iter().map(|l| (l.id, l)).collect();
    let mut subject_hours: HashMap<(SchoolClassId, u8, SubjectId), u8> = HashMap::new();
    for p in &solution.placements {
        let tb = tb_lookup[&p.time_block_id];
        let lesson = lesson_lookup[&p.lesson_id];
        for class in &lesson.school_class_ids {
            let key = (*class, tb.day_of_week, lesson.subject_id);
            *subject_hours.entry(key).or_default() += 1;
        }
    }
    for ((_, _, subject_id), count) in &subject_hours {
        let cap = subject_lookup[subject_id].max_hours_per_day;
        prop_assert!(
            *count <= cap,
            "subject hour cap violated: count={} cap={}",
            count,
            cap,
        );
    }
    Ok(())
}

fn lahc_weights() -> ConstraintWeights {
    ConstraintWeights {
        class_gap: 1,
        teacher_gap: 1,
        ..ConstraintWeights::default()
    }
}

fn lahc_rr_cfg(seed: u64) -> SolveConfig {
    SolveConfig {
        weights: lahc_weights(),
        seed,
        deadline: Some(Duration::from_millis(50)),
        lahc_rr_period: Some(5),
        ..SolveConfig::default()
    }
}

fn lahc_kempe_cfg(seed: u64) -> SolveConfig {
    SolveConfig {
        weights: lahc_weights(),
        seed,
        deadline: Some(Duration::from_millis(50)),
        lahc_kempe_period: Some(5),
        ..SolveConfig::default()
    }
}

fn lahc_rr_kempe_cfg(seed: u64) -> SolveConfig {
    SolveConfig {
        weights: lahc_weights(),
        seed,
        deadline: Some(Duration::from_millis(50)),
        lahc_rr_period: Some(5),
        lahc_kempe_period: Some(5),
        ..SolveConfig::default()
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
        preferred_block_size in 1u8..=2u8,
    ) -> Problem {
        let subject_a = SubjectId(lahc_id_from(1));
        let subjects = vec![Subject { id: subject_a, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0, prefer_late_period: 0, max_hours_per_day: 8 }];

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
                max_lessons_per_day: None,
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
            .map(|(i, sc)| {
                // Vary hours so FFD spreads multi-block lessons across days; sprint item 37
                // rollback bug only fires on multi-block-across-days lessons
                // (preferred_block_size=1 and hours_per_week>=3), the constant 2 hid it.
                // Sprint item 40 widens the generator to draw preferred_block_size from
                // {1, 2} per problem so the Kempe chain code's multi-position window walk
                // gets coverage; hours_per_week stays a multiple of the drawn block size
                // so validate_structural never rejects the generated Problem.
                let hours = if preferred_block_size == 2 {
                    2u8 + 2 * ((i as u8) % 2)
                } else {
                    2u8 + ((i as u8) % 3)
                };
                Lesson {
                    id: LessonId(lahc_id_from(5000 + i as u32)),
                    school_class_ids: vec![sc.id],
                    subject_id: subject_a,
                    teacher_id: teachers[i % teachers.len()].id,
                    hours_per_week: hours,
                    preferred_block_size,
                    lesson_group_id: None,
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
            lahc_rr_period: None,
            lahc_kempe_period: None,
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

    #[test]
    fn lahc_pinned_placements_preserved(seed in any::<u64>()) {
        let problem = build_lahc_pinned_problem();
        let pin = problem.pinned_placements[0].clone();

        let solution = solve_with_config(&problem, &SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            seed,
            deadline: Some(Duration::from_millis(20)),
            ..SolveConfig::default()
        }).unwrap();

        let pinned_in_solution = solution
            .placements
            .iter()
            .find(|p| p.lesson_id == pin.lesson_id)
            .expect("pinned lesson missing from solution");
        prop_assert_eq!(pinned_in_solution.time_block_id, pin.time_block_id);
        prop_assert_eq!(pinned_in_solution.room_id, pin.room_id);
    }

    #[test]
    fn lahc_rr_never_increases_hard_violations(p in lahc_small_problem()) {
        let greedy = solve_with_config(&p, &SolveConfig {
            weights: lahc_weights(),
            ..SolveConfig::default()
        }).unwrap();
        let lahc_rr = solve_with_config(&p, &lahc_rr_cfg(7)).unwrap();
        prop_assert!(lahc_rr.violations.len() <= greedy.violations.len());
    }

    #[test]
    fn lahc_rr_never_decreases_placement_count(p in lahc_small_problem()) {
        let greedy = solve_with_config(&p, &SolveConfig {
            weights: lahc_weights(),
            ..SolveConfig::default()
        }).unwrap();
        let lahc_rr = solve_with_config(&p, &lahc_rr_cfg(7)).unwrap();
        prop_assert!(
            lahc_rr.placements.len() >= greedy.placements.len(),
            "lahc_rr dropped placements: {} < greedy {}",
            lahc_rr.placements.len(),
            greedy.placements.len(),
        );
        assert_subject_cap_conformance(&p, &lahc_rr)?;
    }

    #[test]
    fn lahc_rr_kempe_never_decreases_placement_count(p in lahc_small_problem()) {
        let greedy = solve_with_config(&p, &SolveConfig {
            weights: lahc_weights(),
            ..SolveConfig::default()
        }).unwrap();
        let lahc_rr_kempe = solve_with_config(&p, &lahc_rr_kempe_cfg(7)).unwrap();
        prop_assert!(
            lahc_rr_kempe.placements.len() >= greedy.placements.len(),
            "lahc_rr_kempe dropped placements: {} < greedy {}",
            lahc_rr_kempe.placements.len(),
            greedy.placements.len(),
        );
        assert_subject_cap_conformance(&p, &lahc_rr_kempe)?;
    }

    #[test]
    fn lahc_rr_deterministic_under_seed_and_iter_cap(p in lahc_small_problem()) {
        let cfg = SolveConfig {
            weights: lahc_weights(),
            seed: 42,
            deadline: Some(Duration::from_secs(60)),
            max_iterations: Some(200),
            lahc_rr_period: Some(5),
            lahc_kempe_period: None,
        };
        let a = solve_with_config(&p, &cfg).unwrap();
        let b = solve_with_config(&p, &cfg).unwrap();
        prop_assert_eq!(a, b);
    }

    #[test]
    fn lahc_rr_running_score_matches_recompute_when_feasible(p in lahc_small_problem()) {
        let lahc = solve_with_config(&p, &lahc_rr_cfg(11)).unwrap();
        if lahc.violations.is_empty() {
            let recomputed = score_solution(&p, &lahc.placements, &lahc_weights());
            prop_assert_eq!(lahc.soft_score, recomputed);
        }
    }

    #[test]
    fn lahc_rr_pinned_placements_preserved(seed in any::<u64>()) {
        let problem = build_lahc_pinned_problem();
        let pin = problem.pinned_placements[0].clone();
        let solution = solve_with_config(&problem, &SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            seed,
            deadline: Some(Duration::from_millis(20)),
            lahc_rr_period: Some(5),
            ..SolveConfig::default()
        }).unwrap();

        let pinned_in_solution = solution
            .placements
            .iter()
            .find(|p| p.lesson_id == pin.lesson_id)
            .expect("pinned lesson missing from solution");
        prop_assert_eq!(pinned_in_solution.time_block_id, pin.time_block_id);
        prop_assert_eq!(pinned_in_solution.room_id, pin.room_id);
    }

    #[test]
    fn lahc_kempe_never_increases_hard_violations(p in lahc_small_problem()) {
        let greedy = solve_with_config(&p, &SolveConfig {
            weights: lahc_weights(),
            ..SolveConfig::default()
        }).unwrap();
        let lahc_kempe = solve_with_config(&p, &lahc_kempe_cfg(7)).unwrap();
        prop_assert!(lahc_kempe.violations.len() <= greedy.violations.len());
    }

    #[test]
    fn lahc_kempe_deterministic_under_seed_and_iter_cap(p in lahc_small_problem()) {
        let cfg = SolveConfig {
            weights: lahc_weights(),
            seed: 42,
            deadline: Some(Duration::from_secs(60)),
            max_iterations: Some(200),
            lahc_kempe_period: Some(5),
            ..SolveConfig::default()
        };
        let a = solve_with_config(&p, &cfg).unwrap();
        let b = solve_with_config(&p, &cfg).unwrap();
        prop_assert_eq!(a, b);
    }

    #[test]
    fn lahc_kempe_running_score_matches_recompute_when_feasible(p in lahc_small_problem()) {
        let lahc = solve_with_config(&p, &lahc_kempe_cfg(11)).unwrap();
        if lahc.violations.is_empty() {
            let recomputed = score_solution(&p, &lahc.placements, &lahc_weights());
            prop_assert_eq!(lahc.soft_score, recomputed);
        }
    }

    #[test]
    fn lahc_kempe_pinned_placements_preserved(seed in any::<u64>()) {
        let problem = build_lahc_pinned_problem();
        let pin = problem.pinned_placements[0].clone();
        let solution = solve_with_config(&problem, &SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            seed,
            deadline: Some(Duration::from_millis(20)),
            lahc_kempe_period: Some(5),
            ..SolveConfig::default()
        }).unwrap();

        let pinned_in_solution = solution
            .placements
            .iter()
            .find(|p| p.lesson_id == pin.lesson_id)
            .expect("pinned lesson missing from solution");
        prop_assert_eq!(pinned_in_solution.time_block_id, pin.time_block_id);
        prop_assert_eq!(pinned_in_solution.room_id, pin.room_id);
    }

    #[test]
    fn lahc_stats_ttf_le_tto_le_total(problem in lahc_small_problem(), seed in 0u64..1024) {
        // The probes' invariants under any (problem, seed):
        //   - whenever both ttf and tto are Some, ttf <= tto;
        //   - tto is bounded by the outer wall-clock plus a 50ms slack to
        //     absorb the gap between this test's Instant::now() and
        //     solve_with_config_stats's own entry instant.
        let cfg = SolveConfig {
            weights: lahc_weights(),
            seed,
            deadline: Some(Duration::from_millis(50)),
            max_iterations: Some(2000),
            ..SolveConfig::default()
        };
        let outer_start = std::time::Instant::now();
        let (_sol, stats) = solve_with_config_stats(&problem, &cfg).expect("solve");
        let total_ms = outer_start.elapsed().as_secs_f64() * 1000.0;
        if let (Some(ttf), Some(tto)) = (stats.time_to_first_feasible_ms, stats.time_to_optimal_ms) {
            prop_assert!(ttf <= tto + 1e-6, "ttf {} > tto {}", ttf, tto);
            prop_assert!(tto <= total_ms + 50.0, "tto {} > total+50ms {}", tto, total_ms + 50.0);
        }
    }
}

/// Build a tiny problem with one pinned lesson at TB0 (under `avoid_first_period`,
/// so an unguarded LAHC has incentive to drift it). Four time-blocks on day 0 so
/// the skip-guard assertion is honest per `solver/CLAUDE.md` (with only two TBs
/// LAHC oscillates and lands back on TB0 by accident at the iteration cap).
fn build_lahc_pinned_problem() -> Problem {
    let subject = SubjectId(lahc_id_from(1));
    let teacher = TeacherId(lahc_id_from(1000));
    let class = SchoolClassId(lahc_id_from(2000));
    let room = RoomId(lahc_id_from(3000));
    let tb_zero = TimeBlockId(lahc_id_from(4000));
    let tb_one = TimeBlockId(lahc_id_from(4001));
    let tb_two = TimeBlockId(lahc_id_from(4002));
    let tb_three = TimeBlockId(lahc_id_from(4003));
    let lesson_pinned = LessonId(lahc_id_from(5000));
    let lesson_free_a = LessonId(lahc_id_from(5001));
    let lesson_free_b = LessonId(lahc_id_from(5002));

    Problem {
        time_blocks: vec![
            TimeBlock {
                id: tb_zero,
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: tb_one,
                day_of_week: 0,
                position: 1,
            },
            TimeBlock {
                id: tb_two,
                day_of_week: 0,
                position: 2,
            },
            TimeBlock {
                id: tb_three,
                day_of_week: 0,
                position: 3,
            },
        ],
        teachers: vec![Teacher {
            id: teacher,
            max_hours_per_week: 40,
        }],
        rooms: vec![Room { id: room }],
        subjects: vec![Subject {
            id: subject,
            prefer_early_period: 0,
            avoid_first_period: 1,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass {
            id: class,
            home_room_id: None,
            max_lessons_per_day: None,
        }],
        lessons: vec![
            Lesson {
                id: lesson_pinned,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: lesson_free_a,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: lesson_free_b,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
        ],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: teacher,
            subject_id: subject,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![PinnedPlacement {
            lesson_id: lesson_pinned,
            time_block_id: tb_zero,
            room_id: room,
        }],
    }
}
