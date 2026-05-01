//! Pure soft-score function for `Solution` placements. Used by the lowest-delta
//! greedy in `solve.rs` and by the future LAHC local search.

use std::collections::HashMap;

use crate::ids::{LessonId, RoomId, SchoolClassId, TeacherId, TimeBlockId};
use crate::types::{ConstraintWeights, Lesson, Placement, Problem, TimeBlock};

/// Compute the total weighted soft-score for a placement set.
///
/// Partitions `placements` by `(school_class, day_of_week)` and
/// `(teacher_id, day_of_week)`, then sums weighted gap-hours per partition.
/// Multi-class lessons contribute one entry per member class to the
/// class-day partition.
pub fn score_solution(
    problem: &Problem,
    placements: &[Placement],
    weights: &ConstraintWeights,
) -> u32 {
    if weights.class_gap == 0
        && weights.teacher_gap == 0
        && weights.prefer_early_period == 0
        && weights.avoid_first_period == 0
        && weights.prefer_home_room == 0
        && weights.avoid_last_period == 0
    {
        return 0;
    }
    let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
    let lesson_lookup: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let subject_lookup: std::collections::HashMap<crate::ids::SubjectId, &crate::types::Subject> =
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

    let class_gaps: u32 = by_class_day
        .into_values()
        .map(|mut v| {
            v.sort_unstable();
            v.dedup();
            gap_count(&v)
        })
        .sum();
    let teacher_gaps: u32 = by_teacher_day
        .into_values()
        .map(|mut v| {
            v.sort_unstable();
            v.dedup();
            gap_count(&v)
        })
        .sum();

    let subject_preference: u32 = placements
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

    let home_room_total: u32 = placements
        .iter()
        .map(|p| {
            let lesson = lesson_lookup[&p.lesson_id];
            home_room_penalty(lesson, &home_room_lookup, p.room_id, weights)
        })
        .sum();

    weights
        .class_gap
        .saturating_mul(class_gaps)
        .saturating_add(weights.teacher_gap.saturating_mul(teacher_gaps))
        .saturating_add(subject_preference)
        .saturating_add(home_room_total)
}

/// Count gap-hours in a sorted, deduplicated `positions` slice. A gap-hour is
/// an ordinal strictly between `positions.first()` and `positions.last()` that
/// does not appear in `positions`.
pub(crate) fn gap_count(positions: &[u8]) -> u32 {
    if positions.len() < 2 {
        return 0;
    }
    let span = u32::from(*positions.last().unwrap() - *positions.first().unwrap());
    let count = u32::try_from(positions.len()).unwrap_or(u32::MAX);
    span + 1 - count
}

/// Count gap-hours in `positions` after inserting `pos`. Returns 0 when the
/// resulting slice would have fewer than two distinct positions. When `pos` is
/// already present the length is unchanged (deduplication); when absent the
/// length grows by one. Caller must pass a sorted, deduplicated slice.
pub(crate) fn gap_count_after_insert(positions: Option<&Vec<u8>>, pos: u8) -> u32 {
    let Some(positions) = positions else {
        return 0;
    };
    if positions.is_empty() {
        return 0;
    }
    let already_present = positions.binary_search(&pos).is_ok();
    let len_after = if already_present {
        positions.len()
    } else {
        positions.len() + 1
    };
    if len_after < 2 {
        return 0;
    }
    let first = *positions.first().unwrap();
    let last = *positions.last().unwrap();
    let new_min = first.min(pos);
    let new_max = last.max(pos);
    let span = u32::from(new_max - new_min);
    let count = u32::try_from(len_after).unwrap_or(u32::MAX);
    span + 1 - count
}

/// Count gap-hours in `positions` after removing `pos`. Symmetric to
/// `gap_count_after_insert`. Returns 0 if removal leaves fewer than two
/// elements; returns `gap_count(positions)` if `pos` is not present
/// (defensive: LAHC only removes positions it has just placed, so the absent
/// branch should never fire in production).
pub(crate) fn gap_count_after_remove(positions: &[u8], pos: u8) -> u32 {
    let Ok(removed_at) = positions.binary_search(&pos) else {
        return gap_count(positions);
    };
    let len_after = positions.len() - 1;
    if len_after < 2 {
        return 0;
    }
    let new_first = if removed_at == 0 {
        positions[1]
    } else {
        positions[0]
    };
    let new_last = if removed_at == positions.len() - 1 {
        positions[positions.len() - 2]
    } else {
        positions[positions.len() - 1]
    };
    let span = u32::from(new_last - new_first);
    let count = u32::try_from(len_after).unwrap_or(u32::MAX);
    span + 1 - count
}

/// Per-placement subject-preference score. Returns
/// `tb.position * weights.prefer_early_period` (linear) when the subject's
/// `prefer_early_periods` flag is set, plus `weights.avoid_first_period`
/// (binary) when the `avoid_first_period` flag is set and `tb.position == 0`,
/// plus `weights.avoid_last_period` (binary) when the `avoid_last_period`
/// flag is set and `tb.position == max_position_for_day`. Pure: depends only
/// on `subject`, `tb`, `max_position_for_day`, `weights`. Allocation-free.
pub(crate) fn subject_preference_score(
    subject: &crate::types::Subject,
    tb: &TimeBlock,
    max_position_for_day: u8,
    weights: &ConstraintWeights,
) -> u32 {
    let mut score = 0u32;
    if subject.prefer_early_periods {
        score = score
            .saturating_add(u32::from(tb.position).saturating_mul(weights.prefer_early_period));
    }
    if subject.avoid_first_period && tb.position == 0 {
        score = score.saturating_add(weights.avoid_first_period);
    }
    if subject.avoid_last_period && tb.position == max_position_for_day {
        score = score.saturating_add(weights.avoid_last_period);
    }
    score
}

/// Per-placement home-room penalty. Returns `weights.prefer_home_room` once
/// per class in `lesson.school_class_ids` whose `home_room_id` is set and
/// does not match `placement_room_id`. Returns 0 when
/// `weights.prefer_home_room == 0`. Pure: depends only on the inputs;
/// allocation-free.
pub(crate) fn home_room_penalty(
    lesson: &Lesson,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    placement_room_id: RoomId,
    weights: &ConstraintWeights,
) -> u32 {
    if weights.prefer_home_room == 0 {
        return 0;
    }
    let mut score = 0u32;
    for class_id in &lesson.school_class_ids {
        if let Some(Some(home_id)) = home_room_lookup.get(class_id) {
            if *home_id != placement_room_id {
                score = score.saturating_add(weights.prefer_home_room);
            }
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
    use crate::types::{
        Lesson, Placement, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        TimeBlock,
    };
    use uuid::Uuid;

    fn score_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn three_block_one_class_problem() -> Problem {
        Problem {
            time_blocks: vec![
                TimeBlock {
                    id: TimeBlockId(score_uuid(10)),
                    day_of_week: 0,
                    position: 0,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(12)),
                    day_of_week: 0,
                    position: 2,
                },
            ],
            teachers: vec![Teacher {
                id: TeacherId(score_uuid(20)),
                max_hours_per_week: 10,
            }],
            rooms: vec![Room {
                id: RoomId(score_uuid(30)),
            }],
            subjects: vec![Subject {
                id: SubjectId(score_uuid(40)),
                prefer_early_periods: false,
                avoid_first_period: false,
                avoid_last_period: false,
            }],
            school_classes: vec![SchoolClass {
                id: SchoolClassId(score_uuid(50)),
                home_room_id: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(score_uuid(60)),
                school_class_ids: vec![SchoolClassId(score_uuid(50))],
                subject_id: SubjectId(score_uuid(40)),
                teacher_id: TeacherId(score_uuid(20)),
                hours_per_week: 2,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: TeacherId(score_uuid(20)),
                subject_id: SubjectId(score_uuid(40)),
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
        }
    }

    fn place(lesson_id: u8, tb_id: u8) -> Placement {
        Placement {
            lesson_id: LessonId(score_uuid(lesson_id)),
            time_block_id: TimeBlockId(score_uuid(tb_id)),
            room_id: RoomId(score_uuid(30)),
        }
    }

    #[test]
    fn empty_placements_score_zero() {
        let p = three_block_one_class_problem();
        let weights = ConstraintWeights {
            class_gap: 5,
            teacher_gap: 7,
            ..ConstraintWeights::default()
        };
        assert_eq!(score_solution(&p, &[], &weights), 0);
    }

    #[test]
    fn single_placement_scores_zero() {
        let p = three_block_one_class_problem();
        let weights = ConstraintWeights {
            class_gap: 5,
            teacher_gap: 7,
            ..ConstraintWeights::default()
        };
        let placements = [place(60, 10)];
        assert_eq!(score_solution(&p, &placements, &weights), 0);
    }

    #[test]
    fn contiguous_placements_score_zero() {
        let p = three_block_one_class_problem();
        let weights = ConstraintWeights {
            class_gap: 5,
            teacher_gap: 7,
            ..ConstraintWeights::default()
        };
        let placements = [place(60, 10), place(60, 11)];
        assert_eq!(score_solution(&p, &placements, &weights), 0);
    }

    #[test]
    fn one_gap_scores_class_plus_teacher_weights() {
        // Class 50 and teacher 20 both have placements at positions 0 and 2 with
        // a gap at position 1. Each partition contributes one gap-hour.
        let p = three_block_one_class_problem();
        let weights = ConstraintWeights {
            class_gap: 5,
            teacher_gap: 7,
            ..ConstraintWeights::default()
        };
        let placements = [place(60, 10), place(60, 12)];
        assert_eq!(score_solution(&p, &placements, &weights), 12);
    }

    #[test]
    fn weights_compose_linearly() {
        let p = three_block_one_class_problem();
        let placements = [place(60, 10), place(60, 12)];
        let w1 = ConstraintWeights {
            class_gap: 1,
            teacher_gap: 0,
            ..ConstraintWeights::default()
        };
        let w2 = ConstraintWeights {
            class_gap: 2,
            teacher_gap: 0,
            ..ConstraintWeights::default()
        };
        assert_eq!(score_solution(&p, &placements, &w1), 1);
        assert_eq!(score_solution(&p, &placements, &w2), 2);
    }

    #[test]
    fn cross_day_placements_do_not_combine() {
        let mut p = three_block_one_class_problem();
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(score_uuid(13)),
            day_of_week: 1,
            position: 0,
        });
        let weights = ConstraintWeights {
            class_gap: 5,
            teacher_gap: 7,
            ..ConstraintWeights::default()
        };
        let placements = [place(60, 10), place(60, 13)];
        assert_eq!(score_solution(&p, &placements, &weights), 0);
    }

    #[test]
    fn zero_weights_short_circuit_to_zero() {
        let p = three_block_one_class_problem();
        let weights = ConstraintWeights::default();
        let placements = [place(60, 10), place(60, 12)];
        assert_eq!(score_solution(&p, &placements, &weights), 0);
    }

    #[test]
    fn gap_count_after_remove_single_element_returns_zero() {
        let positions = [3u8];
        assert_eq!(gap_count_after_remove(&positions, 3), 0);
    }

    #[test]
    fn gap_count_after_remove_min_shrinks_span() {
        // positions = [1, 3, 5]; gap_count = 5 - 1 + 1 - 3 = 2
        // remove 1 -> [3, 5]; gap_count = 5 - 3 + 1 - 2 = 1
        let positions = [1u8, 3, 5];
        assert_eq!(gap_count_after_remove(&positions, 1), 1);
    }

    #[test]
    fn gap_count_after_remove_max_shrinks_span() {
        // positions = [1, 3, 5]; remove 5 -> [1, 3]; gap = 3 - 1 + 1 - 2 = 1
        let positions = [1u8, 3, 5];
        assert_eq!(gap_count_after_remove(&positions, 5), 1);
    }

    #[test]
    fn gap_count_after_remove_middle_grows_gap() {
        // positions = [1, 3, 5]; remove 3 -> [1, 5]; gap = 5 - 1 + 1 - 2 = 3
        let positions = [1u8, 3, 5];
        assert_eq!(gap_count_after_remove(&positions, 3), 3);
    }

    #[test]
    fn gap_count_after_remove_absent_returns_unchanged() {
        // pos not in slice; defensive return matches gap_count(positions).
        let positions = [1u8, 3, 5];
        assert_eq!(gap_count_after_remove(&positions, 7), gap_count(&positions));
    }

    #[test]
    fn gap_count_after_remove_two_to_one_returns_zero() {
        let positions = [1u8, 3];
        assert_eq!(gap_count_after_remove(&positions, 1), 0);
    }

    #[test]
    fn subject_preference_score_returns_zero_when_flags_off() {
        let subject = Subject {
            id: SubjectId(score_uuid(40)),
            prefer_early_periods: false,
            avoid_first_period: false,
            avoid_last_period: false,
        };
        let tb = TimeBlock {
            id: TimeBlockId(score_uuid(10)),
            day_of_week: 0,
            position: 3,
        };
        let weights = ConstraintWeights {
            prefer_early_period: 5,
            avoid_first_period: 7,
            ..ConstraintWeights::default()
        };
        assert_eq!(subject_preference_score(&subject, &tb, 5, &weights), 0);
    }

    #[test]
    fn subject_preference_score_linear_in_position_when_prefer_early_set() {
        let subject = Subject {
            id: SubjectId(score_uuid(40)),
            prefer_early_periods: true,
            avoid_first_period: false,
            avoid_last_period: false,
        };
        let weights = ConstraintWeights {
            prefer_early_period: 3,
            ..ConstraintWeights::default()
        };
        for pos in 0u8..7 {
            let tb = TimeBlock {
                id: TimeBlockId(score_uuid(10)),
                day_of_week: 0,
                position: pos,
            };
            assert_eq!(
                subject_preference_score(&subject, &tb, 6, &weights),
                u32::from(pos) * 3
            );
        }
    }

    #[test]
    fn subject_preference_score_constant_at_position_zero_when_avoid_first_set() {
        let subject = Subject {
            id: SubjectId(score_uuid(40)),
            prefer_early_periods: false,
            avoid_first_period: true,
            avoid_last_period: false,
        };
        let weights = ConstraintWeights {
            avoid_first_period: 9,
            ..ConstraintWeights::default()
        };
        let tb_zero = TimeBlock {
            id: TimeBlockId(score_uuid(10)),
            day_of_week: 0,
            position: 0,
        };
        let tb_nonzero = TimeBlock {
            id: TimeBlockId(score_uuid(11)),
            day_of_week: 0,
            position: 1,
        };
        assert_eq!(subject_preference_score(&subject, &tb_zero, 1, &weights), 9);
        assert_eq!(
            subject_preference_score(&subject, &tb_nonzero, 1, &weights),
            0
        );
    }

    #[test]
    fn subject_preference_score_constant_at_max_position_when_avoid_last_set() {
        let subject = Subject {
            id: SubjectId(score_uuid(40)),
            prefer_early_periods: false,
            avoid_first_period: false,
            avoid_last_period: true,
        };
        let weights = ConstraintWeights {
            avoid_last_period: 11,
            ..ConstraintWeights::default()
        };
        let tb_max = TimeBlock {
            id: TimeBlockId(score_uuid(11)),
            day_of_week: 0,
            position: 4,
        };
        let tb_non_max = TimeBlock {
            id: TimeBlockId(score_uuid(10)),
            day_of_week: 0,
            position: 3,
        };
        assert_eq!(subject_preference_score(&subject, &tb_max, 4, &weights), 11);
        assert_eq!(
            subject_preference_score(&subject, &tb_non_max, 4, &weights),
            0
        );
    }

    fn one_class_two_block_problem_with_flagged_subject(
        prefer_early: bool,
        avoid_first: bool,
        avoid_last: bool,
    ) -> Problem {
        Problem {
            time_blocks: vec![
                TimeBlock {
                    id: TimeBlockId(score_uuid(10)),
                    day_of_week: 0,
                    position: 0,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                },
            ],
            teachers: vec![Teacher {
                id: TeacherId(score_uuid(20)),
                max_hours_per_week: 10,
            }],
            rooms: vec![Room {
                id: RoomId(score_uuid(30)),
            }],
            subjects: vec![Subject {
                id: SubjectId(score_uuid(40)),
                prefer_early_periods: prefer_early,
                avoid_first_period: avoid_first,
                avoid_last_period: avoid_last,
            }],
            school_classes: vec![SchoolClass {
                id: SchoolClassId(score_uuid(50)),
                home_room_id: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(score_uuid(60)),
                school_class_ids: vec![SchoolClassId(score_uuid(50))],
                subject_id: SubjectId(score_uuid(40)),
                teacher_id: TeacherId(score_uuid(20)),
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: TeacherId(score_uuid(20)),
                subject_id: SubjectId(score_uuid(40)),
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
        }
    }

    #[test]
    fn score_solution_includes_prefer_early_per_placement() {
        let p = one_class_two_block_problem_with_flagged_subject(true, false, false);
        let weights = ConstraintWeights {
            prefer_early_period: 2,
            ..ConstraintWeights::default()
        };
        // Lesson placed at position 1: contribution = 1 * 2 = 2.
        let placements = [Placement {
            lesson_id: LessonId(score_uuid(60)),
            time_block_id: TimeBlockId(score_uuid(11)),
            room_id: RoomId(score_uuid(30)),
        }];
        assert_eq!(score_solution(&p, &placements, &weights), 2);
    }

    #[test]
    fn score_solution_includes_avoid_first_only_at_position_zero() {
        let p = one_class_two_block_problem_with_flagged_subject(false, true, false);
        let weights = ConstraintWeights {
            avoid_first_period: 7,
            ..ConstraintWeights::default()
        };
        // At position 0: contribution = 7.
        let placements_at_zero = [Placement {
            lesson_id: LessonId(score_uuid(60)),
            time_block_id: TimeBlockId(score_uuid(10)),
            room_id: RoomId(score_uuid(30)),
        }];
        assert_eq!(score_solution(&p, &placements_at_zero, &weights), 7);
        // At position 1: contribution = 0.
        let placements_at_one = [Placement {
            lesson_id: LessonId(score_uuid(60)),
            time_block_id: TimeBlockId(score_uuid(11)),
            room_id: RoomId(score_uuid(30)),
        }];
        assert_eq!(score_solution(&p, &placements_at_one, &weights), 0);
    }

    #[test]
    fn score_solution_zero_with_subject_flags_off_matches_pre_9c_score() {
        let p = three_block_one_class_problem();
        let weights = ConstraintWeights {
            class_gap: 5,
            teacher_gap: 7,
            prefer_early_period: 100,
            avoid_first_period: 100,
            prefer_home_room: 0,
            avoid_last_period: 100,
        };
        // Subject in three_block_one_class_problem has both flags false (default
        // after task 1.1's literal updates). The new axes contribute 0; total
        // matches the pre-9c gap-only score of 12 (one gap each in class + teacher
        // partitions, weights 5 and 7).
        let placements = [place(60, 10), place(60, 12)];
        assert_eq!(score_solution(&p, &placements, &weights), 12);
    }

    #[test]
    fn home_room_penalty_returns_zero_when_weight_is_zero() {
        let class_id = SchoolClassId(score_uuid(50));
        let lesson = Lesson {
            id: LessonId(score_uuid(60)),
            school_class_ids: vec![class_id],
            subject_id: SubjectId(score_uuid(40)),
            teacher_id: TeacherId(score_uuid(20)),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };
        let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
        lookup.insert(class_id, Some(RoomId(score_uuid(99))));
        let weights = ConstraintWeights {
            prefer_home_room: 0,
            ..ConstraintWeights::default()
        };
        let penalty = home_room_penalty(&lesson, &lookup, RoomId(score_uuid(30)), &weights);
        assert_eq!(penalty, 0);
    }

    #[test]
    fn home_room_penalty_returns_zero_when_class_has_no_home_room() {
        let class_id = SchoolClassId(score_uuid(50));
        let lesson = Lesson {
            id: LessonId(score_uuid(60)),
            school_class_ids: vec![class_id],
            subject_id: SubjectId(score_uuid(40)),
            teacher_id: TeacherId(score_uuid(20)),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };
        let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
        lookup.insert(class_id, None);
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let penalty = home_room_penalty(&lesson, &lookup, RoomId(score_uuid(30)), &weights);
        assert_eq!(penalty, 0);
    }

    #[test]
    fn home_room_penalty_returns_zero_when_room_matches_home_room() {
        let class_id = SchoolClassId(score_uuid(50));
        let home_room = RoomId(score_uuid(30));
        let lesson = Lesson {
            id: LessonId(score_uuid(60)),
            school_class_ids: vec![class_id],
            subject_id: SubjectId(score_uuid(40)),
            teacher_id: TeacherId(score_uuid(20)),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };
        let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
        lookup.insert(class_id, Some(home_room));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let penalty = home_room_penalty(&lesson, &lookup, home_room, &weights);
        assert_eq!(penalty, 0);
    }

    #[test]
    fn home_room_penalty_returns_weight_when_room_differs_from_home_room() {
        let class_id = SchoolClassId(score_uuid(50));
        let home_room = RoomId(score_uuid(30));
        let other_room = RoomId(score_uuid(31));
        let lesson = Lesson {
            id: LessonId(score_uuid(60)),
            school_class_ids: vec![class_id],
            subject_id: SubjectId(score_uuid(40)),
            teacher_id: TeacherId(score_uuid(20)),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };
        let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
        lookup.insert(class_id, Some(home_room));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let penalty = home_room_penalty(&lesson, &lookup, other_room, &weights);
        assert_eq!(penalty, 5);
    }

    #[test]
    fn home_room_penalty_sums_per_member_for_multi_class_lessons() {
        let c1 = SchoolClassId(score_uuid(50));
        let c2 = SchoolClassId(score_uuid(51));
        let c3 = SchoolClassId(score_uuid(52));
        let r1 = RoomId(score_uuid(30));
        let r2 = RoomId(score_uuid(31));
        let r3 = RoomId(score_uuid(32));
        let r_other = RoomId(score_uuid(33));
        let lesson = Lesson {
            id: LessonId(score_uuid(60)),
            school_class_ids: vec![c1, c2, c3],
            subject_id: SubjectId(score_uuid(40)),
            teacher_id: TeacherId(score_uuid(20)),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };
        let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
        lookup.insert(c1, Some(r1));
        lookup.insert(c2, Some(r2));
        lookup.insert(c3, Some(r3));
        let weights = ConstraintWeights {
            prefer_home_room: 4,
            ..ConstraintWeights::default()
        };
        // Placement in r_other: every class is mismatched, total = 3 * 4 = 12.
        assert_eq!(home_room_penalty(&lesson, &lookup, r_other, &weights), 12);
        // Placement in r1: only c2 and c3 are mismatched, total = 2 * 4 = 8.
        assert_eq!(home_room_penalty(&lesson, &lookup, r1, &weights), 8);
    }

    #[test]
    fn score_solution_includes_home_room_penalty_per_class() {
        // Class 50 has a home room (uuid 30); placement in non-home room 31
        // contributes weights.prefer_home_room = 7. Class 51 (added below)
        // has no home room, so it contributes 0 (regardless of room).
        let mut p = three_block_one_class_problem();
        let class2 = SchoolClassId(score_uuid(51));
        p.school_classes.push(SchoolClass {
            id: class2,
            home_room_id: None,
        });
        p.school_classes[0].home_room_id = Some(RoomId(score_uuid(30)));
        p.rooms.push(Room {
            id: RoomId(score_uuid(31)),
        });
        let weights = ConstraintWeights {
            prefer_home_room: 7,
            ..ConstraintWeights::default()
        };
        let placements = [Placement {
            lesson_id: LessonId(score_uuid(60)),
            time_block_id: TimeBlockId(score_uuid(10)),
            room_id: RoomId(score_uuid(31)),
        }];
        assert_eq!(score_solution(&p, &placements, &weights), 7);
    }

    #[test]
    fn score_solution_zero_when_only_home_room_weight_set_and_no_home_rooms() {
        // No SchoolClass has a home room; the prefer_home_room weight produces 0.
        let p = three_block_one_class_problem();
        let weights = ConstraintWeights {
            prefer_home_room: 10,
            ..ConstraintWeights::default()
        };
        let placements = [place(60, 10), place(60, 12)];
        assert_eq!(score_solution(&p, &placements, &weights), 0);
    }

    #[test]
    fn subject_preference_score_sums_when_both_flags_on_at_position_zero() {
        let subject = Subject {
            id: SubjectId(score_uuid(40)),
            prefer_early_periods: true,
            avoid_first_period: true,
            avoid_last_period: false,
        };
        let weights = ConstraintWeights {
            prefer_early_period: 2,
            avoid_first_period: 5,
            ..ConstraintWeights::default()
        };
        let tb_zero = TimeBlock {
            id: TimeBlockId(score_uuid(10)),
            day_of_week: 0,
            position: 0,
        };
        let tb_two = TimeBlock {
            id: TimeBlockId(score_uuid(11)),
            day_of_week: 0,
            position: 2,
        };
        // Position 0: prefer_early contributes 0, avoid_first contributes 5; total 5.
        assert_eq!(subject_preference_score(&subject, &tb_zero, 2, &weights), 5);
        // Position 2: prefer_early contributes 4, avoid_first contributes 0; total 4.
        assert_eq!(subject_preference_score(&subject, &tb_two, 2, &weights), 4);
    }

    #[test]
    fn score_solution_includes_avoid_last_only_at_max_day_position() {
        // Two-day fixture: day 0 maxes at position 1, day 1 maxes at position 2.
        // The avoid-last-flagged subject placed at (day 0, pos 1), (day 0, pos 0),
        // (day 1, pos 2), (day 1, pos 1) fires the penalty exactly twice.
        let weights = ConstraintWeights {
            avoid_last_period: 3,
            ..ConstraintWeights::default()
        };
        let subject_id = SubjectId(score_uuid(40));
        let class_id = SchoolClassId(score_uuid(50));
        let teacher_id = TeacherId(score_uuid(20));
        let lesson_id = LessonId(score_uuid(60));
        let room_id = RoomId(score_uuid(30));
        let problem = Problem {
            time_blocks: vec![
                TimeBlock {
                    id: TimeBlockId(score_uuid(10)),
                    day_of_week: 0,
                    position: 0,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(12)),
                    day_of_week: 1,
                    position: 0,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(13)),
                    day_of_week: 1,
                    position: 1,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(14)),
                    day_of_week: 1,
                    position: 2,
                },
            ],
            teachers: vec![Teacher {
                id: teacher_id,
                max_hours_per_week: 10,
            }],
            rooms: vec![Room { id: room_id }],
            subjects: vec![Subject {
                id: subject_id,
                prefer_early_periods: false,
                avoid_first_period: false,
                avoid_last_period: true,
            }],
            school_classes: vec![SchoolClass {
                id: class_id,
                home_room_id: None,
            }],
            lessons: vec![Lesson {
                id: lesson_id,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_id,
                hours_per_week: 4,
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
        };
        let p = |tb: u8| Placement {
            lesson_id,
            time_block_id: TimeBlockId(score_uuid(tb)),
            room_id,
        };
        // p(11) is day 0 max (pos 1); p(14) is day 1 max (pos 2). Two hits at
        // weight 3 = 6.
        let placements = [p(10), p(11), p(12), p(14)];
        assert_eq!(score_solution(&problem, &placements, &weights), 6);
    }
}
