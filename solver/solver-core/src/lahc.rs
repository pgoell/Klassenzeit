//! Late-acceptance hill-climbing loop that polishes the greedy's output.
//! Single Change move (move one lesson-hour to a different time-block,
//! reuse old room or fall back to lowest-id hard-feasible room),
//! deadline-bound, deterministic under (seed, max_iterations).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use crate::index::Indexed;
use crate::score::{gap_count, gap_count_after_insert, gap_count_after_remove};
use crate::types::{
    ConstraintWeights, Lesson, Placement, Problem, SolveConfig, SolveStats, Subject, TimeBlock,
};

/// Length of the LAHC cost-history list. Burke & Bykov 2008 reports the
/// algorithm is robust to this value within a wide band; 500 matches the
/// archive/v2 setting and is enough fill for ~20k iterations on Hessen
/// Grundschule under a 200ms deadline.
const LAHC_LIST_LEN: usize = 500;

/// Run the LAHC loop over the placement set produced by greedy. Mutates
/// `placements` and the partition / used-* state in place via `state`. The
/// post-LAHC running total ends up in `state.soft_score`. Records timing
/// probes (`time_to_first_feasible_ms`, `time_to_optimal_ms`) into `stats`
/// against `solve_start` so the wall-clock origin is shared with
/// `solve_with_config_stats`'s entry instead of LAHC's own start.
#[allow(clippy::too_many_arguments)] // Reason: internal helper threading stats + clock origin
pub(crate) fn run(
    problem: &Problem,
    idx: &Indexed,
    config: &SolveConfig,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
    pinned: &HashSet<LessonId>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    stats: &mut SolveStats,
    solve_start: Instant,
) {
    let Some(deadline) = config.deadline else {
        return;
    };
    if placements.is_empty() {
        return;
    }
    let mut change_rng = SmallRng::seed_from_u64(config.seed);
    let mut rr_rng = SmallRng::seed_from_u64(config.seed.wrapping_add(1));
    let mut kempe_rng = SmallRng::seed_from_u64(config.seed.wrapping_add(2));
    let mut lahc_list = vec![state.soft_score; LAHC_LIST_LEN];
    let lesson_lookup: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
    let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
        .time_blocks
        .iter()
        .map(|tb| ((tb.day_of_week, tb.position), tb.id))
        .collect();
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
    let max_iter = config.max_iterations.unwrap_or(u64::MAX);

    // R&R needs the same precomputed orderings the greedy uses. Recompute
    // them here so lahc::run does not depend on solve.rs's local state.
    let mut tb_order: Vec<usize> = (0..problem.time_blocks.len()).collect();
    tb_order.sort_unstable_by_key(|&i| {
        let tb = &problem.time_blocks[i];
        (tb.day_of_week, tb.position, tb.id.0)
    });
    let mut room_order: Vec<usize> = (0..problem.rooms.len()).collect();
    room_order.sort_unstable_by_key(|&i| problem.rooms[i].id.0);
    let teacher_max: HashMap<TeacherId, u8> = problem
        .teachers
        .iter()
        .map(|t| (t.id, t.max_hours_per_week))
        .collect();
    // Sum of `hours_per_week` across all lessons is the placement-count floor:
    // every lesson-hour materialises as one `Placement`. The LAHC loop can exit
    // early once this floor is reached AND `state.soft_score == 0`, since no
    // further iteration can improve a feasible objective-floor incumbent.
    let placements_expected: usize = problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as usize)
        .sum();

    // Track the running-best soft score so the time-to-optimal probe can
    // capture the wall-clock of the last improvement. If FFD greedy already
    // reached `soft_score == 0` and feasibility, ttf and tto are both already
    // set by `solve_with_config_stats` before LAHC runs; the running_best
    // initialiser still seeds correctly so a never-improving LAHC leaves them
    // untouched.
    let mut running_best = state.soft_score;

    let mut iter: u64 = 0;
    while iter < max_iter && solve_start.elapsed() < deadline {
        let is_rr_iter = config
            .lahc_rr_period
            .is_some_and(|n| n > 0 && (iter as u32) % n == 0);
        let is_kempe_iter = config
            .lahc_kempe_period
            .is_some_and(|n| n > 0 && (iter as u32) % n == 0)
            && !is_rr_iter;

        if is_rr_iter {
            rr_attempt(
                problem,
                idx,
                &config.weights,
                &mut rr_rng,
                &lesson_lookup,
                &tb_lookup,
                pinned,
                placements,
                state,
                &tb_order,
                &room_order,
                &max_position_per_day,
                &teacher_max,
                class_max_lessons_per_day,
                &lahc_list,
                iter,
            );
        } else if is_kempe_iter {
            kempe_attempt(
                problem,
                idx,
                &config.weights,
                &mut kempe_rng,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &tb_by_day_pos,
                pinned,
                placements,
                state,
                &room_order,
                &max_position_per_day,
                class_max_lessons_per_day,
                &lahc_list,
                iter,
            );
        } else {
            // Always consume two random draws per Change iteration so the RNG
            // sequence is invariant across feasibility branches; this is what
            // the determinism property test relies on.
            let placement_idx = change_rng.random_range(0..placements.len());
            let new_tb_idx = change_rng.random_range(0..problem.time_blocks.len());

            try_change_move(
                problem,
                idx,
                placement_idx,
                new_tb_idx,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &max_position_per_day,
                &config.weights,
                placements,
                state,
                pinned,
                class_max_lessons_per_day,
                &lahc_list,
                iter,
            );
        }

        iter += 1;
        lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.soft_score;
        if stats.time_to_first_feasible_ms.is_none()
            && state.soft_score == 0
            && placements.len() == placements_expected
        {
            stats.time_to_first_feasible_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }
        if state.soft_score < running_best {
            running_best = state.soft_score;
            stats.time_to_optimal_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }
        if state.soft_score == 0 && placements.len() == placements_expected {
            break;
        }
    }
}

/// Attempt one Change move: move `placements[placement_idx]` to time-block
/// `problem.time_blocks[new_tb_idx]`, reusing the old room when feasible or
/// falling back to the lowest-id hard-feasible room. Returns true if the
/// move was accepted (LAHC criterion) and applied. Mutates state on accept.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn try_change_move(
    problem: &Problem,
    idx: &Indexed,
    placement_idx: usize,
    new_tb_idx: usize,
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    subject_lookup: &HashMap<SubjectId, &Subject>,
    max_position_per_day: &HashMap<u8, u8>,
    weights: &ConstraintWeights,
    placements: &mut [Placement],
    state: &mut crate::solve::GreedyState,
    pinned: &HashSet<LessonId>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    lahc_list: &[u32],
    iter: u64,
) -> bool {
    let p = placements[placement_idx].clone();
    let lesson = lesson_lookup[&p.lesson_id];
    // LAHC's single-cell Change move would fragment a Doppelstunde mid-search.
    // Skip block placements; the two random_range draws in `run` are already
    // consumed before this check, so the determinism RNG-budget invariant
    // (lahc_property.rs) holds.
    if lesson.preferred_block_size > 1 {
        return false;
    }
    if lesson.lesson_group_id.is_some() {
        return false;
    }
    // Pinned placements are caller-fixed (Problem.pinned_placements) and must
    // survive LAHC verbatim. Same RNG-invariance argument as the block / group
    // guards above: the two random_range draws are already consumed.
    if pinned.contains(&p.lesson_id) {
        return false;
    }
    let old_tb = tb_lookup[&p.time_block_id].clone();
    let new_tb = problem.time_blocks[new_tb_idx].clone();

    if new_tb.id == old_tb.id {
        return false;
    }

    let class_ids: &[SchoolClassId] = &lesson.school_class_ids;
    let teacher = lesson.teacher_id;

    if state.used_teacher.contains(&(teacher, new_tb.id)) {
        return false;
    }
    for class in class_ids {
        if state.used_class.contains(&(*class, new_tb.id)) {
            return false;
        }
    }
    if idx.teacher_blocked(teacher, new_tb.id) {
        return false;
    }

    // Per-day cap check at the destination day. When the move stays on the
    // same day, the (class, day, subject) hour count and (class, day) lesson
    // count both stay constant; the cap cannot newly become violated. When
    // the move crosses to a different day, the destination day gains 1 hour
    // and 1 lesson for each member class, so check destination headroom.
    if old_tb.day_of_week != new_tb.day_of_week {
        for class in class_ids {
            let subject_cap = problem
                .subjects
                .iter()
                .find(|s| s.id == lesson.subject_id)
                .map(|s| s.max_hours_per_day)
                .unwrap_or(u8::MAX);
            let key = (*class, new_tb.day_of_week, lesson.subject_id);
            let current_hours = state
                .subject_hours_by_class_day
                .get(&key)
                .copied()
                .unwrap_or(0);
            if current_hours.saturating_add(1) > subject_cap {
                return false;
            }
            if let Some(cap) = class_max_lessons_per_day.get(class).copied() {
                let lessons_today = state
                    .lessons_by_class_day
                    .get(&(*class, new_tb.day_of_week))
                    .copied()
                    .unwrap_or(0);
                if lessons_today.saturating_add(1) > cap {
                    return false;
                }
            }
        }
    }

    // Same-room hard constraint at new_day: the destination triple's lock
    // (if any) constrains which room the move may use. Disagreement among
    // member classes makes the move infeasible.
    let mut new_day_lock: Option<RoomId> = None;
    for class in class_ids {
        let key = (*class, new_tb.day_of_week, lesson.subject_id);
        if let Some(&(locked, count)) = state.locked_room.get(&key) {
            // When old_day == new_day and the current placement is the only
            // one in the triple, the lock is effectively cleared by removing
            // self before re-adding. Otherwise the lock's room must hold.
            let self_only = old_tb.day_of_week == new_tb.day_of_week && count == 1;
            if self_only {
                continue;
            }
            match new_day_lock {
                None => new_day_lock = Some(locked),
                Some(prev) if prev != locked => return false,
                _ => {}
            }
        }
    }

    let Some(new_room_id) = pick_room(
        problem,
        idx,
        lesson.subject_id,
        p.room_id,
        new_tb.id,
        &state.used_room,
        new_day_lock,
    ) else {
        return false;
    };

    // If a lock exists at the destination triple, the chosen room must match.
    if let Some(locked) = new_day_lock {
        if new_room_id != locked {
            return false;
        }
    }

    let subject = subject_lookup[&lesson.subject_id];
    let old_max = max_position_per_day
        .get(&old_tb.day_of_week)
        .copied()
        .unwrap_or(old_tb.position);
    let new_max = max_position_per_day
        .get(&new_tb.day_of_week)
        .copied()
        .unwrap_or(new_tb.position);
    let subject_pref_old =
        crate::score::subject_preference_score(subject, &old_tb, old_max, weights);
    let subject_pref_new =
        crate::score::subject_preference_score(subject, &new_tb, new_max, weights);
    let subject_pref_delta = i64::from(subject_pref_new) - i64::from(subject_pref_old);

    let delta = score_after_change_move(
        class_ids,
        teacher,
        old_tb.day_of_week,
        old_tb.position,
        new_tb.day_of_week,
        new_tb.position,
        &state.class_positions,
        &state.teacher_positions,
        weights,
    ) + subject_pref_delta;

    let new_score_signed = i64::from(state.soft_score) + delta;
    debug_assert!(
        new_score_signed >= 0,
        "running score must remain non-negative; current_score={} delta={}",
        state.soft_score,
        delta
    );
    let new_score = u32::try_from(new_score_signed.max(0)).unwrap_or(u32::MAX);

    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let accept = new_score <= state.soft_score || new_score <= prior;
    if !accept {
        return false;
    }

    apply_change_move(
        placement_idx,
        &p,
        old_tb,
        new_tb,
        new_room_id,
        class_ids,
        teacher,
        lesson.subject_id,
        placements,
        state,
    );
    state.soft_score = new_score;
    true
}

/// Pick a room for the Change move's destination tb. Prefers reusing
/// `old_room_id`; falls back to the lowest-id hard-feasible room. When
/// `lock` is `Some`, only that room is considered. Returns `None` if no
/// room is feasible.
fn pick_room(
    problem: &Problem,
    idx: &Indexed,
    subject_id: crate::ids::SubjectId,
    old_room_id: RoomId,
    new_tb_id: TimeBlockId,
    used_room: &HashSet<(RoomId, TimeBlockId)>,
    lock: Option<RoomId>,
) -> Option<RoomId> {
    let feasible = |room_id: RoomId| {
        idx.room_suits_subject(room_id, subject_id)
            && !idx.room_blocked(room_id, new_tb_id)
            && !used_room.contains(&(room_id, new_tb_id))
    };
    if let Some(locked) = lock {
        return if feasible(locked) { Some(locked) } else { None };
    }
    if feasible(old_room_id) {
        return Some(old_room_id);
    }
    let mut best: Option<RoomId> = None;
    for room in &problem.rooms {
        if !feasible(room.id) {
            continue;
        }
        match best {
            None => best = Some(room.id),
            Some(current) if room.id.0 < current.0 => best = Some(room.id),
            _ => {}
        }
    }
    best
}

/// Compute the soft-score delta produced by moving a placement from
/// `(old_day, old_pos)` to `(new_day, new_pos)` for the given member classes
/// and teacher. Class-side delta sums across every member of `class_ids`;
/// teacher half is unchanged. Pure function over the partition maps; does
/// not mutate.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn score_after_change_move(
    class_ids: &[SchoolClassId],
    teacher: TeacherId,
    old_day: u8,
    old_pos: u8,
    new_day: u8,
    new_pos: u8,
    class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>,
    teacher_positions: &HashMap<(TeacherId, u8), Vec<u8>>,
    weights: &ConstraintWeights,
) -> i64 {
    let class_delta = class_partitions_delta_sum(
        class_ids,
        old_day,
        new_day,
        old_pos,
        new_pos,
        class_positions,
    );
    let teacher_delta = partition_delta(
        teacher_positions.get(&(teacher, old_day)),
        teacher_positions.get(&(teacher, new_day)),
        old_day,
        new_day,
        old_pos,
        new_pos,
    );
    i64::from(weights.class_gap) * class_delta + i64::from(weights.teacher_gap) * teacher_delta
}

/// Sum of `partition_delta` across every member class. Helper unique to LAHC;
/// the greedy loop in `solve.rs` walks `school_class_ids` directly.
fn class_partitions_delta_sum(
    class_ids: &[SchoolClassId],
    old_day: u8,
    new_day: u8,
    old_pos: u8,
    new_pos: u8,
    class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>,
) -> i64 {
    let mut sum = 0i64;
    for class in class_ids {
        sum += partition_delta(
            class_positions.get(&(*class, old_day)),
            class_positions.get(&(*class, new_day)),
            old_day,
            new_day,
            old_pos,
            new_pos,
        );
    }
    sum
}

/// Compute the gap-count delta for a single (entity, day) partition pair
/// when a position moves from `(old_day, old_pos)` to `(new_day, new_pos)`.
/// Handles same-day and cross-day moves with one shared shape.
fn partition_delta(
    old_part: Option<&Vec<u8>>,
    new_part: Option<&Vec<u8>>,
    old_day: u8,
    new_day: u8,
    old_pos: u8,
    new_pos: u8,
) -> i64 {
    if old_day == new_day {
        let Some(part) = old_part else {
            return 0;
        };
        let before = gap_count(part);
        let after = gap_count_after_swap(part, old_pos, new_pos);
        i64::from(after) - i64::from(before)
    } else {
        let old_before = old_part.map(|v| gap_count(v)).unwrap_or(0);
        let old_after = old_part
            .map(|v| gap_count_after_remove(v, old_pos))
            .unwrap_or(0);
        let new_before = new_part.map(|v| gap_count(v)).unwrap_or(0);
        let new_after = gap_count_after_insert(new_part, new_pos);
        (i64::from(old_after) - i64::from(old_before))
            + (i64::from(new_after) - i64::from(new_before))
    }
}

/// Count gap-hours after removing `old_pos` and inserting `new_pos` against
/// the same sorted slice. Returns 0 when the resulting slice has fewer than
/// two distinct positions.
fn gap_count_after_swap(positions: &[u8], old_pos: u8, new_pos: u8) -> u32 {
    if old_pos == new_pos {
        return gap_count(positions);
    }
    let removed_at = match positions.binary_search(&old_pos) {
        Ok(i) => i,
        Err(_) => {
            return gap_count(positions);
        }
    };
    let already_present = positions.binary_search(&new_pos).is_ok();
    let len_after = if already_present {
        positions.len() - 1
    } else {
        positions.len()
    };
    if len_after < 2 {
        return 0;
    }
    let post_remove_first = if removed_at == 0 {
        positions[1]
    } else {
        positions[0]
    };
    let post_remove_last = if removed_at == positions.len() - 1 {
        positions[positions.len() - 2]
    } else {
        positions[positions.len() - 1]
    };
    let new_first = post_remove_first.min(new_pos);
    let new_last = post_remove_last.max(new_pos);
    let span = u32::from(new_last - new_first);
    let count = u32::try_from(len_after).unwrap_or(u32::MAX);
    span + 1 - count
}

/// Apply the accepted move's mutations: rewrite the placement entry,
/// update the partition maps, swap the used-* set entries, and adjust the
/// per-`(class, day, subject)` same-room lock counts. For multi-class
/// lessons, every member of `class_ids` has its partition and `used_class`
/// entries updated.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn apply_change_move(
    placement_idx: usize,
    old_p: &Placement,
    old_tb: TimeBlock,
    new_tb: TimeBlock,
    new_room_id: RoomId,
    class_ids: &[SchoolClassId],
    teacher: TeacherId,
    subject_id: SubjectId,
    placements: &mut [Placement],
    state: &mut crate::solve::GreedyState,
) {
    placements[placement_idx] = Placement {
        lesson_id: old_p.lesson_id,
        time_block_id: new_tb.id,
        room_id: new_room_id,
    };

    for class in class_ids {
        if let Some(part) = state.class_positions.get_mut(&(*class, old_tb.day_of_week)) {
            if let Ok(i) = part.binary_search(&old_tb.position) {
                part.remove(i);
            }
            if part.is_empty() {
                state.class_positions.remove(&(*class, old_tb.day_of_week));
            }
        }
        let part = state
            .class_positions
            .entry((*class, new_tb.day_of_week))
            .or_default();
        let ins = part.binary_search(&new_tb.position).unwrap_or_else(|i| i);
        if part.get(ins).copied() != Some(new_tb.position) {
            part.insert(ins, new_tb.position);
        }
    }

    if let Some(part) = state
        .teacher_positions
        .get_mut(&(teacher, old_tb.day_of_week))
    {
        if let Ok(i) = part.binary_search(&old_tb.position) {
            part.remove(i);
        }
        if part.is_empty() {
            state
                .teacher_positions
                .remove(&(teacher, old_tb.day_of_week));
        }
    }
    let part = state
        .teacher_positions
        .entry((teacher, new_tb.day_of_week))
        .or_default();
    let ins = part.binary_search(&new_tb.position).unwrap_or_else(|i| i);
    if part.get(ins).copied() != Some(new_tb.position) {
        part.insert(ins, new_tb.position);
    }

    state.used_teacher.remove(&(teacher, old_tb.id));
    state.used_teacher.insert((teacher, new_tb.id));
    for class in class_ids {
        state.used_class.remove(&(*class, old_tb.id));
        state.used_class.insert((*class, new_tb.id));
    }
    state.used_room.remove(&(old_p.room_id, old_tb.id));
    state.used_room.insert((new_room_id, new_tb.id));

    // Same-room lock bookkeeping. The placement leaves
    // `(class, old_day, subject)` and joins `(class, new_day, subject)`.
    // Decrement the old triple's count (removing the entry when zero) and
    // increment the new triple's count.
    for class in class_ids {
        let old_key = (*class, old_tb.day_of_week, subject_id);
        if let Some(entry) = state.locked_room.get_mut(&old_key) {
            entry.1 = entry.1.saturating_sub(1);
            if entry.1 == 0 {
                state.locked_room.remove(&old_key);
            }
        }
        let new_key = (*class, new_tb.day_of_week, subject_id);
        let entry = state.locked_room.entry(new_key).or_insert((new_room_id, 0));
        entry.1 += 1;
    }

    // Per-day cap counters: decrement at old_day and increment at new_day.
    // Same-day moves are net-zero (both keys are identical) so the order
    // matters: do decrement first, then increment.
    for class in class_ids {
        let old_hour_key = (*class, old_tb.day_of_week, subject_id);
        if let Some(h) = state.subject_hours_by_class_day.get_mut(&old_hour_key) {
            *h = h.saturating_sub(1);
            if *h == 0 {
                state.subject_hours_by_class_day.remove(&old_hour_key);
            }
        }
        let old_lesson_key = (*class, old_tb.day_of_week);
        if let Some(c) = state.lessons_by_class_day.get_mut(&old_lesson_key) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                state.lessons_by_class_day.remove(&old_lesson_key);
            }
        }
        *state
            .subject_hours_by_class_day
            .entry((*class, new_tb.day_of_week, subject_id))
            .or_insert(0) += 1;
        *state
            .lessons_by_class_day
            .entry((*class, new_tb.day_of_week))
            .or_insert(0) += 1;
    }
}

/// Number of block-anchors per R&R attempt. Hardcoded today; a follow-up
/// promotes this to `SolveConfig.lahc_rr_k` if `BENCH_RESULTS.md` shows
/// K-sensitivity. See
/// `docs/superpowers/specs/2026-05-04-solver-rr-lahc-move-design.md`.
const RR_K: usize = 5;

/// Snapshot of one block's removed placements, sufficient to replay the
/// removal back into the state. R&R holds a vector of these to roll back if
/// the recreated solution is rejected by the acceptance gate.
struct BlockSnapshot {
    rows: Vec<Placement>,
}

/// Remove the (lesson, day) block anchored at `placement_idx` from
/// `placements` + `state`. Returns the snapshot of removed rows. The anchor's
/// day is read from `tb_lookup`; every placement on that day for this lesson
/// is treated as part of the same block. Caller guarantees the anchor is not
/// pinned and the lesson is not group-tagged (per `rr_collect_anchors`).
fn rr_ruin_block(
    anchor_idx: usize,
    lesson: &Lesson,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
) -> BlockSnapshot {
    let anchor_tb_id = placements[anchor_idx].time_block_id;
    let anchor_day = tb_lookup
        .get(&anchor_tb_id)
        .expect("anchor's time-block must exist in lookup")
        .day_of_week;

    // Collect indices of every placement of this lesson on this day. Sort
    // ascending; iterate in reverse below so removals don't shift unprocessed
    // indices.
    let mut indices: Vec<usize> = placements
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.lesson_id == lesson.id
                && tb_lookup
                    .get(&p.time_block_id)
                    .map(|tb| tb.day_of_week == anchor_day)
                    .unwrap_or(false)
        })
        .map(|(i, _)| i)
        .collect();
    indices.sort_unstable();

    let mut rows: Vec<Placement> = Vec::with_capacity(indices.len());
    for &i in indices.iter().rev() {
        let p = placements.remove(i);
        rr_remove_row_bookkeeping(lesson, &p, tb_lookup, state);
        rows.push(p);
    }

    // The accept path increments `lessons_by_class_day` once per block; mirror
    // that with a single per-block decrement here, after the per-row helper
    // has handled subject hours and other counters.
    if !rows.is_empty() {
        for class in &lesson.school_class_ids {
            let lesson_key = (*class, anchor_day);
            if let Some(c) = state.lessons_by_class_day.get_mut(&lesson_key) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    state.lessons_by_class_day.remove(&lesson_key);
                }
            }
        }
    }

    BlockSnapshot { rows }
}

/// Collect the set of `(lesson, day)` blocks eligible to be ruined by an R&R
/// or Kempe attempt. Returns one tuple per block for lessons that are
/// neither pinned nor part of a lesson group, and only when the day holds
/// exactly one block of the lesson (`count(placements_on_day) <=
/// preferred_block_size`). The single-anchor-per-block contract lets the
/// recreate step call `try_place_block` once per chosen anchor without
/// silently dropping placements when FFD packed multiple `N=1` rows of the
/// same lesson on one day for compactness. Returned in a deterministic
/// order so the R&R / Kempe RNG shuffle reproduces under a fixed seed.
///
/// Tuples (not placement indices) because a single ruin removes every
/// placement of a lesson on its day, which can shift indices both above and
/// below other anchors when a lesson has multiple non-contiguous block
/// placements on the same day. Callers look up the current placement index
/// at ruin time from this tuple.
fn rr_collect_anchors(
    placements: &[Placement],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    pinned: &HashSet<LessonId>,
) -> Vec<(LessonId, u8)> {
    let mut counts: HashMap<(LessonId, u8), u32> = HashMap::new();
    for p in placements.iter() {
        let Some(lesson) = lesson_lookup.get(&p.lesson_id) else {
            continue;
        };
        if pinned.contains(&p.lesson_id) {
            continue;
        }
        if lesson.lesson_group_id.is_some() {
            continue;
        }
        let Some(tb) = tb_lookup.get(&p.time_block_id) else {
            continue;
        };
        *counts.entry((p.lesson_id, tb.day_of_week)).or_insert(0) += 1;
    }

    let mut anchors: Vec<(LessonId, u8)> = counts
        .into_iter()
        .filter_map(|((lesson_id, day), count)| {
            let lesson = lesson_lookup.get(&lesson_id)?;
            if count <= u32::from(lesson.preferred_block_size) {
                Some((lesson_id, day))
            } else {
                None
            }
        })
        .collect();
    // Deterministic order before the R&R RNG shuffles.
    anchors.sort_unstable_by(|a, b| a.0 .0.cmp(&b.0 .0).then(a.1.cmp(&b.1)));
    anchors
}

/// Run one R&R move: pick up to `RR_K` block anchors at random, ruin them,
/// recreate them, accept under the asymmetric LAHC gate. Returns true if the
/// move was accepted (state mutated to keep the new arrangement); returns
/// false if the move was rejected (state restored to the pre-attempt
/// snapshot).
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn rr_attempt(
    problem: &Problem,
    idx: &Indexed,
    weights: &ConstraintWeights,
    rr_rng: &mut SmallRng,
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    pinned: &HashSet<LessonId>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
    tb_order: &[usize],
    room_order: &[usize],
    max_position_per_day: &HashMap<u8, u8>,
    teacher_max: &HashMap<TeacherId, u8>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    lahc_list: &[u32],
    iter: u64,
) -> bool {
    use rand::seq::SliceRandom;

    let mut anchors = rr_collect_anchors(placements, lesson_lookup, tb_lookup, pinned);
    if anchors.is_empty() {
        return false;
    }
    anchors.shuffle(rr_rng);
    let chosen_count = anchors.len().min(RR_K);
    let chosen: Vec<(LessonId, u8)> = anchors.into_iter().take(chosen_count).collect();

    let pre_score = state.soft_score;
    let pre_count = placements.len();
    let mut snapshots: Vec<(LessonId, BlockSnapshot)> = Vec::with_capacity(chosen_count);
    for (lesson_id, day) in &chosen {
        let lesson = match lesson_lookup.get(lesson_id) {
            Some(l) => *l,
            None => continue,
        };
        // Look up the current anchor index at runtime; an earlier ruin in
        // this iteration may have removed placements above OR below this
        // block, so any index cached at collect-time is stale. Skip cleanly
        // if the block is no longer present (would only happen if two
        // distinct (lesson_id, day) anchors aliased the same set, which the
        // dedup above prevents, but is safe regardless).
        let Some(idx_anchor) = placements.iter().position(|p| {
            p.lesson_id == *lesson_id
                && tb_lookup
                    .get(&p.time_block_id)
                    .is_some_and(|tb| tb.day_of_week == *day)
        }) else {
            continue;
        };
        let snap = rr_ruin_block(idx_anchor, lesson, tb_lookup, placements, state);
        snapshots.push((*lesson_id, snap));
    }

    // Capture every placement row added per successful recreate. Rolling
    // back by exact row id avoids the multi-block-across-days bug where
    // `placements.iter().position(|p| p.lesson_id == ...)` would otherwise
    // return one of the lesson's untouched original blocks instead of the
    // recreated one and `rr_ruin_block` would drop pristine rows.
    let snapshotted_lesson_days: HashSet<(LessonId, u8)> = snapshots
        .iter()
        .filter_map(|(lesson_id, snap)| {
            let row = snap.rows.first()?;
            let day = tb_lookup.get(&row.time_block_id)?.day_of_week;
            Some((*lesson_id, day))
        })
        .collect();

    let mut failed_recreates: usize = 0;
    let mut recreated_rows: Vec<Vec<Placement>> = Vec::with_capacity(snapshots.len());
    for (lesson_id, _snap) in snapshots.iter() {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("ruined lesson must resolve");
        let n = lesson.preferred_block_size;
        let len_before = placements.len();
        let placed = crate::solve::try_place_block(
            problem,
            lesson,
            n,
            idx,
            teacher_max,
            class_max_lessons_per_day,
            weights,
            state,
            placements,
            tb_order,
            room_order,
            max_position_per_day,
        );
        if !placed {
            failed_recreates += 1;
            continue;
        }
        let added: Vec<Placement> = placements[len_before..].to_vec();
        // Defensive guard: if the recreate landed on a day where the same
        // lesson already had a placement that wasn't part of this iteration's
        // snapshot, the post-accept state would have two windows of the same
        // lesson on one day, which `rr_collect_anchors` would then filter out
        // forever. Treat as a recreate failure and roll back.
        let dest_day = added
            .first()
            .and_then(|p| tb_lookup.get(&p.time_block_id))
            .map(|tb| tb.day_of_week);
        let collides = match dest_day {
            Some(day) => {
                !snapshotted_lesson_days.contains(&(*lesson_id, day))
                    && placements
                        .iter()
                        .filter(|p| p.lesson_id == *lesson_id)
                        .any(|p| {
                            tb_lookup
                                .get(&p.time_block_id)
                                .is_some_and(|tb| tb.day_of_week == day)
                                && !added.iter().any(|a| a.time_block_id == p.time_block_id)
                        })
            }
            None => false,
        };
        if collides {
            failed_recreates += 1;
            recreated_rows.push(added);
            continue;
        }
        recreated_rows.push(added);
    }

    if failed_recreates > 0 {
        rr_rollback(
            &recreated_rows,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
        );
        state.soft_score = pre_score;
        debug_assert_eq!(
            placements.len(),
            pre_count,
            "rr_rollback left placement count drifted (pre={pre_count} post={})",
            placements.len(),
        );
        return false;
    }

    // `try_place_block` accumulates against `state.soft_score`, but `rr_ruin_block`
    // does not subtract the removed placement's gap contribution from soft_score.
    // For a successful recreate, the post-recreate `state.soft_score` therefore
    // drifts; subsequent Change moves operate on a stale score and the
    // non-negative-delta invariant inside `try_change_move` can fail. Recompute
    // exactly here so the LAHC gate decides on correct numbers and downstream
    // moves see a consistent score. Use the slice-only helper rather than
    // `score::score_solution` because greedy / Change / Kempe maintain the
    // class_gap + teacher_gap + subj_pref slice; including class_day_balance or
    // home_room here contaminates `state.soft_score` and downstream Change-move
    // deltas (slice-only) drive it negative over time.
    let new_score =
        running_slice_from_placements(problem, placements, weights, max_position_per_day);
    state.soft_score = new_score;
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let lahc_ok = new_score <= pre_score || new_score <= prior;
    if !lahc_ok {
        rr_rollback(
            &recreated_rows,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
        );
        state.soft_score = pre_score;
        debug_assert_eq!(
            placements.len(),
            pre_count,
            "rr_rollback left placement count drifted (pre={pre_count} post={})",
            placements.len(),
        );
        return false;
    }

    debug_assert_eq!(
        placements.len(),
        pre_count,
        "rr_attempt accepted but placement count drifted (pre={pre_count} post={})",
        placements.len(),
    );
    true
}

/// Roll back a partial or complete R&R recreate. For each captured set of
/// recreated rows, remove only those exact `(lesson_id, time_block_id,
/// room_id)` rows (the Kempe pattern). Then for each snapshot, replay the
/// original placement rows back into `placements` + `state`. The captured-rows
/// approach avoids the multi-block-across-days hazard the older
/// `placements.iter().position(|p| p.lesson_id == ...)` lookup had.
fn rr_rollback(
    recreated_rows: &[Vec<Placement>],
    snapshots: &[(LessonId, BlockSnapshot)],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
) {
    for rows in recreated_rows.iter().rev() {
        let mut rows_to_remove: Vec<usize> = Vec::with_capacity(rows.len());
        for row in rows.iter() {
            if let Some(idx) = placements.iter().position(|p| {
                p.lesson_id == row.lesson_id
                    && p.time_block_id == row.time_block_id
                    && p.room_id == row.room_id
            }) {
                rows_to_remove.push(idx);
            }
        }
        // Capture the (lesson, day) of the recreated block so we can mirror
        // the per-block lesson-cap decrement after row removals complete.
        let block_day_lesson = rows.first().and_then(|first| {
            tb_lookup
                .get(&first.time_block_id)
                .map(|tb| (first.lesson_id, tb.day_of_week))
        });
        rows_to_remove.sort_unstable();
        for &idx in rows_to_remove.iter().rev() {
            let p = placements.remove(idx);
            let lesson = lesson_lookup
                .get(&p.lesson_id)
                .expect("rolled-back placement's lesson resolves");
            rr_remove_row_bookkeeping(lesson, &p, tb_lookup, state);
        }
        if let Some((lesson_id, day)) = block_day_lesson {
            if let Some(lesson) = lesson_lookup.get(&lesson_id) {
                for class in &lesson.school_class_ids {
                    let lesson_key = (*class, day);
                    if let Some(c) = state.lessons_by_class_day.get_mut(&lesson_key) {
                        *c = c.saturating_sub(1);
                        if *c == 0 {
                            state.lessons_by_class_day.remove(&lesson_key);
                        }
                    }
                }
            }
        }
    }
    for (lesson_id, snapshot) in snapshots.iter().rev() {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("snapshot lesson resolves");
        for row in snapshot.rows.iter().rev() {
            replay_placement(lesson, row, tb_lookup, placements, state);
        }
        // Per-block lesson-cap counter: +1 per replayed snapshot block.
        if let Some(first) = snapshot.rows.first() {
            if let Some(tb) = tb_lookup.get(&first.time_block_id) {
                let day = tb.day_of_week;
                for class in &lesson.school_class_ids {
                    *state.lessons_by_class_day.entry((*class, day)).or_insert(0) += 1;
                }
            }
        }
    }
}

/// Decrement the per-row bookkeeping for one removed placement: matches the
/// inner loop of `rr_ruin_block` row-by-row. Lifted into its own helper so
/// `rr_rollback` and `rr_ruin_block` share the same source of truth.
fn rr_remove_row_bookkeeping(
    lesson: &Lesson,
    row: &Placement,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    state: &mut crate::solve::GreedyState,
) {
    let tb = tb_lookup
        .get(&row.time_block_id)
        .expect("removed row's tb resolves");
    let day = tb.day_of_week;
    let position = tb.position;
    state
        .used_teacher
        .remove(&(lesson.teacher_id, row.time_block_id));
    for class in &lesson.school_class_ids {
        state.used_class.remove(&(*class, row.time_block_id));
        if let Some(part) = state.class_positions.get_mut(&(*class, day)) {
            if let Ok(j) = part.binary_search(&position) {
                part.remove(j);
            }
            if part.is_empty() {
                state.class_positions.remove(&(*class, day));
            }
        }
    }
    state.used_room.remove(&(row.room_id, row.time_block_id));
    if let Some(part) = state.teacher_positions.get_mut(&(lesson.teacher_id, day)) {
        if let Ok(j) = part.binary_search(&position) {
            part.remove(j);
        }
        if part.is_empty() {
            state.teacher_positions.remove(&(lesson.teacher_id, day));
        }
    }
    if let Some(h) = state.hours_by_teacher.get_mut(&lesson.teacher_id) {
        *h = h.saturating_sub(1);
    }
    for class in &lesson.school_class_ids {
        let key = (*class, day, lesson.subject_id);
        if let Some(entry) = state.locked_room.get_mut(&key) {
            entry.1 = entry.1.saturating_sub(1);
            if entry.1 == 0 {
                state.locked_room.remove(&key);
            }
        }
        // Subject-hour cap counter decrements by 1 per removed row (the
        // accept path adds `n` once, balanced by `n` row removals).
        if let Some(h) = state.subject_hours_by_class_day.get_mut(&key) {
            *h = h.saturating_sub(1);
            if *h == 0 {
                state.subject_hours_by_class_day.remove(&key);
            }
        }
    }
    // Note: `lessons_by_class_day` is not touched here. The accept path
    // increments by 1 per block, not per row, so the matching decrement
    // happens once per block in the caller (`rr_ruin_block` /
    // `kempe_rollback`).
}

/// Re-add a single previously-removed placement row to `placements` +
/// `state`. The mirror of one row's removal in `rr_ruin_block`.
fn replay_placement(
    lesson: &Lesson,
    row: &Placement,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
) {
    let tb = tb_lookup
        .get(&row.time_block_id)
        .expect("replay tb resolves");
    let day = tb.day_of_week;
    let position = tb.position;

    placements.push(row.clone());
    state
        .used_teacher
        .insert((lesson.teacher_id, row.time_block_id));
    for class in &lesson.school_class_ids {
        state.used_class.insert((*class, row.time_block_id));
        let part = state.class_positions.entry((*class, day)).or_default();
        let ins = part.binary_search(&position).unwrap_or_else(|i| i);
        if part.get(ins).copied() != Some(position) {
            part.insert(ins, position);
        }
    }
    state.used_room.insert((row.room_id, row.time_block_id));
    let part = state
        .teacher_positions
        .entry((lesson.teacher_id, day))
        .or_default();
    let ins = part.binary_search(&position).unwrap_or_else(|i| i);
    if part.get(ins).copied() != Some(position) {
        part.insert(ins, position);
    }
    *state.hours_by_teacher.entry(lesson.teacher_id).or_insert(0) += 1;
    for class in &lesson.school_class_ids {
        let key = (*class, day, lesson.subject_id);
        let entry = state.locked_room.entry(key).or_insert((row.room_id, 0));
        entry.1 += 1;
        // Subject-hour cap counter increments by 1 per replayed row.
        *state.subject_hours_by_class_day.entry(key).or_insert(0) += 1;
    }
    // Note: `lessons_by_class_day` is not touched here. The caller
    // (`rr_rollback` / `kempe_rollback`) handles the per-block increment
    // once per snapshot.
}

/// Maximum chain length per Kempe attempt. Hardcoded today; a follow-up
/// promotes this to `SolveConfig.lahc_kempe_max_chain` if `BENCH_RESULTS.md`
/// shows depth-sensitivity. See
/// `docs/superpowers/specs/2026-05-04-solver-kempe-lahc-move-design.md`.
const KEMPE_MAX_CHAIN: usize = 8;

/// Outcome of `kempe_build_chain`. `Built(chain)` carries the mapping from
/// each chain member's lesson-id to its destination day; `Aborted` signals
/// that the BFS hit a non-eligible placement (pin, group, missing window,
/// over-bound) and the caller must reject the attempt without ruining
/// anything.
enum ChainBuild {
    Built(HashMap<LessonId, u8>),
    Aborted,
}

/// Build the BFS chain starting from `(seed_lesson, source_day, dest_day)`
/// over the teacher+class conflict graph at the destination window. Pure:
/// reads `placements`, `lesson_lookup`, `tb_lookup`, `pinned`, `start_pos`;
/// does not mutate. Returns `ChainBuild::Aborted` on any of: chain hits a
/// pinned or group-tagged placement, chain length exceeds `KEMPE_MAX_CHAIN`,
/// or a chain neighbour's destination window has missing positions.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn kempe_build_chain(
    seed_lesson: LessonId,
    source_day: u8,
    dest_day: u8,
    start_pos: u8,
    placements: &[Placement],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
    pinned: &HashSet<LessonId>,
) -> ChainBuild {
    let mut chain: HashMap<LessonId, u8> = HashMap::new();
    chain.insert(seed_lesson, dest_day);
    let mut frontier: VecDeque<LessonId> = VecDeque::new();
    frontier.push_back(seed_lesson);
    let mut frontier_seen: HashSet<LessonId> = HashSet::new();
    frontier_seen.insert(seed_lesson);

    while let Some(lesson_id) = frontier.pop_front() {
        let popped_dest_day = match chain.get(&lesson_id) {
            Some(d) => *d,
            None => return ChainBuild::Aborted,
        };
        let popped_lesson = match lesson_lookup.get(&lesson_id) {
            Some(l) => *l,
            None => return ChainBuild::Aborted,
        };
        let n = popped_lesson.preferred_block_size;

        // Compute the destination day for any new neighbour added under this
        // popped member. The chain alternates source_day / dest_day per BFS
        // depth; new neighbours go to the opposite day from the popped member.
        let neighbour_dest = if popped_dest_day == dest_day {
            source_day
        } else {
            dest_day
        };

        // Window verification for the popped lesson at its destination day.
        for k in 0..n {
            if !tb_by_day_pos.contains_key(&(popped_dest_day, start_pos + k)) {
                return ChainBuild::Aborted;
            }
        }

        // Collect every existing placement at the popped lesson's destination
        // window. A placement whose lesson is already in the chain is leaving
        // (it has its own chain assignment) so does not contribute to new
        // neighbours.
        let mut new_neighbours: Vec<LessonId> = Vec::new();
        for k in 0..n {
            let dest_tb_id = tb_by_day_pos[&(popped_dest_day, start_pos + k)];
            for placement in placements.iter() {
                if placement.time_block_id != dest_tb_id {
                    continue;
                }
                if chain.contains_key(&placement.lesson_id) {
                    continue;
                }
                let other = match lesson_lookup.get(&placement.lesson_id) {
                    Some(l) => *l,
                    None => return ChainBuild::Aborted,
                };
                let teacher_conflict = other.teacher_id == popped_lesson.teacher_id;
                let class_conflict = other
                    .school_class_ids
                    .iter()
                    .any(|c| popped_lesson.school_class_ids.contains(c));
                if !teacher_conflict && !class_conflict {
                    continue;
                }
                if pinned.contains(&placement.lesson_id) {
                    return ChainBuild::Aborted;
                }
                if other.lesson_group_id.is_some() {
                    return ChainBuild::Aborted;
                }
                // Block-shape guard: ruining a (lesson, day) anchor removes
                // every hour of `other` on `popped_dest_day`. If FFD packed
                // more than one N-block of `other` onto that day, the swap
                // would drop hours. Abort cleanly so the move stays atomic.
                let hours_on_source = placements
                    .iter()
                    .filter(|q| {
                        q.lesson_id == placement.lesson_id
                            && tb_lookup
                                .get(&q.time_block_id)
                                .is_some_and(|t| t.day_of_week == popped_dest_day)
                    })
                    .count();
                if hours_on_source != usize::from(other.preferred_block_size) {
                    return ChainBuild::Aborted;
                }
                // Bipartiteness invariant: this candidate is about to be assigned
                // `chain[candidate] = neighbour_dest`. Reject the chain if any chain
                // member already at `neighbour_dest` shares a class or teacher with
                // the candidate. Without this check the BFS would silently 2-color a
                // non-bipartite conflict graph, producing same-day same-class
                // collisions at apply time (item 45).
                let same_color_conflict = chain.iter().any(|(existing_id, existing_dest)| {
                    if *existing_dest != neighbour_dest {
                        return false;
                    }
                    let existing_lesson = match lesson_lookup.get(existing_id).copied() {
                        Some(l) => l,
                        None => return false,
                    };
                    let teacher_conflict = existing_lesson.teacher_id == other.teacher_id;
                    let class_conflict = existing_lesson
                        .school_class_ids
                        .iter()
                        .any(|c| other.school_class_ids.contains(c));
                    teacher_conflict || class_conflict
                });
                if same_color_conflict {
                    return ChainBuild::Aborted;
                }
                if !frontier_seen.contains(&placement.lesson_id) {
                    new_neighbours.push(placement.lesson_id);
                    frontier_seen.insert(placement.lesson_id);
                }
            }
        }

        // Determinism: sort new neighbours before extending the frontier so
        // HashSet iteration order does not leak into the chain shape.
        new_neighbours.sort_unstable_by_key(|id| id.0);

        for neighbour_id in new_neighbours {
            chain.insert(neighbour_id, neighbour_dest);
            frontier.push_back(neighbour_id);
            if chain.len() > KEMPE_MAX_CHAIN {
                return ChainBuild::Aborted;
            }
        }
    }

    ChainBuild::Built(chain)
}

/// Pick a destination room for one chain member. Prefer reusing the
/// snapshot's `original_room_id`; fall back to lowest-id hard-feasible room
/// per `room_order`. Honours the same-room lock at the destination triple
/// (only the locked room is feasible if a lock exists for any member class).
/// Returns `None` if no feasible room exists across the full N-block window.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn kempe_pick_room(
    problem: &Problem,
    idx: &Indexed,
    lesson: &Lesson,
    original_room_id: RoomId,
    dest_day: u8,
    start_pos: u8,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
    state: &crate::solve::GreedyState,
    room_order: &[usize],
) -> Option<RoomId> {
    let n = lesson.preferred_block_size;
    let dest_tb_ids: Vec<TimeBlockId> = (0..n)
        .map(|k| tb_by_day_pos[&(dest_day, start_pos + k)])
        .collect();

    // Same-room hard constraint: if any member class already has a lock at
    // (class, dest_day, subject), every member class's lock (if any) must
    // agree, and the chosen room must equal that lock.
    let mut shared_lock: Option<RoomId> = None;
    for class in &lesson.school_class_ids {
        if let Some(&(locked, _)) = state
            .locked_room
            .get(&(*class, dest_day, lesson.subject_id))
        {
            match shared_lock {
                None => shared_lock = Some(locked),
                Some(prev) if prev != locked => return None,
                _ => {}
            }
        }
    }

    let feasible = |room_id: RoomId| -> bool {
        if !idx.room_suits_subject(room_id, lesson.subject_id) {
            return false;
        }
        for tb_id in &dest_tb_ids {
            if idx.room_blocked(room_id, *tb_id) {
                return false;
            }
            if state.used_room.contains(&(room_id, *tb_id)) {
                return false;
            }
        }
        true
    };

    if let Some(locked) = shared_lock {
        return if feasible(locked) { Some(locked) } else { None };
    }
    if feasible(original_room_id) {
        return Some(original_room_id);
    }
    for &i in room_order {
        let candidate = problem.rooms[i].id;
        if feasible(candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Snapshot the gap counts of every (class, day) and (teacher, day) partition
/// touched by the chain. Pure: reads `state` and `placements` through
/// `chain_members`; does not mutate. The caller computes
/// `removed_subject_pref` from the actual ruined rows so multi-block-on-other-
/// day placements aren't wrongly double-counted (a chain member with another
/// untouched block on a different day must contribute zero to the delta).
fn kempe_snapshot_pre_score(
    chain_members: &[(LessonId, u8)],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    placements: &[Placement],
    state: &crate::solve::GreedyState,
) -> KempePartitionSnapshot {
    // Collect every (class, day) and (teacher, day) partition touched by
    // the chain. The `dest_day` of each member is its outgoing day; the
    // member's current placements live on the source day. Both must be
    // tracked.
    let mut class_keys: HashSet<(SchoolClassId, u8)> = HashSet::new();
    let mut teacher_keys: HashSet<(TeacherId, u8)> = HashSet::new();
    for (lesson_id, dest_day) in chain_members {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("chain member lesson resolves");
        // Source day is the day the member currently occupies; locate it
        // from its placements.
        let mut source_days: HashSet<u8> = HashSet::new();
        for p in placements.iter() {
            if p.lesson_id != *lesson_id {
                continue;
            }
            if let Some(tb) = tb_lookup.get(&p.time_block_id) {
                source_days.insert(tb.day_of_week);
            }
        }
        for src in &source_days {
            for class in &lesson.school_class_ids {
                class_keys.insert((*class, *src));
            }
            teacher_keys.insert((lesson.teacher_id, *src));
        }
        for class in &lesson.school_class_ids {
            class_keys.insert((*class, *dest_day));
        }
        teacher_keys.insert((lesson.teacher_id, *dest_day));
    }

    let mut class_pre: HashMap<(SchoolClassId, u8), u32> = HashMap::new();
    for key in &class_keys {
        let g = state
            .class_positions
            .get(key)
            .map(|v| gap_count(v))
            .unwrap_or(0);
        class_pre.insert(*key, g);
    }
    let mut teacher_pre: HashMap<(TeacherId, u8), u32> = HashMap::new();
    for key in &teacher_keys {
        let g = state
            .teacher_positions
            .get(key)
            .map(|v| gap_count(v))
            .unwrap_or(0);
        teacher_pre.insert(*key, g);
    }

    KempePartitionSnapshot {
        class_pre,
        teacher_pre,
    }
}

/// Pre-attempt partition gap snapshot. Used by `kempe_attempt` to compute
/// the post-swap soft-score delta exactly without recomputing the entire
/// `score_solution`.
struct KempePartitionSnapshot {
    class_pre: HashMap<(SchoolClassId, u8), u32>,
    teacher_pre: HashMap<(TeacherId, u8), u32>,
}

/// Compute the post-apply gap delta for every snapshotted partition against
/// the now-mutated `state`. Returns the weighted total class+teacher gap
/// delta (signed).
fn kempe_post_score_delta(
    snapshot: &KempePartitionSnapshot,
    state: &crate::solve::GreedyState,
    weights: &ConstraintWeights,
) -> i64 {
    let mut class_delta: i64 = 0;
    for (key, pre) in &snapshot.class_pre {
        let post = state
            .class_positions
            .get(key)
            .map(|v| gap_count(v))
            .unwrap_or(0);
        class_delta += i64::from(post) - i64::from(*pre);
    }
    let mut teacher_delta: i64 = 0;
    for (key, pre) in &snapshot.teacher_pre {
        let post = state
            .teacher_positions
            .get(key)
            .map(|v| gap_count(v))
            .unwrap_or(0);
        teacher_delta += i64::from(post) - i64::from(*pre);
    }
    i64::from(weights.class_gap) * class_delta + i64::from(weights.teacher_gap) * teacher_delta
}

/// Sum of subject_pref over a chain member's *new* placements at the
/// destination window with the chosen room. Used after apply to add the
/// post-swap subject_pref contribution to the delta.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn kempe_apply_subject_pref(
    problem: &Problem,
    lesson: &Lesson,
    subject: &Subject,
    dest_day: u8,
    start_pos: u8,
    weights: &ConstraintWeights,
    max_position_per_day: &HashMap<u8, u8>,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
) -> u32 {
    let n = lesson.preferred_block_size;
    let max_pos = max_position_per_day
        .get(&dest_day)
        .copied()
        .unwrap_or(start_pos + n - 1);
    let mut total: u32 = 0;
    for k in 0..n {
        let tb_id = tb_by_day_pos[&(dest_day, start_pos + k)];
        let tb = problem
            .time_blocks
            .iter()
            .find(|t| t.id == tb_id)
            .expect("tb_by_day_pos points at an existing time-block");
        total = total.saturating_add(crate::score::subject_preference_score(
            subject, tb, max_pos, weights,
        ));
    }
    total
}

/// Apply one chain member's swap: insert N rows at `(dest_day, start_pos..)`
/// with `room_id`, increment all bookkeeping. Mirrors `replay_placement`
/// across a window of N consecutive positions.
fn kempe_apply_block(
    lesson: &Lesson,
    dest_day: u8,
    start_pos: u8,
    room_id: RoomId,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
) {
    let n = lesson.preferred_block_size;
    for k in 0..n {
        let pos = start_pos + k;
        let tb_id = tb_by_day_pos[&(dest_day, pos)];
        placements.push(Placement {
            lesson_id: lesson.id,
            time_block_id: tb_id,
            room_id,
        });
        state.used_teacher.insert((lesson.teacher_id, tb_id));
        for class in &lesson.school_class_ids {
            state.used_class.insert((*class, tb_id));
            let part = state.class_positions.entry((*class, dest_day)).or_default();
            let ins = part.binary_search(&pos).unwrap_or_else(|i| i);
            if part.get(ins).copied() != Some(pos) {
                part.insert(ins, pos);
            }
        }
        state.used_room.insert((room_id, tb_id));
        let part = state
            .teacher_positions
            .entry((lesson.teacher_id, dest_day))
            .or_default();
        let ins = part.binary_search(&pos).unwrap_or_else(|i| i);
        if part.get(ins).copied() != Some(pos) {
            part.insert(ins, pos);
        }
        *state.hours_by_teacher.entry(lesson.teacher_id).or_insert(0) += 1;
        for class in &lesson.school_class_ids {
            let key = (*class, dest_day, lesson.subject_id);
            let entry = state.locked_room.entry(key).or_insert((room_id, 0));
            entry.1 += 1;
            // Subject-hour cap counter increments by 1 per row.
            *state.subject_hours_by_class_day.entry(key).or_insert(0) += 1;
        }
    }
    // Per-block lesson-cap counter: +1 per block (one call to this fn).
    for class in &lesson.school_class_ids {
        *state
            .lessons_by_class_day
            .entry((*class, dest_day))
            .or_insert(0) += 1;
    }
}

/// Run one Kempe-chain attempt: pick a block-anchor seed, pick a target day,
/// build the BFS chain over the teacher+class conflict graph at the
/// destination window, swap atomically. Asymmetric acceptance: any chain
/// abort or apply failure rolls back to the pre-attempt snapshot. Returns
/// true when the swap was accepted, false when rejected.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn kempe_attempt(
    problem: &Problem,
    idx: &Indexed,
    weights: &ConstraintWeights,
    kempe_rng: &mut SmallRng,
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    subject_lookup: &HashMap<SubjectId, &Subject>,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
    pinned: &HashSet<LessonId>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
    room_order: &[usize],
    max_position_per_day: &HashMap<u8, u8>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    lahc_list: &[u32],
    iter: u64,
) -> bool {
    let pre_score = state.soft_score;

    // Seed pick: rr_collect_anchors filters (lesson, day) where FFD packed
    // multiple N=1 blocks of the same lesson on one day. See its doc
    // comment for the single-anchor-per-block invariant.
    let anchors = rr_collect_anchors(placements, lesson_lookup, tb_lookup, pinned);
    if anchors.is_empty() {
        return false;
    }

    // Always consume two random draws so the K-RNG sequence is invariant
    // across early-abort branches; mirrors the Change move's two-draw
    // invariance for determinism.
    let anchor_idx = kempe_rng.random_range(0..anchors.len());
    let day_offset = kempe_rng.random_range(0..7u8);

    let (seed_lesson_id, source_day) = anchors[anchor_idx];
    // Resample target day from 0..7 excluding source_day. day_offset is in
    // 0..7; bumping by 1 when >= source_day gives a uniform draw over
    // 0..7 \ {source_day}. The week-scheme may have fewer than 7 days, in
    // which case the window-verification step below catches the missing
    // tb and aborts cleanly.
    let dest_day: u8 = if day_offset >= source_day {
        // 0..7 has 7 values; we want to skip source_day, so day_offset in
        // 0..6 indexes into 0..7 \ {source_day}. Clamp here when day_offset
        // == 6 by treating 6 as "the last element" (== 7-1).
        let candidate = day_offset + 1;
        if candidate >= 7 {
            // day_offset == 6 and source_day <= 6 means candidate == 7,
            // which is out of range. Fall back to source_day - 1 if
            // source_day > 0, else abort.
            if source_day > 0 {
                source_day - 1
            } else {
                return false;
            }
        } else {
            candidate
        }
    } else {
        day_offset
    };
    if dest_day == source_day {
        return false;
    }

    let seed_lesson = match lesson_lookup.get(&seed_lesson_id) {
        Some(l) => *l,
        None => return false,
    };

    // Locate the seed block's start position: pick the lowest-position
    // placement of `seed_lesson_id` on `source_day`. Block-anchor
    // contiguity is guaranteed by construction.
    let mut start_pos_opt: Option<u8> = None;
    for placement in placements.iter() {
        if placement.lesson_id != seed_lesson_id {
            continue;
        }
        let tb = match tb_lookup.get(&placement.time_block_id) {
            Some(t) => *t,
            None => continue,
        };
        if tb.day_of_week != source_day {
            continue;
        }
        start_pos_opt = match start_pos_opt {
            None => Some(tb.position),
            Some(prev) => Some(prev.min(tb.position)),
        };
    }
    let start_pos = match start_pos_opt {
        Some(p) => p,
        None => return false,
    };

    // Window verification for the seed at its destination.
    let n_seed = seed_lesson.preferred_block_size;
    for k in 0..n_seed {
        if !tb_by_day_pos.contains_key(&(dest_day, start_pos + k)) {
            return false;
        }
    }

    // Build the chain via BFS. Aborts on pin/group/over-bound/missing-window.
    let chain = match kempe_build_chain(
        seed_lesson_id,
        source_day,
        dest_day,
        start_pos,
        placements,
        lesson_lookup,
        tb_lookup,
        tb_by_day_pos,
        pinned,
    ) {
        ChainBuild::Built(c) => c,
        ChainBuild::Aborted => return false,
    };

    // Snapshot + remove every chain member, in deterministic order
    // (LessonId.0 ascending). Snapshots track the source day so rollback
    // knows where to replay.
    let mut chain_order: Vec<LessonId> = chain.keys().copied().collect();
    chain_order.sort_unstable_by_key(|id| id.0);

    // Snapshot pre-attempt partition gap counts. `removed_subject_pref` is
    // accumulated below from the rr_ruin_block snapshots (the actual rows
    // moved), since a chain member with another untouched block on a
    // different day must not contribute to the delta.
    let chain_with_dest: Vec<(LessonId, u8)> =
        chain_order.iter().map(|id| (*id, chain[id])).collect();
    let partition_snapshot = kempe_snapshot_pre_score(
        &chain_with_dest,
        lesson_lookup,
        tb_lookup,
        placements,
        state,
    );

    let mut removed_subject_pref: u32 = 0;
    let mut snapshots: Vec<(LessonId, u8, BlockSnapshot)> = Vec::with_capacity(chain_order.len());
    for &lesson_id in &chain_order {
        let lesson = match lesson_lookup.get(&lesson_id) {
            Some(l) => *l,
            None => {
                kempe_rollback(
                    &[],
                    &snapshots,
                    lesson_lookup,
                    tb_lookup,
                    tb_by_day_pos,
                    placements,
                    state,
                );
                state.soft_score = pre_score;
                return false;
            }
        };
        let dest = chain[&lesson_id];
        let src = if dest == dest_day {
            source_day
        } else {
            dest_day
        };
        let anchor_idx_opt = placements.iter().position(|p| {
            p.lesson_id == lesson_id
                && tb_lookup
                    .get(&p.time_block_id)
                    .is_some_and(|tb| tb.day_of_week == src)
        });
        let anchor_idx = match anchor_idx_opt {
            Some(i) => i,
            None => {
                kempe_rollback(
                    &[],
                    &snapshots,
                    lesson_lookup,
                    tb_lookup,
                    tb_by_day_pos,
                    placements,
                    state,
                );
                state.soft_score = pre_score;
                return false;
            }
        };
        let snap = rr_ruin_block(anchor_idx, lesson, tb_lookup, placements, state);
        let subject = subject_lookup
            .get(&lesson.subject_id)
            .expect("chain member subject resolves");
        for row in &snap.rows {
            let Some(tb) = tb_lookup.get(&row.time_block_id) else {
                continue;
            };
            let max_pos = max_position_per_day
                .get(&tb.day_of_week)
                .copied()
                .unwrap_or(tb.position);
            removed_subject_pref = removed_subject_pref.saturating_add(
                crate::score::subject_preference_score(subject, tb, max_pos, weights),
            );
        }
        snapshots.push((lesson_id, src, snap));
    }

    // Apply: re-place each chain member at its destination window. Track
    // newly-added subject_pref contributions and the (dest_day, start_pos)
    // of every recreated block so rollback can target only the rows the
    // apply added (other lessons' same-day placements must survive).
    let mut recreated_in_order: Vec<(LessonId, u8, u8)> = Vec::with_capacity(chain_order.len());
    let mut added_subject_pref: u32 = 0;
    let mut failed = false;
    for &lesson_id in &chain_order {
        let lesson = match lesson_lookup.get(&lesson_id) {
            Some(l) => *l,
            None => {
                failed = true;
                break;
            }
        };
        let dest = chain[&lesson_id];
        let original_room_id = snapshots
            .iter()
            .find(|(id, _, _)| *id == lesson_id)
            .map(|(_, _, snap)| snap.rows[0].room_id)
            .expect("snapshot for chain member exists");
        let room_id = match kempe_pick_room(
            problem,
            idx,
            lesson,
            original_room_id,
            dest,
            start_pos,
            tb_by_day_pos,
            state,
            room_order,
        ) {
            Some(r) => r,
            None => {
                failed = true;
                break;
            }
        };
        let subject = subject_lookup
            .get(&lesson.subject_id)
            .copied()
            .expect("lesson subject resolves");
        // Cap legality (ADR 0033). A chain member adds n = preferred_block_size
        // hours of subject and 1 lesson at dest for every member class. Mirror
        // the check in try_change_move so Kempe cannot swap a block onto a
        // destination day where the same class would exceed
        // Subject.max_hours_per_day or SchoolClass.max_lessons_per_day.
        // GreedyState bookkeeping already reflects the ruined chain members
        // plus any earlier successfully-recreated members, so a per-member
        // check against the live counts captures the cumulative chain effect.
        let n = lesson.preferred_block_size;
        let cap_violated = lesson.school_class_ids.iter().any(|class| {
            let key = (*class, dest, lesson.subject_id);
            let current_hours = state
                .subject_hours_by_class_day
                .get(&key)
                .copied()
                .unwrap_or(0);
            if current_hours.saturating_add(n) > subject.max_hours_per_day {
                return true;
            }
            if let Some(cap) = class_max_lessons_per_day.get(class).copied() {
                let lessons_today = state
                    .lessons_by_class_day
                    .get(&(*class, dest))
                    .copied()
                    .unwrap_or(0);
                if lessons_today.saturating_add(1) > cap {
                    return true;
                }
            }
            false
        });
        if cap_violated {
            failed = true;
            break;
        }
        added_subject_pref = added_subject_pref.saturating_add(kempe_apply_subject_pref(
            problem,
            lesson,
            subject,
            dest,
            start_pos,
            weights,
            max_position_per_day,
            tb_by_day_pos,
        ));
        kempe_apply_block(
            lesson,
            dest,
            start_pos,
            room_id,
            tb_by_day_pos,
            placements,
            state,
        );
        recreated_in_order.push((lesson_id, dest, start_pos));
    }

    if failed {
        kempe_rollback(
            &recreated_in_order,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            tb_by_day_pos,
            placements,
            state,
        );
        state.soft_score = pre_score;
        return false;
    }

    let gap_delta = kempe_post_score_delta(&partition_snapshot, state, weights);
    let subject_pref_delta = i64::from(added_subject_pref) - i64::from(removed_subject_pref);
    let total_delta = gap_delta + subject_pref_delta;
    let new_score_signed = i64::from(pre_score) + total_delta;
    let new_score = u32::try_from(new_score_signed.max(0)).unwrap_or(u32::MAX);
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let lahc_ok = new_score <= pre_score || new_score <= prior;
    if !lahc_ok {
        kempe_rollback(
            &recreated_in_order,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            tb_by_day_pos,
            placements,
            state,
        );
        state.soft_score = pre_score;
        return false;
    }
    state.soft_score = new_score;
    true
}

/// Roll back a partial or complete Kempe attempt. For each chain member that
/// was successfully re-placed, undo only the rows added at the chain
/// member's destination window (a different lesson's same-day pre-existing
/// placements must survive rollback). Then for each snapshot, replay the
/// original placement rows back into `placements` + `state`.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn kempe_rollback(
    recreated: &[(LessonId, u8, u8)],
    snapshots: &[(LessonId, u8, BlockSnapshot)],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
) {
    for (lesson_id, dest_day, start_pos) in recreated.iter().rev() {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("recreated lesson resolves");
        let n = lesson.preferred_block_size;
        // Identify the exact (lesson, time_block_id) rows we added at the
        // destination window. Remove them in reverse order so vec indices
        // do not shift while we operate on later rows.
        let mut rows_to_remove: Vec<usize> = Vec::with_capacity(usize::from(n));
        for k in 0..n {
            let pos = start_pos + k;
            let tb_id = tb_by_day_pos[&(*dest_day, pos)];
            if let Some(idx) = placements
                .iter()
                .position(|p| p.lesson_id == *lesson_id && p.time_block_id == tb_id)
            {
                rows_to_remove.push(idx);
            }
        }
        let block_was_present = !rows_to_remove.is_empty();
        rows_to_remove.sort_unstable();
        for &idx in rows_to_remove.iter().rev() {
            let p = placements.remove(idx);
            let tb = tb_lookup
                .get(&p.time_block_id)
                .expect("rollback tb resolves");
            let day = tb.day_of_week;
            let position = tb.position;
            state
                .used_teacher
                .remove(&(lesson.teacher_id, p.time_block_id));
            for class in &lesson.school_class_ids {
                state.used_class.remove(&(*class, p.time_block_id));
                if let Some(part) = state.class_positions.get_mut(&(*class, day)) {
                    if let Ok(j) = part.binary_search(&position) {
                        part.remove(j);
                    }
                    if part.is_empty() {
                        state.class_positions.remove(&(*class, day));
                    }
                }
            }
            state.used_room.remove(&(p.room_id, p.time_block_id));
            if let Some(part) = state.teacher_positions.get_mut(&(lesson.teacher_id, day)) {
                if let Ok(j) = part.binary_search(&position) {
                    part.remove(j);
                }
                if part.is_empty() {
                    state.teacher_positions.remove(&(lesson.teacher_id, day));
                }
            }
            if let Some(h) = state.hours_by_teacher.get_mut(&lesson.teacher_id) {
                *h = h.saturating_sub(1);
            }
            for class in &lesson.school_class_ids {
                let key = (*class, day, lesson.subject_id);
                if let Some(entry) = state.locked_room.get_mut(&key) {
                    entry.1 = entry.1.saturating_sub(1);
                    if entry.1 == 0 {
                        state.locked_room.remove(&key);
                    }
                }
                // Subject-hour cap counter decrements by 1 per removed row.
                if let Some(h) = state.subject_hours_by_class_day.get_mut(&key) {
                    *h = h.saturating_sub(1);
                    if *h == 0 {
                        state.subject_hours_by_class_day.remove(&key);
                    }
                }
            }
        }
        // Per-block lesson-cap counter: -1 once per recreated block we just
        // tore down (matches `kempe_apply_block`'s per-call +1).
        if block_was_present {
            for class in &lesson.school_class_ids {
                let lesson_key = (*class, *dest_day);
                if let Some(c) = state.lessons_by_class_day.get_mut(&lesson_key) {
                    *c = c.saturating_sub(1);
                    if *c == 0 {
                        state.lessons_by_class_day.remove(&lesson_key);
                    }
                }
            }
        }
    }
    for (lesson_id, _src_day, snapshot) in snapshots.iter().rev() {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("snapshot lesson resolves");
        for row in snapshot.rows.iter().rev() {
            replay_placement(lesson, row, tb_lookup, placements, state);
        }
        // Per-block lesson-cap counter: +1 per replayed snapshot block.
        if let Some(first) = snapshot.rows.first() {
            if let Some(tb) = tb_lookup.get(&first.time_block_id) {
                let day = tb.day_of_week;
                for class in &lesson.school_class_ids {
                    *state.lessons_by_class_day.entry((*class, day)).or_insert(0) += 1;
                }
            }
        }
    }
}

/// Recompute the running-score slice (`class_gap + teacher_gap + subject_pref`)
/// from `placements`. Matches the slice greedy / Change / Kempe maintain on
/// `state.soft_score`. R&R uses this after a successful recreate because
/// `rr_ruin_block` does not decrement the removed contribution and a fresh
/// `score::score_solution` would over-count by `class_day_balance + home_room`.
fn running_slice_from_placements(
    problem: &Problem,
    placements: &[Placement],
    weights: &ConstraintWeights,
    max_position_per_day: &HashMap<u8, u8>,
) -> u32 {
    let lesson_lookup: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
    let subject_lookup: HashMap<SubjectId, &Subject> =
        problem.subjects.iter().map(|s| (s.id, s)).collect();
    let mut by_class_day: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    let mut by_teacher_day: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
    let mut subject_pref_total: u32 = 0;
    for p in placements {
        let Some(lesson) = lesson_lookup.get(&p.lesson_id) else {
            continue;
        };
        let Some(tb) = tb_lookup.get(&p.time_block_id) else {
            continue;
        };
        let Some(subject) = subject_lookup.get(&lesson.subject_id) else {
            continue;
        };
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
        let max_pos = max_position_per_day
            .get(&tb.day_of_week)
            .copied()
            .unwrap_or(tb.position);
        subject_pref_total = subject_pref_total.saturating_add(
            crate::score::subject_preference_score(subject, tb, max_pos, weights),
        );
    }
    let class_gap_total: u32 = by_class_day
        .into_values()
        .map(|mut v| {
            v.sort_unstable();
            v.dedup();
            gap_count(&v)
        })
        .sum();
    let teacher_gap_total: u32 = by_teacher_day
        .into_values()
        .map(|mut v| {
            v.sort_unstable();
            v.dedup();
            gap_count(&v)
        })
        .sum();
    weights
        .class_gap
        .saturating_mul(class_gap_total)
        .saturating_add(weights.teacher_gap.saturating_mul(teacher_gap_total))
        .saturating_add(subject_pref_total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubjectId;
    use uuid::Uuid;

    fn lahc_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn vec_part(xs: &[u8]) -> Vec<u8> {
        xs.to_vec()
    }

    #[test]
    fn rr_ruin_block_removes_single_hour_lesson_from_state() {
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_id = LessonId(lahc_uuid(60));
        let tb = TimeBlock {
            id: TimeBlockId(lahc_uuid(10)),
            day_of_week: 0,
            position: 0,
        };

        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_id: teacher,
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };

        let mut placements = vec![Placement {
            lesson_id,
            time_block_id: tb.id,
            room_id: room,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher, tb.id));
        state.used_class.insert((class, tb.id));
        state.used_room.insert((room, tb.id));
        state.class_positions.insert((class, 0), vec_part(&[0]));
        state.teacher_positions.insert((teacher, 0), vec_part(&[0]));
        *state.hours_by_teacher.entry(teacher).or_insert(0) = 1;
        state.locked_room.insert((class, 0, subject), (room, 1));

        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> = std::iter::once((tb.id, &tb)).collect();
        let snapshot = rr_ruin_block(0, &lesson, &tb_lookup, &mut placements, &mut state);

        assert_eq!(placements.len(), 0);
        assert!(!state.used_teacher.contains(&(teacher, tb.id)));
        assert!(!state.used_class.contains(&(class, tb.id)));
        assert!(!state.used_room.contains(&(room, tb.id)));
        assert!(!state.class_positions.contains_key(&(class, 0)));
        assert!(!state.teacher_positions.contains_key(&(teacher, 0)));
        assert_eq!(state.hours_by_teacher.get(&teacher).copied(), Some(0));
        assert!(!state.locked_room.contains_key(&(class, 0, subject)));
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].lesson_id, lesson_id);
    }

    #[test]
    fn rr_ruin_block_keeps_locked_room_when_partial() {
        // Two lessons share (class, day=0, subject); the lock count is 2 before
        // ruin and 1 after.
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_a = LessonId(lahc_uuid(60));
        let lesson_b = LessonId(lahc_uuid(61));
        let tb_a = TimeBlock {
            id: TimeBlockId(lahc_uuid(10)),
            day_of_week: 0,
            position: 0,
        };
        let tb_b = TimeBlock {
            id: TimeBlockId(lahc_uuid(11)),
            day_of_week: 0,
            position: 1,
        };

        let lesson_a_obj = Lesson {
            id: lesson_a,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_id: teacher,
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };

        let mut placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_a.id,
                room_id: room,
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_b.id,
                room_id: room,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.locked_room.insert((class, 0, subject), (room, 2));

        let mut tb_lookup: HashMap<TimeBlockId, &TimeBlock> = HashMap::new();
        tb_lookup.insert(tb_a.id, &tb_a);
        tb_lookup.insert(tb_b.id, &tb_b);
        rr_ruin_block(0, &lesson_a_obj, &tb_lookup, &mut placements, &mut state);

        assert_eq!(
            state.locked_room.get(&(class, 0, subject)),
            Some(&(room, 1))
        );
    }

    #[test]
    fn rr_ruin_block_removes_doppelstunde_atomically() {
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_id = LessonId(lahc_uuid(60));
        let tb_a = TimeBlock {
            id: TimeBlockId(lahc_uuid(10)),
            day_of_week: 0,
            position: 0,
        };
        let tb_b = TimeBlock {
            id: TimeBlockId(lahc_uuid(11)),
            day_of_week: 0,
            position: 1,
        };

        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_id: teacher,
            hours_per_week: 2,
            preferred_block_size: 2,
            lesson_group_id: None,
        };

        let mut placements = vec![
            Placement {
                lesson_id,
                time_block_id: tb_a.id,
                room_id: room,
            },
            Placement {
                lesson_id,
                time_block_id: tb_b.id,
                room_id: room,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher, tb_a.id));
        state.used_teacher.insert((teacher, tb_b.id));
        state.used_class.insert((class, tb_a.id));
        state.used_class.insert((class, tb_b.id));
        state.used_room.insert((room, tb_a.id));
        state.used_room.insert((room, tb_b.id));
        state.class_positions.insert((class, 0), vec_part(&[0, 1]));
        state
            .teacher_positions
            .insert((teacher, 0), vec_part(&[0, 1]));
        *state.hours_by_teacher.entry(teacher).or_insert(0) = 2;
        state.locked_room.insert((class, 0, subject), (room, 2));

        let mut tb_lookup: HashMap<TimeBlockId, &TimeBlock> = HashMap::new();
        tb_lookup.insert(tb_a.id, &tb_a);
        tb_lookup.insert(tb_b.id, &tb_b);
        let snapshot = rr_ruin_block(0, &lesson, &tb_lookup, &mut placements, &mut state);

        assert_eq!(placements.len(), 0);
        assert_eq!(snapshot.rows.len(), 2);
        assert_eq!(state.hours_by_teacher.get(&teacher).copied(), Some(0));
        assert!(!state.locked_room.contains_key(&(class, 0, subject)));
    }

    #[test]
    fn rr_collect_anchors_skips_pinned_and_grouped_lessons() {
        use crate::ids::LessonGroupId;

        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_free = LessonId(lahc_uuid(60));
        let lesson_pinned = LessonId(lahc_uuid(61));
        let lesson_grouped = LessonId(lahc_uuid(62));
        let group_id = LessonGroupId(lahc_uuid(70));

        let lessons = [
            Lesson {
                id: lesson_free,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
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
                id: lesson_grouped,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: Some(group_id),
            },
        ];

        let tbs: Vec<TimeBlock> = (0..3)
            .map(|i| TimeBlock {
                id: TimeBlockId(lahc_uuid(10 + i)),
                day_of_week: 0,
                position: i,
            })
            .collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            tbs.iter().map(|tb| (tb.id, tb)).collect();

        let placements = vec![
            Placement {
                lesson_id: lesson_free,
                time_block_id: tbs[0].id,
                room_id: room,
            },
            Placement {
                lesson_id: lesson_pinned,
                time_block_id: tbs[1].id,
                room_id: room,
            },
            Placement {
                lesson_id: lesson_grouped,
                time_block_id: tbs[2].id,
                room_id: room,
            },
        ];

        let pinned: HashSet<LessonId> = [lesson_pinned].into_iter().collect();
        let lesson_lookup: HashMap<LessonId, &Lesson> = lessons.iter().map(|l| (l.id, l)).collect();

        let anchors = rr_collect_anchors(&placements, &lesson_lookup, &tb_lookup, &pinned);

        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].0, lesson_free);
        assert_eq!(anchors[0].1, 0);
    }

    #[test]
    fn gap_count_after_swap_no_op_when_old_equals_new() {
        let positions = [0u8, 2, 4];
        assert_eq!(gap_count_after_swap(&positions, 2, 2), 2);
    }

    #[test]
    fn gap_count_after_swap_fills_gap() {
        let positions = [0u8, 2, 4];
        assert_eq!(gap_count_after_swap(&positions, 2, 1), 2);
    }

    #[test]
    fn gap_count_after_swap_perfectly_compacts() {
        let positions = [0u8, 2, 4];
        assert_eq!(gap_count_after_swap(&positions, 4, 1), 0);
    }

    #[test]
    fn gap_count_after_swap_extends_span() {
        let positions = [0u8, 1];
        assert_eq!(gap_count_after_swap(&positions, 1, 5), 4);
    }

    #[test]
    fn gap_count_after_swap_target_already_present_dedupes() {
        let positions = [0u8, 1, 2];
        assert_eq!(gap_count_after_swap(&positions, 0, 1), 0);
    }

    #[test]
    fn partition_delta_same_day_compacts_drops_score() {
        let mut class_positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        let class = SchoolClassId(lahc_uuid(50));
        class_positions.insert((class, 0), vec_part(&[0, 2, 4]));
        let teacher_positions: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
        let teacher = TeacherId(lahc_uuid(20));
        let delta = score_after_change_move(
            &[class],
            teacher,
            0,
            4,
            0,
            1,
            &class_positions,
            &teacher_positions,
            &ConstraintWeights {
                class_gap: 1,
                teacher_gap: 1,
                ..ConstraintWeights::default()
            },
        );
        assert_eq!(delta, -2);
    }

    #[test]
    fn partition_delta_cross_day_zero_when_both_partitions_unaffected() {
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let mut class_positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        class_positions.insert((class, 0), vec_part(&[0, 1]));
        let teacher_positions: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
        let delta = score_after_change_move(
            &[class],
            teacher,
            0,
            1,
            1,
            0,
            &class_positions,
            &teacher_positions,
            &ConstraintWeights {
                class_gap: 1,
                teacher_gap: 1,
                ..ConstraintWeights::default()
            },
        );
        assert_eq!(delta, 0);
    }

    #[test]
    fn apply_change_move_updates_placement_partitions_and_used_sets() {
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let old_tb = TimeBlock {
            id: TimeBlockId(lahc_uuid(10)),
            day_of_week: 0,
            position: 0,
        };
        let new_tb = TimeBlock {
            id: TimeBlockId(lahc_uuid(11)),
            day_of_week: 0,
            position: 1,
        };
        let old_room = RoomId(lahc_uuid(30));
        let new_room = RoomId(lahc_uuid(31));
        let lesson_id = LessonId(lahc_uuid(60));

        let mut placements = vec![Placement {
            lesson_id,
            time_block_id: old_tb.id,
            room_id: old_room,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0]));
        state.teacher_positions.insert((teacher, 0), vec_part(&[0]));
        state.used_teacher.insert((teacher, old_tb.id));
        state.used_class.insert((class, old_tb.id));
        state.used_room.insert((old_room, old_tb.id));

        let old_tb_id = old_tb.id;
        let new_tb_id = new_tb.id;
        let subject = SubjectId(lahc_uuid(40));
        state
            .locked_room
            .insert((class, old_tb.day_of_week, subject), (old_room, 1));
        apply_change_move(
            0,
            &placements[0].clone(),
            old_tb,
            new_tb,
            new_room,
            &[class],
            teacher,
            subject,
            &mut placements,
            &mut state,
        );

        assert_eq!(placements[0].time_block_id, new_tb_id);
        assert_eq!(placements[0].room_id, new_room);
        assert_eq!(
            state.class_positions.get(&(class, 0)),
            Some(&vec_part(&[1]))
        );
        assert_eq!(
            state.teacher_positions.get(&(teacher, 0)),
            Some(&vec_part(&[1]))
        );
        assert!(state.used_teacher.contains(&(teacher, new_tb_id)));
        assert!(!state.used_teacher.contains(&(teacher, old_tb_id)));
        assert!(state.used_class.contains(&(class, new_tb_id)));
        assert!(state.used_room.contains(&(new_room, new_tb_id)));
        assert!(!state.used_room.contains(&(old_room, old_tb_id)));
    }

    #[test]
    fn lahc_change_move_reduces_avoid_first_penalty_when_seed_finds_alternative() {
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson = LessonId(lahc_uuid(60));
        let tb_zero = TimeBlockId(lahc_uuid(10));
        let tb_one = TimeBlockId(lahc_uuid(11));

        let problem = Problem {
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
            ],
            teachers: vec![Teacher {
                id: teacher,
                max_hours_per_week: 10,
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
            lessons: vec![Lesson {
                id: lesson,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: teacher,
                subject_id: subject,
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let idx = crate::index::Indexed::new(&problem);

        let mut placements = vec![Placement {
            lesson_id: lesson,
            time_block_id: tb_zero,
            room_id: room,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0]));
        state.teacher_positions.insert((teacher, 0), vec_part(&[0]));
        state.used_teacher.insert((teacher, tb_zero));
        state.used_class.insert((class, tb_zero));
        state.used_room.insert((room, tb_zero));
        state.soft_score = 1; // avoid_first penalty active at position 0

        let config = SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            seed: 0,
            deadline: Some(std::time::Duration::from_millis(50)),
            // 600 iterations fill the entire 500-slot LAHC list with the
            // optimal score (0) so worsening moves are no longer accepted.
            max_iterations: Some(600),
            lahc_rr_period: None,
            lahc_kempe_period: None,
        };

        state.locked_room.insert((class, 0, subject), (room, 1));
        run(
            &problem,
            &idx,
            &config,
            &mut placements,
            &mut state,
            &HashSet::new(),
            &HashMap::new(),
            &mut SolveStats::default(),
            Instant::now(),
        );

        assert_eq!(placements.len(), 1);
        assert_eq!(
            placements[0].time_block_id, tb_one,
            "LAHC should move the avoid-first lesson off position 0"
        );
        assert_eq!(state.soft_score, 0);
    }

    #[test]
    fn lahc_does_not_move_block_placements() {
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson = LessonId(lahc_uuid(60));
        let tb_zero = TimeBlockId(lahc_uuid(10));
        let tb_one = TimeBlockId(lahc_uuid(11));
        let tb_two = TimeBlockId(lahc_uuid(12));
        let tb_three = TimeBlockId(lahc_uuid(13));

        let problem = Problem {
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
                max_hours_per_week: 10,
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
            lessons: vec![Lesson {
                id: lesson,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 2,
                preferred_block_size: 2,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: teacher,
                subject_id: subject,
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let idx = crate::index::Indexed::new(&problem);

        // Seed a block placement at positions 0, 1 (touches avoid_first at pos 0).
        let mut placements = vec![
            Placement {
                lesson_id: lesson,
                time_block_id: tb_zero,
                room_id: room,
            },
            Placement {
                lesson_id: lesson,
                time_block_id: tb_one,
                room_id: room,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0, 1]));
        state
            .teacher_positions
            .insert((teacher, 0), vec_part(&[0, 1]));
        state.used_teacher.insert((teacher, tb_zero));
        state.used_teacher.insert((teacher, tb_one));
        state.used_class.insert((class, tb_zero));
        state.used_class.insert((class, tb_one));
        state.used_room.insert((room, tb_zero));
        state.used_room.insert((room, tb_one));
        state.soft_score = 1; // avoid_first penalty active at position 0

        let config = SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            seed: 0,
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2000),
            lahc_rr_period: None,
            lahc_kempe_period: None,
        };

        state.locked_room.insert((class, 0, subject), (room, 2));
        run(
            &problem,
            &idx,
            &config,
            &mut placements,
            &mut state,
            &HashSet::new(),
            &HashMap::new(),
            &mut SolveStats::default(),
            Instant::now(),
        );

        let tb_ids: HashSet<TimeBlockId> = placements.iter().map(|p| p.time_block_id).collect();
        assert!(
            tb_ids.contains(&tb_zero) && tb_ids.contains(&tb_one),
            "block placement must not be moved by LAHC; got {:?}",
            tb_ids
        );
    }

    #[test]
    fn lahc_does_not_move_grouped_placements() {
        use crate::ids::LessonGroupId;
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class_a = SchoolClassId(lahc_uuid(50));
        let class_b = SchoolClassId(lahc_uuid(51));
        let teacher_a = TeacherId(lahc_uuid(20));
        let teacher_b = TeacherId(lahc_uuid(21));
        let subject = SubjectId(lahc_uuid(40));
        let room_a = RoomId(lahc_uuid(30));
        let room_b = RoomId(lahc_uuid(31));
        let lesson_a = LessonId(lahc_uuid(60));
        let lesson_b = LessonId(lahc_uuid(61));
        let group_id = LessonGroupId(lahc_uuid(70));
        let tb_zero = TimeBlockId(lahc_uuid(10));
        let tb_one = TimeBlockId(lahc_uuid(11));
        let tb_two = TimeBlockId(lahc_uuid(12));
        let tb_three = TimeBlockId(lahc_uuid(13));

        let problem = Problem {
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
            teachers: vec![
                Teacher {
                    id: teacher_a,
                    max_hours_per_week: 10,
                },
                Teacher {
                    id: teacher_b,
                    max_hours_per_week: 10,
                },
            ],
            rooms: vec![Room { id: room_a }, Room { id: room_b }],
            subjects: vec![Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 1,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![
                SchoolClass {
                    id: class_a,
                    home_room_id: None,
                    max_lessons_per_day: None,
                },
                SchoolClass {
                    id: class_b,
                    home_room_id: None,
                    max_lessons_per_day: None,
                },
            ],
            lessons: vec![
                Lesson {
                    id: lesson_a,
                    school_class_ids: vec![class_a, class_b],
                    subject_id: subject,
                    teacher_id: teacher_a,
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: Some(group_id),
                },
                Lesson {
                    id: lesson_b,
                    school_class_ids: vec![class_a, class_b],
                    subject_id: subject,
                    teacher_id: teacher_b,
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: Some(group_id),
                },
            ],
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
        };
        let idx = crate::index::Indexed::new(&problem);

        let mut placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_zero,
                room_id: room_a,
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_zero,
                room_id: room_b,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class_a, 0), vec_part(&[0]));
        state.class_positions.insert((class_b, 0), vec_part(&[0]));
        state
            .teacher_positions
            .insert((teacher_a, 0), vec_part(&[0]));
        state
            .teacher_positions
            .insert((teacher_b, 0), vec_part(&[0]));
        state.used_teacher.insert((teacher_a, tb_zero));
        state.used_teacher.insert((teacher_b, tb_zero));
        state.used_class.insert((class_a, tb_zero));
        state.used_class.insert((class_b, tb_zero));
        state.used_room.insert((room_a, tb_zero));
        state.used_room.insert((room_b, tb_zero));
        state.soft_score = 2;

        let config = SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            seed: 0,
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2000),
            lahc_rr_period: None,
            lahc_kempe_period: None,
        };

        state.locked_room.insert((class_a, 0, subject), (room_a, 1));
        state.locked_room.insert((class_b, 0, subject), (room_b, 1));
        run(
            &problem,
            &idx,
            &config,
            &mut placements,
            &mut state,
            &HashSet::new(),
            &HashMap::new(),
            &mut SolveStats::default(),
            Instant::now(),
        );

        let tb_ids: HashSet<TimeBlockId> = placements.iter().map(|p| p.time_block_id).collect();
        assert!(
            tb_ids.contains(&tb_zero)
                && !tb_ids.contains(&tb_one)
                && !tb_ids.contains(&tb_two)
                && !tb_ids.contains(&tb_three),
            "group placement must not be moved by LAHC; got {:?}",
            tb_ids
        );
    }

    #[test]
    fn pick_room_reuses_old_room_when_feasible() {
        let subject = SubjectId(lahc_uuid(40));
        let old_room = RoomId(lahc_uuid(30));
        let new_tb = TimeBlockId(lahc_uuid(11));

        let problem = crate::types::Problem {
            time_blocks: vec![TimeBlock {
                id: new_tb,
                day_of_week: 0,
                position: 1,
            }],
            teachers: vec![],
            rooms: vec![crate::types::Room { id: old_room }],
            subjects: vec![crate::types::Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![],
            lessons: vec![],
            teacher_qualifications: vec![],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let idx = crate::index::Indexed::new(&problem);
        let used: HashSet<(RoomId, TimeBlockId)> = HashSet::new();

        assert_eq!(
            pick_room(&problem, &idx, subject, old_room, new_tb, &used, None),
            Some(old_room)
        );
    }

    #[test]
    fn pick_room_falls_back_to_lowest_id_when_old_blocked() {
        let subject = SubjectId(lahc_uuid(40));
        let old_room = RoomId(lahc_uuid(30));
        let alt_room = RoomId(lahc_uuid(20));
        let new_tb = TimeBlockId(lahc_uuid(11));

        let problem = crate::types::Problem {
            time_blocks: vec![TimeBlock {
                id: new_tb,
                day_of_week: 0,
                position: 1,
            }],
            teachers: vec![],
            rooms: vec![
                crate::types::Room { id: old_room },
                crate::types::Room { id: alt_room },
            ],
            subjects: vec![crate::types::Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![],
            lessons: vec![],
            teacher_qualifications: vec![],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let idx = crate::index::Indexed::new(&problem);
        let mut used: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
        used.insert((old_room, new_tb));

        assert_eq!(
            pick_room(&problem, &idx, subject, old_room, new_tb, &used, None),
            Some(alt_room)
        );
    }

    #[test]
    fn pick_room_returns_none_when_all_rooms_infeasible() {
        let subject = SubjectId(lahc_uuid(40));
        let old_room = RoomId(lahc_uuid(30));
        let new_tb = TimeBlockId(lahc_uuid(11));

        let problem = crate::types::Problem {
            time_blocks: vec![TimeBlock {
                id: new_tb,
                day_of_week: 0,
                position: 1,
            }],
            teachers: vec![],
            rooms: vec![crate::types::Room { id: old_room }],
            subjects: vec![crate::types::Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![],
            lessons: vec![],
            teacher_qualifications: vec![],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let idx = crate::index::Indexed::new(&problem);
        let mut used: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
        used.insert((old_room, new_tb));

        assert_eq!(
            pick_room(&problem, &idx, subject, old_room, new_tb, &used, None),
            None
        );
    }

    #[test]
    fn rr_attempt_does_not_panic_when_lesson_has_multiple_blocks_on_same_day() {
        // Regression: when a lesson has two block placements on the same day
        // (and possibly non-contiguous indices in the placement vec), ruining
        // the first anchor removes ALL of that lesson's same-day rows, which
        // can shift indices of other anchors above OR below it. A descending
        // sort of cached indices is not enough; the solver must look up each
        // anchor's current placement index at ruin time.
        use crate::types::{
            ConstraintWeights, Lesson, Problem, Room, SchoolClass, Subject, Teacher,
            TeacherQualification,
        };

        let class = SchoolClassId(lahc_uuid(50));
        let teacher_a = TeacherId(lahc_uuid(20));
        let teacher_b = TeacherId(lahc_uuid(21));
        let subject = SubjectId(lahc_uuid(40));
        let room_a = RoomId(lahc_uuid(30));
        let room_b = RoomId(lahc_uuid(31));
        let lesson_a = LessonId(lahc_uuid(60));
        let lesson_b = LessonId(lahc_uuid(61));

        let tbs: Vec<TimeBlock> = (0..8)
            .map(|p| TimeBlock {
                id: TimeBlockId(lahc_uuid(10 + p as u8)),
                day_of_week: 0,
                position: p as u8,
            })
            .collect();

        let problem = Problem {
            time_blocks: tbs.clone(),
            teachers: vec![
                Teacher {
                    id: teacher_a,
                    max_hours_per_week: 10,
                },
                Teacher {
                    id: teacher_b,
                    max_hours_per_week: 10,
                },
            ],
            rooms: vec![Room { id: room_a }, Room { id: room_b }],
            subjects: vec![Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 0,
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
                    id: lesson_a,
                    school_class_ids: vec![class],
                    subject_id: subject,
                    teacher_id: teacher_a,
                    hours_per_week: 4,
                    preferred_block_size: 2,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_b,
                    school_class_ids: vec![class],
                    subject_id: subject,
                    teacher_id: teacher_b,
                    hours_per_week: 2,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
            ],
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
        };

        let cfg = SolveConfig {
            weights: ConstraintWeights::default(),
            seed: 42,
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2000),
            lahc_rr_period: Some(1),
            lahc_kempe_period: None,
        };

        let result = crate::solve_with_config(&problem, &cfg);
        assert!(
            result.is_ok(),
            "solve panicked or failed: {:?}",
            result.err()
        );
    }

    /// Build a tiny problem-shape used by several Kempe unit tests:
    /// `n_lessons` block_size=1 lessons all sharing class A, each with a
    /// distinct teacher and distinct lesson-id. Two days, `slots_per_day`
    /// positions per day. Single shared room. Returns `(problem, lessons,
    /// time_blocks, room)`.
    fn kempe_one_class_fixture(
        n_lessons: u8,
        slots_per_day: u8,
    ) -> (
        crate::types::Problem,
        Vec<LessonId>,
        Vec<TimeBlockId>,
        RoomId,
    ) {
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class = SchoolClassId(lahc_uuid(50));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));

        let teachers_v: Vec<Teacher> = (0..n_lessons)
            .map(|i| Teacher {
                id: TeacherId(lahc_uuid(20 + i)),
                max_hours_per_week: 40,
            })
            .collect();
        let qualifications: Vec<TeacherQualification> = teachers_v
            .iter()
            .map(|t| TeacherQualification {
                teacher_id: t.id,
                subject_id: subject,
            })
            .collect();
        let lessons_v: Vec<Lesson> = (0..n_lessons)
            .map(|i| Lesson {
                id: LessonId(lahc_uuid(60 + i)),
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teachers_v[i as usize].id,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            })
            .collect();
        let lesson_ids: Vec<LessonId> = lessons_v.iter().map(|l| l.id).collect();
        let mut time_blocks_v: Vec<TimeBlock> = Vec::new();
        let mut tb_ids: Vec<TimeBlockId> = Vec::new();
        let mut next: u8 = 0;
        for d in 0..2u8 {
            for p in 0..slots_per_day {
                let id = TimeBlockId(lahc_uuid(100 + next));
                time_blocks_v.push(TimeBlock {
                    id,
                    day_of_week: d,
                    position: p,
                });
                tb_ids.push(id);
                next += 1;
            }
        }

        let problem = Problem {
            time_blocks: time_blocks_v,
            teachers: teachers_v,
            rooms: vec![Room { id: room }],
            subjects: vec![Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![SchoolClass {
                id: class,
                home_room_id: None,
                max_lessons_per_day: None,
            }],
            lessons: lessons_v,
            teacher_qualifications: qualifications,
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        (problem, lesson_ids, tb_ids, room)
    }

    /// Index a slot from `kempe_one_class_fixture`'s tb_ids vec by (day, pos).
    fn kempe_tb_at(tb_ids: &[TimeBlockId], slots_per_day: u8, day: u8, pos: u8) -> TimeBlockId {
        tb_ids[usize::from(day) * usize::from(slots_per_day) + usize::from(pos)]
    }

    #[test]
    fn kempe_chain_swap_moves_single_block_pair_atomically() {
        // Two lessons share one class. L0 at (D=0, P=0), L1 at (D=1, P=0).
        // The BFS chain at the destination window pulls in the other lesson
        // via class conflict and swaps them atomically.
        let (problem_for_attempt, lessons, tb_ids, room) = kempe_one_class_fixture(2, 2);
        let tb_d0_p0 = kempe_tb_at(&tb_ids, 2, 0, 0);
        let tb_d1_p0 = kempe_tb_at(&tb_ids, 2, 1, 0);

        let mut placements = vec![
            Placement {
                lesson_id: lessons[0],
                time_block_id: tb_d0_p0,
                room_id: room,
            },
            Placement {
                lesson_id: lessons[1],
                time_block_id: tb_d1_p0,
                room_id: room,
            },
        ];
        let teacher0 = problem_for_attempt.lessons[0].teacher_id;
        let teacher1 = problem_for_attempt.lessons[1].teacher_id;
        let class = problem_for_attempt.lessons[0].school_class_ids[0];
        let subject = problem_for_attempt.lessons[0].subject_id;
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher0, tb_d0_p0));
        state.used_teacher.insert((teacher1, tb_d1_p0));
        state.used_class.insert((class, tb_d0_p0));
        state.used_class.insert((class, tb_d1_p0));
        state.used_room.insert((room, tb_d0_p0));
        state.used_room.insert((room, tb_d1_p0));
        state.class_positions.insert((class, 0), vec_part(&[0]));
        state.class_positions.insert((class, 1), vec_part(&[0]));
        state
            .teacher_positions
            .insert((teacher0, 0), vec_part(&[0]));
        state
            .teacher_positions
            .insert((teacher1, 1), vec_part(&[0]));
        *state.hours_by_teacher.entry(teacher0).or_insert(0) = 1;
        *state.hours_by_teacher.entry(teacher1).or_insert(0) = 1;
        state.locked_room.insert((class, 0, subject), (room, 1));
        state.locked_room.insert((class, 1, subject), (room, 1));
        state.soft_score = 0;

        let idx = crate::index::Indexed::new(&problem_for_attempt);
        let lesson_lookup: HashMap<LessonId, &Lesson> = problem_for_attempt
            .lessons
            .iter()
            .map(|l| (l.id, l))
            .collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> = problem_for_attempt
            .time_blocks
            .iter()
            .map(|tb| (tb.id, tb))
            .collect();
        let subject_lookup: HashMap<SubjectId, &Subject> = problem_for_attempt
            .subjects
            .iter()
            .map(|s| (s.id, s))
            .collect();
        let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem_for_attempt
            .time_blocks
            .iter()
            .map(|tb| ((tb.day_of_week, tb.position), tb.id))
            .collect();
        let max_position_per_day: HashMap<u8, u8> =
            problem_for_attempt
                .time_blocks
                .iter()
                .fold(HashMap::new(), |mut acc, tb| {
                    acc.entry(tb.day_of_week)
                        .and_modify(|m| *m = (*m).max(tb.position))
                        .or_insert(tb.position);
                    acc
                });
        let mut room_order: Vec<usize> = (0..problem_for_attempt.rooms.len()).collect();
        room_order.sort_unstable_by_key(|&i| problem_for_attempt.rooms[i].id.0);
        let lahc_list = vec![0u32; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();

        // Try Kempe attempts under several seeds; some seeds may pick a
        // dest day with a missing window, others will swap. The test
        // succeeds if at least one accept produces the swap.
        let mut accepted = false;
        for seed in 0u64..32 {
            let mut snap_placements = placements.clone();
            let mut snap_state = clone_state(&state);
            let mut rng = SmallRng::seed_from_u64(seed);
            let ok = kempe_attempt(
                &problem_for_attempt,
                &idx,
                &ConstraintWeights::default(),
                &mut rng,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &tb_by_day_pos,
                &pinned,
                &mut snap_placements,
                &mut snap_state,
                &room_order,
                &max_position_per_day,
                &HashMap::new(),
                &lahc_list,
                0,
            );
            if ok {
                let p0 = snap_placements
                    .iter()
                    .find(|p| p.lesson_id == lessons[0])
                    .unwrap();
                let p1 = snap_placements
                    .iter()
                    .find(|p| p.lesson_id == lessons[1])
                    .unwrap();
                if p0.time_block_id == tb_d1_p0 && p1.time_block_id == tb_d0_p0 {
                    accepted = true;
                    placements = snap_placements;
                    state = snap_state;
                    break;
                }
            }
        }
        assert!(accepted, "no seed in 0..32 produced the canonical swap");
        // Atomicity assertions on the swapped state.
        assert_eq!(placements.len(), 2);
        assert!(state.used_teacher.contains(&(teacher0, tb_d1_p0)));
        assert!(state.used_teacher.contains(&(teacher1, tb_d0_p0)));
        assert!(!state.used_teacher.contains(&(teacher0, tb_d0_p0)));
        assert!(!state.used_teacher.contains(&(teacher1, tb_d1_p0)));
    }

    /// Deep-clone a `GreedyState` for tests that snapshot pre-attempt state.
    fn clone_state(s: &crate::solve::GreedyState) -> crate::solve::GreedyState {
        crate::solve::GreedyState {
            used_teacher: s.used_teacher.clone(),
            used_class: s.used_class.clone(),
            used_room: s.used_room.clone(),
            hours_by_teacher: s.hours_by_teacher.clone(),
            class_positions: s.class_positions.clone(),
            teacher_positions: s.teacher_positions.clone(),
            locked_room: s.locked_room.clone(),
            subject_hours_by_class_day: s.subject_hours_by_class_day.clone(),
            lessons_by_class_day: s.lessons_by_class_day.clone(),
            soft_score: s.soft_score,
        }
    }

    #[test]
    fn kempe_chain_extends_through_class_conflict() {
        // Three lessons share class A; each has its own teacher. L0 at
        // (D=0, P=0), L1 at (D=1, P=0), L2 at (D=0, P=1). The chain seed at
        // L0 with target D=1 pulls in L1 via the class A conflict at the
        // destination window. L2 sits on a different position (P=1) and is
        // never on the BFS path for this seed. Asserts chain composition.
        let (problem, lessons, tb_ids, room) = kempe_one_class_fixture(3, 3);
        let tb_d0_p0 = kempe_tb_at(&tb_ids, 3, 0, 0);
        let tb_d0_p1 = kempe_tb_at(&tb_ids, 3, 0, 1);
        let tb_d1_p0 = kempe_tb_at(&tb_ids, 3, 1, 0);

        let placements = vec![
            Placement {
                lesson_id: lessons[0],
                time_block_id: tb_d0_p0,
                room_id: room,
            },
            Placement {
                lesson_id: lessons[1],
                time_block_id: tb_d1_p0,
                room_id: room,
            },
            Placement {
                lesson_id: lessons[2],
                time_block_id: tb_d0_p1,
                room_id: room,
            },
        ];
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
            .time_blocks
            .iter()
            .map(|tb| ((tb.day_of_week, tb.position), tb.id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();

        let chain = match kempe_build_chain(
            lessons[0],
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
        ) {
            ChainBuild::Built(c) => c,
            ChainBuild::Aborted => panic!("chain build aborted unexpectedly"),
        };
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[&lessons[0]], 1);
        assert_eq!(chain[&lessons[1]], 0);
        assert!(!chain.contains_key(&lessons[2]));
    }

    #[test]
    fn kempe_chain_aborts_on_pin() {
        // L0 at (D=0, P=0), L1 pinned at (D=1, P=0). The BFS at L0's dest
        // window hits L1, sees it is pinned, and aborts.
        let (problem, lessons, tb_ids, room) = kempe_one_class_fixture(2, 2);
        let tb_d0_p0 = kempe_tb_at(&tb_ids, 2, 0, 0);
        let tb_d1_p0 = kempe_tb_at(&tb_ids, 2, 1, 0);

        let placements = vec![
            Placement {
                lesson_id: lessons[0],
                time_block_id: tb_d0_p0,
                room_id: room,
            },
            Placement {
                lesson_id: lessons[1],
                time_block_id: tb_d1_p0,
                room_id: room,
            },
        ];
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
            .time_blocks
            .iter()
            .map(|tb| ((tb.day_of_week, tb.position), tb.id))
            .collect();
        let mut pinned: HashSet<LessonId> = HashSet::new();
        pinned.insert(lessons[1]);

        let outcome = kempe_build_chain(
            lessons[0],
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
        );
        assert!(matches!(outcome, ChainBuild::Aborted));
    }

    #[test]
    fn kempe_chain_aborts_on_lesson_group() {
        use crate::ids::LessonGroupId;
        // Build a 2-lesson fixture, then mutate L1 to be group-tagged.
        let (mut problem, lessons, tb_ids, room) = kempe_one_class_fixture(2, 2);
        problem.lessons[1].lesson_group_id = Some(LessonGroupId(lahc_uuid(80)));
        let tb_d0_p0 = kempe_tb_at(&tb_ids, 2, 0, 0);
        let tb_d1_p0 = kempe_tb_at(&tb_ids, 2, 1, 0);

        let placements = vec![
            Placement {
                lesson_id: lessons[0],
                time_block_id: tb_d0_p0,
                room_id: room,
            },
            Placement {
                lesson_id: lessons[1],
                time_block_id: tb_d1_p0,
                room_id: room,
            },
        ];
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
            .time_blocks
            .iter()
            .map(|tb| ((tb.day_of_week, tb.position), tb.id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();

        let outcome = kempe_build_chain(
            lessons[0],
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
        );
        assert!(matches!(outcome, ChainBuild::Aborted));
    }

    #[test]
    fn kempe_chain_aborts_on_max_length_bound() {
        // 10 lessons each holding a pair of consecutive classes (lesson i
        // has {C_i, C_(i+1) mod 10}); the daisy-chain via class overlap lets
        // BFS hop alternately between days. Even-id lessons start at
        // (D=0, P=0), odd-id at (D=1, P=0); each hop adds the next neighbour
        // and the chain exceeds KEMPE_MAX_CHAIN = 8.
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        const N: u8 = 10;
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let classes: Vec<SchoolClass> = (0..N)
            .map(|i| SchoolClass {
                id: SchoolClassId(lahc_uuid(50 + i)),
                home_room_id: None,
                max_lessons_per_day: None,
            })
            .collect();
        let teachers_v: Vec<Teacher> = (0..N)
            .map(|i| Teacher {
                id: TeacherId(lahc_uuid(20 + i)),
                max_hours_per_week: 40,
            })
            .collect();
        let qualifications: Vec<TeacherQualification> = teachers_v
            .iter()
            .map(|t| TeacherQualification {
                teacher_id: t.id,
                subject_id: subject,
            })
            .collect();
        // Lesson i has classes {i, i+1 mod N} so each consecutive lesson
        // overlaps via one class. Lesson i has teacher i.
        let lessons_v: Vec<Lesson> = (0..N)
            .map(|i| Lesson {
                id: LessonId(lahc_uuid(60 + i)),
                school_class_ids: vec![classes[i as usize].id, classes[((i + 1) % N) as usize].id],
                subject_id: subject,
                teacher_id: teachers_v[i as usize].id,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            })
            .collect();
        let lesson_ids: Vec<LessonId> = lessons_v.iter().map(|l| l.id).collect();
        let tb_d0 = TimeBlockId(lahc_uuid(100));
        let tb_d1 = TimeBlockId(lahc_uuid(101));
        let time_blocks_v = vec![
            TimeBlock {
                id: tb_d0,
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: tb_d1,
                day_of_week: 1,
                position: 0,
            },
        ];

        // Place even lessons at (D=0, P=0), odd at (D=1, P=0). With class
        // overlap between consecutive lessons, BFS hops chain alternately
        // between days, length will exceed KEMPE_MAX_CHAIN=8.
        let placements: Vec<Placement> = (0..N)
            .map(|i| Placement {
                lesson_id: lesson_ids[i as usize],
                time_block_id: if i % 2 == 0 { tb_d0 } else { tb_d1 },
                room_id: room,
            })
            .collect();

        let problem = Problem {
            time_blocks: time_blocks_v,
            teachers: teachers_v,
            rooms: vec![Room { id: room }],
            subjects: vec![Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: classes,
            lessons: lessons_v,
            teacher_qualifications: qualifications,
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
            .time_blocks
            .iter()
            .map(|tb| ((tb.day_of_week, tb.position), tb.id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();

        let outcome = kempe_build_chain(
            lesson_ids[0],
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
        );
        assert!(matches!(outcome, ChainBuild::Aborted));
    }

    #[test]
    fn kempe_chain_swap_doppelstunde_atomically() {
        // Two N=2 lessons sharing one class. L0 at (D=0, P=0..1), L1 at
        // (D=1, P=0..1). Each is one Doppelstunde. Kempe's atomic chain
        // swap moves both blocks together so both hours of each block end
        // up on the swapped day with the same room.
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class = SchoolClassId(lahc_uuid(50));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let teacher0 = TeacherId(lahc_uuid(20));
        let teacher1 = TeacherId(lahc_uuid(21));
        let lesson0 = LessonId(lahc_uuid(60));
        let lesson1 = LessonId(lahc_uuid(61));
        let tb_d0_p0 = TimeBlockId(lahc_uuid(100));
        let tb_d0_p1 = TimeBlockId(lahc_uuid(101));
        let tb_d1_p0 = TimeBlockId(lahc_uuid(102));
        let tb_d1_p1 = TimeBlockId(lahc_uuid(103));
        let problem = Problem {
            time_blocks: vec![
                TimeBlock {
                    id: tb_d0_p0,
                    day_of_week: 0,
                    position: 0,
                },
                TimeBlock {
                    id: tb_d0_p1,
                    day_of_week: 0,
                    position: 1,
                },
                TimeBlock {
                    id: tb_d1_p0,
                    day_of_week: 1,
                    position: 0,
                },
                TimeBlock {
                    id: tb_d1_p1,
                    day_of_week: 1,
                    position: 1,
                },
            ],
            teachers: vec![
                Teacher {
                    id: teacher0,
                    max_hours_per_week: 40,
                },
                Teacher {
                    id: teacher1,
                    max_hours_per_week: 40,
                },
            ],
            rooms: vec![Room { id: room }],
            subjects: vec![Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 0,
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
                    id: lesson0,
                    school_class_ids: vec![class],
                    subject_id: subject,
                    teacher_id: teacher0,
                    hours_per_week: 2,
                    preferred_block_size: 2,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson1,
                    school_class_ids: vec![class],
                    subject_id: subject,
                    teacher_id: teacher1,
                    hours_per_week: 2,
                    preferred_block_size: 2,
                    lesson_group_id: None,
                },
            ],
            teacher_qualifications: vec![
                TeacherQualification {
                    teacher_id: teacher0,
                    subject_id: subject,
                },
                TeacherQualification {
                    teacher_id: teacher1,
                    subject_id: subject,
                },
            ],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let mut placements = vec![
            Placement {
                lesson_id: lesson0,
                time_block_id: tb_d0_p0,
                room_id: room,
            },
            Placement {
                lesson_id: lesson0,
                time_block_id: tb_d0_p1,
                room_id: room,
            },
            Placement {
                lesson_id: lesson1,
                time_block_id: tb_d1_p0,
                room_id: room,
            },
            Placement {
                lesson_id: lesson1,
                time_block_id: tb_d1_p1,
                room_id: room,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher0, tb_d0_p0));
        state.used_teacher.insert((teacher0, tb_d0_p1));
        state.used_teacher.insert((teacher1, tb_d1_p0));
        state.used_teacher.insert((teacher1, tb_d1_p1));
        state.used_class.insert((class, tb_d0_p0));
        state.used_class.insert((class, tb_d0_p1));
        state.used_class.insert((class, tb_d1_p0));
        state.used_class.insert((class, tb_d1_p1));
        state.used_room.insert((room, tb_d0_p0));
        state.used_room.insert((room, tb_d0_p1));
        state.used_room.insert((room, tb_d1_p0));
        state.used_room.insert((room, tb_d1_p1));
        state.class_positions.insert((class, 0), vec_part(&[0, 1]));
        state.class_positions.insert((class, 1), vec_part(&[0, 1]));
        state
            .teacher_positions
            .insert((teacher0, 0), vec_part(&[0, 1]));
        state
            .teacher_positions
            .insert((teacher1, 1), vec_part(&[0, 1]));
        *state.hours_by_teacher.entry(teacher0).or_insert(0) = 2;
        *state.hours_by_teacher.entry(teacher1).or_insert(0) = 2;
        state.locked_room.insert((class, 0, subject), (room, 2));
        state.locked_room.insert((class, 1, subject), (room, 2));
        state.soft_score = 0;

        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
            .time_blocks
            .iter()
            .map(|tb| ((tb.day_of_week, tb.position), tb.id))
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
        let mut room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        room_order.sort_unstable_by_key(|&i| problem.rooms[i].id.0);
        let lahc_list = vec![0u32; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();

        let mut accepted = false;
        for seed in 0u64..32 {
            let mut snap_p = placements.clone();
            let mut snap_s = clone_state(&state);
            let mut rng = SmallRng::seed_from_u64(seed);
            let ok = kempe_attempt(
                &problem,
                &idx,
                &ConstraintWeights::default(),
                &mut rng,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &tb_by_day_pos,
                &pinned,
                &mut snap_p,
                &mut snap_s,
                &room_order,
                &max_position_per_day,
                &HashMap::new(),
                &lahc_list,
                0,
            );
            if ok {
                let l0_tbs: HashSet<TimeBlockId> = snap_p
                    .iter()
                    .filter(|p| p.lesson_id == lesson0)
                    .map(|p| p.time_block_id)
                    .collect();
                let l1_tbs: HashSet<TimeBlockId> = snap_p
                    .iter()
                    .filter(|p| p.lesson_id == lesson1)
                    .map(|p| p.time_block_id)
                    .collect();
                let l0_swapped: HashSet<TimeBlockId> = [tb_d1_p0, tb_d1_p1].into_iter().collect();
                let l1_swapped: HashSet<TimeBlockId> = [tb_d0_p0, tb_d0_p1].into_iter().collect();
                if l0_tbs == l0_swapped && l1_tbs == l1_swapped {
                    accepted = true;
                    placements = snap_p;
                    break;
                }
            }
        }
        assert!(accepted, "no seed produced the Doppelstunde swap");
        // Both blocks land on swapped day with the same room.
        let l0_rooms: HashSet<RoomId> = placements
            .iter()
            .filter(|p| p.lesson_id == lesson0)
            .map(|p| p.room_id)
            .collect();
        let l1_rooms: HashSet<RoomId> = placements
            .iter()
            .filter(|p| p.lesson_id == lesson1)
            .map(|p| p.room_id)
            .collect();
        assert_eq!(l0_rooms.len(), 1);
        assert_eq!(l1_rooms.len(), 1);
    }

    #[test]
    fn kempe_chain_rollback_restores_state_on_room_failure() {
        // L0 at (D=0, P=0). L1 at (D=1, P=0). Shared class. The same-room
        // hard constraint locks (class, 0, subject) -> room_a and
        // (class, 1, subject) -> room_b. After ruining the chain the locks
        // are cleared (count drops to 0 on the last placement), but on
        // apply, kempe_pick_room reads the *current* lock map: if the locks
        // still hold (e.g. seeded by a bystander pinned placement), the swap
        // is forced to land each chain member on the locked room of its
        // destination day. Use room_blocked_times to make the locked rooms
        // hard-infeasible at the destination slot so apply returns None and
        // rollback restores state.
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class_chain = SchoolClassId(lahc_uuid(50));
        let class_lock = SchoolClassId(lahc_uuid(51));
        let subject = SubjectId(lahc_uuid(40));
        let room_a = RoomId(lahc_uuid(30));
        let room_b = RoomId(lahc_uuid(31));
        let teacher0 = TeacherId(lahc_uuid(20));
        let teacher1 = TeacherId(lahc_uuid(21));
        let teacher_lock = TeacherId(lahc_uuid(22));
        let lesson0 = LessonId(lahc_uuid(60));
        let lesson1 = LessonId(lahc_uuid(61));
        let lesson_lock_d1 = LessonId(lahc_uuid(70));
        let lesson_lock_d0 = LessonId(lahc_uuid(71));
        let tb_d0_p0 = TimeBlockId(lahc_uuid(100));
        let tb_d0_p1 = TimeBlockId(lahc_uuid(101));
        let tb_d1_p0 = TimeBlockId(lahc_uuid(102));
        let tb_d1_p1 = TimeBlockId(lahc_uuid(103));
        let problem = Problem {
            time_blocks: vec![
                TimeBlock {
                    id: tb_d0_p0,
                    day_of_week: 0,
                    position: 0,
                },
                TimeBlock {
                    id: tb_d0_p1,
                    day_of_week: 0,
                    position: 1,
                },
                TimeBlock {
                    id: tb_d1_p0,
                    day_of_week: 1,
                    position: 0,
                },
                TimeBlock {
                    id: tb_d1_p1,
                    day_of_week: 1,
                    position: 1,
                },
            ],
            teachers: vec![
                Teacher {
                    id: teacher0,
                    max_hours_per_week: 40,
                },
                Teacher {
                    id: teacher1,
                    max_hours_per_week: 40,
                },
                Teacher {
                    id: teacher_lock,
                    max_hours_per_week: 40,
                },
            ],
            rooms: vec![Room { id: room_a }, Room { id: room_b }],
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
                    id: class_chain,
                    home_room_id: None,
                    max_lessons_per_day: None,
                },
                SchoolClass {
                    id: class_lock,
                    home_room_id: None,
                    max_lessons_per_day: None,
                },
            ],
            lessons: vec![
                Lesson {
                    id: lesson0,
                    school_class_ids: vec![class_chain],
                    subject_id: subject,
                    teacher_id: teacher0,
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson1,
                    school_class_ids: vec![class_chain],
                    subject_id: subject,
                    teacher_id: teacher1,
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_lock_d1,
                    school_class_ids: vec![class_lock],
                    subject_id: subject,
                    teacher_id: teacher_lock,
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_lock_d0,
                    school_class_ids: vec![class_lock],
                    subject_id: subject,
                    teacher_id: teacher_lock,
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
            ],
            teacher_qualifications: vec![
                TeacherQualification {
                    teacher_id: teacher0,
                    subject_id: subject,
                },
                TeacherQualification {
                    teacher_id: teacher1,
                    subject_id: subject,
                },
                TeacherQualification {
                    teacher_id: teacher_lock,
                    subject_id: subject,
                },
            ],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        // L0 at (D=0, P=0, room_a) and L1 at (D=1, P=0, room_b) form the
        // chain. lesson_lock_d1 at (D=1, P=0, room_a) and lesson_lock_d0 at
        // (D=0, P=0, room_b) sit on a separate class so they never enter
        // the chain via class/teacher overlap. room_blocked_times block
        // room_b at tb_d1_p0 and room_a at tb_d0_p0; after the chain ruin
        // the only remaining suitable rooms at the swap destinations are
        // held by the lock-bystanders, so apply's room scan returns None
        // and the rollback path fires.
        let problem = Problem {
            room_blocked_times: vec![
                crate::types::RoomBlockedTime {
                    room_id: room_b,
                    time_block_id: tb_d1_p0,
                },
                crate::types::RoomBlockedTime {
                    room_id: room_a,
                    time_block_id: tb_d0_p0,
                },
            ],
            ..problem
        };
        let placements_pre = vec![
            Placement {
                lesson_id: lesson0,
                time_block_id: tb_d0_p0,
                room_id: room_a,
            },
            Placement {
                lesson_id: lesson1,
                time_block_id: tb_d1_p0,
                room_id: room_b,
            },
            Placement {
                lesson_id: lesson_lock_d1,
                time_block_id: tb_d1_p0,
                room_id: room_a,
            },
            Placement {
                lesson_id: lesson_lock_d0,
                time_block_id: tb_d0_p0,
                room_id: room_b,
            },
        ];
        let mut state_pre = crate::solve::GreedyState::new();
        state_pre.used_teacher.insert((teacher0, tb_d0_p0));
        state_pre.used_teacher.insert((teacher1, tb_d1_p0));
        state_pre.used_teacher.insert((teacher_lock, tb_d1_p0));
        state_pre.used_teacher.insert((teacher_lock, tb_d0_p0));
        state_pre.used_class.insert((class_chain, tb_d0_p0));
        state_pre.used_class.insert((class_chain, tb_d1_p0));
        state_pre.used_class.insert((class_lock, tb_d1_p0));
        state_pre.used_class.insert((class_lock, tb_d0_p0));
        state_pre.used_room.insert((room_a, tb_d0_p0));
        state_pre.used_room.insert((room_b, tb_d1_p0));
        state_pre.used_room.insert((room_a, tb_d1_p0));
        state_pre.used_room.insert((room_b, tb_d0_p0));
        state_pre
            .class_positions
            .insert((class_chain, 0), vec_part(&[0]));
        state_pre
            .class_positions
            .insert((class_chain, 1), vec_part(&[0]));
        state_pre
            .class_positions
            .insert((class_lock, 0), vec_part(&[0]));
        state_pre
            .class_positions
            .insert((class_lock, 1), vec_part(&[0]));
        state_pre
            .teacher_positions
            .insert((teacher0, 0), vec_part(&[0]));
        state_pre
            .teacher_positions
            .insert((teacher1, 1), vec_part(&[0]));
        state_pre
            .teacher_positions
            .insert((teacher_lock, 0), vec_part(&[0]));
        state_pre
            .teacher_positions
            .insert((teacher_lock, 1), vec_part(&[0]));
        *state_pre.hours_by_teacher.entry(teacher0).or_insert(0) = 1;
        *state_pre.hours_by_teacher.entry(teacher1).or_insert(0) = 1;
        *state_pre.hours_by_teacher.entry(teacher_lock).or_insert(0) = 2;
        state_pre
            .locked_room
            .insert((class_chain, 0, subject), (room_a, 1));
        state_pre
            .locked_room
            .insert((class_chain, 1, subject), (room_b, 1));
        state_pre
            .locked_room
            .insert((class_lock, 0, subject), (room_b, 1));
        state_pre
            .locked_room
            .insert((class_lock, 1, subject), (room_a, 1));
        state_pre.soft_score = 0;

        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
            .time_blocks
            .iter()
            .map(|tb| ((tb.day_of_week, tb.position), tb.id))
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
        let mut room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        room_order.sort_unstable_by_key(|&i| problem.rooms[i].id.0);
        let lahc_list = vec![0u32; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();

        // Try seeds; whenever the attempt rejects, state must be byte-equal
        // to the pre-attempt snapshot. Verify at least one seed in the
        // sample triggers a rejection so the rollback path is actually
        // exercised.
        let mut saw_reject = false;
        for seed in 0u64..32 {
            let mut p = placements_pre.clone();
            let mut s = clone_state(&state_pre);
            let mut rng = SmallRng::seed_from_u64(seed);
            let accepted = kempe_attempt(
                &problem,
                &idx,
                &ConstraintWeights::default(),
                &mut rng,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &tb_by_day_pos,
                &pinned,
                &mut p,
                &mut s,
                &room_order,
                &max_position_per_day,
                &HashMap::new(),
                &lahc_list,
                0,
            );
            if accepted {
                continue;
            }
            saw_reject = true;
            // Placement vec ordering may differ after a rollback that
            // pushes snapshot rows back at the end; assert as multiset.
            let p_set: HashSet<(LessonId, TimeBlockId, RoomId)> = p
                .iter()
                .map(|x| (x.lesson_id, x.time_block_id, x.room_id))
                .collect();
            let pre_set: HashSet<(LessonId, TimeBlockId, RoomId)> = placements_pre
                .iter()
                .map(|x| (x.lesson_id, x.time_block_id, x.room_id))
                .collect();
            assert_eq!(p_set, pre_set, "seed {seed}: placements drifted");
            assert_eq!(s.used_teacher, state_pre.used_teacher);
            assert_eq!(s.used_class, state_pre.used_class);
            assert_eq!(s.used_room, state_pre.used_room);
            assert_eq!(s.class_positions, state_pre.class_positions);
            assert_eq!(s.teacher_positions, state_pre.teacher_positions);
            assert_eq!(s.hours_by_teacher, state_pre.hours_by_teacher);
            assert_eq!(s.locked_room, state_pre.locked_room);
            assert_eq!(s.soft_score, state_pre.soft_score);
        }
        assert!(
            saw_reject,
            "no seed exercised the rollback path; tighten the fixture",
        );
    }

    #[test]
    fn kempe_chain_aborts_on_window_position_missing_on_target_day() {
        // D=0 has positions 0..6 (slots_per_day=6 unused positions); D=1
        // has positions 0..1 only (single slot at position 0). B1 with N=1
        // anchored at (D=0, P=4). dest_day = 1 means we need (1, 4) which
        // does not exist. Chain build aborts at window verification.
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class = SchoolClassId(lahc_uuid(50));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let teacher = TeacherId(lahc_uuid(20));
        let lesson = LessonId(lahc_uuid(60));

        let mut time_blocks_v: Vec<TimeBlock> = (0..6u8)
            .map(|p| TimeBlock {
                id: TimeBlockId(lahc_uuid(100 + p)),
                day_of_week: 0,
                position: p,
            })
            .collect();
        time_blocks_v.push(TimeBlock {
            id: TimeBlockId(lahc_uuid(200)),
            day_of_week: 1,
            position: 0,
        });
        let tb_d0_p4 = TimeBlockId(lahc_uuid(104));
        let problem = Problem {
            time_blocks: time_blocks_v,
            teachers: vec![Teacher {
                id: teacher,
                max_hours_per_week: 40,
            }],
            rooms: vec![Room { id: room }],
            subjects: vec![Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![SchoolClass {
                id: class,
                home_room_id: None,
                max_lessons_per_day: None,
            }],
            lessons: vec![Lesson {
                id: lesson,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: teacher,
                subject_id: subject,
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let placements = vec![Placement {
            lesson_id: lesson,
            time_block_id: tb_d0_p4,
            room_id: room,
        }];
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
            .time_blocks
            .iter()
            .map(|tb| ((tb.day_of_week, tb.position), tb.id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();

        // Build chain directly: seed at D=0 P=4, target D=1, the window
        // verification at (D=1, P=4) fails because that TB is missing.
        let outcome = kempe_build_chain(
            lesson,
            0,
            1,
            4,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
        );
        assert!(matches!(outcome, ChainBuild::Aborted));
    }
}
