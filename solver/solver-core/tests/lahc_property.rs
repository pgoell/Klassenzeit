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
use solver_core::validate::{validate_daily_caps, validate_no_double_booking};
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
                class_teacher_id: None,
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
                    teacher_candidates: vec![teachers[i % teachers.len()].id],
                    teacher_pin: Some(teachers[i % teachers.len()].id),
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
            ..SolveConfig::default()
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
            ..SolveConfig::default()
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
    fn lahc_rr_kempe_does_not_double_book_class(p in lahc_small_problem()) {
        // Bug fix for item 45: kempe_build_chain must reject chains whose
        // conflict graph is not bipartite under the BFS 2-coloring.
        // Without the bipartiteness check, a depth-2 chain member could be
        // assigned to the same destination day as the seed and collide on
        // a shared class, producing a double-booking that the post-condition
        // validator (item 39) catches.
        let solution = solve_with_config(&p, &lahc_rr_kempe_cfg(0))
            .expect("solve_with_config should not error on generated problem");
        validate_no_double_booking(&p, &solution.placements)
            .expect("validate_no_double_booking must pass on lahc_rr_kempe output");
    }

    #[test]
    fn lahc_rr_kempe_respects_daily_caps(p in lahc_small_problem()) {
        let cfg = lahc_rr_kempe_cfg(0);
        let solution = solve_with_config(&p, &cfg).expect("lahc_rr_kempe must succeed");
        validate_daily_caps(&p, &solution.placements)
            .expect("validate_daily_caps must pass on lahc_rr_kempe output");
    }

    /// Item 51 acceptance #1: every backend's reported `Solution.soft_score`
    /// must equal `score_solution(problem, placements, weights)` on its
    /// returned placements. This is the property-test form of the
    /// `debug_assert_eq!` at the tail of `solve_with_config_stats`; named
    /// for grep-discoverability.
    #[test]
    fn solve_with_config_stats_solution_soft_score_equals_score_solution(
        problem in lahc_small_problem(),
        seed in any::<u64>(),
    ) {
        let config = SolveConfig {
            seed,
            max_iterations: Some(64),
            deadline: None,
            ..SolveConfig::default()
        };
        let (solution, _stats) = solve_with_config_stats(&problem, &config).expect("solve");
        let canonical = score_solution(&problem, &solution.placements, &config.weights);
        prop_assert_eq!(solution.soft_score, canonical);
    }

    /// Item 52 prep: pin that LAHC leaves `state.canonical_score` (visible
    /// post-solve as `solution.soft_score`) consistent with
    /// `score_solution(problem, placements, weights)` on the returned
    /// placements for every move type. Pinned by the in-loop `debug_assert!`
    /// in `lahc::run`; this test names the contract for grep-discoverability
    /// and provides cross-seed coverage that the assert alone would not. Uses
    /// production-shaped weights (home_room and class_day_balance both
    /// non-zero) so the canonical scorer exercises both axes.
    #[test]
    fn canonical_score_matches_score_solution_at_lahc_exit(
        seed in 0u64..1024,
    ) {
        let weights = ConstraintWeights {
            class_gap: 1,
            teacher_gap: 1,
            prefer_home_room: 5,
            class_day_balance: 5,
            ..ConstraintWeights::default()
        };
        let config = SolveConfig {
            seed,
            weights,
            deadline: Some(Duration::from_millis(50)),
            max_iterations: Some(2_000),
            ..SolveConfig::default()
        };
        // Use a sized fixture rather than a per-case generator so seed
        // drives LAHC behaviour while the problem stays fixed; widening
        // home_room coverage lives in the fixture builder below.
        let problem = canonical_score_test_problem();
        let (solution, _stats) = solve_with_config_stats(&problem, &config).expect("solve");
        let canonical = score_solution(&problem, &solution.placements, &config.weights);
        prop_assert_eq!(solution.soft_score, canonical);
    }

    /// Item 52: LAHC must never return an incumbent whose canonical score
    /// exceeds the post-greedy canonical. Pinned via the running-best
    /// snapshot in `lahc::run`: `best_placements` is initialised to the
    /// post-greedy placements and only swapped on canonical-strict-
    /// improvement events; on loop exit `*placements = best_placements`.
    /// Uses `PRODUCTION_ACTIVE_WEIGHTS` so the production scenario (home_room
    /// and class_day_balance both non-zero) is what is pinned.
    #[test]
    fn lahc_canonical_score_is_non_increasing_versus_greedy_under_production_weights(
        seed in 0u64..1024,
    ) {
        let problem = canonical_score_test_problem();
        // Greedy-only (deadline None short-circuits LAHC).
        let greedy_config = SolveConfig {
            seed,
            weights: solver_core::PRODUCTION_ACTIVE_WEIGHTS.clone(),
            deadline: None,
            ..SolveConfig::default()
        };
        let (greedy_solution, _) =
            solve_with_config_stats(&problem, &greedy_config).expect("greedy solve");
        // Greedy + LAHC.
        let lahc_config = SolveConfig {
            seed,
            weights: solver_core::PRODUCTION_ACTIVE_WEIGHTS.clone(),
            deadline: Some(Duration::from_millis(200)),
            max_iterations: Some(2_000),
            ..SolveConfig::default()
        };
        let (lahc_solution, _) =
            solve_with_config_stats(&problem, &lahc_config).expect("lahc solve");
        prop_assert!(
            lahc_solution.soft_score <= greedy_solution.soft_score,
            "LAHC canonical {} exceeds greedy canonical {} on seed {}",
            lahc_solution.soft_score,
            greedy_solution.soft_score,
            seed,
        );
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

/// Regression for item 45: pin the grundschule seed that triggered the
/// `lahc_rr_kempe` chain double-booking at production budget. Pre-fix
/// (kempe_build_chain without bipartiteness check), `validate_no_double_booking`
/// fires with `Err(Error::Input("double-booking: class ..."))`. Post-fix,
/// the chain BFS aborts cleanly and the solver completes without any
/// post-condition validator hit.
///
/// Deadline tuned to 40s: the bug requires the LAHC outer loop to reach a
/// Kempe iteration that exercises the non-bipartite chain on this seed. At
/// less than ~39s wall-clock the deadline cuts off the loop before the bad
/// Kempe move runs, so the test is silently green pre-fix; 40s is the floor
/// where pre-fix RED is reliable. Post-fix, the LAHC outer loop runs to the
/// same deadline (the score doesn't reach the optimum on grundschule under
/// these settings) and the validator passes.
#[test]
fn lahc_rr_kempe_does_not_double_book_class_at_grundschule() {
    let p = solver_core::test_fixtures::grundschule_fixture();
    let cfg = SolveConfig {
        weights: solver_core::types::PRODUCTION_ACTIVE_WEIGHTS,
        seed: 8,
        deadline: Some(Duration::from_secs(40)),
        lahc_rr_period: Some(25),
        lahc_kempe_period: Some(23),
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&p, &cfg).expect("solve_with_config must not error");
    validate_no_double_booking(&p, &solution.placements)
        .expect("validate_no_double_booking must pass post-fix");
}

/// Item 52 regression guard: pin a deterministic seed where LAHC's
/// search would otherwise drift the current canonical above the post-greedy
/// canonical (Change move accepts a slice-improving but home_room-worsening
/// move on the two-classes-share-room0 fixture; pre-snapshot, LAHC returns
/// the worsened placements). Asserts that the running-best snapshot in
/// `lahc::run` restores the post-greedy placements at loop exit so
/// `lahc_solution.soft_score <= greedy_solution.soft_score` regardless of
/// what the search drifted to. Names the snapshot mechanism for grep so a
/// future PR cannot silently remove it.
#[test]
fn lahc_returns_running_best_canonical_when_search_drifts() {
    let problem = canonical_score_test_problem();
    let lahc_config = SolveConfig {
        seed: 630,
        weights: solver_core::PRODUCTION_ACTIVE_WEIGHTS.clone(),
        deadline: Some(Duration::from_millis(200)),
        max_iterations: Some(2_000),
        ..SolveConfig::default()
    };
    let (lahc_solution, _) = solve_with_config_stats(&problem, &lahc_config).expect("lahc solve");
    let greedy_config = SolveConfig {
        seed: 630,
        weights: solver_core::PRODUCTION_ACTIVE_WEIGHTS.clone(),
        deadline: None,
        ..SolveConfig::default()
    };
    let (greedy_solution, _) =
        solve_with_config_stats(&problem, &greedy_config).expect("greedy solve");
    assert!(
        lahc_solution.soft_score <= greedy_solution.soft_score,
        "lahc canonical {} > greedy canonical {} (snapshot mechanism failed)",
        lahc_solution.soft_score,
        greedy_solution.soft_score,
    );
}

/// Tiny fixture for the canonical-score property tests. Two classes share
/// `rooms[0]` as `home_room_id` so when LAHC's Change-move falls back from
/// `rooms[0]` (occupied by the other class at the destination tb) to
/// `rooms[1]`, the home_room cost rises and canonical can drift above the
/// post-greedy canonical even when the slice strictly improves. Three days
/// with three slots each gives multiple lessons per class with multiple
/// candidate destinations so LAHC has plenty of moves to exercise both
/// home_room and class_day_balance axes.
fn canonical_score_test_problem() -> Problem {
    let subject = SubjectId(lahc_id_from(1));
    let teacher_a = TeacherId(lahc_id_from(1000));
    let teacher_b = TeacherId(lahc_id_from(1001));
    let class_a = SchoolClassId(lahc_id_from(2000));
    let class_b = SchoolClassId(lahc_id_from(2001));
    let room0 = RoomId(lahc_id_from(3000));
    let room1 = RoomId(lahc_id_from(3001));

    let mut time_blocks: Vec<TimeBlock> = Vec::new();
    let mut tb_idx = 0u32;
    for d in 0..3u8 {
        for p in 0..3u8 {
            time_blocks.push(TimeBlock {
                id: TimeBlockId(lahc_id_from(4000 + tb_idx)),
                day_of_week: d,
                position: p,
            });
            tb_idx += 1;
        }
    }

    // Four lessons per class: every slot can hold one lesson per class.
    // Both classes share the same `home_room_id = room0` so room0 / room1
    // collisions force the Change-move's `pick_room` fallback when the
    // destination tb already has a placement of the OTHER class in room0.
    let mut lessons: Vec<Lesson> = Vec::new();
    for i in 0..4u32 {
        lessons.push(Lesson {
            id: LessonId(lahc_id_from(5000 + i)),
            school_class_ids: vec![class_a],
            subject_id: subject,
            teacher_candidates: vec![teacher_a],
            teacher_pin: Some(teacher_a),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
    }
    for i in 0..4u32 {
        lessons.push(Lesson {
            id: LessonId(lahc_id_from(5100 + i)),
            school_class_ids: vec![class_b],
            subject_id: subject,
            teacher_candidates: vec![teacher_b],
            teacher_pin: Some(teacher_b),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
    }

    Problem {
        time_blocks,
        teachers: vec![
            Teacher {
                id: teacher_a,
                max_hours_per_week: 40,
            },
            Teacher {
                id: teacher_b,
                max_hours_per_week: 40,
            },
        ],
        rooms: vec![Room { id: room0 }, Room { id: room1 }],
        subjects: vec![Subject {
            id: subject,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![
            SchoolClass {
                id: class_a,
                home_room_id: Some(room0),
                max_lessons_per_day: None,
                class_teacher_id: None,
            },
            SchoolClass {
                id: class_b,
                home_room_id: Some(room0),
                max_lessons_per_day: None,
                class_teacher_id: None,
            },
        ],
        lessons,
        teacher_qualifications: vec![
            TeacherQualification {
                teacher_id: teacher_a,
                subject_id: subject,
            },
            TeacherQualification {
                teacher_id: teacher_b,
                subject_id: subject,
            },
        ],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
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
            class_teacher_id: None,
        }],
        lessons: vec![
            Lesson {
                id: lesson_pinned,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: lesson_free_a,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: lesson_free_b,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
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
            teacher_id: None,
        }],
    }
}
