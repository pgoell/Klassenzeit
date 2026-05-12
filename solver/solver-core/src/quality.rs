//! Backend-neutral component vector exposing each cost-axis subtotal that
//! `score::score_solution` aggregates into [`crate::types::Solution::soft_score`]. The
//! contract is: every backend's output gets evaluated through
//! [`quality_report`]; the bake-off bench renders the load-bearing
//! components per backend; ADRs and item-51 work compare component
//! vectors instead of one collapsed scalar.
//!
//! Adding a new axis: add a field to [`QualityReport`], extend
//! `quality_report` to populate it, add a unit test that walks the same
//! partition the underlying `score::*` helper walks, and add an entry to
//! the `weighted_score` accumulation. The property test
//! `quality_report_weighted_score_matches_score_solution` keeps the
//! cross-axis sum honest. Future axes flagged by item 50: teacher bad
//! windows / day-quality (no schema yet), pin disruption cost (waits
//! for soft pins).

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

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
    /// Number of `(class, subject)` pairs whose `class.class_teacher_id`
    /// is set and qualified for that subject, but the placement's chosen
    /// teacher (first-encountered for the pair) does not match the class
    /// teacher. Counts at most one miss per `(class, subject)` pair.
    /// Item 67.
    pub prefer_class_teacher_misses: u32,
    /// `score::score_solution(problem, placements, weights)`. The
    /// `quality_report_weighted_score_matches_score_solution` property
    /// test pins this equality.
    pub weighted_score: u32,
    /// Worst per-class daily-load spread:
    /// `max over classes of (max(daily_count) - min(daily_count))`.
    /// Mirrors `score::worst_class_spread`. Item 57.
    pub worst_per_class_spread: u32,
    /// Worst per-class summed interior gaps:
    /// `max over classes of (sum over days of interior_gaps_in_day)`.
    /// Mirrors `score::worst_class_interior_gaps`. Item 57.
    pub worst_per_class_interior_gaps: u32,
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
            .entry((p.teacher_id, tb.day_of_week))
            .or_default()
            .push(tb.position);
    }

    // Dedup `by_class_day` Vec values in-place so a (class, day, position)
    // slot counts once regardless of lesson-group co-placement count.
    // Mirrors `score_solution`'s identical dedup; see the matching comment
    // there. Item 76.
    for v in by_class_day.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    let class_day_balance_cost_value =
        class_day_balance_cost(&by_class_day, &problem.school_classes, days);

    let class_gap_hours: u32 = by_class_day.into_values().map(|v| gap_count(&v)).sum();
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

    // Build (subject_id -> set of qualified teacher ids) from
    // problem.teacher_qualifications, then walk placements to build
    // (class, subject) -> first-encountered teacher. Count one miss per
    // pair whose class.class_teacher_id is qualified for the subject and
    // differs from the encountered teacher. Item 67.
    let mut subject_qualified_teachers: HashMap<SubjectId, std::collections::HashSet<TeacherId>> =
        HashMap::new();
    for q in &problem.teacher_qualifications {
        subject_qualified_teachers
            .entry(q.subject_id)
            .or_default()
            .insert(q.teacher_id);
    }
    let mut class_teacher_lookup: HashMap<SchoolClassId, Option<TeacherId>> = HashMap::new();
    for c in &problem.school_classes {
        class_teacher_lookup.insert(c.id, c.class_teacher_id);
    }
    let mut class_subject_teacher: HashMap<(SchoolClassId, SubjectId), TeacherId> = HashMap::new();
    for p in placements {
        let lesson = lesson_lookup[&p.lesson_id];
        for class_id in &lesson.school_class_ids {
            class_subject_teacher
                .entry((*class_id, lesson.subject_id))
                .or_insert(p.teacher_id);
        }
    }
    let mut prefer_class_teacher_misses: u32 = 0;
    for ((cid, sid), tid) in &class_subject_teacher {
        let Some(Some(klt)) = class_teacher_lookup.get(cid) else {
            continue;
        };
        let Some(qualified) = subject_qualified_teachers.get(sid) else {
            continue;
        };
        if qualified.contains(klt) && tid != klt {
            prefer_class_teacher_misses = prefer_class_teacher_misses.saturating_add(1);
        }
    }

    // Per-class worst-case raw counts. `quality_report` is cold-path
    // (post-solve), so one extra walk per axis is fine. Item 57.
    let worst_per_class_spread = crate::score::worst_class_spread(problem, placements);
    let worst_per_class_interior_gaps =
        crate::score::worst_class_interior_gaps(problem, placements);

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
        .saturating_add(
            weights
                .max_per_class_spread
                .saturating_mul(worst_per_class_spread),
        )
        .saturating_add(
            weights
                .max_per_class_interior_gaps
                .saturating_mul(worst_per_class_interior_gaps),
        )
        .saturating_add(home_room_weighted)
        .saturating_add(
            weights
                .prefer_class_teacher
                .saturating_mul(prefer_class_teacher_misses),
        );

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
        prefer_class_teacher_misses,
        weighted_score,
        worst_per_class_spread,
        worst_per_class_interior_gaps,
    }
}

/// One canonical soft-objective axis. Every backend declares which of these
/// it optimises in its internal acceptance criterion or model objective via
/// [`BackendObjective`]. `HardViolations` and `UnplacedHours` from
/// [`QualityReport`] are intentionally excluded: they are pruned during
/// search rather than optimised, so it is meaningless to ask which backend
/// "optimises" them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QualityComponent {
    /// `weights.class_gap`-weighted axis from `score::class_gap_cost`.
    ClassGap,
    /// `weights.teacher_gap`-weighted axis from `score::teacher_gap_cost`.
    TeacherGap,
    /// `weights.class_day_balance`-weighted axis from `score::class_day_balance_cost`.
    ClassDayBalance,
    /// `weights.prefer_home_room`-weighted axis from `score::home_room_penalty`.
    HomeRoom,
    /// `weights.prefer_early_period`-weighted axis (per-placement
    /// `subject.prefer_early_period * tb.position`).
    PreferEarly,
    /// `weights.avoid_first_period`-weighted axis at `tb.position == 0`.
    AvoidFirst,
    /// `weights.avoid_last_period`-weighted axis at the last position of the day.
    AvoidLast,
    /// `weights.prefer_late_period`-weighted axis from
    /// `subject.prefer_late_period * (max_position_for_day - tb.position)`.
    PreferLate,
    /// `weights.prefer_class_teacher`-weighted axis. One unit per
    /// `(class, subject)` pair whose `class.class_teacher_id` is qualified
    /// for that subject but the placement's chosen teacher is not the
    /// class teacher. Item 67.
    PreferClassTeacher,
}

impl QualityComponent {
    /// Every variant, in [`PartialOrd`] order. Used by tests and by the
    /// bench renderer to enumerate components deterministically.
    pub const ALL: [QualityComponent; 9] = [
        QualityComponent::ClassGap,
        QualityComponent::TeacherGap,
        QualityComponent::ClassDayBalance,
        QualityComponent::HomeRoom,
        QualityComponent::PreferEarly,
        QualityComponent::AvoidFirst,
        QualityComponent::AvoidLast,
        QualityComponent::PreferLate,
        QualityComponent::PreferClassTeacher,
    ];

    /// Lower-snake_case label suitable for markdown rendering and as a
    /// stable identifier in tests. Named `component_label` rather than
    /// `label` to keep `scripts/check_unique_fns.py` happy:
    /// `BenchBackend::label` already exists in `solver-bench/src/main.rs`.
    pub fn component_label(self) -> &'static str {
        match self {
            QualityComponent::ClassGap => "class_gap",
            QualityComponent::TeacherGap => "teacher_gap",
            QualityComponent::ClassDayBalance => "class_day_balance",
            QualityComponent::HomeRoom => "home_room",
            QualityComponent::PreferEarly => "prefer_early",
            QualityComponent::AvoidFirst => "avoid_first",
            QualityComponent::AvoidLast => "avoid_last",
            QualityComponent::PreferLate => "prefer_late",
            QualityComponent::PreferClassTeacher => "prefer_class_teacher",
        }
    }
}

/// Per-backend objective declaration. Describes which canonical
/// [`QualityComponent`]s the backend's *internal* search loop or model
/// objective optimises today, plus the components it explicitly does not
/// optimise. The bench renders this above the bake-off table so reviewers
/// see internal-objective drift instead of staring at a collapsed `Soft
/// score` column.
///
/// The declarations describe today's reality, not the desired end state.
/// As items 48 / 52 / 54 land, each one moves entries from `declared_skipped`
/// into `optimised` in its own commit.
#[derive(Debug)]
pub struct BackendObjective {
    /// Backend identifier matching `solver-bench`'s `--backend` argument
    /// (`"lahc"`, `"lahc_rr"`, `"lahc_rr_kempe"`, `"cpsat"`).
    pub name: &'static str,
    /// Canonical components this backend's internal acceptance criterion
    /// (LAHC) or model objective (CP-SAT) actually steers toward.
    pub optimised: BTreeSet<QualityComponent>,
    /// Canonical components this backend explicitly does not include in
    /// its internal objective today. Their value is still recomputed
    /// post-solve by `quality_report(...)` and contributes to
    /// `Solution.soft_score`, so a backend can score badly on a skipped
    /// axis without that being a bug.
    pub declared_skipped: BTreeSet<QualityComponent>,
    /// One-sentence rationale tying each declaration back to the OPEN_THINGS
    /// item that closes the gap (item 48 for cpsat; item 52 for LAHC's slice).
    pub notes: &'static str,
}

/// Looks up the [`BackendObjective`] for a registered backend. Returns
/// `None` for unknown names; bench callers treat that as a registration bug.
pub fn backend_objective(name: &str) -> Option<&'static BackendObjective> {
    BACKEND_OBJECTIVES
        .get_or_init(build_backend_objectives)
        .iter()
        .find(|bo| bo.name == name)
}

static BACKEND_OBJECTIVES: OnceLock<Vec<BackendObjective>> = OnceLock::new();

fn build_backend_objectives() -> Vec<BackendObjective> {
    let lahc_optimised: BTreeSet<QualityComponent> =
        QualityComponent::ALL.iter().copied().collect();
    let lahc_skipped: BTreeSet<QualityComponent> = BTreeSet::new();
    let lahc_notes = "LAHC accepts and exits on the full canonical (see lahc::run); \
                      FFD greedy ranks windows by slice + home_room + class_day_balance (item 54).";
    let cpsat_optimised: BTreeSet<QualityComponent> =
        QualityComponent::ALL.iter().copied().collect();
    let cpsat_skipped: BTreeSet<QualityComponent> = BTreeSet::new();
    let cpsat_notes = "CP-SAT minimises the canonical objective end-to-end (cpsat.py): \
                       subject_preference plus home_room plus class_gap plus teacher_gap \
                       plus class_day_balance, weights from PRODUCTION_ACTIVE_WEIGHTS.";
    vec![
        BackendObjective {
            name: "lahc",
            optimised: lahc_optimised.clone(),
            declared_skipped: lahc_skipped.clone(),
            notes: lahc_notes,
        },
        BackendObjective {
            name: "lahc_rr",
            optimised: lahc_optimised.clone(),
            declared_skipped: lahc_skipped.clone(),
            notes: "Inherits LAHC's canonical objective; R&R recreate ranks rooms by \
                    home_room delta after item 49.",
        },
        BackendObjective {
            name: "lahc_kempe",
            optimised: lahc_optimised.clone(),
            declared_skipped: lahc_skipped.clone(),
            notes: lahc_notes,
        },
        BackendObjective {
            name: "lahc_rr_kempe",
            optimised: lahc_optimised,
            declared_skipped: lahc_skipped,
            notes: lahc_notes,
        },
        BackendObjective {
            name: "cpsat",
            optimised: cpsat_optimised,
            declared_skipped: cpsat_skipped,
            notes: cpsat_notes,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
    use crate::score;
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
                class_teacher_id: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(quality_uuid(60)),
                school_class_ids: vec![SchoolClassId(quality_uuid(50))],
                subject_id: SubjectId(quality_uuid(40)),
                teacher_candidates: vec![TeacherId(quality_uuid(20))],
                teacher_pin: Some(TeacherId(quality_uuid(20))),
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
            teacher_id: TeacherId(quality_uuid(20)),
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
        // Item 57 Task 4: `quality_report.weighted_score` now folds in the
        // per-class worst-case summands, so the parity test runs against
        // unmodified PRODUCTION_ACTIVE_WEIGHTS.
        let weights = PRODUCTION_ACTIVE_WEIGHTS.clone();
        let cfg = SolveConfig {
            weights: weights.clone(),
            deadline: None,
            ..SolveConfig::default()
        };
        let solution: Solution = solve_with_config(&problem, &cfg).expect("solve");
        let report = quality_report(
            &problem,
            &solution.placements,
            &solution.violations,
            &weights,
        );
        let expected = score_solution(&problem, &solution.placements, &weights);
        assert_eq!(report.weighted_score, expected);
    }

    #[test]
    fn backend_objective_returns_some_for_every_known_backend() {
        for name in ["lahc", "lahc_rr", "lahc_kempe", "lahc_rr_kempe", "cpsat"] {
            assert!(
                backend_objective(name).is_some(),
                "backend_objective({name:?}) should return Some; the bench enumerates this name",
            );
        }
    }

    #[test]
    fn backend_objective_returns_none_for_unknown_name() {
        assert!(backend_objective("timefold").is_none());
        assert!(backend_objective("").is_none());
    }

    #[test]
    fn backend_objective_lahc_family_partitions_quality_components() {
        use std::collections::BTreeSet;
        let all: BTreeSet<QualityComponent> = QualityComponent::ALL.iter().copied().collect();
        for name in ["lahc", "lahc_rr", "lahc_kempe", "lahc_rr_kempe"] {
            let bo = backend_objective(name).expect("registered");
            let union: BTreeSet<QualityComponent> =
                bo.optimised.union(&bo.declared_skipped).copied().collect();
            assert_eq!(
                union, all,
                "{name}: optimised ∪ declared_skipped must cover every QualityComponent",
            );
            let intersection: BTreeSet<QualityComponent> = bo
                .optimised
                .intersection(&bo.declared_skipped)
                .copied()
                .collect();
            assert!(
                intersection.is_empty(),
                "{name}: optimised ∩ declared_skipped must be empty (component cannot be both)",
            );
        }
    }

    #[test]
    fn backend_objective_cpsat_partitions_quality_components() {
        let bo = backend_objective("cpsat").expect("registered");
        assert_eq!(
            bo.optimised.len(),
            QualityComponent::ALL.len(),
            "cpsat: post-port objective optimises every QualityComponent",
        );
        assert!(
            bo.declared_skipped.is_empty(),
            "cpsat: post-port objective skips no QualityComponent",
        );
    }

    /// Two classes over five days, one shared room, one shared subject, no
    /// `class_teacher_id` (so `prefer_class_teacher` stays inert). Mirrors
    /// `score::tests::synthetic_two_class_five_day_problem` in shape so the
    /// `worst_class_*` helpers walk a non-trivial partition. Item 57.
    fn quality_synthetic_two_class_five_day_problem(width: u8) -> Problem {
        let class_a = SchoolClassId(quality_uuid(150));
        let class_b = SchoolClassId(quality_uuid(151));
        let teacher_a = TeacherId(quality_uuid(120));
        let teacher_b = TeacherId(quality_uuid(121));
        let subject_id = SubjectId(quality_uuid(140));
        let room_id = RoomId(quality_uuid(130));
        let mut time_blocks: Vec<TimeBlock> = Vec::new();
        for day in 0u8..5 {
            for pos in 0u8..width {
                time_blocks.push(TimeBlock {
                    id: TimeBlockId(quality_uuid(100 + day * 10 + pos)),
                    day_of_week: day,
                    position: pos,
                });
            }
        }
        Problem {
            time_blocks,
            teachers: vec![
                Teacher {
                    id: teacher_a,
                    max_hours_per_week: 50,
                },
                Teacher {
                    id: teacher_b,
                    max_hours_per_week: 50,
                },
            ],
            rooms: vec![Room { id: room_id }],
            subjects: vec![Subject {
                id: subject_id,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![
                SchoolClass {
                    id: class_a,
                    home_room_id: None,
                    max_lessons_per_day: None,
                    class_teacher_id: None,
                },
                SchoolClass {
                    id: class_b,
                    home_room_id: None,
                    max_lessons_per_day: None,
                    class_teacher_id: None,
                },
            ],
            lessons: vec![
                Lesson {
                    id: LessonId(quality_uuid(160)),
                    school_class_ids: vec![class_a],
                    subject_id,
                    teacher_candidates: vec![teacher_a],
                    teacher_pin: Some(teacher_a),
                    hours_per_week: u8::max(1, width * 5),
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
                Lesson {
                    id: LessonId(quality_uuid(161)),
                    school_class_ids: vec![class_b],
                    subject_id,
                    teacher_candidates: vec![teacher_b],
                    teacher_pin: Some(teacher_b),
                    hours_per_week: u8::max(1, width * 5),
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
            ],
            teacher_qualifications: vec![
                TeacherQualification {
                    teacher_id: teacher_a,
                    subject_id,
                },
                TeacherQualification {
                    teacher_id: teacher_b,
                    subject_id,
                },
            ],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        }
    }

    fn quality_synthetic_place_class_a(day: u8, position: u8) -> Placement {
        Placement {
            lesson_id: LessonId(quality_uuid(160)),
            time_block_id: TimeBlockId(quality_uuid(100 + day * 10 + position)),
            room_id: RoomId(quality_uuid(130)),
            teacher_id: TeacherId(quality_uuid(120)),
        }
    }

    fn quality_synthetic_place_class_b(day: u8, position: u8) -> Placement {
        Placement {
            lesson_id: LessonId(quality_uuid(161)),
            time_block_id: TimeBlockId(quality_uuid(100 + day * 10 + position)),
            room_id: RoomId(quality_uuid(130)),
            teacher_id: TeacherId(quality_uuid(121)),
        }
    }

    /// Class A: 4 placements on Monday positions 0..3 -> per-day counts
    /// [4,0,0,0,0], spread = 4. Class B: 1 placement on every weekday ->
    /// spread = 0. `max over classes = 4`.
    fn quality_synthetic_two_class_unbalanced() -> (Problem, Vec<Placement>) {
        let problem = quality_synthetic_two_class_five_day_problem(4);
        let mut placements: Vec<Placement> = Vec::new();
        for pos in 0u8..4 {
            placements.push(quality_synthetic_place_class_a(0, pos));
        }
        for day in 0u8..5 {
            placements.push(quality_synthetic_place_class_b(day, 0));
        }
        (problem, placements)
    }

    /// Class A occupies positions {0, 3} on Monday only; class B is
    /// contiguous at {0, 1} on Monday only. Class A's interior gaps =
    /// 4 - 2 = 2; class B = 0. `max over classes = 2`.
    fn quality_synthetic_one_class_gappy() -> (Problem, Vec<Placement>) {
        let problem = quality_synthetic_two_class_five_day_problem(4);
        let placements = vec![
            quality_synthetic_place_class_a(0, 0),
            quality_synthetic_place_class_a(0, 3),
            quality_synthetic_place_class_b(0, 0),
            quality_synthetic_place_class_b(0, 1),
        ];
        (problem, placements)
    }

    #[test]
    fn quality_report_worst_per_class_spread_matches_score_helper() {
        let (problem, placements) = quality_synthetic_two_class_unbalanced();
        let weights = ConstraintWeights::default();
        let report = quality_report(&problem, &placements, &[], &weights);
        assert_eq!(
            report.worst_per_class_spread,
            score::worst_class_spread(&problem, &placements)
        );
        // Sanity: the partition is non-trivial.
        assert_eq!(report.worst_per_class_spread, 4);
    }

    #[test]
    fn quality_report_worst_per_class_interior_gaps_matches_score_helper() {
        let (problem, placements) = quality_synthetic_one_class_gappy();
        let weights = ConstraintWeights::default();
        let report = quality_report(&problem, &placements, &[], &weights);
        assert_eq!(
            report.worst_per_class_interior_gaps,
            score::worst_class_interior_gaps(&problem, &placements)
        );
        // Sanity: the partition is non-trivial.
        assert_eq!(report.worst_per_class_interior_gaps, 2);
    }

    #[test]
    fn quality_report_weighted_score_matches_score_solution_under_widened_axes() {
        let (problem, placements) = quality_synthetic_two_class_unbalanced();
        let weights = PRODUCTION_ACTIVE_WEIGHTS.clone();
        let report = quality_report(&problem, &placements, &[], &weights);
        assert_eq!(
            report.weighted_score,
            score::score_solution(&problem, &placements, &weights)
        );
    }
}
