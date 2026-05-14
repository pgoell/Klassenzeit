//! Pure soft-score function for `Solution` placements. Used by the lowest-delta
//! greedy in `solve.rs` and by the future LAHC local search.

use std::collections::{HashMap, HashSet};

use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use crate::types::{
    ConstraintWeights, Lesson, Placement, Problem, SchoolClass, Subject, TimeBlock,
};

/// Sum the slice-axis weighted soft-score (class_gap plus teacher_gap plus
/// subject_preference) from already-built lookups and partitions.
///
/// Shared by `score_solution` (which folds in the canonical-extra axes) and
/// `slice_recompute` (which exposes the slice as a standalone scalar for
/// LAHC's block-Change and Swap recompute paths). `by_class_day`'s vec values
/// must be sorted and deduplicated by the caller; `by_teacher_day`'s are
/// deduped inside the helper because they are still needed in raw form by
/// callers that hold them by reference.
#[allow(clippy::too_many_arguments)]
// Reason: internal helper called by two sites with identical pre-built
// lookups; threading them through avoids re-building HashMaps on every call.
fn slice_costs_inner(
    placements: &[Placement],
    weights: &ConstraintWeights,
    by_class_day: &HashMap<(SchoolClassId, u8), Vec<u8>>,
    by_teacher_day: &HashMap<(TeacherId, u8), Vec<u8>>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    subject_lookup: &HashMap<SubjectId, &Subject>,
    max_position_per_day: &HashMap<u8, u8>,
) -> u32 {
    let class_gaps: u32 = by_class_day.values().map(|v| gap_count(v)).sum();
    let teacher_gaps: u32 = by_teacher_day
        .values()
        .map(|v| {
            let mut deduped = v.clone();
            deduped.sort_unstable();
            deduped.dedup();
            gap_count(&deduped)
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
    weights
        .class_gap
        .saturating_mul(class_gaps)
        .saturating_add(weights.teacher_gap.saturating_mul(teacher_gaps))
        .saturating_add(subject_preference)
}

/// Recompute the slice score (class_gap plus teacher_gap plus subject_pref)
/// from `placements` without consulting `GreedyState`. Used by LAHC's
/// `try_change_block_move` (n>1 path) and `try_swap_move`, where the multi-
/// position / multi-class delta is too tangled for an incremental update.
/// Behaviour: equals the slice component of `score_solution(...)`; builds the
/// same lookups and partitions, then delegates to `slice_costs_inner`.
pub(crate) fn slice_recompute(
    problem: &Problem,
    placements: &[Placement],
    weights: &ConstraintWeights,
) -> u32 {
    let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
    let lesson_lookup: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let subject_lookup: HashMap<SubjectId, &Subject> =
        problem.subjects.iter().map(|s| (s.id, s)).collect();
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
        for cid in &lesson.school_class_ids {
            by_class_day
                .entry((*cid, tb.day_of_week))
                .or_default()
                .push(tb.position);
        }
        by_teacher_day
            .entry((p.teacher_id, tb.day_of_week))
            .or_default()
            .push(tb.position);
    }
    for v in by_class_day.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    slice_costs_inner(
        placements,
        weights,
        &by_class_day,
        &by_teacher_day,
        &tb_lookup,
        &lesson_lookup,
        &subject_lookup,
        &max_position_per_day,
    )
}

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
    soft_pinned_blocks: &HashSet<(LessonId, TimeBlockId)>,
) -> u32 {
    if weights.class_gap == 0
        && weights.teacher_gap == 0
        && weights.prefer_early_period == 0
        && weights.avoid_first_period == 0
        && weights.prefer_home_room == 0
        && weights.avoid_last_period == 0
        && weights.prefer_late_period == 0
        && weights.class_day_balance == 0
        && weights.prefer_class_teacher == 0
        && weights.max_per_class_spread == 0
        && weights.max_per_class_interior_gaps == 0
        && weights.supervision_spread == 0
        && weights.soft_pin_miss == 0
    {
        return 0;
    }
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

    // Dedup `by_class_day` Vec values in-place by sort+dedup so a single
    // (class, day, position) slot counts once regardless of how many
    // lesson-group co-placed lessons sit on it. Mirrors the per-class
    // partition shape that LAHC's `state.class_positions` maintains
    // (`apply_change_move` / `apply_kempe`'s dedup-on-insert guard at
    // `lahc.rs::apply_change_move`'s class-positions update site). The
    // dedup must happen BEFORE `class_day_balance_cost` so its `.len()`
    // count matches the per-class-day partition cardinality, not the
    // raw placement count. Without this, `score_solution`'s
    // `class_day_balance` over-counts lesson-group placements (3 per
    // trio slot) while LAHC's `class_day_balance_cost_for_class` delta
    // arithmetic reads the dedup'd count (1 per trio slot); the
    // mismatch surfaces as a per-iteration `state.canonical_score`
    // drift over Change-move iterations (OPEN_THINGS item 76).
    for v in by_class_day.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    let class_balance = class_day_balance_cost(&by_class_day, &problem.school_classes, days);
    let slice = slice_costs_inner(
        placements,
        weights,
        &by_class_day,
        &by_teacher_day,
        &tb_lookup,
        &lesson_lookup,
        &subject_lookup,
        &max_position_per_day,
    );

    let home_room_total: u32 = placements
        .iter()
        .map(|p| {
            let lesson = lesson_lookup[&p.lesson_id];
            home_room_penalty(lesson, &home_room_lookup, p.room_id, weights)
        })
        .sum();

    // prefer_class_teacher: count one miss per (class, subject) pair
    // whose class.class_teacher_id is qualified for the subject but the
    // first-encountered placement teacher is not the class teacher.
    // Closed-form mirror of `quality::quality_report`'s computation; keep
    // these two in lockstep so the property test stays green. Item 67.
    let mut subject_qualified_teachers: HashMap<SubjectId, HashSet<TeacherId>> = HashMap::new();
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

    let supervision_score =
        weights
            .supervision_spread
            .saturating_mul(crate::supervision::compute_supervision_spread(
                problem, placements,
            ));

    // Soft-pin miss count: one per `(lesson_id, time_block_id)` entry in
    // `soft_pinned_blocks` that is not present in the solution's placement
    // key set. Positioned alongside `prefer_home_room` (both per-placement
    // aspirational axes). Allocation-free when the soft-pin set is empty:
    // the placement-keys HashSet still allocates here but the cold-path
    // call-site cost is bounded by `placements.len()`. See ADR 0042.
    let soft_pin_miss_count: u32 = if weights.soft_pin_miss == 0 || soft_pinned_blocks.is_empty() {
        0
    } else {
        let placement_keys: HashSet<(LessonId, TimeBlockId)> = placements
            .iter()
            .map(|p| (p.lesson_id, p.time_block_id))
            .collect();
        soft_pinned_blocks
            .iter()
            .filter(|key| !placement_keys.contains(key))
            .count() as u32
    };

    slice
        .saturating_add(weights.class_day_balance.saturating_mul(class_balance))
        .saturating_add(
            weights
                .max_per_class_spread
                .saturating_mul(worst_class_spread(problem, placements)),
        )
        .saturating_add(
            weights
                .max_per_class_interior_gaps
                .saturating_mul(worst_class_interior_gaps(problem, placements)),
        )
        .saturating_add(home_room_total)
        .saturating_add(weights.soft_pin_miss.saturating_mul(soft_pin_miss_count))
        .saturating_add(
            weights
                .prefer_class_teacher
                .saturating_mul(prefer_class_teacher_misses),
        )
        .saturating_add(supervision_score)
}

/// Worst per-class daily-load spread:
/// `max over classes of (max(daily_count) - min(daily_count))`
/// where `daily_count` is the dedup'd per-(class, day) placement count.
/// Mirrors the bench predicate `solver_bench::quality::worst_class_day_spread`:
/// counts run over `day_of_week in 0..5` for each class that has any
/// placement, treating zero-placement days as 0 in the `min`. Classes with
/// zero total placements contribute 0 (they do not appear in the
/// inner-class map). Item 57.
pub(crate) fn worst_class_spread(problem: &Problem, placements: &[Placement]) -> u32 {
    let tb_day: HashMap<TimeBlockId, u8> = problem
        .time_blocks
        .iter()
        .map(|tb| (tb.id, tb.day_of_week))
        .collect();
    let lesson_classes: HashMap<LessonId, &Vec<SchoolClassId>> = problem
        .lessons
        .iter()
        .map(|l| (l.id, &l.school_class_ids))
        .collect();
    // Bench predicate uses a fixed-width [0; 5] per-class day array; mirror.
    let mut counts: HashMap<SchoolClassId, [u32; 5]> = HashMap::new();
    for placement in placements {
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

/// Worst per-class interior gaps:
/// `max over classes of (sum over days of interior_gaps_in_day)`.
/// Mirrors the per-(class, day) gap definition of
/// `solver_bench::quality::total_interior_gaps` but aggregates per-class
/// (max-over-classes) rather than the bench predicate's cross-class sum,
/// so the new axis bounds the WORST class, not the cumulative total.
/// Item 57.
pub(crate) fn worst_class_interior_gaps(problem: &Problem, placements: &[Placement]) -> u32 {
    let tb_meta: HashMap<TimeBlockId, (u8, u8)> = problem
        .time_blocks
        .iter()
        .map(|tb| (tb.id, (tb.day_of_week, tb.position)))
        .collect();
    let lesson_classes: HashMap<LessonId, &Vec<SchoolClassId>> = problem
        .lessons
        .iter()
        .map(|l| (l.id, &l.school_class_ids))
        .collect();
    let mut positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    for placement in placements {
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
    let mut per_class: HashMap<SchoolClassId, u32> = HashMap::new();
    for ((class_id, _day), ps) in positions.iter_mut() {
        ps.sort_unstable();
        ps.dedup();
        if let (Some(&first), Some(&last)) = (ps.first(), ps.last()) {
            let span = u32::from(last - first + 1);
            let gaps = span.saturating_sub(ps.len() as u32);
            *per_class.entry(*class_id).or_insert(0) = per_class
                .get(class_id)
                .copied()
                .unwrap_or(0)
                .saturating_add(gaps);
        }
    }
    per_class.values().copied().max().unwrap_or(0)
}

/// L1 distance from per-day mean placement count, summed across classes.
/// Cost for one class is `sum over days of |c[day] * D - sum| / D` where
/// `D` is the day count and `sum` is the class's total placements; the
/// scaling by `D` keeps integer arithmetic precise (a perfectly even
/// spread cancels exactly to zero). A class with zero placements
/// contributes zero. Unweighted: caller multiplies by
/// `weights.class_day_balance`.
///
/// Pure: depends only on the inputs. Allocation: one small per-class
/// `Vec<u32>` of length `days` (typically `days <= 7`). Acceptable
/// because `score_solution` is invoked per full evaluation, not per
/// candidate placement; the placement-time hot paths in `solve.rs` and
/// `lahc.rs` use granular delta helpers and never call this function.
pub(crate) fn class_day_balance_cost(
    by_class_day: &HashMap<(SchoolClassId, u8), Vec<u8>>,
    classes: &[SchoolClass],
    days: u8,
) -> u32 {
    if days == 0 {
        return 0;
    }
    let mut total: u32 = 0;
    let d = u32::from(days);
    for class in classes {
        let mut sum: u32 = 0;
        let mut counts: Vec<u32> = Vec::with_capacity(usize::from(days));
        for day in 0..days {
            let c = by_class_day
                .get(&(class.id, day))
                .map(|v| v.len() as u32)
                .unwrap_or(0);
            counts.push(c);
            sum = sum.saturating_add(c);
        }
        if sum == 0 {
            continue;
        }
        let mut scaled: u32 = 0;
        for c in &counts {
            let lhs = c.saturating_mul(d);
            scaled = scaled.saturating_add(lhs.abs_diff(sum));
        }
        total = total.saturating_add(scaled / d);
    }
    total
}

/// Per-class scaled L1 day-balance cost. Walks the class's per-day counts
/// twice (sum, then scaled), no allocation, returns the unweighted cost
/// for the single class. Caller multiplies by `weights.class_day_balance`.
/// Used by LAHC Change-move and Kempe delta paths so the canonical
/// objective stays incrementally maintained without allocating
/// `Vec<u32>(days)` per call. The cold-path `class_day_balance_cost`
/// equals the sum of this helper across `problem.school_classes`.
pub(crate) fn class_day_balance_cost_for_class(
    class_id: SchoolClassId,
    days: u8,
    class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>,
) -> u32 {
    if days == 0 {
        return 0;
    }
    let d = u32::from(days);
    let mut sum: u32 = 0;
    for day in 0..days {
        sum = sum.saturating_add(
            class_positions
                .get(&(class_id, day))
                .map(|v| v.len() as u32)
                .unwrap_or(0),
        );
    }
    if sum == 0 {
        return 0;
    }
    let mut scaled: u32 = 0;
    for day in 0..days {
        let c = class_positions
            .get(&(class_id, day))
            .map(|v| v.len() as u32)
            .unwrap_or(0);
        scaled = scaled.saturating_add(c.saturating_mul(d).abs_diff(sum));
    }
    scaled / d
}

/// Variant of `class_day_balance_cost_for_class` that overlays a virtual
/// move of one count from `old_day` to `new_day` (single placement swap)
/// without mutating `class_positions`. Returns the per-class scaled L1
/// cost as if the move had been applied. Used by LAHC Change-move's
/// canonical delta to compute pre/post for one class without allocation.
pub(crate) fn class_day_balance_cost_for_class_with_swap(
    class_id: SchoolClassId,
    days: u8,
    class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>,
    old_day: u8,
    new_day: u8,
) -> u32 {
    if days == 0 || old_day == new_day {
        return class_day_balance_cost_for_class(class_id, days, class_positions);
    }
    let d = u32::from(days);
    let mut sum: u32 = 0;
    for day in 0..days {
        sum = sum.saturating_add(
            class_positions
                .get(&(class_id, day))
                .map(|v| v.len() as u32)
                .unwrap_or(0),
        );
    }
    if sum == 0 {
        return 0;
    }
    let mut scaled: u32 = 0;
    for day in 0..days {
        let raw = class_positions
            .get(&(class_id, day))
            .map(|v| v.len() as u32)
            .unwrap_or(0);
        let c = if day == old_day {
            raw.saturating_sub(1)
        } else if day == new_day {
            raw.saturating_add(1)
        } else {
            raw
        };
        scaled = scaled.saturating_add(c.saturating_mul(d).abs_diff(sum));
    }
    scaled / d
}

/// Variant of `class_day_balance_cost_for_class` that overlays a virtual
/// addition of `add_n` placements on `add_day` for `class_id`, without
/// mutating `class_positions`. Returns the per-class scaled L1 cost as if
/// the addition had been applied. Used by FFD greedy's `try_place_block`
/// and `try_place_group` window pickers to rank candidates by post-place
/// class-day-balance contribution alongside the existing slice and
/// home-room terms (item 54). Allocation-free; walks `0..days` twice.
pub(crate) fn class_day_balance_cost_for_class_after_add(
    class_id: SchoolClassId,
    days: u8,
    class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>,
    add_day: u8,
    add_n: u8,
) -> u32 {
    if days == 0 {
        return 0;
    }
    let d = u32::from(days);
    let added = u32::from(add_n);
    let mut sum: u32 = 0;
    for day in 0..days {
        sum = sum.saturating_add(
            class_positions
                .get(&(class_id, day))
                .map(|v| v.len() as u32)
                .unwrap_or(0),
        );
    }
    sum = sum.saturating_add(added);
    if sum == 0 {
        return 0;
    }
    let mut scaled: u32 = 0;
    for day in 0..days {
        let raw = class_positions
            .get(&(class_id, day))
            .map(|v| v.len() as u32)
            .unwrap_or(0);
        let c = if day == add_day {
            raw.saturating_add(added)
        } else {
            raw
        };
        scaled = scaled.saturating_add(c.saturating_mul(d).abs_diff(sum));
    }
    scaled / d
}

/// Per-class scaled L1 day-balance cost computed from a pre-captured
/// counts vector (`counts[day] = placements_for_class_on_day`).
/// Caller supplies the counts; useful when the canonical delta needs
/// the cost against a snapshot that is no longer in `class_positions`
/// (for example Kempe's pre-apply snapshot).
pub(crate) fn class_day_balance_cost_for_class_from_counts(
    _class_id: SchoolClassId,
    days: u8,
    counts: &[u32],
) -> u32 {
    if days == 0 || counts.is_empty() {
        return 0;
    }
    let d = u32::from(days);
    let mut sum: u32 = 0;
    for c in counts.iter().take(usize::from(days)) {
        sum = sum.saturating_add(*c);
    }
    if sum == 0 {
        return 0;
    }
    let mut scaled: u32 = 0;
    for c in counts.iter().take(usize::from(days)) {
        scaled = scaled.saturating_add(c.saturating_mul(d).abs_diff(sum));
    }
    scaled / d
}

/// Per-class home_room penalty. Returns `weights.prefer_home_room` if
/// the class has a `home_room_id` and the given `room_id` differs;
/// 0 otherwise. Used by LAHC Change-move and Kempe canonical deltas
/// where the per-row home_room contribution is needed without
/// re-walking `lesson.school_class_ids` inside the existing
/// `home_room_penalty(lesson, ...)` path. Pure, allocation-free.
pub(crate) fn home_room_penalty_one_class(
    class_id: SchoolClassId,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    room_id: RoomId,
    weights: &ConstraintWeights,
) -> u32 {
    if weights.prefer_home_room == 0 {
        return 0;
    }
    if let Some(Some(home_id)) = home_room_lookup.get(&class_id) {
        if *home_id != room_id {
            return weights.prefer_home_room;
        }
    }
    0
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
/// `tb.position * weights.prefer_early_period * subject.prefer_early_period`
/// (linear, weighted by `subject.prefer_early_period`), plus
/// `weights.avoid_first_period * subject.avoid_first_period` when
/// `tb.position == 0`, plus
/// `weights.avoid_last_period * subject.avoid_last_period` when
/// `tb.position == max_position_for_day`, plus
/// `(max_position_for_day - tb.position) * weights.prefer_late_period * subject.prefer_late_period`
/// for the late-period axis. Each per-Subject weight of zero disables its
/// axis. Pure: depends only on `subject`, `tb`, `max_position_for_day`,
/// `weights`. Allocation-free.
pub(crate) fn subject_preference_score(
    subject: &crate::types::Subject,
    tb: &TimeBlock,
    max_position_for_day: u8,
    weights: &ConstraintWeights,
) -> u32 {
    let mut score = 0u32;
    if subject.prefer_early_period > 0 {
        score = score.saturating_add(
            weights
                .prefer_early_period
                .saturating_mul(subject.prefer_early_period)
                .saturating_mul(u32::from(tb.position)),
        );
    }
    if subject.avoid_first_period > 0 && tb.position == 0 {
        score = score.saturating_add(
            weights
                .avoid_first_period
                .saturating_mul(subject.avoid_first_period),
        );
    }
    if subject.avoid_last_period > 0 && tb.position == max_position_for_day {
        score = score.saturating_add(
            weights
                .avoid_last_period
                .saturating_mul(subject.avoid_last_period),
        );
    }
    if subject.prefer_late_period > 0 && weights.prefer_late_period > 0 {
        let distance = u32::from(max_position_for_day.saturating_sub(tb.position));
        score = score.saturating_add(
            weights
                .prefer_late_period
                .saturating_mul(subject.prefer_late_period)
                .saturating_mul(distance),
        );
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
        TimeBlock, TimeBlockKind,
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
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(12)),
                    day_of_week: 0,
                    position: 2,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![Teacher {
                id: TeacherId(score_uuid(20)),
                max_hours_per_week: 10,
                reserve_hours_per_week: 0,
            }],
            rooms: vec![Room {
                id: RoomId(score_uuid(30)),
            }],
            subjects: vec![Subject {
                id: SubjectId(score_uuid(40)),
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![SchoolClass {
                id: SchoolClassId(score_uuid(50)),
                home_room_id: None,
                max_lessons_per_day: None,
                class_teacher_id: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(score_uuid(60)),
                school_class_ids: vec![SchoolClassId(score_uuid(50))],
                subject_id: SubjectId(score_uuid(40)),
                teacher_candidates: vec![TeacherId(score_uuid(20))],
                teacher_pin: Some(TeacherId(score_uuid(20))),
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
            pinned_placements: vec![],
        }
    }

    fn place(lesson_id: u8, tb_id: u8) -> Placement {
        Placement {
            lesson_id: LessonId(score_uuid(lesson_id)),
            time_block_id: TimeBlockId(score_uuid(tb_id)),
            room_id: RoomId(score_uuid(30)),
            teacher_id: TeacherId(score_uuid(20)),
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
        assert_eq!(
            score_solution(&p, &[], &weights, &::std::collections::HashSet::new()),
            0
        );
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
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            0
        );
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
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            0
        );
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
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            12
        );
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
        assert_eq!(
            score_solution(&p, &placements, &w1, &::std::collections::HashSet::new()),
            1
        );
        assert_eq!(
            score_solution(&p, &placements, &w2, &::std::collections::HashSet::new()),
            2
        );
    }

    #[test]
    fn cross_day_placements_do_not_combine() {
        let mut p = three_block_one_class_problem();
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(score_uuid(13)),
            day_of_week: 1,
            position: 0,
            kind: TimeBlockKind::Lesson,
        });
        let weights = ConstraintWeights {
            class_gap: 5,
            teacher_gap: 7,
            ..ConstraintWeights::default()
        };
        let placements = [place(60, 10), place(60, 13)];
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            0
        );
    }

    #[test]
    fn zero_weights_short_circuit_to_zero() {
        let p = three_block_one_class_problem();
        let weights = ConstraintWeights::default();
        let placements = [place(60, 10), place(60, 12)];
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            0
        );
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
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        let tb = TimeBlock {
            id: TimeBlockId(score_uuid(10)),
            day_of_week: 0,
            position: 3,
            kind: TimeBlockKind::Lesson,
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
            prefer_early_period: 1,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
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
                kind: TimeBlockKind::Lesson,
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
            prefer_early_period: 0,
            avoid_first_period: 1,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        let weights = ConstraintWeights {
            avoid_first_period: 9,
            ..ConstraintWeights::default()
        };
        let tb_zero = TimeBlock {
            id: TimeBlockId(score_uuid(10)),
            day_of_week: 0,
            position: 0,
            kind: TimeBlockKind::Lesson,
        };
        let tb_nonzero = TimeBlock {
            id: TimeBlockId(score_uuid(11)),
            day_of_week: 0,
            position: 1,
            kind: TimeBlockKind::Lesson,
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
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 1,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        let weights = ConstraintWeights {
            avoid_last_period: 11,
            ..ConstraintWeights::default()
        };
        let tb_max = TimeBlock {
            id: TimeBlockId(score_uuid(11)),
            day_of_week: 0,
            position: 4,
            kind: TimeBlockKind::Lesson,
        };
        let tb_non_max = TimeBlock {
            id: TimeBlockId(score_uuid(10)),
            day_of_week: 0,
            position: 3,
            kind: TimeBlockKind::Lesson,
        };
        assert_eq!(subject_preference_score(&subject, &tb_max, 4, &weights), 11);
        assert_eq!(
            subject_preference_score(&subject, &tb_non_max, 4, &weights),
            0
        );
    }

    fn one_class_two_block_problem_with_flagged_subject(
        prefer_early: u32,
        avoid_first: u32,
        avoid_last: u32,
    ) -> Problem {
        Problem {
            time_blocks: vec![
                TimeBlock {
                    id: TimeBlockId(score_uuid(10)),
                    day_of_week: 0,
                    position: 0,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![Teacher {
                id: TeacherId(score_uuid(20)),
                max_hours_per_week: 10,
                reserve_hours_per_week: 0,
            }],
            rooms: vec![Room {
                id: RoomId(score_uuid(30)),
            }],
            subjects: vec![Subject {
                id: SubjectId(score_uuid(40)),
                prefer_early_period: prefer_early,
                avoid_first_period: avoid_first,
                avoid_last_period: avoid_last,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![SchoolClass {
                id: SchoolClassId(score_uuid(50)),
                home_room_id: None,
                max_lessons_per_day: None,
                class_teacher_id: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(score_uuid(60)),
                school_class_ids: vec![SchoolClassId(score_uuid(50))],
                subject_id: SubjectId(score_uuid(40)),
                teacher_candidates: vec![TeacherId(score_uuid(20))],
                teacher_pin: Some(TeacherId(score_uuid(20))),
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
            pinned_placements: vec![],
        }
    }

    #[test]
    fn score_solution_includes_prefer_early_per_placement() {
        let p = one_class_two_block_problem_with_flagged_subject(1, 0, 0);
        let weights = ConstraintWeights {
            prefer_early_period: 2,
            ..ConstraintWeights::default()
        };
        // Lesson placed at position 1: contribution = 1 * 2 = 2.
        let placements = [Placement {
            lesson_id: LessonId(score_uuid(60)),
            time_block_id: TimeBlockId(score_uuid(11)),
            room_id: RoomId(score_uuid(30)),
            teacher_id: TeacherId(Uuid::nil()),
        }];
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            2
        );
    }

    #[test]
    fn score_solution_includes_avoid_first_only_at_position_zero() {
        let p = one_class_two_block_problem_with_flagged_subject(0, 1, 0);
        let weights = ConstraintWeights {
            avoid_first_period: 7,
            ..ConstraintWeights::default()
        };
        // At position 0: contribution = 7.
        let placements_at_zero = [Placement {
            lesson_id: LessonId(score_uuid(60)),
            time_block_id: TimeBlockId(score_uuid(10)),
            room_id: RoomId(score_uuid(30)),
            teacher_id: TeacherId(Uuid::nil()),
        }];
        assert_eq!(
            score_solution(
                &p,
                &placements_at_zero,
                &weights,
                &::std::collections::HashSet::new()
            ),
            7
        );
        // At position 1: contribution = 0.
        let placements_at_one = [Placement {
            lesson_id: LessonId(score_uuid(60)),
            time_block_id: TimeBlockId(score_uuid(11)),
            room_id: RoomId(score_uuid(30)),
            teacher_id: TeacherId(Uuid::nil()),
        }];
        assert_eq!(
            score_solution(
                &p,
                &placements_at_one,
                &weights,
                &::std::collections::HashSet::new()
            ),
            0
        );
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
            prefer_late_period: 0,
            class_day_balance: 0,
            prefer_class_teacher: 0,
            max_per_class_spread: 0,
            max_per_class_interior_gaps: 0,
            supervision_spread: 0,
            soft_pin_miss: 0,
        };
        // Subject in three_block_one_class_problem has both flags false (default
        // after task 1.1's literal updates). The new axes contribute 0; total
        // matches the pre-9c gap-only score of 12 (one gap each in class + teacher
        // partitions, weights 5 and 7).
        let placements = [place(60, 10), place(60, 12)];
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            12
        );
    }

    #[test]
    fn home_room_penalty_returns_zero_when_weight_is_zero() {
        let class_id = SchoolClassId(score_uuid(50));
        let lesson = Lesson {
            id: LessonId(score_uuid(60)),
            school_class_ids: vec![class_id],
            subject_id: SubjectId(score_uuid(40)),
            teacher_candidates: vec![TeacherId(score_uuid(20))],
            teacher_pin: Some(TeacherId(score_uuid(20))),
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
            teacher_candidates: vec![TeacherId(score_uuid(20))],
            teacher_pin: Some(TeacherId(score_uuid(20))),
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
            teacher_candidates: vec![TeacherId(score_uuid(20))],
            teacher_pin: Some(TeacherId(score_uuid(20))),
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
            teacher_candidates: vec![TeacherId(score_uuid(20))],
            teacher_pin: Some(TeacherId(score_uuid(20))),
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
            teacher_candidates: vec![TeacherId(score_uuid(20))],
            teacher_pin: Some(TeacherId(score_uuid(20))),
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
            max_lessons_per_day: None,
            class_teacher_id: None,
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
            teacher_id: TeacherId(Uuid::nil()),
        }];
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            7
        );
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
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            0
        );
    }

    #[test]
    fn subject_preference_score_sums_when_both_flags_on_at_position_zero() {
        let subject = Subject {
            id: SubjectId(score_uuid(40)),
            prefer_early_period: 1,
            avoid_first_period: 1,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
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
            kind: TimeBlockKind::Lesson,
        };
        let tb_two = TimeBlock {
            id: TimeBlockId(score_uuid(11)),
            day_of_week: 0,
            position: 2,
            kind: TimeBlockKind::Lesson,
        };
        // Position 0: prefer_early contributes 0, avoid_first contributes 5; total 5.
        assert_eq!(subject_preference_score(&subject, &tb_zero, 2, &weights), 5);
        // Position 2: prefer_early contributes 4, avoid_first contributes 0; total 4.
        assert_eq!(subject_preference_score(&subject, &tb_two, 2, &weights), 4);
    }

    #[test]
    fn subject_preference_score_scales_linearly_with_prefer_early_subject_weight() {
        let weights = ConstraintWeights {
            prefer_early_period: 2,
            ..ConstraintWeights::default()
        };
        let tb = TimeBlock {
            id: TimeBlockId(uuid::Uuid::nil()),
            day_of_week: 0,
            position: 3,
            kind: TimeBlockKind::Lesson,
        };
        let mk = |w: u32| Subject {
            id: SubjectId(uuid::Uuid::nil()),
            prefer_early_period: w,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        // single-weight = 2 * 3 * 1 = 6, double-weight = 2 * 3 * 2 = 12
        assert_eq!(subject_preference_score(&mk(1), &tb, 6, &weights), 6);
        assert_eq!(subject_preference_score(&mk(2), &tb, 6, &weights), 12);
        assert_eq!(subject_preference_score(&mk(0), &tb, 6, &weights), 0);
    }

    #[test]
    fn subject_preference_score_scales_linearly_with_avoid_first_subject_weight() {
        let weights = ConstraintWeights {
            avoid_first_period: 5,
            ..ConstraintWeights::default()
        };
        let tb = TimeBlock {
            id: TimeBlockId(uuid::Uuid::nil()),
            day_of_week: 0,
            position: 0,
            kind: TimeBlockKind::Lesson,
        };
        let mk = |w: u32| Subject {
            id: SubjectId(uuid::Uuid::nil()),
            prefer_early_period: 0,
            avoid_first_period: w,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        assert_eq!(subject_preference_score(&mk(1), &tb, 6, &weights), 5);
        assert_eq!(subject_preference_score(&mk(3), &tb, 6, &weights), 15);
        assert_eq!(subject_preference_score(&mk(0), &tb, 6, &weights), 0);
    }

    #[test]
    fn subject_preference_score_scales_linearly_with_avoid_last_subject_weight() {
        let weights = ConstraintWeights {
            avoid_last_period: 4,
            ..ConstraintWeights::default()
        };
        let tb = TimeBlock {
            id: TimeBlockId(uuid::Uuid::nil()),
            day_of_week: 0,
            position: 6,
            kind: TimeBlockKind::Lesson,
        };
        let mk = |w: u32| Subject {
            id: SubjectId(uuid::Uuid::nil()),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: w,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        assert_eq!(subject_preference_score(&mk(1), &tb, 6, &weights), 4);
        assert_eq!(subject_preference_score(&mk(2), &tb, 6, &weights), 8);
        assert_eq!(subject_preference_score(&mk(0), &tb, 6, &weights), 0);
    }

    #[test]
    fn subject_preference_score_linear_in_distance_from_max_when_prefer_late_set() {
        let weights = ConstraintWeights {
            prefer_late_period: 4,
            ..ConstraintWeights::default()
        };
        let mk_subject = |w: u32| Subject {
            id: SubjectId(score_uuid(40)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: w,
            max_hours_per_day: 8,
        };
        // max_position_for_day = 5; pos 0 contributes 5 * 4 * 1 = 20,
        // pos 5 contributes 0.
        for pos in 0u8..=5 {
            let tb = TimeBlock {
                id: TimeBlockId(score_uuid(10)),
                day_of_week: 0,
                position: pos,
                kind: TimeBlockKind::Lesson,
            };
            assert_eq!(
                subject_preference_score(&mk_subject(1), &tb, 5, &weights),
                u32::from(5 - pos) * 4
            );
        }
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
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(12)),
                    day_of_week: 1,
                    position: 0,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(13)),
                    day_of_week: 1,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(14)),
                    day_of_week: 1,
                    position: 2,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![Teacher {
                id: teacher_id,
                max_hours_per_week: 10,
                reserve_hours_per_week: 0,
            }],
            rooms: vec![Room { id: room_id }],
            subjects: vec![Subject {
                id: subject_id,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 1,
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
            pinned_placements: vec![],
        };
        let p = |tb: u8| Placement {
            lesson_id,
            time_block_id: TimeBlockId(score_uuid(tb)),
            room_id,
            teacher_id: TeacherId(Uuid::nil()),
        };
        // p(11) is day 0 max (pos 1); p(14) is day 1 max (pos 2). Two hits at
        // weight 3 = 6.
        let placements = [p(10), p(11), p(12), p(14)];
        assert_eq!(
            score_solution(
                &problem,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            6
        );
    }

    #[test]
    fn class_day_balance_zero_for_perfectly_even_spread() {
        // 4 placements over 4 days = 1/1/1/1, balance cost = 0.
        let mut p = three_block_one_class_problem();
        for day in 1..=3u8 {
            p.time_blocks.push(TimeBlock {
                id: TimeBlockId(score_uuid(20 + day)),
                day_of_week: day,
                position: 0,
                kind: TimeBlockKind::Lesson,
            });
        }
        let weights = ConstraintWeights {
            class_day_balance: 5,
            ..ConstraintWeights::default()
        };
        let placements = [place(60, 10), place(60, 21), place(60, 22), place(60, 23)];
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            0
        );
    }

    #[test]
    fn class_day_balance_penalises_lopsided_spread() {
        // 4 placements all on day 0 over 4 days = 4/0/0/0; cost = 6, weighted = 30.
        let mut p = three_block_one_class_problem();
        for day in 1..=3u8 {
            p.time_blocks.push(TimeBlock {
                id: TimeBlockId(score_uuid(20 + day)),
                day_of_week: day,
                position: 0,
                kind: TimeBlockKind::Lesson,
            });
        }
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(score_uuid(13)),
            day_of_week: 0,
            position: 3,
            kind: TimeBlockKind::Lesson,
        });
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(score_uuid(14)),
            day_of_week: 0,
            position: 4,
            kind: TimeBlockKind::Lesson,
        });
        let weights = ConstraintWeights {
            class_day_balance: 5,
            ..ConstraintWeights::default()
        };
        let placements = [place(60, 10), place(60, 11), place(60, 12), place(60, 13)];
        assert_eq!(
            score_solution(
                &p,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            30
        );
    }

    #[test]
    fn class_day_balance_cost_for_class_matches_full_cost_per_class() {
        let class_a = SchoolClassId(score_uuid(91));
        let class_b = SchoolClassId(score_uuid(92));
        let mut by_class_day: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        by_class_day.insert((class_a, 0), vec![0, 1, 2]);
        by_class_day.insert((class_a, 1), vec![0]);
        by_class_day.insert((class_b, 2), vec![0, 1]);
        let classes = vec![
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
        ];
        let days: u8 = 5;
        let total = class_day_balance_cost(&by_class_day, &classes, days);
        let per_class_a = class_day_balance_cost_for_class(class_a, days, &by_class_day);
        let per_class_b = class_day_balance_cost_for_class(class_b, days, &by_class_day);
        assert_eq!(per_class_a + per_class_b, total);
    }

    #[test]
    fn class_day_balance_cost_for_class_with_swap_matches_post_apply_recompute() {
        let class = SchoolClassId(score_uuid(93));
        let mut pre: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        pre.insert((class, 0), vec![0, 1, 2]);
        pre.insert((class, 2), vec![0]);
        let predicted = class_day_balance_cost_for_class_with_swap(class, 5, &pre, 0, 3);
        let mut post = pre.clone();
        let day0 = post.get_mut(&(class, 0)).unwrap();
        day0.pop();
        post.entry((class, 3)).or_default().push(0);
        let actual = class_day_balance_cost_for_class(class, 5, &post);
        assert_eq!(predicted, actual);
    }

    #[test]
    fn class_day_balance_cost_for_class_after_add_matches_post_apply_recompute() {
        let class = SchoolClassId(score_uuid(97));
        let mut pre: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        pre.insert((class, 0), vec![0, 1, 2]);
        pre.insert((class, 2), vec![0]);
        // Predict the cost as if we appended 2 placements on day 1 (currently empty).
        let predicted = class_day_balance_cost_for_class_after_add(class, 5, &pre, 1, 2);
        let mut post = pre.clone();
        let day1 = post.entry((class, 1)).or_default();
        day1.push(0);
        day1.push(1);
        let actual = class_day_balance_cost_for_class(class, 5, &post);
        assert_eq!(predicted, actual);
    }

    #[test]
    fn class_day_balance_cost_for_class_after_add_returns_zero_for_zero_days() {
        let class = SchoolClassId(score_uuid(98));
        let positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        assert_eq!(
            class_day_balance_cost_for_class_after_add(class, 0, &positions, 0, 1),
            0
        );
    }

    #[test]
    fn class_day_balance_cost_for_class_after_add_grows_lopsided_total() {
        // Existing partition: 3 placements all on day 0; days = 4. Adding one more
        // on day 0 should raise the per-class cost; adding one on day 1 should
        // raise it less (pulls toward balance). Both bounded by the unweighted
        // helper's formula.
        let class = SchoolClassId(score_uuid(99));
        let mut positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        positions.insert((class, 0), vec![0, 1, 2]);
        let cost_add_to_packed =
            class_day_balance_cost_for_class_after_add(class, 4, &positions, 0, 1);
        let cost_add_to_empty =
            class_day_balance_cost_for_class_after_add(class, 4, &positions, 1, 1);
        assert!(
            cost_add_to_empty < cost_add_to_packed,
            "adding to an empty day should not increase imbalance more than adding to the packed day; \
             empty={cost_add_to_empty} packed={cost_add_to_packed}"
        );
    }

    #[test]
    fn home_room_penalty_one_class_matches_lesson_walk_for_single_class_lesson() {
        let class = SchoolClassId(score_uuid(94));
        let home = RoomId(score_uuid(95));
        let other = RoomId(score_uuid(96));
        let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
        lookup.insert(class, Some(home));
        let weights = ConstraintWeights {
            prefer_home_room: 7,
            ..ConstraintWeights::default()
        };
        assert_eq!(
            home_room_penalty_one_class(class, &lookup, other, &weights),
            7
        );
        assert_eq!(
            home_room_penalty_one_class(class, &lookup, home, &weights),
            0
        );
    }

    /// Build a one-class, one-subject Problem with two teachers (T1 + T2).
    /// `klt_qualified` flips whether `T1` is in the subject's qualified
    /// teacher set; `class_teacher` is `class.class_teacher_id`. Used by
    /// the prefer_class_teacher unit tests below.
    fn prefer_class_teacher_test_problem(
        klt_qualified: bool,
        class_teacher: Option<TeacherId>,
    ) -> Problem {
        let t1 = TeacherId(score_uuid(20));
        let t2 = TeacherId(score_uuid(21));
        let subject_id = SubjectId(score_uuid(40));
        let class_id = SchoolClassId(score_uuid(50));
        let lesson_id = LessonId(score_uuid(60));
        let room_id = RoomId(score_uuid(30));
        let mut qualifications = vec![TeacherQualification {
            teacher_id: t2,
            subject_id,
        }];
        if klt_qualified {
            qualifications.push(TeacherQualification {
                teacher_id: t1,
                subject_id,
            });
        }
        Problem {
            time_blocks: vec![
                TimeBlock {
                    id: TimeBlockId(score_uuid(10)),
                    day_of_week: 0,
                    position: 0,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: TimeBlockId(score_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![
                Teacher {
                    id: t1,
                    max_hours_per_week: 10,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: t2,
                    max_hours_per_week: 10,
                    reserve_hours_per_week: 0,
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
            school_classes: vec![SchoolClass {
                id: class_id,
                home_room_id: None,
                max_lessons_per_day: None,
                class_teacher_id: class_teacher,
            }],
            lessons: vec![Lesson {
                id: lesson_id,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_candidates: vec![t1, t2],
                teacher_pin: None,
                hours_per_week: 2,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: qualifications,
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        }
    }

    fn prefer_class_teacher_test_make_placements_with_teacher(
        teacher: TeacherId,
    ) -> Vec<Placement> {
        vec![
            Placement {
                lesson_id: LessonId(score_uuid(60)),
                time_block_id: TimeBlockId(score_uuid(10)),
                room_id: RoomId(score_uuid(30)),
                teacher_id: teacher,
            },
            Placement {
                lesson_id: LessonId(score_uuid(60)),
                time_block_id: TimeBlockId(score_uuid(11)),
                room_id: RoomId(score_uuid(30)),
                teacher_id: teacher,
            },
        ]
    }

    #[test]
    fn score_solution_includes_prefer_class_teacher_when_klt_qualified_and_mismatched() {
        let t1 = TeacherId(score_uuid(20));
        let t2 = TeacherId(score_uuid(21));
        let problem = prefer_class_teacher_test_problem(true, Some(t1));
        let placements = prefer_class_teacher_test_make_placements_with_teacher(t2);
        let weights = ConstraintWeights {
            prefer_class_teacher: 5,
            ..ConstraintWeights::default()
        };
        // First-encountered teacher for (class, subject) is T2; class_teacher
        // is T1, qualified for subject; one miss * weight 5 = 5.
        assert_eq!(
            score_solution(
                &problem,
                &placements,
                &weights,
                &::std::collections::HashSet::new()
            ),
            5
        );
    }

    #[test]
    fn score_solution_zero_prefer_class_teacher_when_klt_assigned_matches() {
        let t1 = TeacherId(score_uuid(20));
        let problem = prefer_class_teacher_test_problem(true, Some(t1));
        let placements = prefer_class_teacher_test_make_placements_with_teacher(t1);
        let weights_on = ConstraintWeights {
            prefer_class_teacher: 5,
            ..ConstraintWeights::default()
        };
        let weights_off = ConstraintWeights {
            prefer_class_teacher: 0,
            ..ConstraintWeights::default()
        };
        assert_eq!(
            score_solution(
                &problem,
                &placements,
                &weights_on,
                &::std::collections::HashSet::new()
            ),
            score_solution(
                &problem,
                &placements,
                &weights_off,
                &::std::collections::HashSet::new()
            ),
        );
    }

    #[test]
    fn score_solution_zero_prefer_class_teacher_when_klt_not_qualified() {
        let t1 = TeacherId(score_uuid(20));
        let t2 = TeacherId(score_uuid(21));
        // klt_qualified=false: T1 is NOT in subject's qualified teacher set,
        // so even though placements use T2 (mismatched), no miss is counted.
        let problem = prefer_class_teacher_test_problem(false, Some(t1));
        let placements = prefer_class_teacher_test_make_placements_with_teacher(t2);
        let weights_on = ConstraintWeights {
            prefer_class_teacher: 5,
            ..ConstraintWeights::default()
        };
        let weights_off = ConstraintWeights {
            prefer_class_teacher: 0,
            ..ConstraintWeights::default()
        };
        assert_eq!(
            score_solution(
                &problem,
                &placements,
                &weights_on,
                &::std::collections::HashSet::new()
            ),
            score_solution(
                &problem,
                &placements,
                &weights_off,
                &::std::collections::HashSet::new()
            ),
        );
    }

    // ---- item 57: per-class worst-case axes ----

    /// Build a synthetic Problem with two classes, 5 weekdays, `width`
    /// positions per day, one subject per class, one teacher per class.
    /// Time-block ids are deterministic per `(day, position)` so test
    /// helpers can mint Placements without consulting the problem. The
    /// single subject / teacher / room arrangement and `class_teacher_id:
    /// None` keep the `prefer_class_teacher` axis inert (short-circuits in
    /// `score_solution`) so the new per-class worst-case axes are the
    /// only structural contributors the per-axis tests below need to
    /// reason about. Mirrors the bench predicate `worst_class_day_spread`
    /// in `solver-bench/src/quality.rs`, which fixes the day axis at
    /// `0..5`.
    fn synthetic_two_class_five_day_problem(width: u8) -> Problem {
        let class_a = SchoolClassId(score_uuid(50));
        let class_b = SchoolClassId(score_uuid(51));
        let teacher_a = TeacherId(score_uuid(20));
        let teacher_b = TeacherId(score_uuid(21));
        let subject_id = SubjectId(score_uuid(40));
        let room_id = RoomId(score_uuid(30));
        let mut time_blocks: Vec<TimeBlock> = Vec::new();
        for day in 0u8..5 {
            for pos in 0u8..width {
                // tb_id encoding: day * 10 + pos + 100 (avoids collision with
                // teacher / class / lesson ids 20 / 50 / 60).
                time_blocks.push(TimeBlock {
                    id: TimeBlockId(score_uuid(100 + day * 10 + pos)),
                    day_of_week: day,
                    position: pos,
                    kind: TimeBlockKind::Lesson,
                });
            }
        }
        Problem {
            time_blocks,
            teachers: vec![
                Teacher {
                    id: teacher_a,
                    max_hours_per_week: 50,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: teacher_b,
                    max_hours_per_week: 50,
                    reserve_hours_per_week: 0,
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
                    id: LessonId(score_uuid(60)),
                    school_class_ids: vec![class_a],
                    subject_id,
                    teacher_candidates: vec![teacher_a],
                    teacher_pin: Some(teacher_a),
                    hours_per_week: u8::max(1, width * 5),
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
                Lesson {
                    id: LessonId(score_uuid(61)),
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

    /// Mint a Placement for class A's lesson at the synthetic problem's
    /// time-block at (day, position).
    fn synthetic_place_class_a(day: u8, position: u8) -> Placement {
        Placement {
            lesson_id: LessonId(score_uuid(60)),
            time_block_id: TimeBlockId(score_uuid(100 + day * 10 + position)),
            room_id: RoomId(score_uuid(30)),
            teacher_id: TeacherId(score_uuid(20)),
        }
    }

    /// Mint a Placement for class B's lesson at the synthetic problem's
    /// time-block at (day, position).
    fn synthetic_place_class_b(day: u8, position: u8) -> Placement {
        Placement {
            lesson_id: LessonId(score_uuid(61)),
            time_block_id: TimeBlockId(score_uuid(100 + day * 10 + position)),
            room_id: RoomId(score_uuid(30)),
            teacher_id: TeacherId(score_uuid(21)),
        }
    }

    /// Both classes: 1 placement on every weekday (Mon-Fri), giving
    /// per-day counts [1,1,1,1,1] for each class. spread = 0.
    fn synthetic_placements_balanced_two_class() -> Vec<Placement> {
        let mut out: Vec<Placement> = Vec::new();
        for day in 0u8..5 {
            out.push(synthetic_place_class_a(day, 0));
            out.push(synthetic_place_class_b(day, 0));
        }
        out
    }

    /// Class A: 4 Monday, 0 elsewhere -> per-day counts [4,0,0,0,0],
    /// spread = 4. Class B: 1 placement on every weekday -> spread = 0.
    /// `max over classes = 4`.
    fn synthetic_placements_unbalanced_two_class() -> Vec<Placement> {
        let mut out: Vec<Placement> = Vec::new();
        for pos in 0u8..4 {
            out.push(synthetic_place_class_a(0, pos));
        }
        for day in 0u8..5 {
            out.push(synthetic_place_class_b(day, 0));
        }
        out
    }

    /// Class A occupies positions {0, 3} on Monday only; class B is
    /// contiguous at {0, 1} on Monday only. Class A's interior gaps =
    /// 4 - 2 = 2; class B = 0. `max over classes = 2`.
    fn synthetic_placements_class_a_gaps_one_day() -> Vec<Placement> {
        vec![
            synthetic_place_class_a(0, 0),
            synthetic_place_class_a(0, 3),
            synthetic_place_class_b(0, 0),
            synthetic_place_class_b(0, 1),
        ]
    }

    /// Both classes contiguous on Monday only.
    fn synthetic_placements_contiguous_one_day() -> Vec<Placement> {
        vec![
            synthetic_place_class_a(0, 0),
            synthetic_place_class_a(0, 1),
            synthetic_place_class_b(0, 0),
            synthetic_place_class_b(0, 1),
        ]
    }

    #[test]
    fn worst_class_spread_returns_zero_on_balanced_two_class_problem() {
        let problem = synthetic_two_class_five_day_problem(4);
        let placements = synthetic_placements_balanced_two_class();
        assert_eq!(worst_class_spread(&problem, &placements), 0);
    }

    #[test]
    fn worst_class_spread_returns_four_on_unbalanced_two_class_problem() {
        // Class A: [4,0,0,0,0] -> max - min = 4 - 0 = 4.
        // Class B: [1,1,1,1,1] -> max - min = 1 - 1 = 0.
        // max across classes = 4.
        let problem = synthetic_two_class_five_day_problem(4);
        let placements = synthetic_placements_unbalanced_two_class();
        assert_eq!(worst_class_spread(&problem, &placements), 4);
    }

    #[test]
    fn worst_class_interior_gaps_returns_zero_when_every_class_day_is_contiguous() {
        let problem = synthetic_two_class_five_day_problem(4);
        let placements = synthetic_placements_contiguous_one_day();
        assert_eq!(worst_class_interior_gaps(&problem, &placements), 0);
    }

    #[test]
    fn worst_class_interior_gaps_returns_two_when_one_class_has_two_gap_hours() {
        // Class A occupies positions {0, 3} on Monday => 2 interior gaps;
        // class B contiguous.
        let problem = synthetic_two_class_five_day_problem(4);
        let placements = synthetic_placements_class_a_gaps_one_day();
        assert_eq!(worst_class_interior_gaps(&problem, &placements), 2);
    }

    #[test]
    fn score_solution_increases_when_worst_class_spread_grows() {
        let problem = synthetic_two_class_five_day_problem(4);
        let balanced = synthetic_placements_balanced_two_class();
        let unbalanced = synthetic_placements_unbalanced_two_class();
        let weights = crate::PRODUCTION_ACTIVE_WEIGHTS;
        assert!(
            score_solution(
                &problem,
                &unbalanced,
                &weights,
                &::std::collections::HashSet::new()
            ) > score_solution(
                &problem,
                &balanced,
                &weights,
                &::std::collections::HashSet::new()
            )
        );
    }

    #[test]
    fn score_solution_increases_when_worst_class_interior_gaps_grows() {
        let problem = synthetic_two_class_five_day_problem(4);
        let contiguous = synthetic_placements_contiguous_one_day();
        let gappy = synthetic_placements_class_a_gaps_one_day();
        let weights = crate::PRODUCTION_ACTIVE_WEIGHTS;
        assert!(
            score_solution(
                &problem,
                &gappy,
                &weights,
                &::std::collections::HashSet::new()
            ) > score_solution(
                &problem,
                &contiguous,
                &weights,
                &::std::collections::HashSet::new()
            )
        );
    }

    fn small_fixture_for_slice_test() -> (Problem, Vec<Placement>) {
        // Reuses the existing 1-day / 3-position / 1-class / 1-teacher fixture.
        // Placements at positions 0 and 2 leave a gap at position 1, so
        // `class_gaps == 1` and `teacher_gaps == 1`. Subject preference is
        // zero with the default Subject (all prefer/avoid flags zero), so the
        // slice score is exactly `class_gap * 1 + teacher_gap * 1`.
        let p = three_block_one_class_problem();
        let placements = vec![place(60, 10), place(60, 12)];
        (p, placements)
    }

    #[test]
    fn slice_recompute_matches_score_solution_slice_component() {
        // Build a Problem with class_gap=5, teacher_gap=7, prefer_early_period=3
        // (the slice axes), and zero on all other weights. Then
        // slice_recompute(...) must equal score_solution(...) since canonical =
        // slice + 0 + 0 + ... when the non-slice weights are zero.
        let (p, placements) = small_fixture_for_slice_test();
        let weights = ConstraintWeights {
            class_gap: 5,
            teacher_gap: 7,
            prefer_early_period: 3,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            prefer_home_room: 0,
            class_day_balance: 0,
            prefer_class_teacher: 0,
            max_per_class_spread: 0,
            max_per_class_interior_gaps: 0,
            supervision_spread: 0,
            soft_pin_miss: 0,
        };
        let canonical = score_solution(&p, &placements, &weights, &HashSet::new());
        let slice = slice_recompute(&p, &placements, &weights);
        assert_eq!(slice, canonical);
    }
}
