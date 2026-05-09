//! Schedule-quality predicates for bake-off cells.
//!
//! Mirrors the predicates `backend/src/klassenzeit_backend/scheduling/quality_checks.py`
//! enforces in the demo Grundschule integration test. The Python and Rust
//! implementations are intentionally separate: the Python version operates on
//! persisted ORM rows with a hand-supplied exempt-subjects set; the Rust
//! version operates on the in-memory [`Solution`] and infers exempt subjects
//! from [`Problem::room_subject_suitabilities`]. Cross-language parity is not
//! a contract; the two are designed to drift around their respective inputs.

use std::collections::{HashMap, HashSet};

use solver_core::{Problem, RoomId, SchoolClassId, Solution, SubjectId};

pub use solver_core::QualityReport;

/// Threshold: a class's daily-load spread (max - min across the school week)
/// must not exceed this for the spread predicate to pass. Mirrors the Python
/// test's `check_class_day_balance(max_spread=2)`.
pub const QUALITY_MAX_SPREAD: u32 = 2;

/// Threshold: a class's non-exempt home-room hit rate must meet or exceed this.
/// Mirrors the Python test's `check_home_room_ratio(min_ratio=0.6, ...)`.
pub const QUALITY_MIN_HOME_ROOM_RATIO: f64 = 0.6;

/// Threshold: total interior gaps summed across (class, day) partitions must
/// not exceed this. Mirrors the Python test's
/// `check_interior_gaps(max_gaps_per_class=2)`.
pub const QUALITY_MAX_INTERIOR_GAPS: u32 = 2;

/// Threshold: median normalised position of placements of late-preferred
/// subjects must meet or exceed this (0.5 = latter half of the day).
/// Borrowed from OPEN_THINGS item 14's xfail bar.
pub const QUALITY_MIN_LATE_PERIOD_RATIO: f64 = 0.5;

/// Per-cell quality summary returned by [`evaluate_quality_predicates`]. All four metrics
/// are pure functions over [`Problem`] + [`Solution`]; `None` on either ratio
/// means "no relevant placements to evaluate" and counts as a pass for the
/// composite predicate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QualityPredicates {
    /// Max over classes of `max_lessons_in_day - min_lessons_in_day` across
    /// `day_of_week ∈ 0..5`. Empty schedule returns 0.
    pub worst_spread: u32,
    /// Min over classes of `non_exempt_home_room_hits / non_exempt_placements`.
    /// `None` when no class has any non-exempt placements (e.g. fixture has
    /// no `home_room_id` set on any class).
    pub worst_home_room_ratio: Option<f64>,
    /// Sum over `(class, day)` partitions of `last_position - first_position + 1 - count`.
    pub total_interior_gaps: u32,
    /// Median across all placements of late-preferred subjects of
    /// `position / max_position_per_day(day_of_week)`. `None` when no
    /// subject has `prefer_late_period > 0` or no such placements exist.
    pub late_period_ratio: Option<f64>,
}

fn worst_class_day_spread(problem: &Problem, solution: &Solution) -> u32 {
    // Day axis runs 0..5 (Mon-Fri); SchoolClass.day_of_week assumed in that
    // range per `solver-core/src/types.rs::TimeBlock::day_of_week`.
    let mut counts: HashMap<SchoolClassId, [u32; 5]> = HashMap::new();
    let tb_day: HashMap<_, _> = problem
        .time_blocks
        .iter()
        .map(|tb| (tb.id, tb.day_of_week))
        .collect();
    let lesson_classes: HashMap<_, _> = problem
        .lessons
        .iter()
        .map(|l| (l.id, &l.school_class_ids))
        .collect();
    for placement in &solution.placements {
        let day = match tb_day.get(&placement.time_block_id).copied() {
            Some(d) if d < 5 => d as usize,
            _ => continue,
        };
        let classes = match lesson_classes.get(&placement.lesson_id).copied() {
            Some(c) => c,
            None => continue,
        };
        for class_id in classes {
            counts.entry(*class_id).or_insert([0; 5])[day] += 1;
        }
    }
    counts
        .values()
        .map(|per_day| {
            per_day.iter().max().copied().unwrap_or(0) - per_day.iter().min().copied().unwrap_or(0)
        })
        .max()
        .unwrap_or(0)
}

fn total_interior_gaps(problem: &Problem, solution: &Solution) -> u32 {
    let tb_meta: HashMap<_, _> = problem
        .time_blocks
        .iter()
        .map(|tb| (tb.id, (tb.day_of_week, tb.position)))
        .collect();
    let lesson_classes: HashMap<_, _> = problem
        .lessons
        .iter()
        .map(|l| (l.id, &l.school_class_ids))
        .collect();
    let mut positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    for placement in &solution.placements {
        let (day, pos) = match tb_meta.get(&placement.time_block_id).copied() {
            Some(p) => p,
            None => continue,
        };
        let classes = match lesson_classes.get(&placement.lesson_id).copied() {
            Some(c) => c,
            None => continue,
        };
        for class_id in classes {
            positions.entry((*class_id, day)).or_default().push(pos);
        }
    }
    let mut total = 0u32;
    for ps in positions.values_mut() {
        ps.sort_unstable();
        ps.dedup();
        if let (Some(&first), Some(&last)) = (ps.first(), ps.last()) {
            let span = (last - first + 1) as u32;
            let gaps = span.saturating_sub(ps.len() as u32);
            total = total.saturating_add(gaps);
        }
    }
    total
}

fn worst_home_room_ratio(
    problem: &Problem,
    solution: &Solution,
    home_rooms: &HashMap<SchoolClassId, RoomId>,
) -> Option<f64> {
    // Exempt set per (class, subject): if the subject has any
    // room_subject_suitabilities row and the class's home_room_id is NOT in
    // that subject's suitable rooms, the subject is exempt for that class.
    let mut suitable_rooms_per_subject: HashMap<SubjectId, HashSet<RoomId>> = HashMap::new();
    for s in &problem.room_subject_suitabilities {
        suitable_rooms_per_subject
            .entry(s.subject_id)
            .or_default()
            .insert(s.room_id);
    }

    let lesson_meta: HashMap<_, _> = problem
        .lessons
        .iter()
        .map(|l| (l.id, (l.subject_id, &l.school_class_ids)))
        .collect();

    // (class_id, subject_id) -> exempt?
    let mut exempt: HashMap<(SchoolClassId, SubjectId), bool> = HashMap::new();
    for class in &problem.school_classes {
        let home = match home_rooms.get(&class.id).copied() {
            Some(r) => r,
            None => continue,
        };
        for subject in &problem.subjects {
            let is_exempt = match suitable_rooms_per_subject.get(&subject.id) {
                Some(rooms) => !rooms.contains(&home),
                None => false,
            };
            exempt.insert((class.id, subject.id), is_exempt);
        }
    }

    // (class_id) -> (hits, total)
    let mut counts: HashMap<SchoolClassId, (u32, u32)> = HashMap::new();
    for placement in &solution.placements {
        let (subject_id, classes) = match lesson_meta.get(&placement.lesson_id).copied() {
            Some(m) => m,
            None => continue,
        };
        for class_id in classes {
            let home = match home_rooms.get(class_id).copied() {
                Some(r) => r,
                None => continue,
            };
            if exempt
                .get(&(*class_id, subject_id))
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            let entry = counts.entry(*class_id).or_insert((0, 0));
            entry.1 += 1;
            if placement.room_id == home {
                entry.0 += 1;
            }
        }
    }

    let ratios: Vec<f64> = counts
        .values()
        .filter(|(_, total)| *total > 0)
        .map(|(hits, total)| f64::from(*hits) / f64::from(*total))
        .collect();
    if ratios.is_empty() {
        return None;
    }
    Some(ratios.iter().copied().fold(f64::INFINITY, f64::min))
}

fn late_period_ratio(problem: &Problem, solution: &Solution) -> Option<f64> {
    let late_subjects: HashSet<SubjectId> = problem
        .subjects
        .iter()
        .filter(|s| s.prefer_late_period > 0)
        .map(|s| s.id)
        .collect();
    if late_subjects.is_empty() {
        return None;
    }

    let mut max_pos_per_day: HashMap<u8, u8> = HashMap::new();
    for tb in &problem.time_blocks {
        let entry = max_pos_per_day.entry(tb.day_of_week).or_insert(0);
        if tb.position > *entry {
            *entry = tb.position;
        }
    }

    let tb_meta: HashMap<_, _> = problem
        .time_blocks
        .iter()
        .map(|tb| (tb.id, (tb.day_of_week, tb.position)))
        .collect();
    let lesson_subject: HashMap<_, _> = problem
        .lessons
        .iter()
        .map(|l| (l.id, l.subject_id))
        .collect();

    let mut ratios: Vec<f64> = Vec::new();
    for placement in &solution.placements {
        let subject_id = match lesson_subject.get(&placement.lesson_id).copied() {
            Some(s) => s,
            None => continue,
        };
        if !late_subjects.contains(&subject_id) {
            continue;
        }
        let (day, pos) = match tb_meta.get(&placement.time_block_id).copied() {
            Some(m) => m,
            None => continue,
        };
        let max_pos = max_pos_per_day.get(&day).copied().unwrap_or(0);
        if max_pos == 0 {
            continue;
        }
        ratios.push(f64::from(pos) / f64::from(max_pos));
    }
    if ratios.is_empty() {
        return None;
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(ratios[ratios.len() / 2])
}

/// Pure function over [`Problem`] + [`Solution`]. See module rustdoc for the
/// per-predicate semantics. Never panics; treats empty placements gracefully.
pub fn evaluate_quality_predicates(problem: &Problem, solution: &Solution) -> QualityPredicates {
    let home_rooms: HashMap<SchoolClassId, RoomId> = problem
        .school_classes
        .iter()
        .filter_map(|c| c.home_room_id.map(|r| (c.id, r)))
        .collect();
    QualityPredicates {
        worst_spread: worst_class_day_spread(problem, solution),
        worst_home_room_ratio: worst_home_room_ratio(problem, solution, &home_rooms),
        total_interior_gaps: total_interior_gaps(problem, solution),
        late_period_ratio: late_period_ratio(problem, solution),
    }
}

/// Returns the count (0..=4) of predicates that pass at the configured
/// thresholds. `None` ratios count as passing (vacuous truth).
pub fn quality_pass_count(report: &QualityPredicates) -> u32 {
    let mut n = 0;
    if report.worst_spread <= QUALITY_MAX_SPREAD {
        n += 1;
    }
    if report
        .worst_home_room_ratio
        .is_none_or(|v| v >= QUALITY_MIN_HOME_ROOM_RATIO)
    {
        n += 1;
    }
    if report.total_interior_gaps <= QUALITY_MAX_INTERIOR_GAPS {
        n += 1;
    }
    if report
        .late_period_ratio
        .is_none_or(|v| v >= QUALITY_MIN_LATE_PERIOD_RATIO)
    {
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use solver_core::ids::{LessonId, TimeBlockId};
    use solver_core::test_fixtures::grundschule_fixture;
    use solver_core::types::{
        Lesson, Placement as CorePlacement, RoomSubjectSuitability, SchoolClass, Subject, TimeBlock,
    };
    use solver_core::types::{Solution as CoreSolution, SolveConfig};
    use solver_core::{solve_with_config, PRODUCTION_ACTIVE_WEIGHTS};
    use uuid::Uuid;

    fn quality_test_uuid(n: u128) -> Uuid {
        Uuid::from_u128(0x1000_0000_0000_0000_0000_0000_0000_0000_u128 | n)
    }

    fn empty_problem() -> solver_core::Problem {
        solver_core::Problem {
            time_blocks: vec![],
            teachers: vec![],
            rooms: vec![],
            subjects: vec![],
            school_classes: vec![],
            lessons: vec![],
            teacher_qualifications: vec![],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        }
    }

    #[test]
    fn quality_pass_count_treats_none_ratios_as_pass() {
        let report = QualityPredicates {
            worst_spread: 0,
            worst_home_room_ratio: None,
            total_interior_gaps: 0,
            late_period_ratio: None,
        };
        assert_eq!(quality_pass_count(&report), 4);
    }

    #[test]
    fn quality_pass_count_counts_each_failing_predicate() {
        let report = QualityPredicates {
            worst_spread: 5,                  // fail
            worst_home_room_ratio: Some(0.3), // fail
            total_interior_gaps: 10,          // fail
            late_period_ratio: Some(0.2),     // fail
        };
        assert_eq!(quality_pass_count(&report), 0);

        let report = QualityPredicates {
            worst_spread: 2,                  // pass
            worst_home_room_ratio: Some(0.7), // pass
            total_interior_gaps: 0,           // pass
            late_period_ratio: Some(0.4),     // fail
        };
        assert_eq!(quality_pass_count(&report), 3);
    }

    #[test]
    fn quality_report_default_passes_every_predicate() {
        let report = QualityPredicates::default();
        assert_eq!(quality_pass_count(&report), 4);
    }

    #[test]
    fn evaluate_quality_grundschule_fixture_passes_three_or_four_predicates() {
        // Greedy-only solve per solver/CLAUDE.md: pin solver-core unit tests
        // to greedy when wall-clock cost matters. The bench's actual output
        // uses LAHC and reports the real number; this unit test checks the
        // predicate plumbing on a real fixture without paying LAHC's budget.
        let problem = grundschule_fixture();
        let cfg = SolveConfig {
            weights: PRODUCTION_ACTIVE_WEIGHTS.clone(),
            deadline: None,
            ..SolveConfig::default()
        };
        let solution = solve_with_config(&problem, &cfg).expect("solve");
        let report = evaluate_quality_predicates(&problem, &solution);
        let n = quality_pass_count(&report);
        assert!(
            n >= 3,
            "expected at least 3 of 4 predicates to pass on grundschule greedy: {report:?}",
        );
    }

    #[test]
    fn worst_class_day_spread_returns_zero_for_empty_schedule() {
        let problem = empty_problem();
        let solution = CoreSolution {
            placements: vec![],
            violations: vec![],
            soft_score: 0,
        };
        assert_eq!(worst_class_day_spread(&problem, &solution), 0);
    }

    #[test]
    fn total_interior_gaps_counts_only_holes_inside_first_last_window() {
        // Class C1 places at (day=0, pos=[0, 2, 3]) -> first=0, last=3, span=4,
        // count=3, gaps=1.
        let class_id = SchoolClassId(quality_test_uuid(1));
        let subject_id = SubjectId(quality_test_uuid(2));
        let lesson_id = LessonId(quality_test_uuid(3));
        let room_id = RoomId(quality_test_uuid(4));
        let tb_ids: Vec<TimeBlockId> = (0..4)
            .map(|i| TimeBlockId(quality_test_uuid(10 + i)))
            .collect();
        let problem = solver_core::Problem {
            time_blocks: tb_ids
                .iter()
                .enumerate()
                .map(|(i, id)| TimeBlock {
                    id: *id,
                    day_of_week: 0,
                    position: i as u8,
                })
                .collect(),
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
                teacher_candidates: vec![solver_core::TeacherId(quality_test_uuid(99))],
                teacher_pin: Some(solver_core::TeacherId(quality_test_uuid(99))),
                hours_per_week: 3,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            ..empty_problem()
        };
        let solution = CoreSolution {
            placements: vec![
                CorePlacement {
                    lesson_id,
                    time_block_id: tb_ids[0],
                    room_id,
                    teacher_id: solver_core::TeacherId(quality_test_uuid(99)),
                },
                CorePlacement {
                    lesson_id,
                    time_block_id: tb_ids[2],
                    room_id,
                    teacher_id: solver_core::TeacherId(quality_test_uuid(99)),
                },
                CorePlacement {
                    lesson_id,
                    time_block_id: tb_ids[3],
                    room_id,
                    teacher_id: solver_core::TeacherId(quality_test_uuid(99)),
                },
            ],
            violations: vec![],
            soft_score: 0,
        };
        assert_eq!(total_interior_gaps(&problem, &solution), 1);
    }

    #[test]
    fn worst_home_room_ratio_excludes_subjects_unsuitable_for_home_room() {
        // Class C1 home_room=R1. Subjects: S1 (no suitability rows; not exempt),
        // S2 (suitable for R1 only; not exempt), S3 (suitable for R2 only;
        // exempt for class with home_room=R1). Placements: S1 at R1 (hit),
        // S2 at R1 (hit), S3 at R2 (exempt, ignored). Expected ratio = 2/2 = 1.0.
        let class_id = SchoolClassId(quality_test_uuid(1));
        let r1 = RoomId(quality_test_uuid(20));
        let r2 = RoomId(quality_test_uuid(21));
        let s1 = SubjectId(quality_test_uuid(30));
        let s2 = SubjectId(quality_test_uuid(31));
        let s3 = SubjectId(quality_test_uuid(32));
        let l1 = LessonId(quality_test_uuid(40));
        let l2 = LessonId(quality_test_uuid(41));
        let l3 = LessonId(quality_test_uuid(42));
        let tb1 = TimeBlockId(quality_test_uuid(50));
        let tb2 = TimeBlockId(quality_test_uuid(51));
        let tb3 = TimeBlockId(quality_test_uuid(52));
        let teacher = solver_core::TeacherId(quality_test_uuid(99));
        let make_subject = |id| Subject {
            id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        let make_lesson = |id, sid| Lesson {
            id,
            school_class_ids: vec![class_id],
            subject_id: sid,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };
        let problem = solver_core::Problem {
            time_blocks: vec![
                TimeBlock {
                    id: tb1,
                    day_of_week: 0,
                    position: 0,
                },
                TimeBlock {
                    id: tb2,
                    day_of_week: 0,
                    position: 1,
                },
                TimeBlock {
                    id: tb3,
                    day_of_week: 0,
                    position: 2,
                },
            ],
            subjects: vec![make_subject(s1), make_subject(s2), make_subject(s3)],
            school_classes: vec![SchoolClass {
                id: class_id,
                home_room_id: Some(r1),
                max_lessons_per_day: None,
                class_teacher_id: None,
            }],
            lessons: vec![
                make_lesson(l1, s1),
                make_lesson(l2, s2),
                make_lesson(l3, s3),
            ],
            room_subject_suitabilities: vec![
                RoomSubjectSuitability {
                    room_id: r1,
                    subject_id: s2,
                },
                RoomSubjectSuitability {
                    room_id: r2,
                    subject_id: s3,
                },
            ],
            ..empty_problem()
        };
        let home_rooms: HashMap<_, _> = std::iter::once((class_id, r1)).collect();
        let solution = CoreSolution {
            placements: vec![
                CorePlacement {
                    lesson_id: l1,
                    time_block_id: tb1,
                    room_id: r1,
                    teacher_id: teacher,
                },
                CorePlacement {
                    lesson_id: l2,
                    time_block_id: tb2,
                    room_id: r1,
                    teacher_id: teacher,
                },
                CorePlacement {
                    lesson_id: l3,
                    time_block_id: tb3,
                    room_id: r2,
                    teacher_id: teacher,
                },
            ],
            violations: vec![],
            soft_score: 0,
        };
        let ratio = worst_home_room_ratio(&problem, &solution, &home_rooms);
        assert_eq!(ratio, Some(1.0));
    }

    #[test]
    fn worst_home_room_ratio_returns_none_when_no_class_has_home_room() {
        let problem = empty_problem();
        let solution = CoreSolution {
            placements: vec![],
            violations: vec![],
            soft_score: 0,
        };
        assert_eq!(
            worst_home_room_ratio(&problem, &solution, &HashMap::new()),
            None
        );
    }

    #[test]
    fn late_period_ratio_returns_none_when_no_subject_prefers_late() {
        let problem = empty_problem();
        let solution = CoreSolution {
            placements: vec![],
            violations: vec![],
            soft_score: 0,
        };
        assert_eq!(late_period_ratio(&problem, &solution), None);
    }

    #[test]
    fn late_period_ratio_normalises_position_against_max_per_day() {
        let class_id = SchoolClassId(quality_test_uuid(1));
        let subject_id = SubjectId(quality_test_uuid(2));
        let lesson_id = LessonId(quality_test_uuid(3));
        let room_id = RoomId(quality_test_uuid(4));
        let tb_ids: Vec<TimeBlockId> = (0..4)
            .map(|i| TimeBlockId(quality_test_uuid(10 + i)))
            .collect();
        let teacher = solver_core::TeacherId(quality_test_uuid(99));
        let problem = solver_core::Problem {
            time_blocks: tb_ids
                .iter()
                .enumerate()
                .map(|(i, id)| TimeBlock {
                    id: *id,
                    day_of_week: 0,
                    position: i as u8,
                })
                .collect(),
            subjects: vec![Subject {
                id: subject_id,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 5,
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
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
                hours_per_week: 3,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            ..empty_problem()
        };
        // Place at positions [0, 2, 3]; max_position_per_day = 3.
        // Normalised ratios: 0/3, 2/3, 3/3 -> sorted [0.0, 0.666, 1.0]; median = 0.666.
        let solution = CoreSolution {
            placements: vec![
                CorePlacement {
                    lesson_id,
                    time_block_id: tb_ids[0],
                    room_id,
                    teacher_id: teacher,
                },
                CorePlacement {
                    lesson_id,
                    time_block_id: tb_ids[2],
                    room_id,
                    teacher_id: teacher,
                },
                CorePlacement {
                    lesson_id,
                    time_block_id: tb_ids[3],
                    room_id,
                    teacher_id: teacher,
                },
            ],
            violations: vec![],
            soft_score: 0,
        };
        let ratio = late_period_ratio(&problem, &solution).expect("late ratio");
        assert!((ratio - 2.0 / 3.0).abs() < 1e-9, "got {ratio}");
    }
}
