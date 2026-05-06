//! Backend-neutral component vector exposing each cost-axis subtotal that
//! `score::score_solution` aggregates into [`crate::types::Solution::soft_score`]. The
//! contract is: every backend's output gets evaluated through
//! [`quality_report`]; the bake-off bench renders the load-bearing
//! components per backend; ADRs and item-51 work compare component
//! vectors instead of one collapsed scalar.
//!
//! See `docs/superpowers/specs/2026-05-06-quality-report-design.md` for
//! the design rationale (item 50).
//!
//! Adding a new axis: add a field to [`QualityReport`], extend
//! `quality_report` to populate it, add a unit test that walks the same
//! partition the underlying `score::*` helper walks, and add an entry to
//! the `weighted_score` accumulation. The property test
//! `quality_report_weighted_score_matches_score_solution` keeps the
//! cross-axis sum honest. Future axes flagged by item 50: teacher bad
//! windows / day-quality (no schema yet), pin disruption cost (waits
//! for soft pins).

use std::collections::HashMap;

use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use crate::score::{
    class_day_balance_cost, gap_count, home_room_penalty, subject_preference_score,
};
use crate::types::{ConstraintWeights, Lesson, Placement, Problem, Subject, TimeBlock, Violation};

/// Backend-neutral cost-vector breakdown of one solver result. Every
/// field is a raw count or unweighted unit; `weighted_score` is the
/// scalar sum that matches `score::score_solution(problem, placements, weights)`.
///
/// Construct via [`quality_report`]. `Default::default()` returns a
/// zero-everywhere report (useful for tests and synthesised fixtures).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QualityReport {
    /// Number of `Violation` entries on the solution. Today every
    /// violation is one missed hour; `PinnedConflict` (one per malformed
    /// pin, dropped from the active set) and the debug-only daily-cap
    /// kinds break that coincidence in principle, so this stays distinct
    /// from `unplaced_hours`.
    pub hard_violations: u32,
    /// `expected_hours - placements.len()`, where
    /// `expected_hours = sum_lessons(lesson.hours_per_week)`. Always
    /// non-negative on well-formed solver outputs.
    pub unplaced_hours: u32,
    /// Sum across `(class, day_of_week)` partitions of `gap_count`
    /// (gap-hours per class day). Unweighted; the
    /// `weights.class_gap`-weighted contribution is folded into
    /// `weighted_score`.
    pub class_gap_hours: u32,
    /// Sum across `(teacher, day_of_week)` partitions of `gap_count`.
    /// Unweighted.
    pub teacher_gap_hours: u32,
    /// L1 daily-load imbalance metric, identical to the value
    /// `score::class_day_balance_cost` computes on the same partition.
    /// Unweighted.
    pub class_day_balance_cost: u32,
    /// Number of `(placement, member-class)` pairs whose class has a
    /// non-null `home_room_id` that does not match the placement's
    /// `room_id`. Multi-class lessons accumulate per non-matching
    /// member.
    pub home_room_misses: u32,
    /// Sum over placements of
    /// `subject.prefer_early_period * tb.position`. Unweighted; the
    /// `weights.prefer_early_period`-weighted contribution is folded
    /// into `weighted_score`.
    pub prefer_early_units: u32,
    /// Sum over placements of `subject.avoid_first_period` at
    /// `tb.position == 0`. Unweighted.
    pub avoid_first_units: u32,
    /// Sum over placements of `subject.avoid_last_period` at
    /// `tb.position == max_position_for_day`. Unweighted.
    pub avoid_last_units: u32,
    /// Sum over placements of
    /// `subject.prefer_late_period * (max_position_for_day - tb.position)`.
    /// Unweighted.
    pub prefer_late_units: u32,
    /// `score::score_solution(problem, placements, weights)`. The
    /// `quality_report_weighted_score_matches_score_solution` property
    /// test pins this equality.
    pub weighted_score: u32,
}

/// Build a [`QualityReport`] for a solver result. Pure: depends only on
/// the inputs; allocates per-call lookup `HashMap`s analogous to those
/// in `score::score_solution`. Cold-path (post-solve / bench
/// aggregation); never call from inside the LAHC inner loop.
pub fn quality_report(
    problem: &Problem,
    placements: &[Placement],
    violations: &[Violation],
    weights: &ConstraintWeights,
) -> QualityReport {
    let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
    let lesson_lookup: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let subject_lookup: HashMap<SubjectId, &Subject> =
        problem.subjects.iter().map(|s| (s.id, s)).collect();
    let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
        .school_classes
        .iter()
        .map(|c| (c.id, c.home_room_id))
        .collect();
    let max_position_per_day: HashMap<u8, u8> =
        problem
            .time_blocks
            .iter()
            .fold(HashMap::new(), |mut acc, tb| {
                acc.entry(tb.day_of_week)
                    .and_modify(|m| *m = (*m).max(tb.position))
                    .or_insert(tb.position);
                acc
            });
    let days: u8 = problem
        .time_blocks
        .iter()
        .map(|tb| tb.day_of_week)
        .max()
        .map(|m| m.saturating_add(1))
        .unwrap_or(0);

    let expected_hours: u32 = problem
        .lessons
        .iter()
        .map(|l| u32::from(l.hours_per_week))
        .sum();
    let placed = u32::try_from(placements.len()).unwrap_or(u32::MAX);

    let mut by_class_day: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    let mut by_teacher_day: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();

    for p in placements {
        let tb = tb_lookup[&p.time_block_id];
        let lesson = lesson_lookup[&p.lesson_id];
        for class_id in &lesson.school_class_ids {
            by_class_day
                .entry((*class_id, tb.day_of_week))
                .or_default()
                .push(tb.position);
        }
        by_teacher_day
            .entry((lesson.teacher_id, tb.day_of_week))
            .or_default()
            .push(tb.position);
    }

    let class_day_balance_cost_value =
        class_day_balance_cost(&by_class_day, &problem.school_classes, days);

    let class_gap_hours: u32 = by_class_day
        .into_values()
        .map(|mut v| {
            v.sort_unstable();
            v.dedup();
            gap_count(&v)
        })
        .sum();
    let teacher_gap_hours: u32 = by_teacher_day
        .into_values()
        .map(|mut v| {
            v.sort_unstable();
            v.dedup();
            gap_count(&v)
        })
        .sum();

    let mut prefer_early_units: u32 = 0;
    let mut avoid_first_units: u32 = 0;
    let mut avoid_last_units: u32 = 0;
    let mut prefer_late_units: u32 = 0;
    let mut home_room_misses: u32 = 0;

    let unit_weights = ConstraintWeights {
        prefer_early_period: 1,
        avoid_first_period: 1,
        avoid_last_period: 1,
        prefer_late_period: 1,
        prefer_home_room: 1,
        ..ConstraintWeights::default()
    };

    for p in placements {
        let lesson = lesson_lookup[&p.lesson_id];
        let subject = subject_lookup[&lesson.subject_id];
        let tb = tb_lookup[&p.time_block_id];
        let max_pos = max_position_per_day
            .get(&tb.day_of_week)
            .copied()
            .unwrap_or(tb.position);

        // Per-axis unit decomposition. We re-implement the predicate
        // shape here rather than calling subject_preference_score with
        // unit weights, because the helper folds all four axes into a
        // single u32 we cannot un-mix.
        prefer_early_units = prefer_early_units.saturating_add(
            subject
                .prefer_early_period
                .saturating_mul(u32::from(tb.position)),
        );
        if tb.position == 0 {
            avoid_first_units = avoid_first_units.saturating_add(subject.avoid_first_period);
        }
        if tb.position == max_pos {
            avoid_last_units = avoid_last_units.saturating_add(subject.avoid_last_period);
        }
        prefer_late_units = prefer_late_units.saturating_add(
            subject
                .prefer_late_period
                .saturating_mul(u32::from(max_pos.saturating_sub(tb.position))),
        );

        // Home-room misses: count per non-matching member class. This
        // mirrors `home_room_penalty` with weight==1 and gives us the
        // raw count.
        home_room_misses = home_room_misses.saturating_add(home_room_penalty(
            lesson,
            &home_room_lookup,
            p.room_id,
            &unit_weights,
        ));
    }

    // Re-derive subject_preference contribution under the caller's
    // weights so weighted_score matches score_solution exactly.
    let subject_preference_weighted: u32 = placements
        .iter()
        .map(|p| {
            let lesson = lesson_lookup[&p.lesson_id];
            let subject = subject_lookup[&lesson.subject_id];
            let tb = tb_lookup[&p.time_block_id];
            let max_pos = max_position_per_day
                .get(&tb.day_of_week)
                .copied()
                .unwrap_or(tb.position);
            subject_preference_score(subject, tb, max_pos, weights)
        })
        .sum();
    let home_room_weighted: u32 = placements
        .iter()
        .map(|p| {
            let lesson = lesson_lookup[&p.lesson_id];
            home_room_penalty(lesson, &home_room_lookup, p.room_id, weights)
        })
        .sum();

    let weighted_score = weights
        .class_gap
        .saturating_mul(class_gap_hours)
        .saturating_add(weights.teacher_gap.saturating_mul(teacher_gap_hours))
        .saturating_add(subject_preference_weighted)
        .saturating_add(
            weights
                .class_day_balance
                .saturating_mul(class_day_balance_cost_value),
        )
        .saturating_add(home_room_weighted);

    QualityReport {
        hard_violations: u32::try_from(violations.len()).unwrap_or(u32::MAX),
        unplaced_hours: expected_hours.saturating_sub(placed),
        class_gap_hours,
        teacher_gap_hours,
        class_day_balance_cost: class_day_balance_cost_value,
        home_room_misses,
        prefer_early_units,
        avoid_first_units,
        avoid_last_units,
        prefer_late_units,
        weighted_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
    use crate::score::score_solution;
    use crate::solve_with_config;
    use crate::test_fixtures::grundschule_fixture;
    use crate::types::{
        Lesson, Placement, Problem, Room, SchoolClass, Solution, SolveConfig, Subject, Teacher,
        TeacherQualification, TimeBlock, Violation, ViolationKind, PRODUCTION_ACTIVE_WEIGHTS,
    };
    use uuid::Uuid;

    fn quality_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn quality_three_block_one_class_problem() -> Problem {
        Problem {
            time_blocks: vec![
                TimeBlock {
                    id: TimeBlockId(quality_uuid(10)),
                    day_of_week: 0,
                    position: 0,
                },
                TimeBlock {
                    id: TimeBlockId(quality_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                },
                TimeBlock {
                    id: TimeBlockId(quality_uuid(12)),
                    day_of_week: 0,
                    position: 2,
                },
            ],
            teachers: vec![Teacher {
                id: TeacherId(quality_uuid(20)),
                max_hours_per_week: 10,
            }],
            rooms: vec![Room {
                id: RoomId(quality_uuid(30)),
            }],
            subjects: vec![Subject {
                id: SubjectId(quality_uuid(40)),
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![SchoolClass {
                id: SchoolClassId(quality_uuid(50)),
                home_room_id: None,
                max_lessons_per_day: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(quality_uuid(60)),
                school_class_ids: vec![SchoolClassId(quality_uuid(50))],
                subject_id: SubjectId(quality_uuid(40)),
                teacher_id: TeacherId(quality_uuid(20)),
                hours_per_week: 2,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: TeacherId(quality_uuid(20)),
                subject_id: SubjectId(quality_uuid(40)),
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        }
    }

    fn place_in(lesson_id: u8, tb_id: u8, room_id: u8) -> Placement {
        Placement {
            lesson_id: LessonId(quality_uuid(lesson_id)),
            time_block_id: TimeBlockId(quality_uuid(tb_id)),
            room_id: RoomId(quality_uuid(room_id)),
        }
    }

    #[test]
    fn quality_report_default_returns_zeros() {
        let report = QualityReport::default();
        assert_eq!(report, QualityReport::default());
        assert_eq!(report.weighted_score, 0);
    }

    #[test]
    fn quality_report_hard_violations_equals_violation_count() {
        let problem = quality_three_block_one_class_problem();
        let violations = vec![
            Violation {
                kind: ViolationKind::NoFreeTimeBlock,
                lesson_id: LessonId(quality_uuid(60)),
                hour_index: 0,
                reason: None,
            },
            Violation {
                kind: ViolationKind::NoSuitableRoom,
                lesson_id: LessonId(quality_uuid(60)),
                hour_index: 1,
                reason: None,
            },
        ];
        let report = quality_report(&problem, &[], &violations, &ConstraintWeights::default());
        assert_eq!(report.hard_violations, 2);
    }

    #[test]
    fn quality_report_unplaced_hours_equals_expected_minus_placed() {
        // Problem expects 2 hours; pass one placement; expect unplaced=1.
        let problem = quality_three_block_one_class_problem();
        let placements = vec![place_in(60, 10, 30)];
        let report = quality_report(&problem, &placements, &[], &ConstraintWeights::default());
        assert_eq!(report.unplaced_hours, 1);
    }

    #[test]
    fn quality_report_class_gap_hours_matches_score_helper() {
        // Class places at positions 0 and 2 on day 0: gap_count = 1.
        let problem = quality_three_block_one_class_problem();
        let placements = vec![place_in(60, 10, 30), place_in(60, 12, 30)];
        let report = quality_report(&problem, &placements, &[], &ConstraintWeights::default());
        assert_eq!(report.class_gap_hours, 1);
        assert_eq!(report.teacher_gap_hours, 1);
    }

    #[test]
    fn quality_report_class_day_balance_matches_score_helper() {
        // Four placements all on day 0 over 4 days: lopsided cost = 6.
        let mut problem = quality_three_block_one_class_problem();
        for day in 1..=3u8 {
            problem.time_blocks.push(TimeBlock {
                id: TimeBlockId(quality_uuid(20 + day)),
                day_of_week: day,
                position: 0,
            });
        }
        problem.time_blocks.push(TimeBlock {
            id: TimeBlockId(quality_uuid(13)),
            day_of_week: 0,
            position: 3,
        });
        problem.time_blocks.push(TimeBlock {
            id: TimeBlockId(quality_uuid(14)),
            day_of_week: 0,
            position: 4,
        });
        let placements = vec![
            place_in(60, 10, 30),
            place_in(60, 11, 30),
            place_in(60, 12, 30),
            place_in(60, 13, 30),
        ];
        let report = quality_report(&problem, &placements, &[], &ConstraintWeights::default());
        assert_eq!(report.class_day_balance_cost, 6);
    }

    #[test]
    fn quality_report_home_room_misses_counts_per_member_class() {
        let mut problem = quality_three_block_one_class_problem();
        problem.school_classes[0].home_room_id = Some(RoomId(quality_uuid(30)));
        problem.rooms.push(Room {
            id: RoomId(quality_uuid(31)),
        });
        let placements = vec![
            place_in(60, 10, 30), // hits home room: no miss
            place_in(60, 11, 31), // miss
            place_in(60, 12, 31), // miss
        ];
        let report = quality_report(&problem, &placements, &[], &ConstraintWeights::default());
        assert_eq!(report.home_room_misses, 2);
    }

    #[test]
    fn quality_report_prefer_early_units_match_score_helper() {
        let mut problem = quality_three_block_one_class_problem();
        problem.subjects[0].prefer_early_period = 2;
        // Placements at positions 0 and 2: units = 2*0 + 2*2 = 4.
        let placements = vec![place_in(60, 10, 30), place_in(60, 12, 30)];
        let report = quality_report(&problem, &placements, &[], &ConstraintWeights::default());
        assert_eq!(report.prefer_early_units, 4);
    }

    #[test]
    fn quality_report_avoid_first_units_match_score_helper() {
        let mut problem = quality_three_block_one_class_problem();
        problem.subjects[0].avoid_first_period = 3;
        // Placement at position 0: units = 3.
        let placements = vec![place_in(60, 10, 30)];
        let report = quality_report(&problem, &placements, &[], &ConstraintWeights::default());
        assert_eq!(report.avoid_first_units, 3);
    }

    #[test]
    fn quality_report_avoid_last_units_match_score_helper() {
        let mut problem = quality_three_block_one_class_problem();
        problem.subjects[0].avoid_last_period = 5;
        // Placement at position 2 (max_position_for_day_0 = 2): units = 5.
        let placements = vec![place_in(60, 12, 30)];
        let report = quality_report(&problem, &placements, &[], &ConstraintWeights::default());
        assert_eq!(report.avoid_last_units, 5);
    }

    #[test]
    fn quality_report_prefer_late_units_match_score_helper() {
        let mut problem = quality_three_block_one_class_problem();
        problem.subjects[0].prefer_late_period = 2;
        // Placements at positions 0 and 2 (max = 2):
        // units = 2*(2-0) + 2*(2-2) = 4.
        let placements = vec![place_in(60, 10, 30), place_in(60, 12, 30)];
        let report = quality_report(&problem, &placements, &[], &ConstraintWeights::default());
        assert_eq!(report.prefer_late_units, 4);
    }

    #[test]
    fn quality_report_weighted_score_equals_score_solution_on_grundschule() {
        let problem = grundschule_fixture();
        let cfg = SolveConfig {
            weights: PRODUCTION_ACTIVE_WEIGHTS.clone(),
            deadline: None,
            ..SolveConfig::default()
        };
        let solution: Solution = solve_with_config(&problem, &cfg).expect("solve");
        let report = quality_report(
            &problem,
            &solution.placements,
            &solution.violations,
            &PRODUCTION_ACTIVE_WEIGHTS,
        );
        let expected = score_solution(&problem, &solution.placements, &PRODUCTION_ACTIVE_WEIGHTS);
        assert_eq!(report.weighted_score, expected);
    }
}
