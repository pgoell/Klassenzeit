//! Late-acceptance hill-climbing loop that polishes the greedy's output.
//! Single Change move (move one lesson-hour to a different time-block,
//! reuse old room or fall back to lowest-id hard-feasible room),
//! deadline-bound, deterministic under (seed, max_iterations).

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use crate::index::Indexed;
use crate::score::{gap_count, gap_count_after_insert, gap_count_after_remove};
use crate::types::{
    ConstraintWeights, Lesson, Placement, Problem, SolveConfig, Subject, TimeBlock,
};

/// Length of the LAHC cost-history list. Burke & Bykov 2008 reports the
/// algorithm is robust to this value within a wide band; 500 matches the
/// archive/v2 setting and is enough fill for ~20k iterations on Hessen
/// Grundschule under a 200ms deadline.
const LAHC_LIST_LEN: usize = 500;

/// Run the LAHC loop over the placement set produced by greedy. Mutates
/// `placements` and the partition / used-* state in place via `state`. The
/// post-LAHC running total ends up in `state.soft_score`.
pub(crate) fn run(
    problem: &Problem,
    idx: &Indexed,
    config: &SolveConfig,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
    pinned: &HashSet<LessonId>,
) {
    let Some(deadline) = config.deadline else {
        return;
    };
    if placements.is_empty() {
        return;
    }
    let start = Instant::now();
    let mut change_rng = SmallRng::seed_from_u64(config.seed);
    let mut rr_rng = SmallRng::seed_from_u64(config.seed.wrapping_add(1));
    let mut lahc_list = vec![state.soft_score; LAHC_LIST_LEN];
    let lesson_lookup: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
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

    let mut iter: u64 = 0;
    while iter < max_iter && start.elapsed() < deadline {
        let is_rr_iter = config
            .lahc_rr_period
            .is_some_and(|n| n > 0 && (iter as u32) % n == 0);

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
                &lahc_list,
                iter,
            );
        }

        iter += 1;
        lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.soft_score;
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
        let tb = tb_lookup
            .get(&p.time_block_id)
            .expect("ruin: placement tb must resolve");
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
        }
        rows.push(p);
    }

    BlockSnapshot { rows }
}

/// Collect the set of `(lesson, day)` blocks eligible to be ruined by an R&R
/// attempt. Returns one tuple per block for lessons that are neither pinned
/// nor part of a lesson group. The single-anchor-per-block contract lets the
/// recreate step call `try_place_block` once per chosen anchor. Returned in a
/// deterministic order so the R&R RNG shuffle reproduces under a fixed seed.
///
/// Tuples (not placement indices) because a single ruin removes every
/// placement of a lesson on its day, which can shift indices both above and
/// below other anchors when a lesson has multiple non-contiguous block
/// placements on the same day. Callers look up the current placement index at
/// ruin time from this tuple.
fn rr_collect_anchors(
    placements: &[Placement],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    pinned: &HashSet<LessonId>,
) -> Vec<(LessonId, u8)> {
    let mut seen: HashSet<(LessonId, u8)> = HashSet::new();
    let mut anchors: Vec<(LessonId, u8)> = Vec::new();
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
        let key = (p.lesson_id, tb.day_of_week);
        if seen.insert(key) {
            anchors.push(key);
        }
    }
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

    // Recreate: walk the ruined lessons in the order they were ruined.
    let mut failed_recreates: usize = 0;
    let mut recreated_in_order: Vec<LessonId> = Vec::with_capacity(snapshots.len());
    for (lesson_id, _snap) in snapshots.iter() {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("ruined lesson must resolve");
        let n = lesson.preferred_block_size;
        let placed = crate::solve::try_place_block(
            problem,
            lesson,
            n,
            idx,
            teacher_max,
            weights,
            state,
            placements,
            tb_order,
            room_order,
            max_position_per_day,
        );
        if !placed {
            failed_recreates += 1;
        } else {
            recreated_in_order.push(*lesson_id);
        }
    }

    if failed_recreates > 0 {
        rr_rollback(
            &recreated_in_order,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
        );
        state.soft_score = pre_score;
        return false;
    }

    let new_score = state.soft_score;
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let lahc_ok = new_score <= pre_score || new_score <= prior;
    if !lahc_ok {
        rr_rollback(
            &recreated_in_order,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
        );
        state.soft_score = pre_score;
        return false;
    }

    true
}

/// Roll back a partial or complete R&R recreate. For each successfully
/// recreated lesson, ruin it again to undo the recreate's bookkeeping. Then
/// for each snapshot's rows, replay the original placement back into
/// `placements` + `state`.
fn rr_rollback(
    recreated: &[LessonId],
    snapshots: &[(LessonId, BlockSnapshot)],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
) {
    for lesson_id in recreated.iter().rev() {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("recreated lesson resolves");
        if let Some(idx) = placements.iter().position(|p| p.lesson_id == *lesson_id) {
            rr_ruin_block(idx, lesson, tb_lookup, placements, state);
        }
    }
    for (lesson_id, snapshot) in snapshots.iter().rev() {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("snapshot lesson resolves");
        for row in snapshot.rows.iter().rev() {
            replay_placement(lesson, row, tb_lookup, placements, state);
        }
    }
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
    }
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
            }],
            school_classes: vec![SchoolClass {
                id: class,
                home_room_id: None,
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
        };

        state.locked_room.insert((class, 0, subject), (room, 1));
        run(
            &problem,
            &idx,
            &config,
            &mut placements,
            &mut state,
            &HashSet::new(),
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
            }],
            school_classes: vec![SchoolClass {
                id: class,
                home_room_id: None,
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
        };

        state.locked_room.insert((class, 0, subject), (room, 2));
        run(
            &problem,
            &idx,
            &config,
            &mut placements,
            &mut state,
            &HashSet::new(),
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
            }],
            school_classes: vec![
                SchoolClass {
                    id: class_a,
                    home_room_id: None,
                },
                SchoolClass {
                    id: class_b,
                    home_room_id: None,
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
            }],
            school_classes: vec![SchoolClass {
                id: class,
                home_room_id: None,
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
        };

        let result = crate::solve_with_config(&problem, &cfg);
        assert!(
            result.is_ok(),
            "solve panicked or failed: {:?}",
            result.err()
        );
    }
}
