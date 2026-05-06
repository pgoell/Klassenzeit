# Backend-neutral `QualityReport` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote per-axis cost subtotals from `score_solution`'s internals into a public `solver_core::QualityReport` struct, render four load-bearing components per backend in `solver-bench`'s markdown output, and document the mapping onto the backend's `quality_checks.py` `QualityIssue.kind` Literal.

**Architecture:** New `solver/solver-core/src/quality.rs` module exposes `pub struct QualityReport` (eleven fields) and `pub fn quality_report(...)`. `score_solution` stays untouched in the hot path; a property test in `tests/quality_property.rs` asserts `quality_report(...).weighted_score == score_solution(...)`. Solver-bench's existing predicate-style `QualityReport` renames to `QualityPredicates`; nine new median fields land on `CellResult`; four new markdown columns surface the load-bearing axes (`class_gap_h`, `teacher_gap_h`, `home_room_miss`, `day_balance`).

**Tech Stack:** Rust 2021 (`solver-core` no_std-friendly, `solver-bench` cdylib-free supervisor + cell-child), proptest 1.x, serde, Python 3.13 (backend docstring update only).

---

## Task 1 — solver-core: `QualityReport` struct + `quality_report` fn + co-located unit tests + property test

**Files:**
- Create: `solver/solver-core/src/quality.rs`
- Modify: `solver/solver-core/src/lib.rs` (add `pub mod quality;` + re-export)
- Create: `solver/solver-core/tests/quality_property.rs`

**Subagent dispatch.** This task lands in one subagent (substantial Rust + tests + property generator + 5×128 sweep). Acceptance: all unit tests pass, property test passes, 5×128 PROPTEST_CASES sweep is clean, `cargo nextest run -p solver-core` is green, `mise run lint` is green. Commit on success: `feat(solver-core): add QualityReport component vector (item 50)`.

- [ ] **Step 1: Add `quality.rs` skeleton with the `QualityReport` struct and a stub `quality_report` fn that returns `Default::default()`**

```rust
//! Backend-neutral component vector exposing each cost-axis subtotal that
//! `score::score_solution` aggregates into [`Solution::soft_score`]. The
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
use crate::score::{class_day_balance_cost, gap_count, home_room_penalty, subject_preference_score};
use crate::types::{
    ConstraintWeights, Lesson, Placement, Problem, SchoolClass, Subject, TimeBlock, Violation,
};

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
    _problem: &Problem,
    _placements: &[Placement],
    _violations: &[Violation],
    _weights: &ConstraintWeights,
) -> QualityReport {
    QualityReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_report_default_returns_zeros() {
        let report = QualityReport::default();
        assert_eq!(report, QualityReport::default());
        assert_eq!(report.weighted_score, 0);
    }
}
```

- [ ] **Step 2: Wire the module into `lib.rs`**

Add to `solver/solver-core/src/lib.rs` near the other `pub mod` declarations (alphabetical-ish order):

```rust
pub mod quality;

pub use quality::{quality_report, QualityReport};
```

- [ ] **Step 3: Run the stub tests to verify the module compiles and the default test passes**

Run: `cargo nextest run -p solver-core --lib quality::tests`
Expected: PASS (`quality_report_default_returns_zeros`).

- [ ] **Step 4: Add the failing per-axis unit tests**

Append to `solver/solver-core/src/quality.rs::tests` (the imports go at the top of the `tests` module):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{
        LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId,
    };
    use crate::score::score_solution;
    use crate::test_fixtures::grundschule_fixture;
    use crate::types::{
        Lesson, Placement, Problem, Room, RoomSubjectSuitability, SchoolClass,
        Solution, SolveConfig, Subject, Teacher, TeacherQualification, TimeBlock,
        Violation, ViolationKind, PRODUCTION_ACTIVE_WEIGHTS,
    };
    use crate::solve_with_config;
    use uuid::Uuid;

    fn quality_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn three_block_one_class_problem() -> Problem {
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
        let problem = three_block_one_class_problem();
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
        let report = quality_report(
            &problem,
            &[],
            &violations,
            &ConstraintWeights::default(),
        );
        assert_eq!(report.hard_violations, 2);
    }

    #[test]
    fn quality_report_unplaced_hours_equals_expected_minus_placed() {
        // Problem expects 2 hours; pass one placement; expect unplaced=1.
        let problem = three_block_one_class_problem();
        let placements = vec![place_in(60, 10, 30)];
        let report = quality_report(
            &problem,
            &placements,
            &[],
            &ConstraintWeights::default(),
        );
        assert_eq!(report.unplaced_hours, 1);
    }

    #[test]
    fn quality_report_class_gap_hours_matches_score_helper() {
        // Class places at positions 0 and 2 on day 0: gap_count = 1.
        let problem = three_block_one_class_problem();
        let placements = vec![place_in(60, 10, 30), place_in(60, 12, 30)];
        let report = quality_report(
            &problem,
            &placements,
            &[],
            &ConstraintWeights::default(),
        );
        assert_eq!(report.class_gap_hours, 1);
        assert_eq!(report.teacher_gap_hours, 1);
    }

    #[test]
    fn quality_report_class_day_balance_matches_score_helper() {
        // Four placements all on day 0 over 4 days: lopsided cost = 6.
        let mut problem = three_block_one_class_problem();
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
        let report = quality_report(
            &problem,
            &placements,
            &[],
            &ConstraintWeights::default(),
        );
        assert_eq!(report.class_day_balance_cost, 6);
    }

    #[test]
    fn quality_report_home_room_misses_counts_per_member_class() {
        let mut problem = three_block_one_class_problem();
        problem.school_classes[0].home_room_id = Some(RoomId(quality_uuid(30)));
        problem.rooms.push(Room {
            id: RoomId(quality_uuid(31)),
        });
        let placements = vec![
            place_in(60, 10, 30), // hits home room: no miss
            place_in(60, 11, 31), // miss
            place_in(60, 12, 31), // miss
        ];
        let report = quality_report(
            &problem,
            &placements,
            &[],
            &ConstraintWeights::default(),
        );
        assert_eq!(report.home_room_misses, 2);
    }

    #[test]
    fn quality_report_prefer_early_units_match_score_helper() {
        let mut problem = three_block_one_class_problem();
        problem.subjects[0].prefer_early_period = 2;
        // Placements at positions 0 and 2: units = 2*0 + 2*2 = 4.
        let placements = vec![place_in(60, 10, 30), place_in(60, 12, 30)];
        let report = quality_report(
            &problem,
            &placements,
            &[],
            &ConstraintWeights::default(),
        );
        assert_eq!(report.prefer_early_units, 4);
    }

    #[test]
    fn quality_report_avoid_first_units_match_score_helper() {
        let mut problem = three_block_one_class_problem();
        problem.subjects[0].avoid_first_period = 3;
        // Placement at position 0: units = 3.
        let placements = vec![place_in(60, 10, 30)];
        let report = quality_report(
            &problem,
            &placements,
            &[],
            &ConstraintWeights::default(),
        );
        assert_eq!(report.avoid_first_units, 3);
    }

    #[test]
    fn quality_report_avoid_last_units_match_score_helper() {
        let mut problem = three_block_one_class_problem();
        problem.subjects[0].avoid_last_period = 5;
        // Placement at position 2 (max_position_for_day_0 = 2): units = 5.
        let placements = vec![place_in(60, 12, 30)];
        let report = quality_report(
            &problem,
            &placements,
            &[],
            &ConstraintWeights::default(),
        );
        assert_eq!(report.avoid_last_units, 5);
    }

    #[test]
    fn quality_report_prefer_late_units_match_score_helper() {
        let mut problem = three_block_one_class_problem();
        problem.subjects[0].prefer_late_period = 2;
        // Placements at positions 0 and 2 (max = 2):
        // units = 2*(2-0) + 2*(2-2) = 4.
        let placements = vec![place_in(60, 10, 30), place_in(60, 12, 30)];
        let report = quality_report(
            &problem,
            &placements,
            &[],
            &ConstraintWeights::default(),
        );
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
```

- [ ] **Step 5: Run the unit tests to verify they all FAIL (red)**

Run: `cargo nextest run -p solver-core --lib quality::tests`
Expected: 10 tests reported, 9 fail with `assertion failed: report.<field> == <value>` (the stub returns `Default::default()` → all zeros). The default test passes.

- [ ] **Step 6: Implement `quality_report` to make the unit tests pass**

Replace the stub `quality_report` body in `solver/solver-core/src/quality.rs` with:

```rust
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
            subject.prefer_early_period.saturating_mul(u32::from(tb.position)),
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
        home_room_misses = home_room_misses
            .saturating_add(home_room_penalty(lesson, &home_room_lookup, p.room_id, &unit_weights));
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
        .saturating_add(weights.class_day_balance.saturating_mul(class_day_balance_cost_value))
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
```

Note: `class_day_balance_cost`, `gap_count`, `subject_preference_score`, and `home_room_penalty` are `pub(crate)` in `score.rs`; they are visible from within `solver-core::quality` without a visibility change.

- [ ] **Step 7: Run the unit tests to verify they all PASS (green)**

Run: `cargo nextest run -p solver-core --lib quality::tests`
Expected: 10 tests pass.

- [ ] **Step 8: Add the property test in a new integration test file**

Create `solver/solver-core/tests/quality_property.rs`:

```rust
//! Property test pinning the contract that
//! `quality_report(...).weighted_score == score_solution(...)` for any
//! `(Problem, placements, weights)` triple. Generator shape mirrors
//! `lahc_property::lahc_small_problem` but draws non-zero subject-axis
//! weights and an optional home_room_id so the per-axis subtotals get
//! exercised; the property still holds even when the data underlying an
//! axis is zero.

use proptest::prelude::*;
use solver_core::ids::{
    LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId,
};
use solver_core::score::score_solution;
use solver_core::types::{
    ConstraintWeights, Lesson, Placement, Problem, Room, SchoolClass, Solution, SolveConfig,
    Subject, Teacher, TeacherQualification, TimeBlock,
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
            .map(|i| SchoolClass {
                id: SchoolClassId(quality_property_id_from(2000 + i as u32)),
                home_room_id: if set_home_room { Some(rooms[0].id) } else { None },
                max_lessons_per_day: None,
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
        let expected = score_solution(&problem, placements, &weights);
        prop_assert_eq!(report.weighted_score, expected);
    }
}
```

- [ ] **Step 9: Run the property test to verify it passes at default `cases=32`**

Run: `cargo nextest run -p solver-core --test quality_property`
Expected: PASS.

- [ ] **Step 10: Run the 5×128 PROPTEST_CASES sweep before commit (per `solver/CLAUDE.md`)**

Run:

```bash
for s in 1 2 3 4 5; do
  PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test quality_property
done
```

Expected: every iteration PASS. If a seed pins a counterexample, proptest writes it to `solver/solver-core/tests/quality_property.proptest-regressions`; commit that file alongside the test and re-run the sweep to confirm the pinned regression replays cleanly.

- [ ] **Step 11: Run the lint and full solver-core test suite**

Run: `mise run lint && cargo nextest run -p solver-core`
Expected: green.

- [ ] **Step 12: Commit**

```bash
git add solver/solver-core/src/quality.rs solver/solver-core/src/lib.rs solver/solver-core/tests/quality_property.rs
git commit -m "$(cat <<'EOF'
feat(solver-core): add QualityReport component vector (item 50)

Promotes per-axis cost subtotals from score::score_solution's internals
into a public QualityReport struct living in solver-core/src/quality.rs.
Eleven fields (hard_violations, unplaced_hours, class_gap_hours,
teacher_gap_hours, class_day_balance_cost, home_room_misses, the four
subject-timing axes, and weighted_score) decompose the soft cost so
ADRs and item-51 backend rewiring can compare component vectors instead
of one collapsed scalar. score_solution is unchanged in the LAHC hot
path; the contract quality_report.weighted_score == score_solution is
pinned by a tests/quality_property.rs property test under random
weights.
EOF
)"
```

If the commit also picked up a `quality_property.proptest-regressions` file from the sweep, include it in the same commit (the file is the test's deterministic replay log).

---

## Task 2 — solver-bench: rename `QualityReport` → `QualityPredicates`

**Files:**
- Modify: `solver/solver-bench/src/quality.rs`
- Modify: `solver/solver-bench/src/main.rs`

**Inline execution.** This is a pure rename. No subagent. Touches files the main session has already loaded; spinning up an agent would be pure overhead.

- [ ] **Step 1: Rename the struct and its associated functions in `solver-bench/src/quality.rs`**

In `solver/solver-bench/src/quality.rs`:
- Replace every `pub struct QualityReport` with `pub struct QualityPredicates`.
- Replace every `pub fn evaluate_quality(` with `pub fn evaluate_quality_predicates(`.
- Update `pub fn quality_pass_count(report: &QualityReport)` to `pub fn quality_pass_count(report: &QualityPredicates)`.
- Update internal helper signatures and doc comments that name the old type (search for `QualityReport` across the file).
- Update the inline `tests` module references (the test functions construct `QualityReport { ... }` literals; rename to `QualityPredicates { ... }`).

- [ ] **Step 2: Update call sites in `solver-bench/src/main.rs`**

Search and replace within `solver/solver-bench/src/main.rs`:
- `Vec<quality::QualityReport>` → `Vec<quality::QualityPredicates>`
- `quality::QualityReport` → `quality::QualityPredicates`
- `quality::evaluate_quality(` → `quality::evaluate_quality_predicates(`
- `aggregate_quality_medians(reports: &[quality::QualityReport])` → `aggregate_quality_medians(reports: &[quality::QualityPredicates])`

- [ ] **Step 3: Run lint and bench test suite**

Run: `mise run lint && cargo nextest run -p solver-bench --bin solver-bench && cargo nextest run -p solver-bench --test end_to_end`
Expected: green.

- [ ] **Step 4: Commit**

```bash
git add solver/solver-bench/src/quality.rs solver/solver-bench/src/main.rs
git commit -m "$(cat <<'EOF'
refactor(solver-bench): rename QualityReport to QualityPredicates

Frees the QualityReport name for the new component-style import from
solver-core::quality (item 50). The bench-side struct is the
predicate-style "is this plan usable on the Grundschule bar?" report
(four Some/None fields plus thresholds); the new core-side struct is
the cost-vector breakdown that sums to soft_score. Both will coexist.
Pure rename, no semantic change.
EOF
)"
```

---

## Task 3 — solver-bench: render `QualityReport` components per backend

**Files:**
- Modify: `solver/solver-bench/src/quality.rs` (re-export the core type to make `quality::QualityReport` resolve to the new component-style report)
- Modify: `solver/solver-bench/src/main.rs` (extend `CellResult`, add `aggregate_component_medians`, render four new columns)

**Subagent dispatch.** This task lands in one subagent. Acceptance: existing tests pass post-edit, new tests pass, `cargo nextest run -p solver-bench` is green, the markdown render unit tests assert the new column headers. Commit on success: `feat(solver-bench): render QualityReport components per backend (item 50)`.

- [ ] **Step 1: Re-export `solver_core::QualityReport` from `solver-bench::quality`**

Add to the top of `solver/solver-bench/src/quality.rs`:

```rust
pub use solver_core::QualityReport;
```

This lets existing call sites that say `quality::QualityReport` continue to compile while resolving to the new component-style type. The renamed `QualityPredicates` and its functions stay as-is.

- [ ] **Step 2: Extend `CellResult` with nine new median fields**

In `solver/solver-bench/src/main.rs`, find the `CellResult` struct definition (around line 80) and add nine new fields after `quality_pass_count_median`:

```rust
unplaced_hours_median: Option<u32>,
class_gap_hours_median: Option<u32>,
teacher_gap_hours_median: Option<u32>,
class_day_balance_cost_median: Option<u32>,
home_room_misses_median: Option<u32>,
prefer_early_units_median: Option<u32>,
avoid_first_units_median: Option<u32>,
avoid_last_units_median: Option<u32>,
prefer_late_units_median: Option<u32>,
```

`hard_violations_median: u32` and `soft_score_median: Option<u32>` already exist and mirror `QualityReport.hard_violations` and `QualityReport.weighted_score` respectively; do not duplicate them.

- [ ] **Step 3: Add a `Vec<QualityReport>` accumulator + `aggregate_component_medians` helper**

In `solver/solver-bench/src/main.rs`, find the per-cell loop (around line 423 in the LAHC path and the cpsat path; both follow the same shape). After the existing `quality_reports: Vec<quality::QualityPredicates>` declaration, add:

```rust
let mut component_reports: Vec<quality::QualityReport> =
    Vec::with_capacity(seeds as usize);
```

After the existing `evaluate_quality_predicates` call inside the per-seed loop, append:

```rust
component_reports.push(solver_core::quality_report(
    problem,
    &solution.placements,
    &solution.violations,
    &PRODUCTION_ACTIVE_WEIGHTS,
));
```

Mirror the same change in the cpsat seed loop (around line 612).

Add a new aggregation helper near `aggregate_quality_medians`:

```rust
/// Nine-tuple returned by [`aggregate_component_medians`]: one entry
/// per `QualityReport` field that does not already have a CellResult
/// median (hard_violations and weighted_score are mirrored by the
/// existing `hard_violations_median` and `soft_score_median`).
type ComponentMedians = (
    Option<u32>, // unplaced_hours
    Option<u32>, // class_gap_hours
    Option<u32>, // teacher_gap_hours
    Option<u32>, // class_day_balance_cost
    Option<u32>, // home_room_misses
    Option<u32>, // prefer_early_units
    Option<u32>, // avoid_first_units
    Option<u32>, // avoid_last_units
    Option<u32>, // prefer_late_units
);

fn aggregate_component_medians(
    reports: &[solver_core::QualityReport],
) -> ComponentMedians {
    if reports.is_empty() {
        return (None, None, None, None, None, None, None, None, None);
    }
    let median = |samples: Vec<u32>| -> Option<u32> {
        if samples.is_empty() {
            None
        } else {
            let mut s = samples;
            Some(median_u32(&mut s))
        }
    };
    (
        median(reports.iter().map(|r| r.unplaced_hours).collect()),
        median(reports.iter().map(|r| r.class_gap_hours).collect()),
        median(reports.iter().map(|r| r.teacher_gap_hours).collect()),
        median(reports.iter().map(|r| r.class_day_balance_cost).collect()),
        median(reports.iter().map(|r| r.home_room_misses).collect()),
        median(reports.iter().map(|r| r.prefer_early_units).collect()),
        median(reports.iter().map(|r| r.avoid_first_units).collect()),
        median(reports.iter().map(|r| r.avoid_last_units).collect()),
        median(reports.iter().map(|r| r.prefer_late_units).collect()),
    )
}
```

- [ ] **Step 4: Wire the new medians into `CellResult` construction**

In each `CellResult { ... }` literal site (LAHC seed loop ~line 473, cpsat seed loop ~line 632, plus any test fixtures), add the nine new fields populated from `aggregate_component_medians(&component_reports)`:

```rust
let (
    unplaced_hours_median,
    class_gap_hours_median,
    teacher_gap_hours_median,
    class_day_balance_cost_median,
    home_room_misses_median,
    prefer_early_units_median,
    avoid_first_units_median,
    avoid_last_units_median,
    prefer_late_units_median,
) = aggregate_component_medians(&component_reports);
```

Add these names to the `CellResult` struct literal alongside the existing `worst_spread_median` etc. Synthesised test fixtures throughout `mod tests` need the same nine fields added; populate with `None` (or realistic synthesised values where the test asserts on them).

- [ ] **Step 5: Update `write_header` and `write_row` to render the four new columns**

In `solver/solver-bench/src/main.rs::write_header`, change the header line to include the four new columns between `Soft score (median, feasible)` and `FFD wall-clock (ms, median)`:

```rust
"| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Placements (median / expected) | Soft score (median, feasible) | Class gap h (median) | Teacher gap h (median) | Home room miss (median) | Day balance (median) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) | Peak RSS (kB) | Time to first feasible (ms, median) | Time to optimal (ms, median) | Worst spread (median) | Worst home-room ratio (median) | Total interior gaps (median) | Late-period ratio (median) | Quality (pass / 4) |\n"
```

And update the alignment row to add four `| ---: |` cells in the same positions:

```rust
"| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n"
```

In `write_row`, render the four new column values (after the existing `soft` variable, before `ffd`):

```rust
let class_gap_h = match cell.class_gap_hours_median {
    Some(v) => v.to_string(),
    None => "-".to_string(),
};
let teacher_gap_h = match cell.teacher_gap_hours_median {
    Some(v) => v.to_string(),
    None => "-".to_string(),
};
let home_room_miss = match cell.home_room_misses_median {
    Some(v) => v.to_string(),
    None => "-".to_string(),
};
let day_balance = match cell.class_day_balance_cost_median {
    Some(v) => v.to_string(),
    None => "-".to_string(),
};
```

Update the format string in `write_row` to insert the four new fields in the right column positions:

```rust
out.push_str(&format!(
    "| {fixture} | {backend} | {seeds} | {n}/{seeds} | {hard} | {placed}/{expected} | {soft} | {class_gap_h} | {teacher_gap_h} | {home_room_miss} | {day_balance} | {ffd:.2} | {total:.0} | {peak} | {ttf} | {tto} | {worst_spread} | {worst_home} | {gaps} | {late} | {quality} |\n",
    backend = backend.label(),
    seeds = cell.seeds,
    n = cell.feasibility_count,
    hard = cell.hard_violations_median,
    placed = cell.placements_total_median,
    expected = cell.placements_expected,
    ffd = cell.ffd_ms_median,
    total = cell.total_ms_median,
    peak = cell.peak_kb,
));
```

`write_error_row` does not consume the new fields; leave it alone but verify the error-row column count matches the data-row column count (count the pipes after the change; both should agree).

- [ ] **Step 6: Update existing render unit tests to assert the new headers**

Find every render-test in `solver/solver-bench/src/main.rs::tests` (search for `write_header(`, `write_row(`, `render_markdown`). Update the expected-string assertions to include the four new column headers and four new data cells. The existing tests synthesise `CellResult` literals; populate the nine new fields with `Some(<plausible value>)` for tests that assert on rendering, `None` for tests that don't.

- [ ] **Step 7: Add the JSON round-trip test for the nine new fields**

In `solver/solver-bench/src/main.rs::tests`, add:

```rust
#[test]
fn cell_result_serialises_nine_new_quality_report_medians() {
    let cell = CellResult {
        seeds: 4,
        feasibility_count: 4,
        hard_violations_median: 0,
        placements_total_median: 45,
        placements_expected: 45,
        soft_score_median: Some(90),
        ffd_ms_median: 1.0,
        total_ms_median: 2.0,
        peak_kb: 1024,
        time_to_first_feasible_ms_median: Some(1.0),
        time_to_optimal_ms_median: Some(2.0),
        worst_spread_median: Some(2),
        worst_home_room_ratio_median: Some(0.7),
        total_interior_gaps_median: Some(0),
        late_period_ratio_median: None,
        quality_pass_count_median: Some(4),
        unplaced_hours_median: Some(0),
        class_gap_hours_median: Some(1),
        teacher_gap_hours_median: Some(2),
        class_day_balance_cost_median: Some(3),
        home_room_misses_median: Some(4),
        prefer_early_units_median: Some(5),
        avoid_first_units_median: Some(6),
        avoid_last_units_median: Some(7),
        prefer_late_units_median: Some(8),
    };
    let json = serde_json::to_string(&cell).expect("serialise");
    let parsed: CellResult = serde_json::from_str(&json).expect("parse");
    assert_eq!(cell, parsed);
}

#[test]
fn aggregate_component_medians_returns_per_field_medians() {
    let reports = vec![
        solver_core::QualityReport {
            unplaced_hours: 0,
            class_gap_hours: 1,
            teacher_gap_hours: 2,
            class_day_balance_cost: 3,
            home_room_misses: 4,
            prefer_early_units: 5,
            avoid_first_units: 6,
            avoid_last_units: 7,
            prefer_late_units: 8,
            ..solver_core::QualityReport::default()
        },
        solver_core::QualityReport {
            unplaced_hours: 0,
            class_gap_hours: 3,
            teacher_gap_hours: 4,
            class_day_balance_cost: 5,
            home_room_misses: 6,
            prefer_early_units: 7,
            avoid_first_units: 8,
            avoid_last_units: 9,
            prefer_late_units: 10,
            ..solver_core::QualityReport::default()
        },
        solver_core::QualityReport {
            unplaced_hours: 0,
            class_gap_hours: 2,
            teacher_gap_hours: 3,
            class_day_balance_cost: 4,
            home_room_misses: 5,
            prefer_early_units: 6,
            avoid_first_units: 7,
            avoid_last_units: 8,
            prefer_late_units: 9,
            ..solver_core::QualityReport::default()
        },
    ];
    let medians = aggregate_component_medians(&reports);
    // Three samples sorted; median is the middle one.
    assert_eq!(medians.0, Some(0)); // unplaced_hours
    assert_eq!(medians.1, Some(2)); // class_gap_hours
    assert_eq!(medians.2, Some(3)); // teacher_gap_hours
    assert_eq!(medians.3, Some(4)); // class_day_balance_cost
    assert_eq!(medians.4, Some(5)); // home_room_misses
    assert_eq!(medians.5, Some(6)); // prefer_early_units
    assert_eq!(medians.6, Some(7)); // avoid_first_units
    assert_eq!(medians.7, Some(8)); // avoid_last_units
    assert_eq!(medians.8, Some(9)); // prefer_late_units
}

#[test]
fn aggregate_component_medians_returns_none_on_empty_input() {
    let medians = aggregate_component_medians(&[]);
    assert_eq!(medians, (None, None, None, None, None, None, None, None, None));
}
```

`CellResult` needs `Eq` and `Clone` for the assertions; if they are not derived already, add them to the `derive(...)` line.

- [ ] **Step 8: Run lint and bench suite**

Run: `mise run lint && cargo nextest run -p solver-bench --bin solver-bench && cargo nextest run -p solver-bench --test end_to_end`
Expected: green. The `end_to_end` test asserts on markdown shape; if it pins a specific column count, update its assertions to match the post-change four-extra-columns shape.

- [ ] **Step 9: Commit**

```bash
git add solver/solver-bench/src/quality.rs solver/solver-bench/src/main.rs
git commit -m "$(cat <<'EOF'
feat(solver-bench): render QualityReport components per backend (item 50)

Wires solver_core::quality_report into the per-cell loop alongside the
existing predicate-style evaluator. Adds nine new median fields to
CellResult (one per QualityReport axis that did not already have a
CellResult mirror) and renders four load-bearing columns (class gap h,
teacher gap h, home room miss, day balance) in BENCH_RESULTS.md so
cross-backend comparisons see component drift instead of one collapsed
soft score. Subject-timing axes stay JSON-only until a fixture goes
non-zero (item 12 + item 14 will surface them); promote later via the
same lazy pattern OPEN_THINGS item 43 documents.

This PR adds the rendering code and the new column headers; refreshing
the BENCH_RESULTS.md numbers at production cell shape (~5 h wall-clock
via mise run bench:bakeoff) is queued for the next maintainer-driven
refresh.
EOF
)"
```

---

## Task 4 — backend: map `quality_checks.py` kinds to `QualityReport` components

**Files:**
- Modify: `backend/src/klassenzeit_backend/scheduling/quality_checks.py` (module docstring only)

**Inline execution.** Trivial docstring update; no subagent.

- [ ] **Step 1: Replace the module docstring with the mapping table**

Edit `backend/src/klassenzeit_backend/scheduling/quality_checks.py`. Replace the existing module docstring (lines 1-5) with:

```python
"""Pure-function predicates over a generated schedule.

Used by integration tests and (eventually) by an admin-facing quality
endpoint to surface issues without re-deriving the predicate logic per
consumer.

`QualityIssue.kind` maps onto the backend-neutral `QualityReport`
(item 50) component vector exposed by `solver_core::quality_report`.
The two are not 1:1: predicates carry thresholds and per-class
shape, the report carries unweighted axis subtotals. Mapping table
for cross-language consistency:

    | QualityIssue.kind | QualityReport field      | Notes                                                               |
    | ----------------- | ------------------------ | ------------------------------------------------------------------- |
    | imbalance         | class_day_balance_cost   | Same axis. Predicate: per-class spread vs. max_spread threshold.   |
    | home_room_miss    | home_room_misses         | Same axis. Predicate: per-class ratio vs. min_ratio threshold.     |
    | interior_gap      | class_gap_hours          | Same axis. Predicate: per-class total gaps vs. threshold.          |
    | day_too_long      | avoid_last_units (loose) | Closest soft component. Predicate's max_position is sharper.        |
    | room_hop          | (none)                   | Hard constraint, pruned via solver_core::validate_no_room_hopping.  |

The teacher-gap axis (`QualityReport.teacher_gap_hours`) and the four
subject-timing axes (`prefer_early_units`, `avoid_first_units`,
`avoid_last_units`, `prefer_late_units`) have no QualityIssue today;
add new `kind` literals if a future integration test or admin endpoint
needs to report on them.
"""
```

- [ ] **Step 2: Run backend lint and tests**

Run: `mise run lint && uv run pytest backend/tests/scheduling/test_grundschule_schedule_quality.py`
Expected: green. The docstring change does not affect runtime behaviour.

- [ ] **Step 3: Commit**

```bash
git add backend/src/klassenzeit_backend/scheduling/quality_checks.py
git commit -m "$(cat <<'EOF'
docs(backend): map quality_checks.py kinds to QualityReport (item 50)

Adds the cross-language mapping table from QualityIssue.kind Literal
values to solver_core::QualityReport component fields, completing item
50's "backend quality_checks.py names map to the same dimensions"
acceptance criterion. Documentation-only; no rename, no behaviour
change. The teacher-gap and subject-timing axes have no QualityIssue
counterpart today; the docstring flags this so future kinds get added
deliberately rather than re-derived.
EOF
)"
```

---

## Self-review checklist

Spec coverage:
- Task 1 covers spec deliverables: `solver-core/src/quality.rs`, `solver-core/src/lib.rs` re-export, `tests/quality_property.rs`, all eleven unit tests, the property test, the 5×128 sweep.
- Task 2 covers the rename deliverable.
- Task 3 covers the bench rendering deliverable: nine new `CellResult` median fields, `aggregate_component_medians`, four new markdown columns, JSON round-trip + median helper tests.
- Task 4 covers the backend docstring mapping.
- Spec deliverable "delete OPEN_THINGS item 50 entry; cross-reference cleanups" + "auto-memory roadmap update" + "solver/CLAUDE.md entry" are handled in the autopilot finalize step (step 6 of `.claude/commands/autopilot.md`), not in the implementation plan.

Type consistency:
- `quality_report` signature matches between Task 1 (definition), Task 1 step 8 (property test caller), and Task 3 step 3 (bench caller).
- Field names match between `QualityReport` (Task 1), `quality_report` body (Task 1 step 6), `aggregate_component_medians` (Task 3 step 3), and the JSON round-trip test (Task 3 step 7).
- `QualityPredicates` and `evaluate_quality_predicates` names match between Task 2 step 1 (rename) and Task 3 step 3 (preserved call site for the predicate path).
- Existing `CellResult` field names (`hard_violations_median`, `soft_score_median`) carry forward unchanged into the JSON round-trip test (Task 3 step 7).

Placeholder scan: zero "TBD" / "TODO" / vague directives. Every step has the actual code or command.
