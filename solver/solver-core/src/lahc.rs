//! Late-acceptance hill-climbing loop that polishes the greedy's output.
//! Single Change move (move one lesson-hour to a different time-block,
//! reuse old room or fall back to lowest-id hard-feasible room),
//! deadline-bound, deterministic under (seed, max_iterations).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::ids::{
    LessonGroupId, LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId,
};
use crate::index::Indexed;
use crate::score::{gap_count, gap_count_after_insert, gap_count_after_remove};
use crate::types::{
    ConstraintWeights, Lesson, Placement, Problem, SolveConfig, SolveStats, Subject, TimeBlock,
    TimeBlockKind,
};

/// Length of the LAHC cost-history list. Burke & Bykov 2008 reports the
/// algorithm is robust to this value within a wide band; 500 matches the
/// archive/v2 setting and is enough fill for ~20k iterations on Hessen
/// Grundschule under a 200ms deadline.
const LAHC_LIST_LEN: usize = 500;

/// Resolve a lesson's currently-assigned teacher from greedy state. Item
/// 68: when `state.class_subject_teacher` carries an entry for any
/// member `(class, subject)` pair (set at first FFD placement of that
/// pair), the lock is the source of truth. Otherwise fall back to
/// `lesson.assigned_teacher_id()` (the pin shorthand).
///
/// Kempe BFS conflict-detection sites (`kempe_build_chain`'s
/// popped-vs-new-neighbour check and the bipartiteness same-color
/// cross-check) flow through this helper because they read teacher
/// identity from `&[Placement]` views without a row in hand. Row-based
/// callers (rollback, row removal) read `row.teacher_id` directly
/// instead; the row is the canonical record of which teacher actually
/// populated `state.used_teacher` at apply time, and trusting state
/// during rollback drifts (item 75).
fn lesson_teacher_in_state(state: &crate::solve::GreedyState, lesson: &Lesson) -> TeacherId {
    for class in &lesson.school_class_ids {
        if let Some(t) = state
            .class_subject_teacher
            .get(&(*class, lesson.subject_id))
        {
            return *t;
        }
    }
    lesson.assigned_teacher_id()
}

/// Run the LAHC loop over the placement set produced by greedy. Mutates
/// `placements` and the partition / used-* state in place via `state`. The
/// post-LAHC running total ends up in `state.search_score_slice`. Records
/// timing probes (`time_to_first_feasible_ms`, `time_to_optimal_ms`) into
/// `stats` against `solve_start` so the wall-clock origin is shared with
/// `solve_with_config_stats`'s entry instead of LAHC's own start.
///
/// `progress` is the optional [`crate::ProgressBeacon`] driven by the
/// public `solve_with_progress` entry. When `Some`, every iteration writes
/// the current `(iter, placement_count, best_score, feasible)` tuple to
/// the beacon and checks `cancel_requested`; setting the flag causes the
/// loop to break at the next iteration boundary. The beacon block consumes
/// no RNG draws so the deterministic `(None, Some(beacon))` byte-equality
/// contract holds across the seed sweep.
///
/// Returns `was_cancelled = true` iff the loop exited because
/// `progress.cancel_requested()` was observed.
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
    progress: Option<&std::sync::Arc<crate::ProgressBeacon>>,
) -> bool {
    let Some(deadline) = config.deadline else {
        return false;
    };
    if placements.is_empty() {
        return false;
    }
    let mut was_cancelled = false;
    let mut change_rng = SmallRng::seed_from_u64(config.seed);
    let mut rr_rng = SmallRng::seed_from_u64(config.seed.wrapping_add(1));
    let mut kempe_rng = SmallRng::seed_from_u64(config.seed.wrapping_add(2));
    let mut home_room_rng = SmallRng::seed_from_u64(config.seed.wrapping_add(3));
    let mut lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
    let lesson_lookup: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
    let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
        .school_classes
        .iter()
        .map(|c| (c.id, c.home_room_id))
        .collect();
    // Item 68 precompute pair: same shape as `solve_with_config_stats`
    // builds; used by R&R's `try_place_block` recreate calls to score
    // the prefer_class_teacher axis.
    let class_teacher_lookup: HashMap<SchoolClassId, Option<TeacherId>> = problem
        .school_classes
        .iter()
        .map(|c| (c.id, c.class_teacher_id))
        .collect();
    let mut subject_qualified_teachers: HashMap<SubjectId, HashSet<TeacherId>> = HashMap::new();
    for q in &problem.teacher_qualifications {
        subject_qualified_teachers
            .entry(q.subject_id)
            .or_default()
            .insert(q.teacher_id);
    }
    // Kempe destination lookup. Filtered to lesson-kind so the seed window
    // verification and the chain neighbour window verification in
    // `kempe_build_chain` abort cleanly when any destination position is a
    // break slot (lessons must never land on Hofpause TBs).
    let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
        .time_blocks
        .iter()
        .filter(|tb| tb.kind == TimeBlockKind::Lesson)
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
    // Break-kind time blocks are excluded for the same reason as in
    // `solve_with_config_stats_inner`: R&R recreate calls `try_place_block`
    // which iterates `tb_order` to enumerate candidate windows; a break slot
    // is never a valid lesson destination.
    let mut tb_order: Vec<usize> = (0..problem.time_blocks.len())
        .filter(|&i| problem.time_blocks[i].kind == TimeBlockKind::Lesson)
        .collect();
    tb_order.sort_unstable_by_key(|&i| {
        let tb = &problem.time_blocks[i];
        (tb.day_of_week, tb.position, tb.id.0)
    });
    let mut room_order: Vec<usize> = (0..problem.rooms.len()).collect();
    room_order.sort_unstable_by_key(|&i| problem.rooms[i].id.0);
    let teacher_max: HashMap<TeacherId, u8> = problem
        .teachers
        .iter()
        .map(|t| (t.id, t.effective_max_hours_per_week()))
        .collect();
    // Sum of `hours_per_week` across all lessons is the placement-count floor:
    // every lesson-hour materialises as one `Placement`. The LAHC loop can exit
    // early once this floor is reached AND `state.search_score_slice == 0`,
    // since no further iteration can improve a feasible objective-floor
    // incumbent.
    let placements_expected: usize = problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as usize)
        .sum();

    // Track the running-best canonical score so the time-to-optimal probe
    // can capture the wall-clock of the last improvement. LAHC accepts on
    // canonical (item 52); ttf still gates on feasibility + slice floor as
    // before. If FFD greedy already reached `canonical_score == 0` and
    // feasibility, ttf and tto are both already set by
    // `solve_with_config_stats` before LAHC runs; the `running_best`
    // initialiser still seeds correctly so a never-improving LAHC leaves
    // them untouched.
    let mut running_best = state.canonical_score;
    // Item 78: track the running-best placement count alongside canonical
    // so the R&R rescue move's feasibility-restoring accepts don't get
    // discarded at loop exit. `score_solution` does NOT include hard
    // violations (`NoFreeTimeBlock` / `TeacherOverCapacity`); adding a
    // placement strictly increases canonical (more placements -> more gap
    // / day-balance contributions). Without a placement-count axis on the
    // running-best update, a rescue that adds the missing 12th hour gets
    // restored away at `*placements = best_placements` because canonical
    // rose. The lexicographic order is (higher placement count, lower
    // canonical on tie): more placements always wins, ties broken by
    // canonical so soft-quality improvements still register.
    let mut running_best_count = placements.len();
    // Item 52: snapshot the running-best canonical placements so LAHC's
    // accept criterion (which can drift the current canonical above the
    // post-greedy canonical) does not contaminate the returned incumbent.
    // Initialised to the post-greedy placements; refreshed on every
    // running-best canonical event; restored at LAHC loop exit so the
    // returned `solution.soft_score <= greedy_solution.soft_score` invariant
    // holds across deadline / max_iter / early-exit paths.
    let mut best_placements: Vec<Placement> = placements.clone();

    let mut iter: u64 = 0;
    while iter < max_iter && solve_start.elapsed() < deadline {
        let is_rr_iter = config
            .lahc_rr_period
            .is_some_and(|n| n > 0 && (iter as u32) % n == 0);
        let is_kempe_iter = config
            .lahc_kempe_period
            .is_some_and(|n| n > 0 && (iter as u32) % n == 0)
            && !is_rr_iter;
        // Home-room repair fires only when neither R&R nor Kempe claims the
        // iteration. Mirrors the R&R/Kempe precedence ladder. Dedicated RNG
        // channel `home_room_rng` consumes exactly one draw per fired iter so
        // existing channels stay byte-identical to pre-PR behaviour when the
        // new period is `None`.
        let is_home_room_iter = config
            .lahc_home_room_period
            .is_some_and(|n| n > 0 && (iter as u32) % n == 0)
            && !is_rr_iter
            && !is_kempe_iter;

        if is_rr_iter {
            // Item 78: rescue FFD-unplaced lessons first. If rescue aborts
            // (no under-placed lessons, no same-class anchor) or rejects
            // (failed recreate, partial L placement), fall through to the
            // existing ruin-only-recreate path. Each function consumes its
            // own RNG draws from the shared `rr_rng`; the rescue branch's
            // two draws are unconditional, so the rr_rng sequence is
            // invariant across rescue's abort branches.
            let rescued = rr_rescue_attempt(
                problem,
                idx,
                &config.weights,
                &home_room_lookup,
                &class_teacher_lookup,
                &subject_qualified_teachers,
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
            );
            if !rescued {
                rr_attempt(
                    problem,
                    idx,
                    &config.weights,
                    &home_room_lookup,
                    &class_teacher_lookup,
                    &subject_qualified_teachers,
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
                    config.lahc_rr_k,
                );
            }
        } else if is_kempe_iter {
            kempe_attempt(
                problem,
                idx,
                &config.weights,
                &mut kempe_rng,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &home_room_lookup,
                &tb_by_day_pos,
                pinned,
                placements,
                state,
                &room_order,
                &max_position_per_day,
                class_max_lessons_per_day,
                &lahc_list,
                iter,
                config.lahc_kempe_max_chain as usize,
            );
        } else if is_home_room_iter {
            // One RNG draw: the placement index. The kernel maintains
            // state.canonical_score on accept; the per-iter running-best
            // check below the chain handles snapshotting best_placements.
            let placement_idx = home_room_rng.random_range(0..placements.len());
            try_home_room_repair_move(
                problem,
                idx,
                placement_idx,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &home_room_lookup,
                &config.weights,
                placements,
                state,
                pinned,
                &lahc_list,
                iter,
                &room_order,
            );
        } else {
            // Three unconditional draws per Change-branch iteration. The
            // determinism property test pins this; solver/CLAUDE.md documents
            // the 3-draw invariant. `move_selector` routes to block-aware
            // Change (selectors 0-4) or cell-cell Swap (selector 5).
            // The 5:1 ratio biases the workhorse toward Change; the bench
            // delta vs same-day master is grundschule -13.5%, zweizuegig
            // +2.5%, dreizuegig +0.75% on LAHC soft score (all within
            // BASELINE 20% budget; grundschule is a soft-score win).
            // `partner_or_tb_idx` is reinterpreted per branch (modulo
            // `time_blocks.len()` for Change, `placements.len()` for Swap).
            let placement_idx = change_rng.random_range(0..placements.len());
            let move_selector = change_rng.random_range(0..6u32);
            let partner_or_tb_idx = change_rng.random_range(0..usize::MAX);

            if move_selector < 5 {
                let new_tb_idx = partner_or_tb_idx % problem.time_blocks.len();
                try_change_block_move(
                    problem,
                    idx,
                    placement_idx,
                    new_tb_idx,
                    &lesson_lookup,
                    &tb_lookup,
                    &subject_lookup,
                    &home_room_lookup,
                    &max_position_per_day,
                    &config.weights,
                    placements,
                    state,
                    pinned,
                    class_max_lessons_per_day,
                    &lahc_list,
                    iter,
                    &tb_by_day_pos,
                    &room_order,
                );
            } else {
                let partner_idx = partner_or_tb_idx % placements.len();
                try_swap_move(
                    problem,
                    idx,
                    placement_idx,
                    partner_idx,
                    &lesson_lookup,
                    &tb_lookup,
                    &config.weights,
                    placements,
                    state,
                    pinned,
                    class_max_lessons_per_day,
                    &lahc_list,
                    iter,
                );
            }
        }

        iter += 1;
        lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.canonical_score;
        if stats.time_to_first_feasible_ms.is_none()
            && state.canonical_score == 0
            && placements.len() == placements_expected
        {
            stats.time_to_first_feasible_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }
        // Item 78: lexicographic order (placement count desc, canonical asc).
        // More placements always beats fewer; ties broken by lower canonical.
        let is_better = placements.len() > running_best_count
            || (placements.len() == running_best_count && state.canonical_score < running_best);
        if is_better {
            running_best = state.canonical_score;
            running_best_count = placements.len();
            best_placements = placements.clone();
            stats.time_to_optimal_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            state.canonical_score,
            crate::score::score_solution(
                problem,
                placements,
                &config.weights,
                &state.soft_pinned_blocks,
            ),
            "LAHC must keep state.canonical_score == score_solution(...) at every iteration tail",
        );
        // Progress beacon write + cancel check. This block consumes no RNG
        // draws so the `(None, Some(beacon))` byte-equality determinism
        // contract holds across the seed sweep (see CLAUDE.md "LAHC RNG
        // draw count is invariant across loop branches"). Reads
        // `running_best_count` / `running_best` (the lexicographic
        // incumbent) so external observers see the same `(placements,
        // canonical)` pair the loop will eventually return.
        if let Some(beacon) = progress {
            let placement_count_u64 = running_best_count as u64;
            let best_score_u64 = running_best as u64;
            let feasible_now = running_best_count >= placements_expected;
            beacon.record(iter, placement_count_u64, best_score_u64, feasible_now);
            if beacon.cancel_requested() {
                was_cancelled = true;
                break;
            }
        }
        if state.canonical_score == 0 && placements.len() == placements_expected {
            break;
        }
    }
    // Item 52: restore the running-best canonical placements at every loop
    // exit (deadline, max_iter, early-exit, or cancel). After this assignment,
    // `state.search_score_slice` and `state.canonical_score` may not match
    // `placements`; that is fine because `solve_with_config_stats` reads
    // neither field after `lahc::run` returns and recomputes
    // `solution.soft_score = score_solution(problem, &solution.placements, weights)`.
    *placements = best_placements;
    was_cancelled
}

/// n=1 Change move: lifted verbatim from the original `try_change_move`
/// body minus the `preferred_block_size > 1` skip. Routed from
/// `try_change_block_move`'s n=1 dispatch (the production call site under
/// the 3-draw RNG budget).
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn try_change_move_n1(
    problem: &Problem,
    idx: &Indexed,
    placement_idx: usize,
    new_tb_idx: usize,
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    subject_lookup: &HashMap<SubjectId, &Subject>,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
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
    if lesson.lesson_group_id.is_some() {
        return false;
    }
    // Pinned placements are caller-fixed (Problem.pinned_placements) and must
    // survive LAHC verbatim. Same RNG-invariance argument as the block / group
    // guards above: the three random_range draws are already consumed.
    if pinned.contains(&p.lesson_id) {
        return false;
    }
    let old_tb = tb_lookup[&p.time_block_id].clone();
    let new_tb = problem.time_blocks[new_tb_idx].clone();

    if new_tb.id == old_tb.id {
        return false;
    }
    // Lessons must never land on a Hofpause slot. The three random_range
    // draws for placement_idx / move_selector / partner_or_tb_idx are already
    // consumed in `run`, so the determinism RNG-budget invariant
    // (lahc_property.rs) holds.
    if new_tb.kind != TimeBlockKind::Lesson {
        return false;
    }

    let class_ids: &[SchoolClassId] = &lesson.school_class_ids;
    // Item 68: teacher comes from the placement's recorded `teacher_id`
    // (the solver-picked teacher), not `lesson.assigned_teacher_id()`
    // (the pin shorthand which mismatches when the lesson is unpinned
    // and the solver picked a non-first candidate). The Change move
    // never changes the teacher; it only moves the placement to a new
    // time block / room.
    let teacher = p.teacher_id;

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

    // Travel-buffer pruning (ADR 0044). Reject the move if the destination
    // would leave the buffered lesson adjacent to a same-class or
    // same-teacher placement (or at a day edge) without an intervening
    // break. `Some((old_tb.day_of_week, old_tb.position))` tells the helper
    // to ignore the lesson's pre-move block when scanning class/teacher
    // positions (else a simple shift would self-collide). Cheap for the
    // unbuffered hot path: the helper short-circuits when
    // pre+post == 0.
    if crate::validate::would_violate_travel_buffer(
        problem,
        state,
        placements,
        lesson,
        new_tb.id,
        teacher,
        Some((old_tb.day_of_week, old_tb.position)),
    ) {
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

    let new_score_signed = i64::from(state.search_score_slice) + delta;
    debug_assert!(
        new_score_signed >= 0,
        "running score must remain non-negative; current_score={} delta={}",
        state.search_score_slice,
        delta
    );
    let new_score = u32::try_from(new_score_signed.max(0)).unwrap_or(u32::MAX);

    // Canonical delta: slice delta + home_room delta + class_day_balance delta.
    // Both new axes short-circuit when their weight is zero or when the move
    // does not affect the axis (same-day moves leave class_day_balance
    // unchanged). Pure, allocation-free.
    let home_room_delta: i64 = if weights.prefer_home_room == 0 {
        0
    } else {
        let mut acc: i64 = 0;
        for class in class_ids {
            let old_pen = crate::score::home_room_penalty_one_class(
                *class,
                home_room_lookup,
                p.room_id,
                weights,
            );
            let new_pen = crate::score::home_room_penalty_one_class(
                *class,
                home_room_lookup,
                new_room_id,
                weights,
            );
            acc += i64::from(new_pen) - i64::from(old_pen);
        }
        acc
    };

    let class_day_balance_delta: i64 =
        if weights.class_day_balance == 0 || old_tb.day_of_week == new_tb.day_of_week {
            0
        } else {
            let days = problem
                .time_blocks
                .iter()
                .map(|tb| tb.day_of_week)
                .max()
                .map(|m| m.saturating_add(1))
                .unwrap_or(0);
            let mut acc: i64 = 0;
            for class in class_ids {
                let pre = crate::score::class_day_balance_cost_for_class(
                    *class,
                    days,
                    &state.class_positions,
                );
                let post = crate::score::class_day_balance_cost_for_class_with_swap(
                    *class,
                    days,
                    &state.class_positions,
                    old_tb.day_of_week,
                    new_tb.day_of_week,
                );
                acc += i64::from(post) - i64::from(pre);
            }
            i64::from(weights.class_day_balance) * acc
        };

    // Item 57: per-class worst-case axes delta. Both helpers walk
    // `placements` by `lesson_id` and `time_block_id` only; `room_id` /
    // `teacher_id` are not consulted, so a temporary swap of
    // `placements[placement_idx].time_block_id` is a sound stand-in for
    // the post-move shape. Short-circuit when both weights are zero.
    // Cost: two full passes over placements per call site (O(P)); the
    // helpers themselves allocate small per-class HashMaps internally.
    // Acceptable because Change is the cheapest LAHC move; on reject the
    // swap is restored before we return.
    let new_axes_delta: i64 =
        if weights.max_per_class_spread == 0 && weights.max_per_class_interior_gaps == 0 {
            0
        } else {
            let pre_spread = i64::from(weights.max_per_class_spread)
                * i64::from(crate::score::worst_class_spread(problem, placements));
            let pre_gaps = i64::from(weights.max_per_class_interior_gaps)
                * i64::from(crate::score::worst_class_interior_gaps(problem, placements));
            let saved_tb = placements[placement_idx].time_block_id;
            placements[placement_idx].time_block_id = new_tb.id;
            let post_spread = i64::from(weights.max_per_class_spread)
                * i64::from(crate::score::worst_class_spread(problem, placements));
            let post_gaps = i64::from(weights.max_per_class_interior_gaps)
                * i64::from(crate::score::worst_class_interior_gaps(problem, placements));
            placements[placement_idx].time_block_id = saved_tb;
            (post_spread - pre_spread) + (post_gaps - pre_gaps)
        };

    let canonical_delta = delta + home_room_delta + class_day_balance_delta + new_axes_delta;
    let new_canonical_signed = i64::from(state.canonical_score) + canonical_delta;
    let new_canonical = u32::try_from(new_canonical_signed.max(0)).unwrap_or(u32::MAX);

    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let accept = new_canonical <= state.canonical_score || new_canonical <= prior;
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
    state.search_score_slice = new_score;
    // Item 3 (supervision spread): `score_solution` includes the
    // supervision contribution but the Change-move delta arithmetic above
    // does not, so the delta-predicted `new_canonical` drifts from the
    // true score by `weights.supervision_spread * (new_spread -
    // old_spread)` whenever an accepted move alters the supervision
    // adjacency surface. Recompute the canonical score from the full
    // scorer post-apply so the per-iteration `debug_assert_eq!` invariant
    // at the LAHC iteration tail holds. R&R and Kempe accept paths
    // already recompute via `score_solution` (see `rr_attempt` and
    // `kempe_apply_block` finalisation); Change is the only delta-only
    // path. Cost: one supervision pass per accept; accepts are 1-3
    // orders of magnitude rarer than proposals so the hot-path budget
    // is unaffected. The accept criterion above still compares the
    // delta-predicted `new_canonical` against the pre-move
    // `state.canonical_score`; supervision is intentionally not
    // delta-tracked per the supervision-objective design doc ("full
    // rescore at Kempe + finalization captures supervision cost").
    state.canonical_score =
        crate::score::score_solution(problem, placements, weights, &state.soft_pinned_blocks);
    true
}

/// Block-aware Change move. n=1 delegates to the existing delta-score path
/// in `try_change_move_n1`; n>1 takes a snapshot-apply-recompute-rollback
/// path with feasibility checks via a subtract-source overlay (the source
/// block's TBs are treated as free for double-booking checks against
/// `state.used_*`).
///
/// Wired into `run`'s Change branch under the 3-draw RNG budget (Task 5).
/// See spec `/tmp/kz-autopilot/2026-05-14-lahc-block-change-swap-moves-design.md`.
#[allow(clippy::too_many_arguments)] // Reason: internal helper, parameters mirror try_change_move
fn try_change_block_move(
    problem: &Problem,
    idx: &Indexed,
    placement_idx: usize,
    new_tb_idx: usize,
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    subject_lookup: &HashMap<SubjectId, &Subject>,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    max_position_per_day: &HashMap<u8, u8>,
    weights: &ConstraintWeights,
    placements: &mut [Placement],
    state: &mut crate::solve::GreedyState,
    pinned: &HashSet<LessonId>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    lahc_list: &[u32],
    iter: u64,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
    room_order: &[usize],
) -> bool {
    let p = placements[placement_idx].clone();
    let lesson = lesson_lookup[&p.lesson_id];
    let n = lesson.preferred_block_size as usize;

    // Standard exclusions (group, pinned, kind, no-op). All before any
    // short-circuit so the three RNG draws upstream are already consumed
    // by the call site (Task 5).
    if lesson.lesson_group_id.is_some() {
        return false;
    }
    if pinned.contains(&p.lesson_id) {
        return false;
    }
    let anchor_new_tb = problem.time_blocks[new_tb_idx].clone();
    if anchor_new_tb.kind != TimeBlockKind::Lesson {
        return false;
    }

    // n=1 fast path: delegate to the existing delta-score Change move. The
    // anchor `new_tb` resolves through `tb_by_day_pos` upstream just as well
    // as through `problem.time_blocks[new_tb_idx]`; we delegate with the
    // index the caller provided.
    if n == 1 {
        return try_change_move_n1(
            problem,
            idx,
            placement_idx,
            new_tb_idx,
            lesson_lookup,
            tb_lookup,
            subject_lookup,
            home_room_lookup,
            max_position_per_day,
            weights,
            placements,
            state,
            pinned,
            class_max_lessons_per_day,
            lahc_list,
            iter,
        );
    }

    // n>1 path. Identify the source block: every placement of this lesson
    // that lives on the same day as the anchor (`placements[placement_idx]`).
    // The block-room invariant guarantees one shared room across the block.
    let anchor_old_tb = tb_lookup[&p.time_block_id].clone();
    let old_anchor_day = anchor_old_tb.day_of_week;
    let mut source_rows: Vec<(usize, u8)> = placements
        .iter()
        .enumerate()
        .filter_map(|(i, pl)| {
            if pl.lesson_id != lesson.id {
                return None;
            }
            let tb = tb_lookup.get(&pl.time_block_id)?;
            if tb.day_of_week == old_anchor_day {
                Some((i, tb.position))
            } else {
                None
            }
        })
        .collect();
    source_rows.sort_by_key(|(_, pos)| *pos);
    if source_rows.len() != n {
        // FFD invariant: a block lesson on one day owns exactly n
        // contiguous placements. Mismatch means the source isn't a clean
        // block we can move atomically; bail out.
        return false;
    }
    // Contiguity check (block invariant under FFD + R&R).
    for w in source_rows.windows(2) {
        if w[1].1 != w[0].1 + 1 {
            return false;
        }
    }
    let source_indices: Vec<usize> = source_rows.iter().map(|(i, _)| *i).collect();
    let source_positions: Vec<u8> = source_rows.iter().map(|(_, pos)| *pos).collect();
    let old_anchor_pos = source_positions[0];
    let source_tb_ids: Vec<TimeBlockId> = source_indices
        .iter()
        .map(|&i| placements[i].time_block_id)
        .collect();
    let source_tb_set: HashSet<TimeBlockId> = source_tb_ids.iter().copied().collect();
    let source_room_id = placements[source_indices[0]].room_id;
    let teacher = placements[source_indices[0]].teacher_id;

    // Destination window via tb_by_day_pos lookup. Off-day-end or break-slot
    // positions miss (tb_by_day_pos is filtered to Lesson-kind upstream).
    let new_day = anchor_new_tb.day_of_week;
    let new_start_pos = anchor_new_tb.position;
    let mut dest_tb_ids: Vec<TimeBlockId> = Vec::with_capacity(n);
    for i in 0..n {
        let pos = new_start_pos.checked_add(i as u8);
        let Some(pos) = pos else {
            return false;
        };
        let Some(tb_id) = tb_by_day_pos.get(&(new_day, pos)).copied() else {
            return false;
        };
        dest_tb_ids.push(tb_id);
    }

    // Exact self-window reject.
    if new_day == old_anchor_day && new_start_pos == old_anchor_pos {
        return false;
    }

    // Feasibility with subtract-source overlay. Teacher, per-class, and
    // teacher-blocked-time checks at every dest TB.
    let class_ids: &[SchoolClassId] = &lesson.school_class_ids;
    for &dest_tb in &dest_tb_ids {
        if idx.teacher_blocked(teacher, dest_tb) {
            return false;
        }
        if state.used_teacher.contains(&(teacher, dest_tb)) && !source_tb_set.contains(&dest_tb) {
            return false;
        }
        for class in class_ids {
            if state.used_class.contains(&(*class, dest_tb)) && !source_tb_set.contains(&dest_tb) {
                return false;
            }
        }
    }

    // Travel-buffer pruning (ADR 0044). The anchor of the destination window
    // is `dest_tb_ids[0]`; the helper inspects only the buffer-adjacent
    // slots (pre at start - 1, post at start + n) so a single anchor check
    // covers the whole block. `Some((old_anchor_day, old_anchor_pos))`
    // tells the helper the source block is leaving, so a same-day shift
    // does not self-collide.
    if crate::validate::would_violate_travel_buffer(
        problem,
        state,
        placements,
        lesson,
        dest_tb_ids[0],
        teacher,
        Some((old_anchor_day, old_anchor_pos)),
    ) {
        return false;
    }

    // Daily caps (per-class total + per-subject-per-class-day). Same-day
    // moves preserve counts; cross-day moves move n source -> n dest.
    if old_anchor_day != new_day {
        let subject_cap = problem
            .subjects
            .iter()
            .find(|s| s.id == lesson.subject_id)
            .map(|s| s.max_hours_per_day)
            .unwrap_or(u8::MAX);
        for class in class_ids {
            let dest_lessons = state
                .lessons_by_class_day
                .get(&(*class, new_day))
                .copied()
                .unwrap_or(0);
            if let Some(cap) = class_max_lessons_per_day.get(class).copied() {
                if (dest_lessons as u16) + (n as u16) > cap as u16 {
                    return false;
                }
            }
            let dest_subject_hours = state
                .subject_hours_by_class_day
                .get(&(*class, new_day, lesson.subject_id))
                .copied()
                .unwrap_or(0);
            if (dest_subject_hours as u16) + (n as u16) > subject_cap as u16 {
                return false;
            }
        }
    }

    // Same-room hard constraint at new_day (mirror try_change_move_n1).
    // If any member-class has a (class, new_day, subject) lock with a count
    // not entirely owned by the source block, the destination triple's room
    // is fixed. Source-block-only locks at the destination triple are
    // effectively self (the move clears them as part of the apply step).
    let mut new_day_lock: Option<RoomId> = None;
    for class in class_ids {
        let key = (*class, new_day, lesson.subject_id);
        if let Some(&(locked, count)) = state.locked_room.get(&key) {
            // When old_day == new_day and the source block fully owns the
            // triple, the lock clears as the move applies (n entries removed,
            // n added). Otherwise the lock's room must hold post-move.
            let self_only = old_anchor_day == new_day && count as usize == n;
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

    // Room selection. If the destination triple is locked, the move must use
    // that room (no walking room_order). Otherwise try source room first;
    // failing that, walk `room_order` for a subject-suitable room
    // hard-feasible at all n dest TBs (under the subtract-source overlay).
    let room_feasible_all = |room_id: RoomId| -> bool {
        if !idx.room_suits_subject(room_id, lesson.subject_id) {
            return false;
        }
        for &dest_tb in &dest_tb_ids {
            if idx.room_blocked(room_id, dest_tb) {
                return false;
            }
            if state.used_room.contains(&(room_id, dest_tb)) {
                // Source room at source TBs is the overlay-allowed case;
                // any non-source room must not be in `used_room` for the
                // dest TB.
                if room_id == source_room_id && source_tb_set.contains(&dest_tb) {
                    continue;
                }
                return false;
            }
        }
        true
    };
    let chosen_room_id = if let Some(locked) = new_day_lock {
        if !room_feasible_all(locked) {
            return false;
        }
        locked
    } else if room_feasible_all(source_room_id) {
        source_room_id
    } else {
        let mut picked: Option<RoomId> = None;
        for &room_idx in room_order {
            let cand = problem.rooms[room_idx].id;
            if cand == source_room_id {
                continue;
            }
            if room_feasible_all(cand) {
                picked = Some(cand);
                break;
            }
        }
        let Some(room_id) = picked else {
            return false;
        };
        room_id
    };

    // Apply: snapshot the source rows + the state-map entries the move
    // touches, then rewrite placements + state in place.
    let placements_snapshot: Vec<Placement> = source_indices
        .iter()
        .map(|&i| placements[i].clone())
        .collect();
    let class_positions_snapshot: HashMap<(SchoolClassId, u8), Vec<u8>> = {
        let mut m: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        for class in class_ids {
            for day in [old_anchor_day, new_day] {
                if let Some(v) = state.class_positions.get(&(*class, day)) {
                    m.insert((*class, day), v.clone());
                }
            }
        }
        m
    };
    let teacher_positions_snapshot: HashMap<(TeacherId, u8), Vec<u8>> = {
        let mut m: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
        for day in [old_anchor_day, new_day] {
            if let Some(v) = state.teacher_positions.get(&(teacher, day)) {
                m.insert((teacher, day), v.clone());
            }
        }
        m
    };
    let used_teacher_snapshot: HashSet<(TeacherId, TimeBlockId)> = source_tb_ids
        .iter()
        .chain(dest_tb_ids.iter())
        .filter_map(|tb| {
            let key = (teacher, *tb);
            if state.used_teacher.contains(&key) {
                Some(key)
            } else {
                None
            }
        })
        .collect();
    let used_class_snapshot: HashSet<(SchoolClassId, TimeBlockId)> = {
        let mut s: HashSet<(SchoolClassId, TimeBlockId)> = HashSet::new();
        for class in class_ids {
            for tb in source_tb_ids.iter().chain(dest_tb_ids.iter()) {
                let key = (*class, *tb);
                if state.used_class.contains(&key) {
                    s.insert(key);
                }
            }
        }
        s
    };
    let used_room_snapshot: HashSet<(RoomId, TimeBlockId)> = {
        let mut s: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
        for room in [source_room_id, chosen_room_id] {
            for tb in source_tb_ids.iter().chain(dest_tb_ids.iter()) {
                let key = (room, *tb);
                if state.used_room.contains(&key) {
                    s.insert(key);
                }
            }
        }
        s
    };
    let locked_room_snapshot: HashMap<(SchoolClassId, u8, SubjectId), (RoomId, u32)> = {
        let mut m = HashMap::new();
        for class in class_ids {
            for day in [old_anchor_day, new_day] {
                let key = (*class, day, lesson.subject_id);
                if let Some(v) = state.locked_room.get(&key).copied() {
                    m.insert(key, v);
                }
            }
        }
        m
    };
    let subject_hours_snapshot: HashMap<(SchoolClassId, u8, SubjectId), u8> = {
        let mut m = HashMap::new();
        for class in class_ids {
            for day in [old_anchor_day, new_day] {
                let key = (*class, day, lesson.subject_id);
                if let Some(v) = state.subject_hours_by_class_day.get(&key).copied() {
                    m.insert(key, v);
                }
            }
        }
        m
    };
    let lessons_by_class_day_snapshot: HashMap<(SchoolClassId, u8), u8> = {
        let mut m = HashMap::new();
        for class in class_ids {
            for day in [old_anchor_day, new_day] {
                let key = (*class, day);
                if let Some(v) = state.lessons_by_class_day.get(&key).copied() {
                    m.insert(key, v);
                }
            }
        }
        m
    };
    let canonical_before = state.canonical_score;
    let search_slice_before = state.search_score_slice;

    // Apply step: rewrite source rows and update state maps.
    for i in 0..n {
        let row_idx = source_indices[i];
        placements[row_idx].time_block_id = dest_tb_ids[i];
        placements[row_idx].room_id = chosen_room_id;
    }
    // Rebuild class_positions / teacher_positions touched days from
    // scratch from the current placements view, restricted to the two
    // (class, day) and (teacher, day) keys we own. This keeps the partition
    // shape (sorted, dedup'd) in lockstep with score_solution's reader.
    for class in class_ids {
        for day in [old_anchor_day, new_day] {
            let mut positions: Vec<u8> = Vec::new();
            for pl in placements.iter() {
                let l = lesson_lookup[&pl.lesson_id];
                if !l.school_class_ids.contains(class) {
                    continue;
                }
                let tb = tb_lookup[&pl.time_block_id];
                if tb.day_of_week == day {
                    positions.push(tb.position);
                }
            }
            positions.sort_unstable();
            positions.dedup();
            if positions.is_empty() {
                state.class_positions.remove(&(*class, day));
            } else {
                state.class_positions.insert((*class, day), positions);
            }
        }
    }
    for day in [old_anchor_day, new_day] {
        let mut positions: Vec<u8> = Vec::new();
        for pl in placements.iter() {
            if pl.teacher_id != teacher {
                continue;
            }
            let tb = tb_lookup[&pl.time_block_id];
            if tb.day_of_week == day {
                positions.push(tb.position);
            }
        }
        positions.sort_unstable();
        positions.dedup();
        if positions.is_empty() {
            state.teacher_positions.remove(&(teacher, day));
        } else {
            state.teacher_positions.insert((teacher, day), positions);
        }
    }
    // used_teacher / used_class / used_room: subtract source, add dest.
    for &tb in &source_tb_ids {
        state.used_teacher.remove(&(teacher, tb));
        for class in class_ids {
            state.used_class.remove(&(*class, tb));
        }
        state.used_room.remove(&(source_room_id, tb));
    }
    for &tb in &dest_tb_ids {
        state.used_teacher.insert((teacher, tb));
        for class in class_ids {
            state.used_class.insert((*class, tb));
        }
        state.used_room.insert((chosen_room_id, tb));
    }
    // locked_room bookkeeping: drop n at source triple, add n at dest.
    for class in class_ids {
        let old_key = (*class, old_anchor_day, lesson.subject_id);
        if let Some(entry) = state.locked_room.get_mut(&old_key) {
            entry.1 = entry.1.saturating_sub(n as u32);
            if entry.1 == 0 {
                state.locked_room.remove(&old_key);
            }
        }
        let new_key = (*class, new_day, lesson.subject_id);
        let entry = state
            .locked_room
            .entry(new_key)
            .or_insert((chosen_room_id, 0));
        entry.0 = chosen_room_id;
        entry.1 += n as u32;
    }
    // Per-day caps: source day -n; dest day +n.
    for class in class_ids {
        let old_hour_key = (*class, old_anchor_day, lesson.subject_id);
        if let Some(h) = state.subject_hours_by_class_day.get_mut(&old_hour_key) {
            *h = h.saturating_sub(n as u8);
            if *h == 0 {
                state.subject_hours_by_class_day.remove(&old_hour_key);
            }
        }
        let old_lesson_key = (*class, old_anchor_day);
        if let Some(c) = state.lessons_by_class_day.get_mut(&old_lesson_key) {
            *c = c.saturating_sub(n as u8);
            if *c == 0 {
                state.lessons_by_class_day.remove(&old_lesson_key);
            }
        }
        *state
            .subject_hours_by_class_day
            .entry((*class, new_day, lesson.subject_id))
            .or_insert(0) += n as u8;
        *state
            .lessons_by_class_day
            .entry((*class, new_day))
            .or_insert(0) += n as u8;
    }

    // Recompute scores (full recompute; the multi-position multi-class
    // delta is too tangled for an incremental update on the n>1 path).
    let new_canonical =
        crate::score::score_solution(problem, placements, weights, &state.soft_pinned_blocks);
    let new_slice = crate::score::slice_recompute(problem, placements, weights);

    // LAHC accept criterion (identical to try_change_move_n1).
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let accept = new_canonical <= state.canonical_score || new_canonical <= prior;

    if accept {
        state.canonical_score = new_canonical;
        state.search_score_slice = new_slice;
        return true;
    }

    // Reject: restore from snapshot. Placements first, then state maps.
    for (i, snap) in source_indices.iter().zip(placements_snapshot.iter()) {
        placements[*i] = snap.clone();
    }
    for class in class_ids {
        for day in [old_anchor_day, new_day] {
            let key = (*class, day);
            match class_positions_snapshot.get(&key) {
                Some(v) => {
                    state.class_positions.insert(key, v.clone());
                }
                None => {
                    state.class_positions.remove(&key);
                }
            }
        }
    }
    for day in [old_anchor_day, new_day] {
        let key = (teacher, day);
        match teacher_positions_snapshot.get(&key) {
            Some(v) => {
                state.teacher_positions.insert(key, v.clone());
            }
            None => {
                state.teacher_positions.remove(&key);
            }
        }
    }
    for tb in source_tb_ids.iter().chain(dest_tb_ids.iter()) {
        let key = (teacher, *tb);
        if used_teacher_snapshot.contains(&key) {
            state.used_teacher.insert(key);
        } else {
            state.used_teacher.remove(&key);
        }
        for class in class_ids {
            let ckey = (*class, *tb);
            if used_class_snapshot.contains(&ckey) {
                state.used_class.insert(ckey);
            } else {
                state.used_class.remove(&ckey);
            }
        }
        for room in [source_room_id, chosen_room_id] {
            let rkey = (room, *tb);
            if used_room_snapshot.contains(&rkey) {
                state.used_room.insert(rkey);
            } else {
                state.used_room.remove(&rkey);
            }
        }
    }
    for class in class_ids {
        for day in [old_anchor_day, new_day] {
            let key = (*class, day, lesson.subject_id);
            match locked_room_snapshot.get(&key).copied() {
                Some(v) => {
                    state.locked_room.insert(key, v);
                }
                None => {
                    state.locked_room.remove(&key);
                }
            }
            match subject_hours_snapshot.get(&key).copied() {
                Some(v) => {
                    state.subject_hours_by_class_day.insert(key, v);
                }
                None => {
                    state.subject_hours_by_class_day.remove(&key);
                }
            }
            let lkey = (*class, day);
            match lessons_by_class_day_snapshot.get(&lkey).copied() {
                Some(v) => {
                    state.lessons_by_class_day.insert(lkey, v);
                }
                None => {
                    state.lessons_by_class_day.remove(&lkey);
                }
            }
        }
    }
    state.canonical_score = canonical_before;
    state.search_score_slice = search_slice_before;
    false
}

/// Cell-with-cell swap of two n=1 placements. Builds a virtual
/// "subtract the two swap participants" overlay over `state.used_*` for
/// the feasibility check, then snapshot-apply-recompute-rollback through
/// `score_solution` + `slice_recompute` on the LAHC accept criterion.
/// Teachers and rooms stay attached to their lessons; only `time_block_id`
/// is rewritten on apply.
///
/// Wired into `run`'s Change branch under the 3-draw RNG budget (Task 5).
/// See spec `/tmp/kz-autopilot/2026-05-14-lahc-block-change-swap-moves-design.md`.
#[allow(clippy::too_many_arguments)] // Reason: internal helper, parameters mirror try_change_move
fn try_swap_move(
    problem: &Problem,
    idx: &Indexed,
    placement_idx: usize,
    partner_idx: usize,
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    weights: &ConstraintWeights,
    placements: &mut [Placement],
    state: &mut crate::solve::GreedyState,
    pinned: &HashSet<LessonId>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    lahc_list: &[u32],
    iter: u64,
) -> bool {
    // Early reject (RNG-budget-safe; the three draws upstream are already
    // consumed by the call site in Task 5).
    if placement_idx == partner_idx {
        return false;
    }
    if placement_idx >= placements.len() || partner_idx >= placements.len() {
        return false;
    }
    let p_a = placements[placement_idx].clone();
    let p_b = placements[partner_idx].clone();
    if p_a.lesson_id == p_b.lesson_id {
        return false;
    }
    let lesson_a = lesson_lookup[&p_a.lesson_id];
    let lesson_b = lesson_lookup[&p_b.lesson_id];
    // Block-block (or block-cell) swap is out of scope; defer to a P2
    // follow-up per the spec.
    if lesson_a.preferred_block_size > 1 || lesson_b.preferred_block_size > 1 {
        return false;
    }
    if lesson_a.lesson_group_id.is_some() || lesson_b.lesson_group_id.is_some() {
        return false;
    }
    if pinned.contains(&p_a.lesson_id) || pinned.contains(&p_b.lesson_id) {
        return false;
    }

    let tb_a = tb_lookup[&p_a.time_block_id].clone();
    let tb_b = tb_lookup[&p_b.time_block_id].clone();
    // Both destination TBs are Lesson-kind by placement invariant; skip the
    // kind check.
    let teacher_a = p_a.teacher_id;
    let teacher_b = p_b.teacher_id;
    let room_a = p_a.room_id;
    let room_b = p_b.room_id;
    let class_ids_a: &[SchoolClassId] = &lesson_a.school_class_ids;
    let class_ids_b: &[SchoolClassId] = &lesson_b.school_class_ids;

    // Feasibility via subtract-source overlay. The two swap rows
    // (teacher_a@tb_a, teacher_b@tb_b) are removed before the conflict
    // check. For teacher_a landing at tb_b: the only entry the overlay
    // erases at tb_b is (teacher_b, tb_b); so the conflict only goes
    // away iff teacher_a == teacher_b. Symmetric at tb_a.
    let teacher_a_at_b_conflict =
        state.used_teacher.contains(&(teacher_a, tb_b.id)) && teacher_a != teacher_b;
    let teacher_b_at_a_conflict =
        state.used_teacher.contains(&(teacher_b, tb_a.id)) && teacher_a != teacher_b;
    if teacher_a_at_b_conflict || teacher_b_at_a_conflict {
        return false;
    }
    if idx.teacher_blocked(teacher_a, tb_b.id) || idx.teacher_blocked(teacher_b, tb_a.id) {
        return false;
    }

    // Class double-booking: for each class in lesson_A.school_class_ids,
    // check (class, tb_b) ignoring the swap participant (the same class's
    // own entry at tb_b iff that class is also in lesson_B's classes).
    for class in class_ids_a {
        if state.used_class.contains(&(*class, tb_b.id)) && !class_ids_b.contains(class) {
            return false;
        }
    }
    for class in class_ids_b {
        if state.used_class.contains(&(*class, tb_a.id)) && !class_ids_a.contains(class) {
            return false;
        }
    }

    // Room double-booking: room_A at tb_B is OK iff (room_A == room_B) or
    // the entry at (room_A, tb_b) is the one we're removing (i.e.
    // room_B == room_A). Same shape symmetrically.
    if idx.room_blocked(room_a, tb_b.id) || idx.room_blocked(room_b, tb_a.id) {
        return false;
    }
    if state.used_room.contains(&(room_a, tb_b.id)) && room_a != room_b {
        return false;
    }
    if state.used_room.contains(&(room_b, tb_a.id)) && room_a != room_b {
        return false;
    }

    // Daily caps. Same-day swap leaves all per-day counts unchanged; only
    // cross-day swap can breach. For each (class, day) touched, compute the
    // delta with both participants' contributions (deltas partially cancel
    // when the two lessons share a class).
    if tb_a.day_of_week != tb_b.day_of_week {
        let subject_cap_a = problem
            .subjects
            .iter()
            .find(|s| s.id == lesson_a.subject_id)
            .map(|s| s.max_hours_per_day)
            .unwrap_or(u8::MAX);
        let subject_cap_b = problem
            .subjects
            .iter()
            .find(|s| s.id == lesson_b.subject_id)
            .map(|s| s.max_hours_per_day)
            .unwrap_or(u8::MAX);
        // Compute per-(class, day) lessons-count delta from each participant.
        let mut lessons_delta: HashMap<(SchoolClassId, u8), i32> = HashMap::new();
        for class in class_ids_a {
            *lessons_delta.entry((*class, tb_a.day_of_week)).or_insert(0) -= 1;
            *lessons_delta.entry((*class, tb_b.day_of_week)).or_insert(0) += 1;
        }
        for class in class_ids_b {
            *lessons_delta.entry((*class, tb_b.day_of_week)).or_insert(0) -= 1;
            *lessons_delta.entry((*class, tb_a.day_of_week)).or_insert(0) += 1;
        }
        for ((class, day), delta) in &lessons_delta {
            if *delta <= 0 {
                continue;
            }
            let cur = state
                .lessons_by_class_day
                .get(&(*class, *day))
                .copied()
                .unwrap_or(0) as i32;
            if let Some(cap) = class_max_lessons_per_day.get(class).copied() {
                if cur + delta > cap as i32 {
                    return false;
                }
            }
        }
        // Per-subject-per-class-day cap. Each (class, day, subject) is
        // distinct between A and B because the participants have distinct
        // subjects in the general case, but we also handle the shared-class
        // shared-subject overlap (deltas cancel).
        let mut hours_delta: HashMap<(SchoolClassId, u8, SubjectId), i32> = HashMap::new();
        for class in class_ids_a {
            *hours_delta
                .entry((*class, tb_a.day_of_week, lesson_a.subject_id))
                .or_insert(0) -= 1;
            *hours_delta
                .entry((*class, tb_b.day_of_week, lesson_a.subject_id))
                .or_insert(0) += 1;
        }
        for class in class_ids_b {
            *hours_delta
                .entry((*class, tb_b.day_of_week, lesson_b.subject_id))
                .or_insert(0) -= 1;
            *hours_delta
                .entry((*class, tb_a.day_of_week, lesson_b.subject_id))
                .or_insert(0) += 1;
        }
        for ((class, day, subject), delta) in &hours_delta {
            if *delta <= 0 {
                continue;
            }
            let cur = state
                .subject_hours_by_class_day
                .get(&(*class, *day, *subject))
                .copied()
                .unwrap_or(0) as i32;
            let cap = if *subject == lesson_a.subject_id {
                subject_cap_a
            } else {
                subject_cap_b
            };
            if cur + delta > cap as i32 {
                return false;
            }
        }
    }

    // Same-room hard constraint at destination triples (mirror
    // try_change_move_n1). After the swap, lesson_A occupies tb_b (so
    // (class_a, tb_b.day, subject_a) must hold any existing lock unless A
    // is its only occupant from the source side), and lesson_B occupies
    // tb_a symmetrically. Same-day swap is a no-op on the locked_room map.
    if tb_a.day_of_week != tb_b.day_of_week {
        for class in class_ids_a {
            let key = (*class, tb_b.day_of_week, lesson_a.subject_id);
            if let Some(&(locked, _count)) = state.locked_room.get(&key) {
                if locked != room_a {
                    return false;
                }
            }
        }
        for class in class_ids_b {
            let key = (*class, tb_a.day_of_week, lesson_b.subject_id);
            if let Some(&(locked, _count)) = state.locked_room.get(&key) {
                if locked != room_b {
                    return false;
                }
            }
        }
    }

    // Travel-buffer pruning (ADR 0044). Both lessons land at each other's
    // pre-swap time blocks; either side could newly violate the buffer
    // constraint. `ignore_self` lets the helper skip the lesson's pre-swap
    // position so a same-day shift does not self-collide.
    if crate::validate::would_violate_travel_buffer(
        problem,
        state,
        placements,
        lesson_a,
        tb_b.id,
        teacher_a,
        Some((tb_a.day_of_week, tb_a.position)),
    ) {
        return false;
    }
    if crate::validate::would_violate_travel_buffer(
        problem,
        state,
        placements,
        lesson_b,
        tb_a.id,
        teacher_b,
        Some((tb_b.day_of_week, tb_b.position)),
    ) {
        return false;
    }

    // Snapshot the rows + state-map keys the apply step touches, then
    // rewrite placements + state in place.
    let placements_snapshot: [Placement; 2] = [p_a.clone(), p_b.clone()];
    // class_positions / teacher_positions touched (class, day) and
    // (teacher, day) keys: the two days, the union of A's + B's classes,
    // and the two teachers.
    let touched_days: Vec<u8> = if tb_a.day_of_week == tb_b.day_of_week {
        vec![tb_a.day_of_week]
    } else {
        vec![tb_a.day_of_week, tb_b.day_of_week]
    };
    let mut all_classes: Vec<SchoolClassId> = class_ids_a.to_vec();
    for c in class_ids_b {
        if !all_classes.contains(c) {
            all_classes.push(*c);
        }
    }
    let touched_teachers: Vec<TeacherId> = if teacher_a == teacher_b {
        vec![teacher_a]
    } else {
        vec![teacher_a, teacher_b]
    };
    let touched_rooms: Vec<RoomId> = if room_a == room_b {
        vec![room_a]
    } else {
        vec![room_a, room_b]
    };
    let touched_subjects: Vec<SubjectId> = if lesson_a.subject_id == lesson_b.subject_id {
        vec![lesson_a.subject_id]
    } else {
        vec![lesson_a.subject_id, lesson_b.subject_id]
    };

    let class_positions_snapshot: HashMap<(SchoolClassId, u8), Vec<u8>> = {
        let mut m: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        for class in &all_classes {
            for day in &touched_days {
                if let Some(v) = state.class_positions.get(&(*class, *day)) {
                    m.insert((*class, *day), v.clone());
                }
            }
        }
        m
    };
    let teacher_positions_snapshot: HashMap<(TeacherId, u8), Vec<u8>> = {
        let mut m: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
        for teacher in &touched_teachers {
            for day in &touched_days {
                if let Some(v) = state.teacher_positions.get(&(*teacher, *day)) {
                    m.insert((*teacher, *day), v.clone());
                }
            }
        }
        m
    };
    let used_teacher_snapshot: HashSet<(TeacherId, TimeBlockId)> = {
        let mut s: HashSet<(TeacherId, TimeBlockId)> = HashSet::new();
        for teacher in &touched_teachers {
            for tb in [tb_a.id, tb_b.id] {
                let key = (*teacher, tb);
                if state.used_teacher.contains(&key) {
                    s.insert(key);
                }
            }
        }
        s
    };
    let used_class_snapshot: HashSet<(SchoolClassId, TimeBlockId)> = {
        let mut s: HashSet<(SchoolClassId, TimeBlockId)> = HashSet::new();
        for class in &all_classes {
            for tb in [tb_a.id, tb_b.id] {
                let key = (*class, tb);
                if state.used_class.contains(&key) {
                    s.insert(key);
                }
            }
        }
        s
    };
    let used_room_snapshot: HashSet<(RoomId, TimeBlockId)> = {
        let mut s: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
        for room in &touched_rooms {
            for tb in [tb_a.id, tb_b.id] {
                let key = (*room, tb);
                if state.used_room.contains(&key) {
                    s.insert(key);
                }
            }
        }
        s
    };
    let locked_room_snapshot: HashMap<(SchoolClassId, u8, SubjectId), (RoomId, u32)> = {
        let mut m = HashMap::new();
        for class in &all_classes {
            for day in &touched_days {
                for subject in &touched_subjects {
                    let key = (*class, *day, *subject);
                    if let Some(v) = state.locked_room.get(&key).copied() {
                        m.insert(key, v);
                    }
                }
            }
        }
        m
    };
    let subject_hours_snapshot: HashMap<(SchoolClassId, u8, SubjectId), u8> = {
        let mut m = HashMap::new();
        for class in &all_classes {
            for day in &touched_days {
                for subject in &touched_subjects {
                    let key = (*class, *day, *subject);
                    if let Some(v) = state.subject_hours_by_class_day.get(&key).copied() {
                        m.insert(key, v);
                    }
                }
            }
        }
        m
    };
    let lessons_by_class_day_snapshot: HashMap<(SchoolClassId, u8), u8> = {
        let mut m = HashMap::new();
        for class in &all_classes {
            for day in &touched_days {
                let key = (*class, *day);
                if let Some(v) = state.lessons_by_class_day.get(&key).copied() {
                    m.insert(key, v);
                }
            }
        }
        m
    };
    let canonical_before = state.canonical_score;
    let search_slice_before = state.search_score_slice;

    // Apply: swap time_block_ids. Teachers and rooms stay attached to
    // their lessons (the placement rows carry them).
    placements[placement_idx].time_block_id = tb_b.id;
    placements[partner_idx].time_block_id = tb_a.id;

    // Rebuild class_positions / teacher_positions from the post-apply
    // placements view, restricted to the touched (class, day) and
    // (teacher, day) keys. Keeps the partition shape (sorted, dedup'd)
    // in lockstep with score_solution's reader.
    for class in &all_classes {
        for day in &touched_days {
            let mut positions: Vec<u8> = Vec::new();
            for pl in placements.iter() {
                let l = lesson_lookup[&pl.lesson_id];
                if !l.school_class_ids.contains(class) {
                    continue;
                }
                let tb = tb_lookup[&pl.time_block_id];
                if tb.day_of_week == *day {
                    positions.push(tb.position);
                }
            }
            positions.sort_unstable();
            positions.dedup();
            if positions.is_empty() {
                state.class_positions.remove(&(*class, *day));
            } else {
                state.class_positions.insert((*class, *day), positions);
            }
        }
    }
    for teacher in &touched_teachers {
        for day in &touched_days {
            let mut positions: Vec<u8> = Vec::new();
            for pl in placements.iter() {
                if pl.teacher_id != *teacher {
                    continue;
                }
                let tb = tb_lookup[&pl.time_block_id];
                if tb.day_of_week == *day {
                    positions.push(tb.position);
                }
            }
            positions.sort_unstable();
            positions.dedup();
            if positions.is_empty() {
                state.teacher_positions.remove(&(*teacher, *day));
            } else {
                state.teacher_positions.insert((*teacher, *day), positions);
            }
        }
    }
    // used_teacher / used_class / used_room: remove source entries, add
    // dest entries. Order matters when A and B share teacher / class / room:
    // remove both first, then insert both.
    state.used_teacher.remove(&(teacher_a, tb_a.id));
    state.used_teacher.remove(&(teacher_b, tb_b.id));
    state.used_teacher.insert((teacher_a, tb_b.id));
    state.used_teacher.insert((teacher_b, tb_a.id));
    for class in class_ids_a {
        state.used_class.remove(&(*class, tb_a.id));
    }
    for class in class_ids_b {
        state.used_class.remove(&(*class, tb_b.id));
    }
    for class in class_ids_a {
        state.used_class.insert((*class, tb_b.id));
    }
    for class in class_ids_b {
        state.used_class.insert((*class, tb_a.id));
    }
    state.used_room.remove(&(room_a, tb_a.id));
    state.used_room.remove(&(room_b, tb_b.id));
    state.used_room.insert((room_a, tb_b.id));
    state.used_room.insert((room_b, tb_a.id));

    // locked_room: drop 1 at (class_a, day_a, subj_a) and (class_b, day_b,
    // subj_b); add 1 at (class_a, day_b, subj_a) and (class_b, day_a,
    // subj_b). Room follows the lesson (room_a stays with lesson_a).
    for class in class_ids_a {
        let old_key = (*class, tb_a.day_of_week, lesson_a.subject_id);
        if let Some(entry) = state.locked_room.get_mut(&old_key) {
            entry.1 = entry.1.saturating_sub(1);
            if entry.1 == 0 {
                state.locked_room.remove(&old_key);
            }
        }
        let new_key = (*class, tb_b.day_of_week, lesson_a.subject_id);
        let entry = state.locked_room.entry(new_key).or_insert((room_a, 0));
        entry.0 = room_a;
        entry.1 += 1;
    }
    for class in class_ids_b {
        let old_key = (*class, tb_b.day_of_week, lesson_b.subject_id);
        if let Some(entry) = state.locked_room.get_mut(&old_key) {
            entry.1 = entry.1.saturating_sub(1);
            if entry.1 == 0 {
                state.locked_room.remove(&old_key);
            }
        }
        let new_key = (*class, tb_a.day_of_week, lesson_b.subject_id);
        let entry = state.locked_room.entry(new_key).or_insert((room_b, 0));
        entry.0 = room_b;
        entry.1 += 1;
    }

    // subject_hours_by_class_day: -1 at source triple, +1 at dest triple,
    // per class and per participant.
    for class in class_ids_a {
        let old_key = (*class, tb_a.day_of_week, lesson_a.subject_id);
        if let Some(h) = state.subject_hours_by_class_day.get_mut(&old_key) {
            *h = h.saturating_sub(1);
            if *h == 0 {
                state.subject_hours_by_class_day.remove(&old_key);
            }
        }
        *state
            .subject_hours_by_class_day
            .entry((*class, tb_b.day_of_week, lesson_a.subject_id))
            .or_insert(0) += 1;
    }
    for class in class_ids_b {
        let old_key = (*class, tb_b.day_of_week, lesson_b.subject_id);
        if let Some(h) = state.subject_hours_by_class_day.get_mut(&old_key) {
            *h = h.saturating_sub(1);
            if *h == 0 {
                state.subject_hours_by_class_day.remove(&old_key);
            }
        }
        *state
            .subject_hours_by_class_day
            .entry((*class, tb_a.day_of_week, lesson_b.subject_id))
            .or_insert(0) += 1;
    }
    // lessons_by_class_day: -1 at source day, +1 at dest day per class.
    for class in class_ids_a {
        let old_key = (*class, tb_a.day_of_week);
        if let Some(c) = state.lessons_by_class_day.get_mut(&old_key) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                state.lessons_by_class_day.remove(&old_key);
            }
        }
        *state
            .lessons_by_class_day
            .entry((*class, tb_b.day_of_week))
            .or_insert(0) += 1;
    }
    for class in class_ids_b {
        let old_key = (*class, tb_b.day_of_week);
        if let Some(c) = state.lessons_by_class_day.get_mut(&old_key) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                state.lessons_by_class_day.remove(&old_key);
            }
        }
        *state
            .lessons_by_class_day
            .entry((*class, tb_a.day_of_week))
            .or_insert(0) += 1;
    }

    // Recompute scores (full recompute; the multi-class delta under swap
    // is too tangled for an incremental update).
    let new_canonical =
        crate::score::score_solution(problem, placements, weights, &state.soft_pinned_blocks);
    let new_slice = crate::score::slice_recompute(problem, placements, weights);

    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let accept = new_canonical <= state.canonical_score || new_canonical <= prior;

    if accept {
        state.canonical_score = new_canonical;
        state.search_score_slice = new_slice;
        return true;
    }

    // Reject: restore from snapshot. Placements first, then state maps.
    placements[placement_idx] = placements_snapshot[0].clone();
    placements[partner_idx] = placements_snapshot[1].clone();
    for class in &all_classes {
        for day in &touched_days {
            let key = (*class, *day);
            match class_positions_snapshot.get(&key) {
                Some(v) => {
                    state.class_positions.insert(key, v.clone());
                }
                None => {
                    state.class_positions.remove(&key);
                }
            }
        }
    }
    for teacher in &touched_teachers {
        for day in &touched_days {
            let key = (*teacher, *day);
            match teacher_positions_snapshot.get(&key) {
                Some(v) => {
                    state.teacher_positions.insert(key, v.clone());
                }
                None => {
                    state.teacher_positions.remove(&key);
                }
            }
        }
    }
    for teacher in &touched_teachers {
        for tb in [tb_a.id, tb_b.id] {
            let key = (*teacher, tb);
            if used_teacher_snapshot.contains(&key) {
                state.used_teacher.insert(key);
            } else {
                state.used_teacher.remove(&key);
            }
        }
    }
    for class in &all_classes {
        for tb in [tb_a.id, tb_b.id] {
            let key = (*class, tb);
            if used_class_snapshot.contains(&key) {
                state.used_class.insert(key);
            } else {
                state.used_class.remove(&key);
            }
        }
    }
    for room in &touched_rooms {
        for tb in [tb_a.id, tb_b.id] {
            let key = (*room, tb);
            if used_room_snapshot.contains(&key) {
                state.used_room.insert(key);
            } else {
                state.used_room.remove(&key);
            }
        }
    }
    for class in &all_classes {
        for day in &touched_days {
            for subject in &touched_subjects {
                let key = (*class, *day, *subject);
                match locked_room_snapshot.get(&key).copied() {
                    Some(v) => {
                        state.locked_room.insert(key, v);
                    }
                    None => {
                        state.locked_room.remove(&key);
                    }
                }
                match subject_hours_snapshot.get(&key).copied() {
                    Some(v) => {
                        state.subject_hours_by_class_day.insert(key, v);
                    }
                    None => {
                        state.subject_hours_by_class_day.remove(&key);
                    }
                }
            }
            let lkey = (*class, *day);
            match lessons_by_class_day_snapshot.get(&lkey).copied() {
                Some(v) => {
                    state.lessons_by_class_day.insert(lkey, v);
                }
                None => {
                    state.lessons_by_class_day.remove(&lkey);
                }
            }
        }
    }
    state.canonical_score = canonical_before;
    state.search_score_slice = search_slice_before;
    false
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
        teacher_id: teacher,
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

/// Run one R&R move: pick up to `rr_k` block anchors at random, ruin them,
/// recreate them, accept under the asymmetric LAHC gate. Returns true if the
/// move was accepted (state mutated to keep the new arrangement); returns
/// false if the move was rejected (state restored to the pre-attempt
/// snapshot).
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn rr_attempt(
    problem: &Problem,
    idx: &Indexed,
    weights: &ConstraintWeights,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    class_teacher_lookup: &HashMap<SchoolClassId, Option<TeacherId>>,
    subject_qualified_teachers: &HashMap<SubjectId, HashSet<TeacherId>>,
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
    rr_k: u32,
) -> bool {
    use rand::seq::SliceRandom;

    let mut anchors = rr_collect_anchors(placements, lesson_lookup, tb_lookup, pinned);
    if anchors.is_empty() {
        return false;
    }
    anchors.shuffle(rr_rng);
    let chosen_count = anchors.len().min(rr_k as usize);
    let chosen: Vec<(LessonId, u8)> = anchors.into_iter().take(chosen_count).collect();

    let pre_slice = state.search_score_slice;
    let pre_canonical = state.canonical_score;
    let pre_count = placements.len();
    // Item 66: snapshot the per-(class, subject) teacher lock map so the
    // post-destroy cleanup (clear stale locks for pairs whose every
    // placement was ruined, so the recreate loop picks a fresh teacher)
    // and a possible later rollback can both restore the pre-attempt
    // mapping verbatim.
    let pre_class_subject_teacher = state.class_subject_teacher.clone();
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

    // Item 66 post-destroy lock cleanup. After the ruin phase, walk the
    // surviving placements to determine which `(class, subject)` pairs
    // still have a placement; remove every lock whose pair lost its
    // last placement so the recreate phase's `try_place_block` can pick
    // a fresh teacher. Pairs with surviving placements keep their lock,
    // forcing every recreated placement of that pair to reuse the
    // existing teacher (uniformity invariant). Rollback restores
    // `pre_class_subject_teacher` verbatim, so the cleanup is reversible.
    {
        let mut surviving_pairs: HashSet<(SchoolClassId, SubjectId)> = HashSet::new();
        for p in placements.iter() {
            let Some(lesson) = lesson_lookup.get(&p.lesson_id) else {
                continue;
            };
            for class in &lesson.school_class_ids {
                surviving_pairs.insert((*class, lesson.subject_id));
            }
        }
        state
            .class_subject_teacher
            .retain(|key, _| surviving_pairs.contains(key));
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
    let days: u8 = problem
        .time_blocks
        .iter()
        .map(|tb| tb.day_of_week)
        .max()
        .map(|m| m.saturating_add(1))
        .unwrap_or(0);
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
            home_room_lookup,
            class_teacher_lookup,
            subject_qualified_teachers,
            state,
            placements,
            tb_order,
            room_order,
            max_position_per_day,
            days,
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
        state.search_score_slice = pre_slice;
        state.canonical_score = pre_canonical;
        // Item 66: restore the snapshot of `class_subject_teacher` so
        // any rolled-back recreate's freshly inserted lock disappears
        // and any cleared lock for a pair whose every placement was
        // ruined is re-added.
        state.class_subject_teacher = pre_class_subject_teacher;
        debug_assert_eq!(
            placements.len(),
            pre_count,
            "rr_rollback left placement count drifted (pre={pre_count} post={})",
            placements.len(),
        );
        return false;
    }

    // `try_place_block` accumulates against `state.search_score_slice`, but
    // `rr_ruin_block` does not subtract the removed placement's gap
    // contribution from the slice. For a successful recreate, the
    // post-recreate `state.search_score_slice` therefore drifts; subsequent
    // Change moves operate on a stale score and the non-negative-delta
    // invariant inside `try_change_move` can fail. Recompute exactly here so
    // the LAHC gate decides on correct numbers and downstream moves see a
    // consistent score. Use the slice-only helper rather than
    // `score::score_solution` because greedy / Change / Kempe maintain the
    // class_gap + teacher_gap + subj_pref slice; including class_day_balance
    // or home_room here contaminates `state.search_score_slice` and
    // downstream Change-move deltas (slice-only) drive it negative over time.
    let new_slice =
        running_slice_from_placements(problem, placements, weights, max_position_per_day);
    state.search_score_slice = new_slice;
    let new_canonical =
        crate::score::score_solution(problem, placements, weights, &state.soft_pinned_blocks);
    state.canonical_score = new_canonical;
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    // Item 52: R&R accepts on canonical so the move's home_room and
    // class_day_balance contributions are gated alongside class_gap /
    // teacher_gap / subject_pref. The slice ride-along stays so downstream
    // Change moves still maintain `state.search_score_slice` consistently.
    let lahc_ok = new_canonical <= pre_canonical || new_canonical <= prior;
    if !lahc_ok {
        rr_rollback(
            &recreated_rows,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
        );
        state.search_score_slice = pre_slice;
        state.canonical_score = pre_canonical;
        state.class_subject_teacher = pre_class_subject_teacher;
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

/// Run one R&R rescue move: pick an under-placed lesson (whose
/// `placement_count < hours_per_week`), ruin all of its existing
/// placements plus one randomly-chosen same-class anchor of a different
/// lesson (to free additional class-day slots the target needs), clear
/// the resulting `(class, subject)` teacher locks, then re-place the
/// target lesson from scratch in blocks of `preferred_block_size` plus
/// recreate the ruined sibling anchor. Returns true when the move was
/// accepted (target's placement count strictly increased), false when
/// aborted (no under-placed lesson, no eligible sibling anchor) or
/// rejected (no net progress on the target).
///
/// Two RNG draws are consumed unconditionally before any abort path so
/// the R&R RNG sequence is invariant across rescue branches. R&R uses
/// its own `SmallRng` seeded from `config.seed.wrapping_add(1)`,
/// separate from the Change move's RNG, so the determinism property
/// test in `lahc_property.rs` is untouched.
///
/// Acceptance gate is "rescue made forward progress on the target":
/// the post-attempt placement count for the target lesson is strictly
/// greater than the pre-attempt count. Soft-cost regression is
/// permitted because hard violations dominate any solution's quality.
/// The standard LAHC late-acceptance gate is bypassed. The canonical
/// score is recomputed via `score::score_solution` post-accept;
/// `state.canonical_score` and `state.search_score_slice` are updated in
/// lockstep so the per-iteration `debug_assert_eq!` invariant holds.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn rr_rescue_attempt(
    problem: &Problem,
    idx: &Indexed,
    weights: &ConstraintWeights,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    class_teacher_lookup: &HashMap<SchoolClassId, Option<TeacherId>>,
    subject_qualified_teachers: &HashMap<SubjectId, HashSet<TeacherId>>,
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
) -> bool {
    // Count placements per lesson, then collect lessons whose count is
    // below `hours_per_week`. Pinned lessons are excluded (placements
    // caller-fixed). Lesson-group members are included; the rescue
    // dispatches to a group-aware path that ruins all members and calls
    // `try_place_group`.
    let mut placement_counts: HashMap<LessonId, u8> = HashMap::new();
    for p in placements.iter() {
        *placement_counts.entry(p.lesson_id).or_insert(0) += 1;
    }
    let mut underplaced: Vec<LessonId> = problem
        .lessons
        .iter()
        .filter_map(|l| {
            if pinned.contains(&l.id) {
                return None;
            }
            let count = placement_counts.get(&l.id).copied().unwrap_or(0);
            if count < l.hours_per_week {
                Some(l.id)
            } else {
                None
            }
        })
        .collect();
    // Deterministic order before the rescue RNG samples.
    underplaced.sort_unstable_by_key(|id| id.0);

    // Consume both RNG draws unconditionally so the R&R RNG sequence is
    // invariant across abort branches. `random_range(0..1)` always returns
    // 0 and is the cheapest placeholder when the real range is empty.
    let unplaced_idx = if underplaced.is_empty() {
        let _ = rr_rng.random_range(0..1u32);
        let _ = rr_rng.random_range(0..1u32);
        return false;
    } else {
        rr_rng.random_range(0..underplaced.len())
    };

    let target_id = underplaced[unplaced_idx];
    let target_lesson = match lesson_lookup.get(&target_id) {
        Some(l) => *l,
        None => {
            let _ = rr_rng.random_range(0..1u32);
            return false;
        }
    };

    // Lesson-group target: dispatch to group-rescue, ruining all members'
    // placements and re-placing the group via `try_place_group`. This
    // path consumes its own RNG draws so the rr_rng stream stays
    // invariant across branches.
    if let Some(group_id) = target_lesson.lesson_group_id {
        // Second RNG draw consumed for invariance; group rescue is
        // deterministic given state and does not need a second decision.
        let _ = rr_rng.random_range(0..1u32);
        return rr_rescue_group_attempt(
            problem,
            idx,
            weights,
            class_teacher_lookup,
            subject_qualified_teachers,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
            tb_order,
            room_order,
            max_position_per_day,
            teacher_max,
            class_max_lessons_per_day,
            group_id,
        );
    }
    let target_count_before = placement_counts.get(&target_id).copied().unwrap_or(0);
    let target_classes: HashSet<SchoolClassId> =
        target_lesson.school_class_ids.iter().copied().collect();

    // Collect indices of same-class, non-target sibling anchors so the
    // rescue can free additional class-day slots beyond the target's
    // own. The sibling pick consumes the second RNG draw below;
    // emptiness aborts after both draws are consumed.
    let mut sibling_candidates: Vec<usize> = placements
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            if pinned.contains(&p.lesson_id) {
                return None;
            }
            if p.lesson_id == target_id {
                return None;
            }
            let lesson = lesson_lookup.get(&p.lesson_id)?;
            if lesson.lesson_group_id.is_some() {
                return None;
            }
            let shares_class = lesson
                .school_class_ids
                .iter()
                .any(|c| target_classes.contains(c));
            if shares_class {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    sibling_candidates.sort_unstable();

    let sibling_pick = if sibling_candidates.is_empty() {
        let _ = rr_rng.random_range(0..1u32);
        return false;
    } else {
        rr_rng.random_range(0..sibling_candidates.len())
    };
    let sibling_anchor_idx = sibling_candidates[sibling_pick];
    let sibling_lesson_id = placements[sibling_anchor_idx].lesson_id;
    let sibling_lesson = lesson_lookup
        .get(&sibling_lesson_id)
        .copied()
        .expect("sibling lesson must resolve");

    // Snapshot every piece of state the rollback path needs to restore.
    // Mirrors `rr_attempt`'s pre-attempt snapshot pattern.
    let pre_slice = state.search_score_slice;
    let pre_canonical = state.canonical_score;
    let pre_count = placements.len();
    let pre_class_subject_teacher = state.class_subject_teacher.clone();

    // Phase 1a: ruin every existing placement of the target lesson, grouped
    // by day so `rr_ruin_block` handles the per-(lesson, day) block
    // atomically. Collect the days first; the inner ruin call mutates
    // placements.
    let target_days: Vec<u8> = {
        let mut days: HashSet<u8> = HashSet::new();
        for p in placements.iter() {
            if p.lesson_id == target_id {
                if let Some(tb) = tb_lookup.get(&p.time_block_id) {
                    days.insert(tb.day_of_week);
                }
            }
        }
        let mut v: Vec<u8> = days.into_iter().collect();
        v.sort_unstable();
        v
    };

    let mut snapshots: Vec<(LessonId, BlockSnapshot)> = Vec::with_capacity(target_days.len() + 1);
    for day in &target_days {
        let Some(idx_anchor) = placements.iter().position(|p| {
            p.lesson_id == target_id
                && tb_lookup
                    .get(&p.time_block_id)
                    .is_some_and(|tb| tb.day_of_week == *day)
        }) else {
            continue;
        };
        let snap = rr_ruin_block(idx_anchor, target_lesson, tb_lookup, placements, state);
        snapshots.push((target_id, snap));
    }

    // Phase 1b: ruin the chosen same-class sibling anchor to free
    // additional class-day slots the target needs. Look up its index
    // fresh; the prior target-ruin may have shifted indices.
    let sibling_snap = {
        let Some(fresh_idx) = placements
            .iter()
            .position(|p| p.lesson_id == sibling_lesson_id)
        else {
            // Sibling vanished (would only happen if target somehow shared
            // the sibling slot, which the filter above rules out). Abort
            // gracefully; rollback path will re-add the target's rows.
            rr_rollback(
                &[Vec::new()],
                &snapshots,
                lesson_lookup,
                tb_lookup,
                placements,
                state,
            );
            state.search_score_slice = pre_slice;
            state.canonical_score = pre_canonical;
            state.class_subject_teacher = pre_class_subject_teacher;
            return false;
        };
        rr_ruin_block(fresh_idx, sibling_lesson, tb_lookup, placements, state)
    };
    snapshots.push((sibling_lesson_id, sibling_snap));

    // Post-destroy lock cleanup: walk the surviving placements to
    // determine which `(class, subject)` pairs still have a placement,
    // and clear any lock for pairs whose every placement was ruined.
    // The target lesson's `(class, subject)` pair is precisely the one
    // we want to clear, so the next `try_place_block` can pick a fresh
    // teacher with capacity.
    {
        let mut surviving_pairs: HashSet<(SchoolClassId, SubjectId)> = HashSet::new();
        for p in placements.iter() {
            let Some(lesson) = lesson_lookup.get(&p.lesson_id) else {
                continue;
            };
            for class in &lesson.school_class_ids {
                surviving_pairs.insert((*class, lesson.subject_id));
            }
        }
        state
            .class_subject_teacher
            .retain(|key, _| surviving_pairs.contains(key));
    }

    let days: u8 = problem
        .time_blocks
        .iter()
        .map(|tb| tb.day_of_week)
        .max()
        .map(|m| m.saturating_add(1))
        .unwrap_or(0);

    // Phase 2a: re-place the target lesson from scratch in blocks of
    // `preferred_block_size`, iterating up to `hours_per_week /
    // preferred_block_size` times. `validate_structural` guarantees
    // divisibility. Each successful block places `n` placements.
    let target_n = target_lesson.preferred_block_size;
    let block_count: u8 = target_lesson.hours_per_week / target_n.max(1);
    let target_len_before = placements.len();
    let mut blocks_placed: u8 = 0;
    for _ in 0..block_count {
        let placed = crate::solve::try_place_block(
            problem,
            target_lesson,
            target_n,
            idx,
            teacher_max,
            class_max_lessons_per_day,
            weights,
            home_room_lookup,
            class_teacher_lookup,
            subject_qualified_teachers,
            state,
            placements,
            tb_order,
            room_order,
            max_position_per_day,
            days,
        );
        if !placed {
            break;
        }
        blocks_placed += 1;
    }
    let target_added: Vec<Placement> = placements[target_len_before..].to_vec();
    let target_count_after: u8 = blocks_placed.saturating_mul(target_n);

    // Phase 2b: recreate the ruined sibling anchor.
    let sibling_n = sibling_lesson.preferred_block_size;
    let sibling_len_before = placements.len();
    let _ = crate::solve::try_place_block(
        problem,
        sibling_lesson,
        sibling_n,
        idx,
        teacher_max,
        class_max_lessons_per_day,
        weights,
        home_room_lookup,
        class_teacher_lookup,
        subject_qualified_teachers,
        state,
        placements,
        tb_order,
        room_order,
        max_position_per_day,
        days,
    );
    let sibling_added: Vec<Placement> = placements[sibling_len_before..].to_vec();
    let sibling_fully_recreated = sibling_added.len() == sibling_n as usize;

    // Acceptance gate: target's post-attempt count is strictly greater
    // than its pre-attempt count AND the sibling anchor fully recreated.
    // The combined gate ensures the rescue never leaves the placement
    // count below where it started: ruin removed `target_count_before +
    // sibling_n`; recreate adds `target_count_after + sibling_added.len()`;
    // accepting iff `target_count_after > target_count_before AND
    // sibling_added.len() == sibling_n` keeps net delta strictly
    // positive (>= 1).
    let made_progress = target_count_after > target_count_before && sibling_fully_recreated;

    if !made_progress {
        // Reject. Use `rr_rollback` to undo: target_added and sibling_added
        // are the two recreated row groups; snapshots are the per-day
        // ruined blocks plus the sibling snapshot.
        let recreated_rows = vec![target_added, sibling_added];
        rr_rollback(
            &recreated_rows,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
        );
        state.search_score_slice = pre_slice;
        state.canonical_score = pre_canonical;
        state.class_subject_teacher = pre_class_subject_teacher;
        debug_assert_eq!(
            placements.len(),
            pre_count,
            "rr_rescue rollback left placement count drifted (pre={pre_count} post={})",
            placements.len(),
        );
        return false;
    }

    // Accept. Recompute canonical and slice from the post-recreate state;
    // mirror `rr_attempt`'s post-accept score recomputation. The
    // per-iteration `debug_assert_eq!` invariant at the LAHC loop tail
    // gates correctness; an off-by-something here would fire it
    // immediately under any property or integration test.
    state.canonical_score =
        crate::score::score_solution(problem, placements, weights, &state.soft_pinned_blocks);
    state.search_score_slice =
        running_slice_from_placements(problem, placements, weights, max_position_per_day);
    true
}

/// Group-rescue variant of `rr_rescue_attempt`: ruin every placement of
/// every member of the target lesson group, clear the relevant
/// `(class, subject)` teacher locks, then re-place the group from
/// scratch via `try_place_group`. Returns true when accepted (any
/// member's placement count strictly increased), false when rejected.
/// Group placement is atomic: all members co-place at the same `(day,
/// position)` window. Acceptance requires net forward progress on the
/// group; per-member counts move in lockstep so checking any member is
/// sufficient.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn rr_rescue_group_attempt(
    problem: &Problem,
    idx: &Indexed,
    weights: &ConstraintWeights,
    class_teacher_lookup: &HashMap<SchoolClassId, Option<TeacherId>>,
    subject_qualified_teachers: &HashMap<SubjectId, HashSet<TeacherId>>,
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
    tb_order: &[usize],
    room_order: &[usize],
    max_position_per_day: &HashMap<u8, u8>,
    teacher_max: &HashMap<TeacherId, u8>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    group_id: LessonGroupId,
) -> bool {
    // Collect member indices and ids by walking problem.lessons.
    let mut member_indices: Vec<usize> = Vec::new();
    let mut member_ids: HashSet<LessonId> = HashSet::new();
    for (i, l) in problem.lessons.iter().enumerate() {
        if l.lesson_group_id == Some(group_id) {
            member_indices.push(i);
            member_ids.insert(l.id);
        }
    }
    if member_indices.len() < 2 {
        return false;
    }

    // First member dictates the block shape used by FFD (`try_place_group`
    // uses the first member's `preferred_block_size` and `hours_per_week`).
    let first_member = &problem.lessons[member_indices[0]];
    let n = first_member.preferred_block_size;
    let block_count: u8 = first_member.hours_per_week / n.max(1);

    // Snapshot for rollback.
    let pre_slice = state.search_score_slice;
    let pre_canonical = state.canonical_score;
    let pre_count = placements.len();
    let pre_class_subject_teacher = state.class_subject_teacher.clone();

    // Phase 1: ruin every existing placement of every member. Group
    // co-placement means each `(member_lesson, day)` block contains a
    // single `(member, day)` block, so we can use `rr_ruin_block` per
    // member per day.
    let mut snapshots: Vec<(LessonId, BlockSnapshot)> = Vec::new();
    let mut member_pre_counts: HashMap<LessonId, u8> = HashMap::new();
    for p in placements.iter() {
        if member_ids.contains(&p.lesson_id) {
            *member_pre_counts.entry(p.lesson_id).or_insert(0) += 1;
        }
    }
    loop {
        let Some(pos) = placements
            .iter()
            .position(|p| member_ids.contains(&p.lesson_id))
        else {
            break;
        };
        let lesson_id = placements[pos].lesson_id;
        let lesson = lesson_lookup
            .get(&lesson_id)
            .copied()
            .expect("member lesson must resolve");
        let snap = rr_ruin_block(pos, lesson, tb_lookup, placements, state);
        snapshots.push((lesson_id, snap));
    }

    // Post-destroy lock cleanup: clear locks for pairs whose every
    // placement was ruined. The group's (member_class, member_subject)
    // pairs are the ones we want cleared so `try_place_group` can pick
    // fresh teachers within the per-member candidate sets.
    {
        let mut surviving_pairs: HashSet<(SchoolClassId, SubjectId)> = HashSet::new();
        for p in placements.iter() {
            let Some(lesson) = lesson_lookup.get(&p.lesson_id) else {
                continue;
            };
            for class in &lesson.school_class_ids {
                surviving_pairs.insert((*class, lesson.subject_id));
            }
        }
        state
            .class_subject_teacher
            .retain(|key, _| surviving_pairs.contains(key));
    }

    let days: u8 = problem
        .time_blocks
        .iter()
        .map(|tb| tb.day_of_week)
        .max()
        .map(|m| m.saturating_add(1))
        .unwrap_or(0);

    // Phase 2: re-place the group from scratch in `block_count` blocks
    // of size `n`. `try_place_group` co-places all members at one
    // `(day, position)` window; each successful call places
    // `members.len() * n` placements.
    let pre_replace_count = placements.len();
    let mut blocks_placed: u8 = 0;
    for _ in 0..block_count {
        let placed = crate::solve::try_place_group(
            problem,
            &member_indices,
            n,
            idx,
            teacher_max,
            class_max_lessons_per_day,
            weights,
            class_teacher_lookup,
            subject_qualified_teachers,
            state,
            placements,
            tb_order,
            room_order,
            max_position_per_day,
            days,
        );
        if !placed {
            break;
        }
        blocks_placed += 1;
    }

    // Acceptance gate: each member's post-attempt count is strictly
    // greater than its pre-attempt count. Since per-member counts move
    // in lockstep with `blocks_placed * n`, check `blocks_placed` against
    // the worst-case pre-count across members.
    let max_pre_count: u8 = member_ids
        .iter()
        .map(|id| member_pre_counts.get(id).copied().unwrap_or(0))
        .max()
        .unwrap_or(0);
    let post_count: u8 = blocks_placed.saturating_mul(n);
    let made_progress = post_count > max_pre_count;

    if !made_progress {
        // Reject. Remove every newly-added placement (rows added since
        // pre_replace_count), then replay snapshots.
        let recreated: Vec<Placement> = placements[pre_replace_count..].to_vec();
        rr_rollback(
            &[recreated],
            &snapshots,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
        );
        state.search_score_slice = pre_slice;
        state.canonical_score = pre_canonical;
        state.class_subject_teacher = pre_class_subject_teacher;
        debug_assert_eq!(
            placements.len(),
            pre_count,
            "rr_rescue_group rollback left placement count drifted (pre={pre_count} post={})",
            placements.len(),
        );
        return false;
    }

    // Accept. Recompute canonical and slice in lockstep.
    state.canonical_score =
        crate::score::score_solution(problem, placements, weights, &state.soft_pinned_blocks);
    state.search_score_slice =
        running_slice_from_placements(problem, placements, weights, max_position_per_day);
    true
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
    // Item 75: read the teacher from the row itself. The Placement was
    // written by `try_place_block` / `kempe_apply_block` with
    // `teacher_id = chosen_teacher` and the matching
    // `state.used_teacher.insert((chosen_teacher, tb.id))`, so the row
    // is the canonical record of which slot we must decrement.
    // `lesson_teacher_in_state` is unsafe here because
    // `state.class_subject_teacher` may have drifted (R&R rollback can
    // run after a recreate inserted a different teacher into the lock
    // map, while the snapshot row preserves the original teacher).
    let teacher = row.teacher_id;
    state.used_teacher.remove(&(teacher, row.time_block_id));
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
    if let Some(part) = state.teacher_positions.get_mut(&(teacher, day)) {
        if let Ok(j) = part.binary_search(&position) {
            part.remove(j);
        }
        if part.is_empty() {
            state.teacher_positions.remove(&(teacher, day));
        }
    }
    if let Some(h) = state.hours_by_teacher.get_mut(&teacher) {
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

    // Item 75: read the teacher from the row itself, not from
    // `state.class_subject_teacher`. The snapshot row preserves the
    // original solver-picked teacher; reading state during R&R rollback
    // returns the recreate's pick (the lock map drifted) and inserts
    // into the wrong `used_teacher` slot, desynchronising state from
    // the replayed placement and surfacing as a double-booking when a
    // subsequent move writes a conflicting placement.
    let teacher = row.teacher_id;
    placements.push(row.clone());
    state.used_teacher.insert((teacher, row.time_block_id));
    for class in &lesson.school_class_ids {
        state.used_class.insert((*class, row.time_block_id));
        let part = state.class_positions.entry((*class, day)).or_default();
        let ins = part.binary_search(&position).unwrap_or_else(|i| i);
        if part.get(ins).copied() != Some(position) {
            part.insert(ins, position);
        }
    }
    state.used_room.insert((row.room_id, row.time_block_id));
    let part = state.teacher_positions.entry((teacher, day)).or_default();
    let ins = part.binary_search(&position).unwrap_or_else(|i| i);
    if part.get(ins).copied() != Some(position) {
        part.insert(ins, position);
    }
    *state.hours_by_teacher.entry(teacher).or_insert(0) += 1;
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
/// pinned or group-tagged placement, chain length exceeds
/// `config.lahc_kempe_max_chain` (passed as `max_chain`), or a chain
/// neighbour's destination window has missing positions.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn kempe_build_chain(
    state: &crate::solve::GreedyState,
    seed_lesson: LessonId,
    source_day: u8,
    dest_day: u8,
    start_pos: u8,
    placements: &[Placement],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
    pinned: &HashSet<LessonId>,
    max_chain: usize,
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
                let teacher_conflict = lesson_teacher_in_state(state, other)
                    == lesson_teacher_in_state(state, popped_lesson);
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
                // collisions at apply time (item 45). The chain is updated eagerly
                // (a few lines below) so two same-iteration new neighbours of the
                // same popped member that both go to `neighbour_dest` and share a
                // class or teacher are caught here too: the second one's check sees
                // the first one already in `chain`.
                let same_color_conflict = chain.iter().any(|(existing_id, existing_dest)| {
                    if *existing_dest != neighbour_dest {
                        return false;
                    }
                    let existing_lesson = match lesson_lookup.get(existing_id).copied() {
                        Some(l) => l,
                        None => return false,
                    };
                    let teacher_conflict = lesson_teacher_in_state(state, existing_lesson)
                        == lesson_teacher_in_state(state, other);
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
                    // Eager chain insert so the bipartiteness check above sees this
                    // candidate when later placements in the same popped iteration
                    // are evaluated. Frontier extension happens after the inner
                    // loops in deterministic LessonId.0 order; chain ordering is
                    // unobservable (HashMap, used only for membership / dest lookup).
                    chain.insert(placement.lesson_id, neighbour_dest);
                    if chain.len() > max_chain {
                        return ChainBuild::Aborted;
                    }
                }
            }
        }

        // Determinism: sort new neighbours before extending the frontier so
        // HashSet iteration order does not leak into the chain shape.
        new_neighbours.sort_unstable_by_key(|id| id.0);

        for neighbour_id in new_neighbours {
            frontier.push_back(neighbour_id);
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
        // Item 76: the partition gap delta key MUST match the teacher in
        // `state.teacher_positions` (and the row that
        // `kempe_apply_block` will insert), which is the lock-map teacher,
        // NOT `lesson.assigned_teacher_id()` (the pin shorthand that
        // falls back to `teacher_candidates[0]` under unpinned mode).
        // Using the wrong teacher key snapshots an irrelevant
        // partition's gap count, so `kempe_post_score_delta` misses the
        // actual change in the real teacher's `(teacher, day)` gap
        // count and `state.canonical_score` drifts from
        // `score_solution(...)` by the missed gap delta.
        let teacher = lesson_teacher_in_state(state, lesson);
        for src in &source_days {
            for class in &lesson.school_class_ids {
                class_keys.insert((*class, *src));
            }
            teacher_keys.insert((teacher, *src));
        }
        for class in &lesson.school_class_ids {
            class_keys.insert((*class, *dest_day));
        }
        teacher_keys.insert((teacher, *dest_day));
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
/// across a window of N consecutive positions. Item 68: caller supplies
/// `teacher` from the snapshot row so the swap preserves the original
/// solver-picked teacher (the lock invariant is preserved across Kempe).
#[allow(clippy::too_many_arguments)] // Reason: internal helper; teacher param avoids lookup churn
fn kempe_apply_block(
    lesson: &Lesson,
    dest_day: u8,
    start_pos: u8,
    room_id: RoomId,
    teacher: TeacherId,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
) {
    // Item 68 invariant gate: the chain swap must preserve every chain
    // member's teacher. The supplied `teacher` is the snapshot row's
    // `teacher_id`, which equals `state.class_subject_teacher` for any
    // member class of this lesson when the lock is set. The check here
    // guards against a future caller passing a different teacher (e.g.,
    // a refactor that drops the snapshot lookup).
    #[cfg(debug_assertions)]
    {
        for class in &lesson.school_class_ids {
            if let Some(locked_teacher) = state
                .class_subject_teacher
                .get(&(*class, lesson.subject_id))
            {
                debug_assert_eq!(
                    *locked_teacher, teacher,
                    "Kempe apply must preserve the per-(class, subject) teacher lock",
                );
            }
        }
    }
    let n = lesson.preferred_block_size;
    for k in 0..n {
        let pos = start_pos + k;
        let tb_id = tb_by_day_pos[&(dest_day, pos)];
        placements.push(Placement {
            lesson_id: lesson.id,
            time_block_id: tb_id,
            room_id,
            teacher_id: teacher,
        });
        state.used_teacher.insert((teacher, tb_id));
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
            .entry((teacher, dest_day))
            .or_default();
        let ins = part.binary_search(&pos).unwrap_or_else(|i| i);
        if part.get(ins).copied() != Some(pos) {
            part.insert(ins, pos);
        }
        *state.hours_by_teacher.entry(teacher).or_insert(0) += 1;
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
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    tb_by_day_pos: &HashMap<(u8, u8), TimeBlockId>,
    pinned: &HashSet<LessonId>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
    room_order: &[usize],
    max_position_per_day: &HashMap<u8, u8>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    lahc_list: &[u32],
    iter: u64,
    max_chain: usize,
) -> bool {
    let pre_slice = state.search_score_slice;
    let pre_canonical = state.canonical_score;

    // Item 57: capture the pre-attempt per-class worst-case axes cost so
    // the canonical delta below can include the new axes alongside slice /
    // home_room / class_day_balance. Snapshot the WEIGHTED total here;
    // post-apply we recompute against the now-mutated `placements`. Both
    // helpers short-circuit allocation when their weights are zero; the
    // outer-block guard cuts the call entirely when both axes are off.
    // Full recompute on Kempe accept is acceptable for ship-1 per the
    // item 57 plan: per-class-max delta arithmetic over a Kempe chain is
    // not free (chain members may belong to disjoint or overlapping
    // class sets), and the amortised cost is bounded by `lahc_kempe_period`
    // (default 50).
    let pre_new_axes_cost: u32 =
        if weights.max_per_class_spread == 0 && weights.max_per_class_interior_gaps == 0 {
            0
        } else {
            weights
                .max_per_class_spread
                .saturating_mul(crate::score::worst_class_spread(problem, placements))
                .saturating_add(
                    weights.max_per_class_interior_gaps.saturating_mul(
                        crate::score::worst_class_interior_gaps(problem, placements),
                    ),
                )
        };

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
        state,
        seed_lesson_id,
        source_day,
        dest_day,
        start_pos,
        placements,
        lesson_lookup,
        tb_lookup,
        tb_by_day_pos,
        pinned,
        max_chain,
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

    // Snapshot per-affected-class day-count vectors before ruin so the
    // canonical class_day_balance delta can be computed pre/post without
    // re-walking the full classes list. Pre-ruin so the math matches the
    // gap_delta pattern (gap_delta = post-apply - pre-ruin).
    let canonical_days: u8 = problem
        .time_blocks
        .iter()
        .map(|tb| tb.day_of_week)
        .max()
        .map(|m| m.saturating_add(1))
        .unwrap_or(0);
    let mut affected_classes: Vec<SchoolClassId> = Vec::new();
    {
        let mut seen: HashSet<SchoolClassId> = HashSet::new();
        for &lesson_id in &chain_order {
            let Some(lesson) = lesson_lookup.get(&lesson_id) else {
                continue;
            };
            for class in &lesson.school_class_ids {
                if seen.insert(*class) {
                    affected_classes.push(*class);
                }
            }
        }
    }
    let class_day_counts_pre: Vec<(SchoolClassId, Vec<u32>)> = affected_classes
        .iter()
        .map(|class_id| {
            let counts: Vec<u32> = (0..canonical_days)
                .map(|day| {
                    state
                        .class_positions
                        .get(&(*class_id, day))
                        .map(|v| v.len() as u32)
                        .unwrap_or(0)
                })
                .collect();
            (*class_id, counts)
        })
        .collect();

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
                state.search_score_slice = pre_slice;
                state.canonical_score = pre_canonical;
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
                state.search_score_slice = pre_slice;
                state.canonical_score = pre_canonical;
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
        let original_snapshot_row = snapshots
            .iter()
            .find(|(id, _, _)| *id == lesson_id)
            .map(|(_, _, snap)| snap.rows[0].clone())
            .expect("snapshot for chain member exists");
        let original_room_id = original_snapshot_row.room_id;
        // Item 68: derive the teacher from `state.class_subject_teacher`
        // (the source of truth for which teacher this lesson uses) so
        // unit tests that pre-seed state with placeholder placement
        // teacher_ids still chain correctly. In production-shaped
        // runs the snapshot row's `teacher_id` equals the lock-map
        // teacher; the debug_assert in `kempe_apply_block` enforces it.
        let original_teacher_id = lesson_teacher_in_state(state, lesson);
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
        // Travel-buffer pruning (ADR 0044). By the time this loop runs, every
        // chain member has been ruined, so the lesson's pre-move position is
        // not in `state.class_positions` / `state.teacher_positions`; pass
        // `ignore_self = None`. The anchor `tb_by_day_pos[&(dest, start_pos)]`
        // covers the whole block window because the helper inspects only the
        // pre / post adjacent slots. Reject the chain on violation so
        // `failed = true` routes to `kempe_rollback`.
        let Some(anchor_tb_id) = tb_by_day_pos.get(&(dest, start_pos)).copied() else {
            failed = true;
            break;
        };
        if crate::validate::would_violate_travel_buffer(
            problem,
            state,
            placements,
            lesson,
            anchor_tb_id,
            original_teacher_id,
            None,
        ) {
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
            original_teacher_id,
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
        state.search_score_slice = pre_slice;
        state.canonical_score = pre_canonical;
        return false;
    }

    let gap_delta = kempe_post_score_delta(&partition_snapshot, state, weights);
    let subject_pref_delta = i64::from(added_subject_pref) - i64::from(removed_subject_pref);
    let total_delta = gap_delta + subject_pref_delta;
    let new_slice_signed = i64::from(pre_slice) + total_delta;
    let new_slice = u32::try_from(new_slice_signed.max(0)).unwrap_or(u32::MAX);

    // Canonical home_room delta: walk snapshot rows for `removed`,
    // recreated_in_order rows for `added`. Mirror's the existing
    // removed_subject_pref / added_subject_pref accumulation pattern.
    let home_room_delta: i64 = if weights.prefer_home_room == 0 {
        0
    } else {
        let mut removed_home_room: u32 = 0;
        let mut added_home_room: u32 = 0;
        for (lesson_id, _src, snap) in snapshots.iter() {
            let Some(lesson) = lesson_lookup.get(lesson_id) else {
                continue;
            };
            for row in &snap.rows {
                for class in &lesson.school_class_ids {
                    removed_home_room = removed_home_room.saturating_add(
                        crate::score::home_room_penalty_one_class(
                            *class,
                            home_room_lookup,
                            row.room_id,
                            weights,
                        ),
                    );
                }
            }
        }
        for (lesson_id, dest_d, dest_start_pos) in recreated_in_order.iter() {
            let Some(lesson) = lesson_lookup.get(lesson_id) else {
                continue;
            };
            let n = lesson.preferred_block_size;
            for k in 0..n {
                let row_pos = dest_start_pos + k;
                let Some(tb_id) = tb_by_day_pos.get(&(*dest_d, row_pos)) else {
                    continue;
                };
                let Some(p) = placements
                    .iter()
                    .find(|p| p.lesson_id == *lesson_id && p.time_block_id == *tb_id)
                else {
                    continue;
                };
                for class in &lesson.school_class_ids {
                    added_home_room =
                        added_home_room.saturating_add(crate::score::home_room_penalty_one_class(
                            *class,
                            home_room_lookup,
                            p.room_id,
                            weights,
                        ));
                }
            }
        }
        i64::from(added_home_room) - i64::from(removed_home_room)
    };

    // Canonical class_day_balance delta: per-affected-class pre-cost from
    // the pre-ruin snapshot, post-cost from the now-mutated state.
    let class_day_balance_delta: i64 = if weights.class_day_balance == 0 {
        0
    } else {
        let mut acc: i64 = 0;
        for (class_id, pre_counts) in &class_day_counts_pre {
            let pre_cost = crate::score::class_day_balance_cost_for_class_from_counts(
                *class_id,
                canonical_days,
                pre_counts,
            );
            let post_cost = crate::score::class_day_balance_cost_for_class(
                *class_id,
                canonical_days,
                &state.class_positions,
            );
            acc += i64::from(post_cost) - i64::from(pre_cost);
        }
        i64::from(weights.class_day_balance) * acc
    };

    // Item 57: per-class worst-case axes delta. `placements` is now in the
    // post-apply state; compute the weighted post-cost and subtract the
    // pre-attempt snapshot captured at the top of `kempe_attempt`.
    let new_axes_delta: i64 =
        if weights.max_per_class_spread == 0 && weights.max_per_class_interior_gaps == 0 {
            0
        } else {
            let post_cost: u32 =
                weights
                    .max_per_class_spread
                    .saturating_mul(crate::score::worst_class_spread(problem, placements))
                    .saturating_add(weights.max_per_class_interior_gaps.saturating_mul(
                        crate::score::worst_class_interior_gaps(problem, placements),
                    ));
            i64::from(post_cost) - i64::from(pre_new_axes_cost)
        };

    let canonical_delta = total_delta + home_room_delta + class_day_balance_delta + new_axes_delta;
    let new_canonical_signed = i64::from(pre_canonical) + canonical_delta;
    let new_canonical = u32::try_from(new_canonical_signed.max(0)).unwrap_or(u32::MAX);

    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    // Item 52: Kempe accepts on canonical (slice + home_room + class_day_balance)
    // so chain swaps that strictly improve slice while worsening canonical
    // are rejected. The slice ride-along stays so the next Change move sees
    // a consistent `state.search_score_slice` baseline.
    let lahc_ok = new_canonical <= pre_canonical || new_canonical <= prior;
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
        state.search_score_slice = pre_slice;
        state.canonical_score = pre_canonical;
        return false;
    }
    state.search_score_slice = new_slice;
    state.canonical_score = new_canonical;
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
            // Item 75: read the teacher from the row itself. Same
            // canonical-record argument as `rr_remove_row_bookkeeping`;
            // safer than the state-derived helper because the lock map
            // may have drifted before rollback runs.
            let teacher = p.teacher_id;
            state.used_teacher.remove(&(teacher, p.time_block_id));
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
            if let Some(part) = state.teacher_positions.get_mut(&(teacher, day)) {
                if let Ok(j) = part.binary_search(&position) {
                    part.remove(j);
                }
                if part.is_empty() {
                    state.teacher_positions.remove(&(teacher, day));
                }
            }
            if let Some(h) = state.hours_by_teacher.get_mut(&teacher) {
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
/// `state.search_score_slice`. R&R uses this after a successful recreate
/// because `rr_ruin_block` does not decrement the removed contribution and a
/// fresh `score::score_solution` would over-count by
/// `class_day_balance + home_room`.
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
        // Item 76: read teacher from the placement row, not from
        // `lesson.assigned_teacher_id()` (the pin shorthand). Under
        // unpinned mode, the actually-placed teacher comes from the
        // solver's pick recorded on `Placement.teacher_id`; the static
        // fallback would partition under the wrong key and produce a
        // teacher_gap total that disagrees with `score_solution(...)`.
        by_teacher_day
            .entry((p.teacher_id, tb.day_of_week))
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

/// Home-room repair move (item 86 option b). At a fixed time block, attempts
/// to move the placement at `placement_idx` into its class's `home_room`. Two
/// paths fused: (A) room-free: home_room is unbooked at the placement's TB,
/// rewrite the placement's room_id; (B) collision-swap: home_room is held by
/// exactly one other placement Q, swap rooms between P and Q. Both paths
/// accept on the LAHC canonical-delta criterion against
/// `lahc_list[iter % L]`. At a fixed time block, only the `prefer_home_room`
/// axis can change (class/teacher gap, day balance, per-class spread,
/// interior-gap, supervision-spread all partition on `(class|teacher, day)`
/// not rooms), so the canonical delta is exactly
/// `sum(home_room_penalty_one_class(class, new_room) - home_room_penalty_one_class(class, old_room))`
/// summed across the affected member classes. Returns `true` on accept.
/// Pure helper: feasibility-rejecting paths leave `placements` and `state`
/// untouched.
#[allow(clippy::too_many_arguments)] // Reason: internal helper
fn try_home_room_repair_move(
    _problem: &Problem,
    idx: &Indexed,
    placement_idx: usize,
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    _subject_lookup: &HashMap<SubjectId, &Subject>,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    weights: &ConstraintWeights,
    placements: &mut [Placement],
    state: &mut crate::solve::GreedyState,
    pinned: &HashSet<LessonId>,
    lahc_list: &[u32],
    iter: u64,
    _room_order: &[usize],
) -> bool {
    if placement_idx >= placements.len() {
        return false;
    }
    let p = placements[placement_idx].clone();
    let lesson = lesson_lookup[&p.lesson_id];
    if lesson.preferred_block_size > 1 {
        return false;
    }
    if lesson.lesson_group_id.is_some() {
        return false;
    }
    if pinned.contains(&p.lesson_id) {
        return false;
    }
    // Resolve home_room: all member classes must share the same Some(home).
    let mut home_room: Option<RoomId> = None;
    for cls in &lesson.school_class_ids {
        match home_room_lookup.get(cls) {
            Some(Some(h)) => match home_room {
                None => home_room = Some(*h),
                Some(existing) => {
                    if existing != *h {
                        return false;
                    }
                }
            },
            _ => return false,
        }
    }
    let Some(home_room) = home_room else {
        return false;
    };
    if p.room_id == home_room {
        return false;
    }

    let Some(tb) = tb_lookup.get(&p.time_block_id) else {
        return false;
    };
    if idx.room_blocked(home_room, tb.id) {
        return false;
    }

    // Compute P's per-class delta: home_room_penalty_one_class(class, home)
    // - home_room_penalty_one_class(class, old_room) for each member class.
    let mut p_delta_signed: i64 = 0;
    for cls in &lesson.school_class_ids {
        let pre =
            crate::score::home_room_penalty_one_class(*cls, home_room_lookup, p.room_id, weights)
                as i64;
        let post =
            crate::score::home_room_penalty_one_class(*cls, home_room_lookup, home_room, weights)
                as i64;
        p_delta_signed += post - pre;
    }

    let lahc_threshold = lahc_list[(iter as usize) % lahc_list.len()];
    let old_room = p.room_id;

    // Check whether home_room is occupied at this TB. used_room is keyed by
    // (room, tb). If absent, we take path (A).
    if !state.used_room.contains(&(home_room, tb.id)) {
        // Path (A): room-free.
        // Subject suitability: if home_room is not suitable for the lesson's
        // subject, the move is feasible only if we treat it as infeasible.
        if !idx.room_suits_subject(home_room, lesson.subject_id) {
            return false;
        }
        // Canonical accept: new_canonical = state.canonical_score + p_delta_signed.
        let new_canonical = saturating_apply_delta(state.canonical_score, p_delta_signed);
        if new_canonical > lahc_threshold {
            return false;
        }
        // Apply: rewrite placement room; update used_room.
        placements[placement_idx].room_id = home_room;
        state.used_room.remove(&(old_room, tb.id));
        state.used_room.insert((home_room, tb.id));
        state.canonical_score = new_canonical;
        return true;
    }

    // Path (B): collision-swap. Locate the placement Q whose
    // (room_id, time_block_id) == (home_room, tb.id).
    let q_idx_opt = placements
        .iter()
        .position(|q| q.room_id == home_room && q.time_block_id == tb.id);
    let q_idx = match q_idx_opt {
        Some(i) if i != placement_idx => i,
        _ => return false,
    };
    let q = placements[q_idx].clone();
    let q_lesson = lesson_lookup[&q.lesson_id];
    if pinned.contains(&q.lesson_id) {
        return false;
    }
    if q_lesson.lesson_group_id.is_some() {
        return false;
    }
    if q_lesson.preferred_block_size > 1 {
        return false;
    }
    // Q must fit subject-wise in P's old room.
    if !idx.room_suits_subject(old_room, q_lesson.subject_id) {
        return false;
    }
    // P must fit subject-wise in home_room.
    if !idx.room_suits_subject(home_room, lesson.subject_id) {
        return false;
    }
    // The (room_id, tb.id) entry for old_room is P's; the entry for home_room
    // is Q's. Both stay occupied post-swap (rooms persist, owners shift), so
    // the used_room HashSet is unchanged on accept.
    //
    // Compute Q's per-class delta: post is home_room_penalty_one_class(...,
    // old_room) (Q's new room), pre is ...(..., home_room) (Q's current room).
    let mut q_delta_signed: i64 = 0;
    for cls in &q_lesson.school_class_ids {
        let pre =
            crate::score::home_room_penalty_one_class(*cls, home_room_lookup, q.room_id, weights)
                as i64;
        let post =
            crate::score::home_room_penalty_one_class(*cls, home_room_lookup, old_room, weights)
                as i64;
        q_delta_signed += post - pre;
    }
    let total_delta_signed = p_delta_signed + q_delta_signed;
    let new_canonical = saturating_apply_delta(state.canonical_score, total_delta_signed);
    if new_canonical > lahc_threshold {
        return false;
    }
    // Apply.
    placements[placement_idx].room_id = home_room;
    placements[q_idx].room_id = old_room;
    state.canonical_score = new_canonical;
    true
}

/// Saturating apply of a signed delta to an unsigned canonical score.
/// Used by `try_home_room_repair_move` to compute `new_canonical` without
/// underflowing or overflowing on extreme deltas.
fn saturating_apply_delta(score: u32, delta: i64) -> u32 {
    if delta >= 0 {
        score.saturating_add(delta as u32)
    } else {
        let abs = (-delta) as u32;
        score.saturating_sub(abs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubjectId;
    use crate::types::TimeBlockKind;
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
            kind: TimeBlockKind::Lesson,
        };

        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };

        // Item 75: rr_remove_row_bookkeeping reads row.teacher_id, so the
        // placeholder must match the teacher_id seeded into state.
        let mut placements = vec![Placement {
            lesson_id,
            time_block_id: tb.id,
            room_id: room,
            teacher_id: teacher,
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
            kind: TimeBlockKind::Lesson,
        };
        let tb_b = TimeBlock {
            id: TimeBlockId(lahc_uuid(11)),
            day_of_week: 0,
            position: 1,
            kind: TimeBlockKind::Lesson,
        };

        let lesson_a_obj = Lesson {
            id: lesson_a,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };

        let mut placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_a.id,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_b.id,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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
            kind: TimeBlockKind::Lesson,
        };
        let tb_b = TimeBlock {
            id: TimeBlockId(lahc_uuid(11)),
            day_of_week: 0,
            position: 1,
            kind: TimeBlockKind::Lesson,
        };

        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 2,
            preferred_block_size: 2,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };

        // Item 75: rr_remove_row_bookkeeping reads row.teacher_id (the
        // canonical record of which used_teacher slot to decrement), so
        // the placeholder must match the teacher_id seeded into state.
        let mut placements = vec![
            Placement {
                lesson_id,
                time_block_id: tb_a.id,
                room_id: room,
                teacher_id: teacher,
            },
            Placement {
                lesson_id,
                time_block_id: tb_b.id,
                room_id: room,
                teacher_id: teacher,
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
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
                hours_per_week: 1,
                preferred_block_size: 1,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
                lesson_group_id: None,
            },
            Lesson {
                id: lesson_pinned,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
                hours_per_week: 1,
                preferred_block_size: 1,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
                lesson_group_id: None,
            },
            Lesson {
                id: lesson_grouped,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
                hours_per_week: 1,
                preferred_block_size: 1,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
                lesson_group_id: Some(group_id),
            },
        ];

        let tbs: Vec<TimeBlock> = (0..3)
            .map(|i| TimeBlock {
                id: TimeBlockId(lahc_uuid(10 + i)),
                day_of_week: 0,
                position: i,
                kind: TimeBlockKind::Lesson,
            })
            .collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            tbs.iter().map(|tb| (tb.id, tb)).collect();

        let placements = vec![
            Placement {
                lesson_id: lesson_free,
                time_block_id: tbs[0].id,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lesson_pinned,
                time_block_id: tbs[1].id,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lesson_grouped,
                time_block_id: tbs[2].id,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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
            kind: TimeBlockKind::Lesson,
        };
        let new_tb = TimeBlock {
            id: TimeBlockId(lahc_uuid(11)),
            day_of_week: 0,
            position: 1,
            kind: TimeBlockKind::Lesson,
        };
        let old_room = RoomId(lahc_uuid(30));
        let new_room = RoomId(lahc_uuid(31));
        let lesson_id = LessonId(lahc_uuid(60));

        let mut placements = vec![Placement {
            lesson_id,
            time_block_id: old_tb.id,
            room_id: old_room,
            teacher_id: TeacherId(Uuid::nil()),
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
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_one,
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![Teacher {
                id: teacher,
                max_hours_per_week: 10,
                reserve_hours_per_week: 0,
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
            lessons: vec![Lesson {
                id: lesson,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
                hours_per_week: 1,
                preferred_block_size: 1,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
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
            teacher_id: TeacherId(Uuid::nil()),
        }];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0]));
        state.teacher_positions.insert((teacher, 0), vec_part(&[0]));
        state.used_teacher.insert((teacher, tb_zero));
        state.used_class.insert((class, tb_zero));
        state.used_room.insert((room, tb_zero));
        state.search_score_slice = 1; // avoid_first penalty active at position 0
        state.canonical_score = 1;

        let config = SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            deadline: Some(std::time::Duration::from_millis(50)),
            // 600 iterations fill the entire 500-slot LAHC list with the
            // optimal score (0) so worsening moves are no longer accepted.
            max_iterations: Some(600),
            ..SolveConfig::default()
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
            None,
        );

        assert_eq!(placements.len(), 1);
        assert_eq!(
            placements[0].time_block_id, tb_one,
            "LAHC should move the avoid-first lesson off position 0"
        );
        assert_eq!(state.search_score_slice, 0);
    }

    /// Pre-Task-5, this test asserted that LAHC never moved a block placement
    /// (because the Change branch had a `preferred_block_size > 1` short-circuit
    /// and Kempe is asymmetric on block placements). Task 5 wires
    /// `try_change_block_move` into the production Change branch, so block
    /// placements now move. The test is repurposed: with `avoid_first_period=1`
    /// active on the seed window (positions 0 + 1), LAHC must shift the
    /// doppelstunde off position 0 and keep it contiguous.
    #[test]
    fn lahc_moves_block_placement_off_avoid_first_window() {
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
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_one,
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_two,
                    day_of_week: 0,
                    position: 2,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_three,
                    day_of_week: 0,
                    position: 3,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![Teacher {
                id: teacher,
                max_hours_per_week: 10,
                reserve_hours_per_week: 0,
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
            lessons: vec![Lesson {
                id: lesson,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
                hours_per_week: 2,
                preferred_block_size: 2,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
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
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lesson,
                time_block_id: tb_one,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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
        state.search_score_slice = 1; // avoid_first penalty active at position 0
        state.canonical_score = 1;

        let config = SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2000),
            ..SolveConfig::default()
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
            None,
        );

        // The block must move (Change branch routes through try_change_block_move
        // under the 3-draw budget) and stay contiguous on a single day.
        assert_eq!(
            placements.len(),
            2,
            "block size invariant: still 2 placements"
        );
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let days: HashSet<u8> = placements
            .iter()
            .map(|p| tb_lookup[&p.time_block_id].day_of_week)
            .collect();
        assert_eq!(
            days.len(),
            1,
            "block lesson on one day; got days={:?}",
            days
        );
        let mut positions: Vec<u8> = placements
            .iter()
            .map(|p| tb_lookup[&p.time_block_id].position)
            .collect();
        positions.sort_unstable();
        assert_eq!(
            positions[1] - positions[0],
            1,
            "block lesson positions must be contiguous; got positions={:?}",
            positions
        );
        // Avoid-first penalty escaped: the anchor must be off position 0.
        assert!(
            positions[0] > 0,
            "LAHC must move the block off avoid_first window; got positions={:?}",
            positions
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
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_one,
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_two,
                    day_of_week: 0,
                    position: 2,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_three,
                    day_of_week: 0,
                    position: 3,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![
                Teacher {
                    id: teacher_a,
                    max_hours_per_week: 10,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: teacher_b,
                    max_hours_per_week: 10,
                    reserve_hours_per_week: 0,
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
                    id: lesson_a,
                    school_class_ids: vec![class_a, class_b],
                    subject_id: subject,
                    teacher_candidates: vec![teacher_a],
                    teacher_pin: Some(teacher_a),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: Some(group_id),
                },
                Lesson {
                    id: lesson_b,
                    school_class_ids: vec![class_a, class_b],
                    subject_id: subject,
                    teacher_candidates: vec![teacher_b],
                    teacher_pin: Some(teacher_b),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
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
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_zero,
                room_id: room_b,
                teacher_id: TeacherId(Uuid::nil()),
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
        state.search_score_slice = 2;
        state.canonical_score = 2;

        let config = SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2000),
            ..SolveConfig::default()
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
            None,
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
                kind: TimeBlockKind::Lesson,
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
                kind: TimeBlockKind::Lesson,
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
                kind: TimeBlockKind::Lesson,
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
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class = SchoolClassId(lahc_uuid(50));
        let teacher_a = TeacherId(lahc_uuid(20));
        let teacher_b = TeacherId(lahc_uuid(21));
        // Item 68: distinct subjects per lesson so the per-(class,
        // subject) teacher-uniformity lock does not collide across the
        // two lessons (which carry different teacher pins).
        let subject_a = SubjectId(lahc_uuid(40));
        let subject_b = SubjectId(lahc_uuid(41));
        let room_a = RoomId(lahc_uuid(30));
        let room_b = RoomId(lahc_uuid(31));
        let lesson_a = LessonId(lahc_uuid(60));
        let lesson_b = LessonId(lahc_uuid(61));

        let tbs: Vec<TimeBlock> = (0..8)
            .map(|p| TimeBlock {
                id: TimeBlockId(lahc_uuid(10 + p as u8)),
                day_of_week: 0,
                position: p as u8,
                kind: TimeBlockKind::Lesson,
            })
            .collect();

        let problem = Problem {
            time_blocks: tbs.clone(),
            teachers: vec![
                Teacher {
                    id: teacher_a,
                    max_hours_per_week: 10,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: teacher_b,
                    max_hours_per_week: 10,
                    reserve_hours_per_week: 0,
                },
            ],
            rooms: vec![Room { id: room_a }, Room { id: room_b }],
            subjects: vec![
                Subject {
                    id: subject_a,
                    prefer_early_period: 0,
                    avoid_first_period: 0,
                    avoid_last_period: 0,
                    prefer_late_period: 0,
                    max_hours_per_day: 8,
                },
                Subject {
                    id: subject_b,
                    prefer_early_period: 0,
                    avoid_first_period: 0,
                    avoid_last_period: 0,
                    prefer_late_period: 0,
                    max_hours_per_day: 8,
                },
            ],
            school_classes: vec![SchoolClass {
                id: class,
                home_room_id: None,
                max_lessons_per_day: None,
                class_teacher_id: None,
            }],
            lessons: vec![
                Lesson {
                    id: lesson_a,
                    school_class_ids: vec![class],
                    subject_id: subject_a,
                    teacher_candidates: vec![teacher_a],
                    teacher_pin: Some(teacher_a),
                    hours_per_week: 4,
                    preferred_block_size: 2,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_b,
                    school_class_ids: vec![class],
                    subject_id: subject_b,
                    teacher_candidates: vec![teacher_b],
                    teacher_pin: Some(teacher_b),
                    hours_per_week: 2,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
            ],
            teacher_qualifications: vec![
                TeacherQualification {
                    teacher_id: teacher_a,
                    subject_id: subject_a,
                },
                TeacherQualification {
                    teacher_id: teacher_b,
                    subject_id: subject_b,
                },
            ],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };

        let cfg = SolveConfig {
            seed: 42,
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2000),
            lahc_rr_period: Some(1),
            ..SolveConfig::default()
        };

        let result = crate::solve_with_config(&problem, &cfg);
        assert!(
            result.is_ok(),
            "solve panicked or failed: {:?}",
            result.err()
        );
    }

    /// RED for OPEN_THINGS item 75. Without `Placement.teacher_id` keying,
    /// LAHC R&R's rollback path silently desynchronises `state.used_teacher`
    /// from `placements`: `replay_placement` reads
    /// `lesson_teacher_in_state(state, lesson)`, but the recreate attempt
    /// may have inserted a different teacher into
    /// `state.class_subject_teacher` before the rollback fires, so the
    /// helper returns the recreate's pick instead of the snapshot's
    /// canonical teacher. `state.used_teacher` is inserted with the wrong
    /// key, the running `state.canonical_score` diverges from
    /// `score::score_solution(...)`, and the per-iteration
    /// `debug_assert_eq!` at the LAHC iteration tail (lahc.rs:256) trips
    /// within a few thousand iterations. In release builds the same drift
    /// surfaces later as a `validate_no_double_booking` failure.
    ///
    /// Exercises the canonical zweizuegig fixture with `teacher_pin`
    /// cleared and `teacher_candidates` widened to the dedup'd,
    /// qualified-teachers set (mirrors `solver-bench --teacher-pins off`,
    /// per OPEN_THINGS item 75) under production-active weights.
    ///
    /// Iteration-count-bound (not wall-clock-bound) so the same
    /// (seed, max_iterations) shape REDs deterministically in DEBUG mode
    /// (which CI runs via `cargo nextest run --workspace`) as well as
    /// RELEASE. `lahc_rr_period = 5` raises R&R frequency relative to the
    /// bench default (50) so a fixed iteration cap stays within ~15s of
    /// wall-clock in debug; seed=1 plus `max_iterations=10_000` REDs
    /// inside ~3.3s pre-fix (debug `debug_assert_eq!` at the iteration
    /// tail trips well before iter 10000) and PASSES inside ~14s post-fix
    /// (debug, exhausts the full iteration cap). The generous 60s
    /// `deadline` exists only so debug-mode runtime is never truncated;
    /// the actual stopping criterion is `max_iterations`.
    #[test]
    fn rr_attempt_rollback_does_not_desync_used_teacher_when_classes_share_unpinned_teacher() {
        use std::collections::HashMap as Map;

        let mut problem = crate::test_fixtures::zweizuegig_fixture();
        let mut quals_by_subject: Map<SubjectId, Vec<TeacherId>> = Map::new();
        for q in &problem.teacher_qualifications {
            quals_by_subject
                .entry(q.subject_id)
                .or_default()
                .push(q.teacher_id);
        }
        for v in quals_by_subject.values_mut() {
            v.sort_by_key(|t| t.0);
            v.dedup();
        }
        for lesson in &mut problem.lessons {
            lesson.teacher_pin = None;
            lesson.teacher_candidates = quals_by_subject
                .get(&lesson.subject_id)
                .cloned()
                .unwrap_or_default();
        }

        let cfg = SolveConfig {
            seed: 1,
            deadline: Some(std::time::Duration::from_secs(60)),
            max_iterations: Some(10_000),
            lahc_rr_period: Some(5),
            weights: crate::PRODUCTION_ACTIVE_WEIGHTS,
            ..SolveConfig::default()
        };

        let result = crate::solve_with_config(&problem, &cfg);
        assert!(
            result.is_ok(),
            "LAHC R&R must not produce a double-booked teacher under unpinned candidates; got {:?}",
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
        let room = RoomId(lahc_uuid(30));

        // Item 68: each lesson gets its own SubjectId so the per-(class,
        // subject) teacher-uniformity invariant treats them as independent.
        // Earlier the fixture shared one subject across all `n_lessons`,
        // which collided with the lock map (two lessons sharing a (class,
        // subject) but pinned to different teachers).
        let subjects_v: Vec<Subject> = (0..n_lessons)
            .map(|i| Subject {
                id: SubjectId(lahc_uuid(40 + i)),
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            })
            .collect();

        let teachers_v: Vec<Teacher> = (0..n_lessons)
            .map(|i| Teacher {
                id: TeacherId(lahc_uuid(20 + i)),
                max_hours_per_week: 40,
                reserve_hours_per_week: 0,
            })
            .collect();
        let qualifications: Vec<TeacherQualification> = teachers_v
            .iter()
            .enumerate()
            .map(|(i, t)| TeacherQualification {
                teacher_id: t.id,
                subject_id: subjects_v[i].id,
            })
            .collect();
        let lessons_v: Vec<Lesson> = (0..n_lessons)
            .map(|i| Lesson {
                id: LessonId(lahc_uuid(60 + i)),
                school_class_ids: vec![class],
                subject_id: subjects_v[i as usize].id,
                teacher_candidates: vec![teachers_v[i as usize].id],
                teacher_pin: Some(teachers_v[i as usize].id),
                hours_per_week: 1,
                preferred_block_size: 1,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
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
                    kind: TimeBlockKind::Lesson,
                });
                tb_ids.push(id);
                next += 1;
            }
        }

        let problem = Problem {
            time_blocks: time_blocks_v,
            teachers: teachers_v,
            rooms: vec![Room { id: room }],
            subjects: subjects_v,
            school_classes: vec![SchoolClass {
                id: class,
                home_room_id: None,
                max_lessons_per_day: None,
                class_teacher_id: None,
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

        let teacher0 = problem_for_attempt.lessons[0].assigned_teacher_id();
        let teacher1 = problem_for_attempt.lessons[1].assigned_teacher_id();
        // Item 75: Kempe rollback reads p.teacher_id; seed the rows with
        // the canonical teacher (matches the used_teacher slot below).
        let mut placements = vec![
            Placement {
                lesson_id: lessons[0],
                time_block_id: tb_d0_p0,
                room_id: room,
                teacher_id: teacher0,
            },
            Placement {
                lesson_id: lessons[1],
                time_block_id: tb_d1_p0,
                room_id: room,
                teacher_id: teacher1,
            },
        ];
        let class = problem_for_attempt.lessons[0].school_class_ids[0];
        let subject0 = problem_for_attempt.lessons[0].subject_id;
        let subject1 = problem_for_attempt.lessons[1].subject_id;
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
        state.locked_room.insert((class, 0, subject0), (room, 1));
        state.locked_room.insert((class, 1, subject1), (room, 1));
        // Item 68: seed the per-(class, subject) teacher lock map so
        // Kempe's row-bookkeeping helpers find the right teacher key.
        state
            .class_subject_teacher
            .insert((class, subject0), teacher0);
        state
            .class_subject_teacher
            .insert((class, subject1), teacher1);
        state.search_score_slice = 0;

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
            let home_room_lookup_test: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
            let ok = kempe_attempt(
                &problem_for_attempt,
                &idx,
                &ConstraintWeights::default(),
                &mut rng,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &home_room_lookup_test,
                &tb_by_day_pos,
                &pinned,
                &mut snap_placements,
                &mut snap_state,
                &room_order,
                &max_position_per_day,
                &HashMap::new(),
                &lahc_list,
                0,
                8,
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
            class_subject_teacher: s.class_subject_teacher.clone(),
            search_score_slice: s.search_score_slice,
            canonical_score: s.canonical_score,
            soft_pinned_blocks: s.soft_pinned_blocks.clone(),
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
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lessons[1],
                time_block_id: tb_d1_p0,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lessons[2],
                time_block_id: tb_d0_p1,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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
            &crate::solve::GreedyState::new(),
            lessons[0],
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
            8,
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
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lessons[1],
                time_block_id: tb_d1_p0,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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
            &crate::solve::GreedyState::new(),
            lessons[0],
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
            8,
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
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lessons[1],
                time_block_id: tb_d1_p0,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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
            &crate::solve::GreedyState::new(),
            lessons[0],
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
            8,
        );
        assert!(matches!(outcome, ChainBuild::Aborted));
    }

    #[test]
    fn kempe_chain_aborts_when_two_same_iteration_neighbours_share_class() {
        // Same-iteration hole in the bipartiteness check from item 45's first
        // fix: when the seed pulls in two new neighbours in one popped
        // iteration, both going to the same `neighbour_dest`, the original
        // check walked only `chain` (which is updated at end-of-iteration),
        // so it never saw the two new neighbours as same-color peers. If they
        // share a class, the apply produces a class double-booking that
        // `validate_no_double_booking` catches at production budget.
        //
        // Setup: 3 lessons all in class A, three different teachers. Seed L0
        // has n=2 at (D=0, P=0..=1). L1 (n=1) at (D=1, P=0), L2 (n=1) at
        // (D=1, P=1). Both L1 and L2 conflict with L0 via class A and are
        // added in L0's pop; both go to neighbour_dest = source_day = 0. They
        // share class A with each other, so the chain is non-bipartite and
        // must abort. With the eager-chain-insert fix, the second neighbour
        // sees the first one in `chain` and the bipartiteness check fires.
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class_a = SchoolClassId(lahc_uuid(50));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let t0 = TeacherId(lahc_uuid(20));
        let t1 = TeacherId(lahc_uuid(21));
        let t2 = TeacherId(lahc_uuid(22));
        let l0 = LessonId(lahc_uuid(60));
        let l1 = LessonId(lahc_uuid(61));
        let l2 = LessonId(lahc_uuid(62));
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
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d0_p1,
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d1_p0,
                    day_of_week: 1,
                    position: 0,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d1_p1,
                    day_of_week: 1,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![
                Teacher {
                    id: t0,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: t1,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: t2,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
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
                id: class_a,
                home_room_id: None,
                max_lessons_per_day: None,
                class_teacher_id: None,
            }],
            lessons: vec![
                Lesson {
                    id: l0,
                    school_class_ids: vec![class_a],
                    subject_id: subject,
                    teacher_candidates: vec![t0],
                    teacher_pin: Some(t0),
                    hours_per_week: 2,
                    preferred_block_size: 2,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
                Lesson {
                    id: l1,
                    school_class_ids: vec![class_a],
                    subject_id: subject,
                    teacher_candidates: vec![t1],
                    teacher_pin: Some(t1),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
                Lesson {
                    id: l2,
                    school_class_ids: vec![class_a],
                    subject_id: subject,
                    teacher_candidates: vec![t2],
                    teacher_pin: Some(t2),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
            ],
            teacher_qualifications: vec![
                TeacherQualification {
                    teacher_id: t0,
                    subject_id: subject,
                },
                TeacherQualification {
                    teacher_id: t1,
                    subject_id: subject,
                },
                TeacherQualification {
                    teacher_id: t2,
                    subject_id: subject,
                },
            ],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };

        let placements = vec![
            Placement {
                lesson_id: l0,
                time_block_id: tb_d0_p0,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: l0,
                time_block_id: tb_d0_p1,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: l1,
                time_block_id: tb_d1_p0,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: l2,
                time_block_id: tb_d1_p1,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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
            &crate::solve::GreedyState::new(),
            l0,
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
            8,
        );
        assert!(
            matches!(outcome, ChainBuild::Aborted),
            "chain must abort when two same-iteration neighbours both go to source_day and share class A",
        );
    }

    #[test]
    fn kempe_chain_aborts_on_max_length_bound() {
        // 10 lessons each holding a pair of consecutive classes (lesson i
        // has {C_i, C_(i+1) mod 10}); the daisy-chain via class overlap lets
        // BFS hop alternately between days. Even-id lessons start at
        // (D=0, P=0), odd-id at (D=1, P=0); each hop adds the next neighbour
        // and the chain exceeds the default `config.lahc_kempe_max_chain` of 8.
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
                class_teacher_id: None,
            })
            .collect();
        let teachers_v: Vec<Teacher> = (0..N)
            .map(|i| Teacher {
                id: TeacherId(lahc_uuid(20 + i)),
                max_hours_per_week: 40,
                reserve_hours_per_week: 0,
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
                teacher_candidates: vec![teachers_v[i as usize].id],
                teacher_pin: Some(teachers_v[i as usize].id),
                hours_per_week: 1,
                preferred_block_size: 1,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
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
                kind: TimeBlockKind::Lesson,
            },
            TimeBlock {
                id: tb_d1,
                day_of_week: 1,
                position: 0,
                kind: TimeBlockKind::Lesson,
            },
        ];

        // Place even lessons at (D=0, P=0), odd at (D=1, P=0). With class
        // overlap between consecutive lessons, BFS hops chain alternately
        // between days; length will exceed the default
        // `config.lahc_kempe_max_chain` of 8.
        let placements: Vec<Placement> = (0..N)
            .map(|i| Placement {
                lesson_id: lesson_ids[i as usize],
                time_block_id: if i % 2 == 0 { tb_d0 } else { tb_d1 },
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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
            &crate::solve::GreedyState::new(),
            lesson_ids[0],
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
            8,
        );
        assert!(matches!(outcome, ChainBuild::Aborted));
    }

    #[test]
    fn kempe_chain_capped_at_config_value() {
        // Item 23: end-to-end check that `SolveConfig.lahc_kempe_max_chain`
        // flows from public config into the BFS chain-cap. Use the same
        // 4-lesson same-class fixture as the other integration-style Kempe
        // tests (greedy places all four; LAHC then runs Kempe-only
        // iterations) and run with a non-default `lahc_kempe_max_chain: 2`.
        // Each Kempe attempt either builds a chain of length <= 2 or aborts
        // and rolls back; the run must not crash and the post-condition
        // validators (`validate_no_double_booking` etc.) must pass, which
        // they would not if a chain-cap bypass ever placed two same-day
        // same-class lessons.
        let (problem, _lessons, _tbs, _room) = kempe_one_class_fixture(4, 4);

        let cfg = SolveConfig {
            lahc_kempe_max_chain: 2,
            lahc_kempe_period: Some(1),
            deadline: Some(std::time::Duration::from_millis(100)),
            max_iterations: Some(2_000),
            ..SolveConfig::default()
        };
        let solution = crate::solve_with_config(&problem, &cfg).expect("solve must not panic");
        assert!(
            solution.violations.is_empty(),
            "validators must hold under non-default lahc_kempe_max_chain; got {:?}",
            solution.violations,
        );
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
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d0_p1,
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d1_p0,
                    day_of_week: 1,
                    position: 0,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d1_p1,
                    day_of_week: 1,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![
                Teacher {
                    id: teacher0,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: teacher1,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
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
                class_teacher_id: None,
            }],
            lessons: vec![
                Lesson {
                    id: lesson0,
                    school_class_ids: vec![class],
                    subject_id: subject,
                    teacher_candidates: vec![teacher0],
                    teacher_pin: Some(teacher0),
                    hours_per_week: 2,
                    preferred_block_size: 2,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson1,
                    school_class_ids: vec![class],
                    subject_id: subject,
                    teacher_candidates: vec![teacher1],
                    teacher_pin: Some(teacher1),
                    hours_per_week: 2,
                    preferred_block_size: 2,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
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
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lesson0,
                time_block_id: tb_d0_p1,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lesson1,
                time_block_id: tb_d1_p0,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: lesson1,
                time_block_id: tb_d1_p1,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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
        state.search_score_slice = 0;

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
            let home_room_lookup_test: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
            let ok = kempe_attempt(
                &problem,
                &idx,
                &ConstraintWeights::default(),
                &mut rng,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &home_room_lookup_test,
                &tb_by_day_pos,
                &pinned,
                &mut snap_p,
                &mut snap_s,
                &room_order,
                &max_position_per_day,
                &HashMap::new(),
                &lahc_list,
                0,
                8,
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
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d0_p1,
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d1_p0,
                    day_of_week: 1,
                    position: 0,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d1_p1,
                    day_of_week: 1,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![
                Teacher {
                    id: teacher0,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: teacher1,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: teacher_lock,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
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
                    class_teacher_id: None,
                },
                SchoolClass {
                    id: class_lock,
                    home_room_id: None,
                    max_lessons_per_day: None,
                    class_teacher_id: None,
                },
            ],
            lessons: vec![
                Lesson {
                    id: lesson0,
                    school_class_ids: vec![class_chain],
                    subject_id: subject,
                    teacher_candidates: vec![teacher0],
                    teacher_pin: Some(teacher0),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson1,
                    school_class_ids: vec![class_chain],
                    subject_id: subject,
                    teacher_candidates: vec![teacher1],
                    teacher_pin: Some(teacher1),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_lock_d1,
                    school_class_ids: vec![class_lock],
                    subject_id: subject,
                    teacher_candidates: vec![teacher_lock],
                    teacher_pin: Some(teacher_lock),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_lock_d0,
                    school_class_ids: vec![class_lock],
                    subject_id: subject,
                    teacher_candidates: vec![teacher_lock],
                    teacher_pin: Some(teacher_lock),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
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
        // Item 75: kempe_rollback reads p.teacher_id (the row is the
        // canonical record of which `used_teacher` slot was populated at
        // apply time), so the placeholders must match the teacher_ids
        // seeded into state.used_teacher below.
        let placements_pre = vec![
            Placement {
                lesson_id: lesson0,
                time_block_id: tb_d0_p0,
                room_id: room_a,
                teacher_id: teacher0,
            },
            Placement {
                lesson_id: lesson1,
                time_block_id: tb_d1_p0,
                room_id: room_b,
                teacher_id: teacher1,
            },
            Placement {
                lesson_id: lesson_lock_d1,
                time_block_id: tb_d1_p0,
                room_id: room_a,
                teacher_id: teacher_lock,
            },
            Placement {
                lesson_id: lesson_lock_d0,
                time_block_id: tb_d0_p0,
                room_id: room_b,
                teacher_id: teacher_lock,
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
        state_pre.search_score_slice = 0;

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
            let home_room_lookup_test: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
            let accepted = kempe_attempt(
                &problem,
                &idx,
                &ConstraintWeights::default(),
                &mut rng,
                &lesson_lookup,
                &tb_lookup,
                &subject_lookup,
                &home_room_lookup_test,
                &tb_by_day_pos,
                &pinned,
                &mut p,
                &mut s,
                &room_order,
                &max_position_per_day,
                &HashMap::new(),
                &lahc_list,
                0,
                8,
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
            assert_eq!(s.search_score_slice, state_pre.search_score_slice);
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
                kind: TimeBlockKind::Lesson,
            })
            .collect();
        time_blocks_v.push(TimeBlock {
            id: TimeBlockId(lahc_uuid(200)),
            day_of_week: 1,
            position: 0,
            kind: TimeBlockKind::Lesson,
        });
        let tb_d0_p4 = TimeBlockId(lahc_uuid(104));
        let problem = Problem {
            time_blocks: time_blocks_v,
            teachers: vec![Teacher {
                id: teacher,
                max_hours_per_week: 40,
                reserve_hours_per_week: 0,
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
                class_teacher_id: None,
            }],
            lessons: vec![Lesson {
                id: lesson,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_candidates: vec![teacher],
                teacher_pin: Some(teacher),
                hours_per_week: 1,
                preferred_block_size: 1,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
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
            teacher_id: TeacherId(Uuid::nil()),
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
            &crate::solve::GreedyState::new(),
            lesson,
            0,
            1,
            4,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
            8,
        );
        assert!(matches!(outcome, ChainBuild::Aborted));
    }

    #[test]
    fn kempe_build_chain_uses_lock_map_teacher_for_bfs_conflict_detection() {
        // Regression guard for the latent concern flagged in commit 410f2c9.
        // With solver-driven teacher picks (item 68), the teacher bound by
        // `state.class_subject_teacher` may differ from `assigned_teacher_id()`
        // (which only consults `teacher_pin` then `teacher_candidates[0]`).
        // The Kempe BFS must read the lock-map teacher via
        // `lesson_teacher_in_state` so it does not silently miss conflicts
        // under unpinned LAHC+Kempe runs.
        //
        // Setup: two lessons in two different classes (class_a, class_b)
        // sharing one subject. Neither lesson has a teacher_pin.
        //   L0: candidates [T1, T2], assigned_teacher_id() = T1
        //   L1: candidates [T3, T2], assigned_teacher_id() = T3
        // The lock map binds (class_a, subject) -> T2 AND (class_b, subject)
        // -> T2 so `lesson_teacher_in_state` returns T2 for both lessons.
        // L0 placed at (D=0, P=0); L1 placed at (D=1, P=0). Seed at L0
        // with target dest_day=1 walks the destination window and hits L1.
        //
        // Pre-fix (`assigned_teacher_id()`): T1 != T3, no class overlap,
        // BFS skips L1 and the chain has length 1.
        // Post-fix (`lesson_teacher_in_state`): T2 == T2, teacher conflict
        // detected, BFS pulls L1 into the chain (length 2).
        use crate::types::{
            Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
        };

        let class_a = SchoolClassId(lahc_uuid(50));
        let class_b = SchoolClassId(lahc_uuid(51));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let t1 = TeacherId(lahc_uuid(21));
        let t2 = TeacherId(lahc_uuid(22));
        let t3 = TeacherId(lahc_uuid(23));
        let l0 = LessonId(lahc_uuid(60));
        let l1 = LessonId(lahc_uuid(61));
        let tb_d0_p0 = TimeBlockId(lahc_uuid(100));
        let tb_d1_p0 = TimeBlockId(lahc_uuid(101));

        let problem = Problem {
            time_blocks: vec![
                TimeBlock {
                    id: tb_d0_p0,
                    day_of_week: 0,
                    position: 0,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: tb_d1_p0,
                    day_of_week: 1,
                    position: 0,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![
                Teacher {
                    id: t1,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: t2,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: t3,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
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
                    id: l0,
                    school_class_ids: vec![class_a],
                    subject_id: subject,
                    teacher_candidates: vec![t1, t2],
                    teacher_pin: None,
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
                Lesson {
                    id: l1,
                    school_class_ids: vec![class_b],
                    subject_id: subject,
                    teacher_candidates: vec![t3, t2],
                    teacher_pin: None,
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    pre_buffer_minutes: 0,
                    post_buffer_minutes: 0,
                    lesson_group_id: None,
                },
            ],
            teacher_qualifications: vec![
                TeacherQualification {
                    teacher_id: t1,
                    subject_id: subject,
                },
                TeacherQualification {
                    teacher_id: t2,
                    subject_id: subject,
                },
                TeacherQualification {
                    teacher_id: t3,
                    subject_id: subject,
                },
            ],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };

        let placements = vec![
            Placement {
                lesson_id: l0,
                time_block_id: tb_d0_p0,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: l1,
                time_block_id: tb_d1_p0,
                room_id: room,
                teacher_id: TeacherId(Uuid::nil()),
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

        // Sanity: confirm assigned_teacher_id() really diverges from the
        // lock map teacher we are about to install. Without this, the test
        // could pass for the wrong reason (e.g. if assigned_teacher_id()
        // already returned T2 the pre-fix BFS would already detect the
        // conflict and the post-fix change would be a no-op).
        assert_eq!(problem.lessons[0].assigned_teacher_id(), t1);
        assert_eq!(problem.lessons[1].assigned_teacher_id(), t3);

        // Empty state: BFS uses `assigned_teacher_id()` fallback. T1 != T3
        // and the classes are disjoint, so L1 is not pulled into the chain.
        let empty_state = crate::solve::GreedyState::new();
        let outcome_pre = kempe_build_chain(
            &empty_state,
            l0,
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
            8,
        );
        let chain_pre = match outcome_pre {
            ChainBuild::Built(c) => c,
            ChainBuild::Aborted => panic!("empty-state chain build aborted unexpectedly"),
        };
        assert_eq!(
            chain_pre.len(),
            1,
            "without lock map, BFS sees no teacher conflict (T1 != T3) and chain stays a singleton",
        );
        assert!(!chain_pre.contains_key(&l1));

        // Lock map binds both pairs to T2: BFS now sees T2 == T2 and pulls
        // L1 into the chain via the teacher-conflict branch.
        let mut state_with_lock = crate::solve::GreedyState::new();
        state_with_lock
            .class_subject_teacher
            .insert((class_a, subject), t2);
        state_with_lock
            .class_subject_teacher
            .insert((class_b, subject), t2);
        let outcome_post = kempe_build_chain(
            &state_with_lock,
            l0,
            0,
            1,
            0,
            &placements,
            &lesson_lookup,
            &tb_lookup,
            &tb_by_day_pos,
            &pinned,
            8,
        );
        let chain_post = match outcome_post {
            ChainBuild::Built(c) => c,
            ChainBuild::Aborted => panic!("lock-map chain build aborted unexpectedly"),
        };
        assert_eq!(
            chain_post.len(),
            2,
            "with lock map binding both lessons to T2, BFS detects teacher conflict and adds L1",
        );
        assert_eq!(chain_post[&l0], 1);
        assert_eq!(chain_post[&l1], 0);
    }

    // ---------------------------------------------------------------------
    // RED tests for `try_change_block_move` and `try_swap_move`.
    //
    // These tests cover the move semantics specified in
    // `/tmp/kz-autopilot/2026-05-14-lahc-block-change-swap-moves-design.md`.
    // Task 2 lands them against stub implementations that always return
    // `false`; acceptance-shape tests FAIL here (RED). Rejection-shape tests
    // would pass vacuously against the stub, so they are marked `#[ignore]`
    // and unignored by Task 3 / Task 4 once the real implementations land.
    // ---------------------------------------------------------------------

    use crate::types::{Room, SchoolClass, Subject, Teacher, TeacherQualification};

    /// Build a `Problem` with `n_days * slots_per_day` lesson-kind TBs, a
    /// single subject and pre-populated rooms/teachers/classes. Lesson +
    /// placement seeding is left to the per-test builder.
    fn block_move_problem(
        n_days: u8,
        slots_per_day: u8,
        rooms: Vec<RoomId>,
        teachers: Vec<TeacherId>,
        classes: Vec<SchoolClassId>,
        subject: SubjectId,
        lessons: Vec<Lesson>,
    ) -> Problem {
        let mut time_blocks = Vec::new();
        let mut tb_idx: u32 = 0;
        for d in 0..n_days {
            for p in 0..slots_per_day {
                time_blocks.push(TimeBlock {
                    id: TimeBlockId(lahc_uuid((100 + tb_idx) as u8)),
                    day_of_week: d,
                    position: p,
                    kind: TimeBlockKind::Lesson,
                });
                tb_idx += 1;
            }
        }
        let teacher_qualifications: Vec<TeacherQualification> = teachers
            .iter()
            .map(|t| TeacherQualification {
                teacher_id: *t,
                subject_id: subject,
            })
            .collect();
        Problem {
            time_blocks,
            teachers: teachers
                .iter()
                .map(|t| Teacher {
                    id: *t,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
                })
                .collect(),
            rooms: rooms.iter().map(|r| Room { id: *r }).collect(),
            subjects: vec![Subject {
                id: subject,
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: classes
                .iter()
                .map(|c| SchoolClass {
                    id: *c,
                    home_room_id: None,
                    max_lessons_per_day: None,
                    class_teacher_id: None,
                })
                .collect(),
            lessons,
            teacher_qualifications,
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        }
    }

    /// Look up the TB id at `(day, pos)` in `problem.time_blocks`.
    fn tb_at(problem: &Problem, day: u8, pos: u8) -> TimeBlockId {
        problem
            .time_blocks
            .iter()
            .find(|tb| tb.day_of_week == day && tb.position == pos)
            .expect("tb_at: day/pos not in problem")
            .id
    }

    /// Walk problem.time_blocks for the (day_of_week, position) of the TB
    /// with `tb_id`.
    fn tb_day_pos(problem: &Problem, tb_id: TimeBlockId) -> (u8, u8) {
        let tb = problem
            .time_blocks
            .iter()
            .find(|tb| tb.id == tb_id)
            .expect("tb_day_pos: tb_id not in problem");
        (tb.day_of_week, tb.position)
    }

    /// Bundle returned by `block_move_lookups` so the call site can stay
    /// readable. Mirrors the lookups built at the top of `run`.
    type BlockMoveLookups<'a> = (
        HashMap<LessonId, &'a Lesson>,
        HashMap<TimeBlockId, &'a TimeBlock>,
        HashMap<SubjectId, &'a Subject>,
        HashMap<SchoolClassId, Option<RoomId>>,
        HashMap<u8, u8>,
        HashMap<(u8, u8), TimeBlockId>,
        Vec<usize>,
    );

    /// Build the lookups + auxiliary maps `try_change_block_move` consumes
    /// from a `&Problem`. Mirrors the lookups built at the top of `run`.
    fn block_move_lookups(problem: &Problem) -> BlockMoveLookups<'_> {
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
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
        let tb_by_day_pos: HashMap<(u8, u8), TimeBlockId> = problem
            .time_blocks
            .iter()
            .filter(|tb| tb.kind == TimeBlockKind::Lesson)
            .map(|tb| ((tb.day_of_week, tb.position), tb.id))
            .collect();
        let mut room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        room_order.sort_unstable_by_key(|&i| problem.rooms[i].id.0);
        (
            lesson_lookup,
            tb_lookup,
            subject_lookup,
            home_room_lookup,
            max_position_per_day,
            tb_by_day_pos,
            room_order,
        )
    }

    /// Doppelstunde block at `(day=0, pos=0..2)` with one class and one
    /// teacher and one room; day 1 entirely free. Two-day schedule
    /// (`n_days=2, slots_per_day=3`).
    fn block_change_doppelstunde_fixture() -> (
        Problem,
        Vec<Placement>,
        crate::solve::GreedyState,
        ConstraintWeights,
    ) {
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_id = LessonId(lahc_uuid(60));
        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 2,
            preferred_block_size: 2,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let problem = block_move_problem(
            2,
            3,
            vec![room],
            vec![teacher],
            vec![class],
            subject,
            vec![lesson],
        );
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let tb_d0_p1 = tb_at(&problem, 0, 1);
        let placements = vec![
            Placement {
                lesson_id,
                time_block_id: tb_d0_p0,
                room_id: room,
                teacher_id: teacher,
            },
            Placement {
                lesson_id,
                time_block_id: tb_d0_p1,
                room_id: room,
                teacher_id: teacher,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0, 1]));
        state
            .teacher_positions
            .insert((teacher, 0), vec_part(&[0, 1]));
        state.used_teacher.insert((teacher, tb_d0_p0));
        state.used_teacher.insert((teacher, tb_d0_p1));
        state.used_class.insert((class, tb_d0_p0));
        state.used_class.insert((class, tb_d0_p1));
        state.used_room.insert((room, tb_d0_p0));
        state.used_room.insert((room, tb_d0_p1));
        state.locked_room.insert((class, 0, subject), (room, 2));
        let weights = ConstraintWeights::default();
        (problem, placements, state, weights)
    }

    /// Doppelstunde at `(day=0, pos=0..2)`; day 1 has only `pos=0` and
    /// `pos=1` (no `pos=2`). Used by `rejects_off_day_end`: when an n=2
    /// block anchors at `(day=1, pos=1)` it needs `pos=2` on day=1, which
    /// is missing.
    fn block_change_off_day_end_fixture() -> (
        Problem,
        Vec<Placement>,
        crate::solve::GreedyState,
        ConstraintWeights,
        usize,
    ) {
        // Build a problem where day=1 has only 2 positions (not 3). We
        // can't easily express asymmetric day lengths via
        // `block_move_problem`; build manually.
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_id = LessonId(lahc_uuid(60));
        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 2,
            preferred_block_size: 2,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(lahc_uuid(100)),
                day_of_week: 0,
                position: 0,
                kind: TimeBlockKind::Lesson,
            },
            TimeBlock {
                id: TimeBlockId(lahc_uuid(101)),
                day_of_week: 0,
                position: 1,
                kind: TimeBlockKind::Lesson,
            },
            TimeBlock {
                id: TimeBlockId(lahc_uuid(102)),
                day_of_week: 0,
                position: 2,
                kind: TimeBlockKind::Lesson,
            },
            TimeBlock {
                id: TimeBlockId(lahc_uuid(103)),
                day_of_week: 1,
                position: 0,
                kind: TimeBlockKind::Lesson,
            },
            TimeBlock {
                id: TimeBlockId(lahc_uuid(104)),
                day_of_week: 1,
                position: 1,
                kind: TimeBlockKind::Lesson,
            },
        ];
        let problem = Problem {
            time_blocks: time_blocks.clone(),
            teachers: vec![Teacher {
                id: teacher,
                max_hours_per_week: 40,
                reserve_hours_per_week: 0,
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
                class_teacher_id: None,
            }],
            lessons: vec![lesson],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: teacher,
                subject_id: subject,
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let tb_d0_p0 = time_blocks[0].id;
        let tb_d0_p1 = time_blocks[1].id;
        let placements = vec![
            Placement {
                lesson_id,
                time_block_id: tb_d0_p0,
                room_id: room,
                teacher_id: teacher,
            },
            Placement {
                lesson_id,
                time_block_id: tb_d0_p1,
                room_id: room,
                teacher_id: teacher,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0, 1]));
        state
            .teacher_positions
            .insert((teacher, 0), vec_part(&[0, 1]));
        state.used_teacher.insert((teacher, tb_d0_p0));
        state.used_teacher.insert((teacher, tb_d0_p1));
        state.used_class.insert((class, tb_d0_p0));
        state.used_class.insert((class, tb_d0_p1));
        state.used_room.insert((room, tb_d0_p0));
        state.used_room.insert((room, tb_d0_p1));
        state.locked_room.insert((class, 0, subject), (room, 2));
        // new_tb_idx for the anchor at (day=1, pos=1) is index 4 in
        // `time_blocks`.
        let bad_anchor_idx = 4;
        (
            problem,
            placements,
            state,
            ConstraintWeights::default(),
            bad_anchor_idx,
        )
    }

    /// Doppelstunde at `(day=0, pos=0..2)`; day 1 has a `Break` TB at
    /// `pos=1` so an anchor at `(day=1, pos=0)` would need `pos=1` which
    /// is not lesson-kind. `tb_by_day_pos` filters Break out so the
    /// destination lookup misses.
    fn block_change_break_in_window_fixture() -> (
        Problem,
        Vec<Placement>,
        crate::solve::GreedyState,
        ConstraintWeights,
        usize,
    ) {
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_id = LessonId(lahc_uuid(60));
        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 2,
            preferred_block_size: 2,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(lahc_uuid(100)),
                day_of_week: 0,
                position: 0,
                kind: TimeBlockKind::Lesson,
            },
            TimeBlock {
                id: TimeBlockId(lahc_uuid(101)),
                day_of_week: 0,
                position: 1,
                kind: TimeBlockKind::Lesson,
            },
            TimeBlock {
                id: TimeBlockId(lahc_uuid(102)),
                day_of_week: 0,
                position: 2,
                kind: TimeBlockKind::Lesson,
            },
            TimeBlock {
                id: TimeBlockId(lahc_uuid(103)),
                day_of_week: 1,
                position: 0,
                kind: TimeBlockKind::Lesson,
            },
            // Break slot at (day=1, pos=1).
            TimeBlock {
                id: TimeBlockId(lahc_uuid(104)),
                day_of_week: 1,
                position: 1,
                kind: TimeBlockKind::Break,
            },
            TimeBlock {
                id: TimeBlockId(lahc_uuid(105)),
                day_of_week: 1,
                position: 2,
                kind: TimeBlockKind::Lesson,
            },
        ];
        let problem = Problem {
            time_blocks: time_blocks.clone(),
            teachers: vec![Teacher {
                id: teacher,
                max_hours_per_week: 40,
                reserve_hours_per_week: 0,
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
                class_teacher_id: None,
            }],
            lessons: vec![lesson],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: teacher,
                subject_id: subject,
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let tb_d0_p0 = time_blocks[0].id;
        let tb_d0_p1 = time_blocks[1].id;
        let placements = vec![
            Placement {
                lesson_id,
                time_block_id: tb_d0_p0,
                room_id: room,
                teacher_id: teacher,
            },
            Placement {
                lesson_id,
                time_block_id: tb_d0_p1,
                room_id: room,
                teacher_id: teacher,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0, 1]));
        state
            .teacher_positions
            .insert((teacher, 0), vec_part(&[0, 1]));
        state.used_teacher.insert((teacher, tb_d0_p0));
        state.used_teacher.insert((teacher, tb_d0_p1));
        state.used_class.insert((class, tb_d0_p0));
        state.used_class.insert((class, tb_d0_p1));
        state.used_room.insert((room, tb_d0_p0));
        state.used_room.insert((room, tb_d0_p1));
        state.locked_room.insert((class, 0, subject), (room, 2));
        // Anchor at (day=1, pos=0): the index in time_blocks is 3. The
        // window needs pos=0+pos=1; pos=1 on day=1 is a Break TB.
        let bad_anchor_idx = 3;
        (
            problem,
            placements,
            state,
            ConstraintWeights::default(),
            bad_anchor_idx,
        )
    }

    /// Doppelstunde at `(day=0, pos=0..2)` using `room_a`; on day=1 the
    /// same `room_a` is booked at `(day=1, pos=1)` by an unrelated
    /// placement. An alternate `room_b` is fully free on day=1. The
    /// fallback must walk `room_order` and pick `room_b`.
    fn block_change_alternate_room_fixture() -> (
        Problem,
        Vec<Placement>,
        crate::solve::GreedyState,
        ConstraintWeights,
    ) {
        let class = SchoolClassId(lahc_uuid(50));
        let other_class = SchoolClassId(lahc_uuid(51));
        let teacher = TeacherId(lahc_uuid(20));
        let other_teacher = TeacherId(lahc_uuid(21));
        let subject = SubjectId(lahc_uuid(40));
        let room_a = RoomId(lahc_uuid(30));
        let room_b = RoomId(lahc_uuid(31));
        let lesson_id = LessonId(lahc_uuid(60));
        let other_lesson_id = LessonId(lahc_uuid(61));
        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 2,
            preferred_block_size: 2,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let other_lesson = Lesson {
            id: other_lesson_id,
            school_class_ids: vec![other_class],
            subject_id: subject,
            teacher_candidates: vec![other_teacher],
            teacher_pin: Some(other_teacher),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let problem = block_move_problem(
            2,
            3,
            vec![room_a, room_b],
            vec![teacher, other_teacher],
            vec![class, other_class],
            subject,
            vec![lesson, other_lesson],
        );
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let tb_d0_p1 = tb_at(&problem, 0, 1);
        let tb_d1_p1 = tb_at(&problem, 1, 1);
        let placements = vec![
            Placement {
                lesson_id,
                time_block_id: tb_d0_p0,
                room_id: room_a,
                teacher_id: teacher,
            },
            Placement {
                lesson_id,
                time_block_id: tb_d0_p1,
                room_id: room_a,
                teacher_id: teacher,
            },
            Placement {
                lesson_id: other_lesson_id,
                time_block_id: tb_d1_p1,
                room_id: room_a,
                teacher_id: other_teacher,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0, 1]));
        state
            .teacher_positions
            .insert((teacher, 0), vec_part(&[0, 1]));
        state.used_teacher.insert((teacher, tb_d0_p0));
        state.used_teacher.insert((teacher, tb_d0_p1));
        state.used_class.insert((class, tb_d0_p0));
        state.used_class.insert((class, tb_d0_p1));
        state.used_room.insert((room_a, tb_d0_p0));
        state.used_room.insert((room_a, tb_d0_p1));
        state.locked_room.insert((class, 0, subject), (room_a, 2));
        // Other class's blocking placement on day=1, pos=1, room_a.
        state
            .class_positions
            .insert((other_class, 1), vec_part(&[1]));
        state
            .teacher_positions
            .insert((other_teacher, 1), vec_part(&[1]));
        state.used_teacher.insert((other_teacher, tb_d1_p1));
        state.used_class.insert((other_class, tb_d1_p1));
        state.used_room.insert((room_a, tb_d1_p1));
        state
            .locked_room
            .insert((other_class, 1, subject), (room_a, 1));
        let weights = ConstraintWeights::default();
        (problem, placements, state, weights)
    }

    /// Doppelstunde at `(day=0, pos=0..2)`; day 0 has positions 0,1,2; an
    /// anchor at `(day=0, pos=1)` would lay the block at pos=1+pos=2
    /// (partial overlap with source at pos=1). The subtract-source
    /// overlay must treat pos=1 (source TB) as free for the dest check.
    fn block_change_partial_overlap_fixture() -> (
        Problem,
        Vec<Placement>,
        crate::solve::GreedyState,
        ConstraintWeights,
    ) {
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_id = LessonId(lahc_uuid(60));
        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 2,
            preferred_block_size: 2,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let problem = block_move_problem(
            1,
            3,
            vec![room],
            vec![teacher],
            vec![class],
            subject,
            vec![lesson],
        );
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let tb_d0_p1 = tb_at(&problem, 0, 1);
        let placements = vec![
            Placement {
                lesson_id,
                time_block_id: tb_d0_p0,
                room_id: room,
                teacher_id: teacher,
            },
            Placement {
                lesson_id,
                time_block_id: tb_d0_p1,
                room_id: room,
                teacher_id: teacher,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0, 1]));
        state
            .teacher_positions
            .insert((teacher, 0), vec_part(&[0, 1]));
        state.used_teacher.insert((teacher, tb_d0_p0));
        state.used_teacher.insert((teacher, tb_d0_p1));
        state.used_class.insert((class, tb_d0_p0));
        state.used_class.insert((class, tb_d0_p1));
        state.used_room.insert((room, tb_d0_p0));
        state.used_room.insert((room, tb_d0_p1));
        state.locked_room.insert((class, 0, subject), (room, 2));
        let weights = ConstraintWeights::default();
        (problem, placements, state, weights)
    }

    /// Single-hour lesson at `(day=0, pos=0)` and a free slot at
    /// `(day=0, pos=1)`. n=1 path through `try_change_block_move` must
    /// behave identically to the existing `try_change_move` delta path.
    fn block_change_n1_fixture() -> (
        Problem,
        Vec<Placement>,
        crate::solve::GreedyState,
        ConstraintWeights,
    ) {
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_id = LessonId(lahc_uuid(60));
        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let problem = block_move_problem(
            1,
            3,
            vec![room],
            vec![teacher],
            vec![class],
            subject,
            vec![lesson],
        );
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let placements = vec![Placement {
            lesson_id,
            time_block_id: tb_d0_p0,
            room_id: room,
            teacher_id: teacher,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class, 0), vec_part(&[0]));
        state.teacher_positions.insert((teacher, 0), vec_part(&[0]));
        state.used_teacher.insert((teacher, tb_d0_p0));
        state.used_class.insert((class, tb_d0_p0));
        state.used_room.insert((room, tb_d0_p0));
        state.locked_room.insert((class, 0, subject), (room, 1));
        let weights = ConstraintWeights::default();
        (problem, placements, state, weights)
    }

    /// Two unrelated single-hour lessons: lesson A at `(day=0, pos=0)`,
    /// lesson B at `(day=1, pos=0)`. Different classes, teachers, rooms.
    /// Swapping their TBs is feasible.
    fn swap_two_unrelated_fixture() -> (
        Problem,
        Vec<Placement>,
        crate::solve::GreedyState,
        ConstraintWeights,
    ) {
        let class_a = SchoolClassId(lahc_uuid(50));
        let class_b = SchoolClassId(lahc_uuid(51));
        let teacher_a = TeacherId(lahc_uuid(20));
        let teacher_b = TeacherId(lahc_uuid(21));
        let subject = SubjectId(lahc_uuid(40));
        let room_a = RoomId(lahc_uuid(30));
        let room_b = RoomId(lahc_uuid(31));
        let lesson_a = LessonId(lahc_uuid(60));
        let lesson_b = LessonId(lahc_uuid(61));
        let lesson_a_obj = Lesson {
            id: lesson_a,
            school_class_ids: vec![class_a],
            subject_id: subject,
            teacher_candidates: vec![teacher_a],
            teacher_pin: Some(teacher_a),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let lesson_b_obj = Lesson {
            id: lesson_b,
            school_class_ids: vec![class_b],
            subject_id: subject,
            teacher_candidates: vec![teacher_b],
            teacher_pin: Some(teacher_b),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let problem = block_move_problem(
            2,
            3,
            vec![room_a, room_b],
            vec![teacher_a, teacher_b],
            vec![class_a, class_b],
            subject,
            vec![lesson_a_obj, lesson_b_obj],
        );
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let tb_d1_p0 = tb_at(&problem, 1, 0);
        let placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_d0_p0,
                room_id: room_a,
                teacher_id: teacher_a,
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_d1_p0,
                room_id: room_b,
                teacher_id: teacher_b,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.class_positions.insert((class_a, 0), vec_part(&[0]));
        state.class_positions.insert((class_b, 1), vec_part(&[0]));
        state
            .teacher_positions
            .insert((teacher_a, 0), vec_part(&[0]));
        state
            .teacher_positions
            .insert((teacher_b, 1), vec_part(&[0]));
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_teacher.insert((teacher_b, tb_d1_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_class.insert((class_b, tb_d1_p0));
        state.used_room.insert((room_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d1_p0));
        state.locked_room.insert((class_a, 0, subject), (room_a, 1));
        state.locked_room.insert((class_b, 1, subject), (room_b, 1));
        let weights = ConstraintWeights::default();
        (problem, placements, state, weights)
    }

    // -- try_change_block_move unit tests --

    #[test]
    fn try_change_block_move_moves_doppelstunde_to_free_day() {
        let (problem, mut placements, mut state, weights) = block_change_doppelstunde_fixture();
        let idx = crate::index::Indexed::new(&problem);
        let (
            lesson_lookup,
            tb_lookup,
            subject_lookup,
            home_room_lookup,
            max_position_per_day,
            tb_by_day_pos,
            room_order,
        ) = block_move_lookups(&problem);
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        // Anchor at (day=1, pos=0). new_tb_idx for this is the position in
        // problem.time_blocks.
        let target_tb = tb_at(&problem, 1, 0);
        let new_tb_idx = problem
            .time_blocks
            .iter()
            .position(|tb| tb.id == target_tb)
            .unwrap();
        let accepted = try_change_block_move(
            &problem,
            &idx,
            0,
            new_tb_idx,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &max_position_per_day,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
            &tb_by_day_pos,
            &room_order,
        );
        assert!(accepted, "block-change to free day must accept");
        let lesson_id = problem.lessons[0].id;
        let block_placements: Vec<&Placement> = placements
            .iter()
            .filter(|p| p.lesson_id == lesson_id)
            .collect();
        assert_eq!(block_placements.len(), 2);
        let positions: Vec<(u8, u8)> = block_placements
            .iter()
            .map(|p| tb_day_pos(&problem, p.time_block_id))
            .collect();
        for (day, _) in &positions {
            assert_eq!(*day, 1, "block must land entirely on day 1");
        }
        let mut pos_only: Vec<u8> = positions.iter().map(|(_, p)| *p).collect();
        pos_only.sort_unstable();
        assert_eq!(pos_only, vec![0, 1]);
    }

    #[test]
    fn try_change_block_move_rejects_off_day_end() {
        let (problem, mut placements, mut state, weights, bad_anchor_idx) =
            block_change_off_day_end_fixture();
        let idx = crate::index::Indexed::new(&problem);
        let (
            lesson_lookup,
            tb_lookup,
            subject_lookup,
            home_room_lookup,
            max_position_per_day,
            tb_by_day_pos,
            room_order,
        ) = block_move_lookups(&problem);
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let placements_before = placements.clone();
        let canonical_before = state.canonical_score;
        let accepted = try_change_block_move(
            &problem,
            &idx,
            0,
            bad_anchor_idx,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &max_position_per_day,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
            &tb_by_day_pos,
            &room_order,
        );
        assert!(!accepted, "off-day-end block move must reject");
        assert_eq!(placements, placements_before);
        assert_eq!(state.canonical_score, canonical_before);
    }

    #[test]
    fn try_change_block_move_rejects_break_in_window() {
        let (problem, mut placements, mut state, weights, bad_anchor_idx) =
            block_change_break_in_window_fixture();
        let idx = crate::index::Indexed::new(&problem);
        let (
            lesson_lookup,
            tb_lookup,
            subject_lookup,
            home_room_lookup,
            max_position_per_day,
            tb_by_day_pos,
            room_order,
        ) = block_move_lookups(&problem);
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let placements_before = placements.clone();
        let canonical_before = state.canonical_score;
        let accepted = try_change_block_move(
            &problem,
            &idx,
            0,
            bad_anchor_idx,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &max_position_per_day,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
            &tb_by_day_pos,
            &room_order,
        );
        assert!(!accepted, "break-in-window block move must reject");
        assert_eq!(placements, placements_before);
        assert_eq!(state.canonical_score, canonical_before);
    }

    #[test]
    fn try_change_block_move_falls_back_to_alternate_room() {
        let (problem, mut placements, mut state, weights) = block_change_alternate_room_fixture();
        let idx = crate::index::Indexed::new(&problem);
        let (
            lesson_lookup,
            tb_lookup,
            subject_lookup,
            home_room_lookup,
            max_position_per_day,
            tb_by_day_pos,
            room_order,
        ) = block_move_lookups(&problem);
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        // Anchor at (day=1, pos=0): block lays at d1p0+d1p1. room_a is
        // booked at d1p1 by the other_lesson, so the room_a path is
        // infeasible; room_b must be chosen.
        let target_tb = tb_at(&problem, 1, 0);
        let new_tb_idx = problem
            .time_blocks
            .iter()
            .position(|tb| tb.id == target_tb)
            .unwrap();
        let accepted = try_change_block_move(
            &problem,
            &idx,
            0,
            new_tb_idx,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &max_position_per_day,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
            &tb_by_day_pos,
            &room_order,
        );
        assert!(accepted, "block-change must accept with room fallback");
        let lesson_id = problem.lessons[0].id;
        let block_placements: Vec<&Placement> = placements
            .iter()
            .filter(|p| p.lesson_id == lesson_id)
            .collect();
        assert_eq!(block_placements.len(), 2);
        // Both placements must share the fallback room (room_b).
        let room_b = RoomId(lahc_uuid(31));
        for p in &block_placements {
            assert_eq!(p.room_id, room_b, "fallback room must be room_b");
        }
    }

    #[test]
    fn try_change_block_move_same_window_rejected() {
        let (problem, mut placements, mut state, weights) = block_change_doppelstunde_fixture();
        let idx = crate::index::Indexed::new(&problem);
        let (
            lesson_lookup,
            tb_lookup,
            subject_lookup,
            home_room_lookup,
            max_position_per_day,
            tb_by_day_pos,
            room_order,
        ) = block_move_lookups(&problem);
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        // Anchor at the source window itself: (day=0, pos=0).
        let target_tb = tb_at(&problem, 0, 0);
        let new_tb_idx = problem
            .time_blocks
            .iter()
            .position(|tb| tb.id == target_tb)
            .unwrap();
        let placements_before = placements.clone();
        let canonical_before = state.canonical_score;
        let accepted = try_change_block_move(
            &problem,
            &idx,
            0,
            new_tb_idx,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &max_position_per_day,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
            &tb_by_day_pos,
            &room_order,
        );
        assert!(!accepted, "same-window block move must reject as no-op");
        assert_eq!(placements, placements_before);
        assert_eq!(state.canonical_score, canonical_before);
    }

    #[test]
    fn try_change_block_move_partial_overlap_accepted() {
        let (problem, mut placements, mut state, weights) = block_change_partial_overlap_fixture();
        let idx = crate::index::Indexed::new(&problem);
        let (
            lesson_lookup,
            tb_lookup,
            subject_lookup,
            home_room_lookup,
            max_position_per_day,
            tb_by_day_pos,
            room_order,
        ) = block_move_lookups(&problem);
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        // Anchor at (day=0, pos=1): source d0p0+d0p1, dest d0p1+d0p2.
        // Overlap at pos=1; subtract-source overlay must allow this.
        let target_tb = tb_at(&problem, 0, 1);
        let new_tb_idx = problem
            .time_blocks
            .iter()
            .position(|tb| tb.id == target_tb)
            .unwrap();
        let accepted = try_change_block_move(
            &problem,
            &idx,
            0,
            new_tb_idx,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &max_position_per_day,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
            &tb_by_day_pos,
            &room_order,
        );
        assert!(accepted, "partial-overlap block move must accept");
        let lesson_id = problem.lessons[0].id;
        let block_placements: Vec<&Placement> = placements
            .iter()
            .filter(|p| p.lesson_id == lesson_id)
            .collect();
        let mut positions: Vec<u8> = block_placements
            .iter()
            .map(|p| tb_day_pos(&problem, p.time_block_id).1)
            .collect();
        positions.sort_unstable();
        assert_eq!(positions, vec![1, 2], "block must lay at pos=1+pos=2");
    }

    #[test]
    fn try_change_block_move_n_equals_1_matches_existing_change() {
        // The n=1 branch must delegate to the existing delta-score path.
        // After accepting a move from (day=0, pos=0) to (day=0, pos=1):
        // - placement is at the new TB
        // - state.canonical_score and state.search_score_slice are
        //   consistent with the existing delta path's writes.
        let (problem, mut placements, mut state, weights) = block_change_n1_fixture();
        let idx = crate::index::Indexed::new(&problem);
        let (
            lesson_lookup,
            tb_lookup,
            subject_lookup,
            home_room_lookup,
            max_position_per_day,
            tb_by_day_pos,
            room_order,
        ) = block_move_lookups(&problem);
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let target_tb = tb_at(&problem, 0, 1);
        let new_tb_idx = problem
            .time_blocks
            .iter()
            .position(|tb| tb.id == target_tb)
            .unwrap();
        let accepted = try_change_block_move(
            &problem,
            &idx,
            0,
            new_tb_idx,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &max_position_per_day,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
            &tb_by_day_pos,
            &room_order,
        );
        assert!(
            accepted,
            "n=1 block-change must accept (delegates to delta path)"
        );
        assert_eq!(placements[0].time_block_id, target_tb);
    }

    // -- try_swap_move unit tests --

    #[test]
    fn try_swap_move_swaps_two_unrelated_cells() {
        let (problem, mut placements, mut state, weights) = swap_two_unrelated_fixture();
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let tb_a = placements[0].time_block_id;
        let tb_b = placements[1].time_block_id;
        let accepted = try_swap_move(
            &problem,
            &idx,
            0,
            1,
            &lesson_lookup,
            &tb_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
        );
        assert!(accepted, "two-unrelated-cell swap must accept");
        assert_eq!(placements[0].time_block_id, tb_b);
        assert_eq!(placements[1].time_block_id, tb_a);
    }

    #[test]
    fn try_swap_move_rejects_same_lesson() {
        // Two placements that share the same lesson_id (e.g., a
        // hours_per_week=2 single-block-size lesson with two placements).
        let class = SchoolClassId(lahc_uuid(50));
        let teacher = TeacherId(lahc_uuid(20));
        let subject = SubjectId(lahc_uuid(40));
        let room = RoomId(lahc_uuid(30));
        let lesson_id = LessonId(lahc_uuid(60));
        let lesson = Lesson {
            id: lesson_id,
            school_class_ids: vec![class],
            subject_id: subject,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 2,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let problem = block_move_problem(
            2,
            3,
            vec![room],
            vec![teacher],
            vec![class],
            subject,
            vec![lesson],
        );
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let tb_d1_p0 = tb_at(&problem, 1, 0);
        let mut placements = vec![
            Placement {
                lesson_id,
                time_block_id: tb_d0_p0,
                room_id: room,
                teacher_id: teacher,
            },
            Placement {
                lesson_id,
                time_block_id: tb_d1_p0,
                room_id: room,
                teacher_id: teacher,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher, tb_d0_p0));
        state.used_teacher.insert((teacher, tb_d1_p0));
        state.used_class.insert((class, tb_d0_p0));
        state.used_class.insert((class, tb_d1_p0));
        state.used_room.insert((room, tb_d0_p0));
        state.used_room.insert((room, tb_d1_p0));
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let weights = ConstraintWeights::default();
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let placements_before = placements.clone();
        let accepted = try_swap_move(
            &problem,
            &idx,
            0,
            1,
            &lesson_lookup,
            &tb_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
        );
        assert!(!accepted, "same-lesson swap must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_swap_move_rejects_pinned_partner() {
        let (problem, mut placements, mut state, weights) = swap_two_unrelated_fixture();
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        // Pin lesson A.
        let mut pinned: HashSet<LessonId> = HashSet::new();
        pinned.insert(problem.lessons[0].id);
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let placements_before = placements.clone();
        let accepted = try_swap_move(
            &problem,
            &idx,
            0,
            1,
            &lesson_lookup,
            &tb_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
        );
        assert!(!accepted, "pinned-partner swap must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_swap_move_rejects_group_partner() {
        // Lesson A is a member of a lesson group; swap must reject.
        use crate::ids::LessonGroupId;
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
        let lesson_a_obj = Lesson {
            id: lesson_a,
            school_class_ids: vec![class_a],
            subject_id: subject,
            teacher_candidates: vec![teacher_a],
            teacher_pin: Some(teacher_a),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: Some(group_id),
        };
        let lesson_b_obj = Lesson {
            id: lesson_b,
            school_class_ids: vec![class_b],
            subject_id: subject,
            teacher_candidates: vec![teacher_b],
            teacher_pin: Some(teacher_b),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let problem = block_move_problem(
            2,
            3,
            vec![room_a, room_b],
            vec![teacher_a, teacher_b],
            vec![class_a, class_b],
            subject,
            vec![lesson_a_obj, lesson_b_obj],
        );
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let tb_d1_p0 = tb_at(&problem, 1, 0);
        let mut placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_d0_p0,
                room_id: room_a,
                teacher_id: teacher_a,
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_d1_p0,
                room_id: room_b,
                teacher_id: teacher_b,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_teacher.insert((teacher_b, tb_d1_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_class.insert((class_b, tb_d1_p0));
        state.used_room.insert((room_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d1_p0));
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let weights = ConstraintWeights::default();
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let placements_before = placements.clone();
        let accepted = try_swap_move(
            &problem,
            &idx,
            0,
            1,
            &lesson_lookup,
            &tb_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
        );
        assert!(!accepted, "group-member swap must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_swap_move_rejects_class_double_book() {
        // Lesson A at (d0p0, class_a, room_a); lesson B at (d1p0, class_a,
        // room_b) (same class!); a third placement C at (d1p0, class_a)
        // already books class_a for the destination TB of A. Post-swap, A
        // would land at d1p0 alongside C, both in class_a -> reject.
        //
        // Build manually because we need a third unrelated placement.
        let class_a = SchoolClassId(lahc_uuid(50));
        let class_c = SchoolClassId(lahc_uuid(52));
        let teacher_a = TeacherId(lahc_uuid(20));
        let teacher_b = TeacherId(lahc_uuid(21));
        let teacher_c = TeacherId(lahc_uuid(22));
        let subject = SubjectId(lahc_uuid(40));
        let room_a = RoomId(lahc_uuid(30));
        let room_b = RoomId(lahc_uuid(31));
        let room_c = RoomId(lahc_uuid(32));
        let lesson_a = LessonId(lahc_uuid(60));
        let lesson_b = LessonId(lahc_uuid(61));
        let lesson_c = LessonId(lahc_uuid(62));
        let lesson_a_obj = Lesson {
            id: lesson_a,
            school_class_ids: vec![class_a],
            subject_id: subject,
            teacher_candidates: vec![teacher_a],
            teacher_pin: Some(teacher_a),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let lesson_b_obj = Lesson {
            id: lesson_b,
            school_class_ids: vec![class_c],
            subject_id: subject,
            teacher_candidates: vec![teacher_b],
            teacher_pin: Some(teacher_b),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        // Lesson C: class_a, day=1, pos=0. Conflicts with A's swap dest.
        let lesson_c_obj = Lesson {
            id: lesson_c,
            school_class_ids: vec![class_a],
            subject_id: subject,
            teacher_candidates: vec![teacher_c],
            teacher_pin: Some(teacher_c),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let problem = block_move_problem(
            2,
            3,
            vec![room_a, room_b, room_c],
            vec![teacher_a, teacher_b, teacher_c],
            vec![class_a, class_c],
            subject,
            vec![lesson_a_obj, lesson_b_obj, lesson_c_obj],
        );
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let tb_d1_p0 = tb_at(&problem, 1, 0);
        let tb_d1_p1 = tb_at(&problem, 1, 1);
        let mut placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_d0_p0,
                room_id: room_a,
                teacher_id: teacher_a,
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_d1_p0,
                room_id: room_b,
                teacher_id: teacher_b,
            },
            // The blocking class_a placement at tb_d1_p0 would force
            // double-booking, but we instead place at tb_d1_p1 to keep
            // pre-swap state legal; lesson C is at the swap destination
            // for A (tb_d1_p0). Actually wait: we need C at tb_d1_p0,
            // not tb_d1_p1. Let's reposition.
            Placement {
                lesson_id: lesson_c,
                time_block_id: tb_d1_p1,
                room_id: room_c,
                teacher_id: teacher_c,
            },
        ];
        // Pre-state: B is at d1p0 (with class_c). If we swap A and B, A
        // lands at d1p0; class_a has no other placement there until we
        // move C. To force a double-book, put another placement of
        // class_a at d1p0 via lesson_c. We can't do that here without
        // breaking pre-state legality. Instead, the cleanest construction
        // is: leave placements as above, but seed state such that
        // (class_a, tb_d1_p0) is already used by yet another row that
        // shares no row in `placements`. That's awkward.
        //
        // Simpler: keep placements as above and seed state.used_class to
        // include (class_a, tb_d1_p0) via a phantom row. The real check
        // only consults state.used_class; the discrepancy is fine for a
        // RED test against the stub.
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_teacher.insert((teacher_b, tb_d1_p0));
        state.used_teacher.insert((teacher_c, tb_d1_p1));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_class.insert((class_c, tb_d1_p0));
        state.used_class.insert((class_a, tb_d1_p1));
        // Phantom: simulate a third lesson putting class_a at tb_d1_p0
        // post-swap.
        state.used_class.insert((class_a, tb_d1_p0));
        state.used_room.insert((room_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d1_p0));
        state.used_room.insert((room_c, tb_d1_p1));
        let _ = tb_d1_p1;
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let weights = ConstraintWeights::default();
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let placements_before = placements.clone();
        let accepted = try_swap_move(
            &problem,
            &idx,
            0,
            1,
            &lesson_lookup,
            &tb_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
        );
        assert!(!accepted, "class double-book swap must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_swap_move_rejects_daily_cap_breach() {
        // Lesson A at (d0p0); lesson B at (d1p0). class_a has
        // max_lessons_per_day=1 AND already has another lesson at
        // (d1p1), so the swap would push d1's count to 2 for class_a.
        let class_a = SchoolClassId(lahc_uuid(50));
        let class_b = SchoolClassId(lahc_uuid(51));
        let teacher_a = TeacherId(lahc_uuid(20));
        let teacher_b = TeacherId(lahc_uuid(21));
        let teacher_c = TeacherId(lahc_uuid(22));
        let subject = SubjectId(lahc_uuid(40));
        let room_a = RoomId(lahc_uuid(30));
        let room_b = RoomId(lahc_uuid(31));
        let room_c = RoomId(lahc_uuid(32));
        let lesson_a = LessonId(lahc_uuid(60));
        let lesson_b = LessonId(lahc_uuid(61));
        let lesson_c = LessonId(lahc_uuid(62));
        let lesson_a_obj = Lesson {
            id: lesson_a,
            school_class_ids: vec![class_a],
            subject_id: subject,
            teacher_candidates: vec![teacher_a],
            teacher_pin: Some(teacher_a),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let lesson_b_obj = Lesson {
            id: lesson_b,
            school_class_ids: vec![class_b],
            subject_id: subject,
            teacher_candidates: vec![teacher_b],
            teacher_pin: Some(teacher_b),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let lesson_c_obj = Lesson {
            id: lesson_c,
            school_class_ids: vec![class_a],
            subject_id: subject,
            teacher_candidates: vec![teacher_c],
            teacher_pin: Some(teacher_c),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let mut problem = block_move_problem(
            2,
            3,
            vec![room_a, room_b, room_c],
            vec![teacher_a, teacher_b, teacher_c],
            vec![class_a, class_b],
            subject,
            vec![lesson_a_obj, lesson_b_obj, lesson_c_obj],
        );
        // Cap class_a to 1 lesson per day.
        problem.school_classes[0].max_lessons_per_day = Some(1);
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let tb_d1_p0 = tb_at(&problem, 1, 0);
        let tb_d1_p1 = tb_at(&problem, 1, 1);
        let mut placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_d0_p0,
                room_id: room_a,
                teacher_id: teacher_a,
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_d1_p0,
                room_id: room_b,
                teacher_id: teacher_b,
            },
            Placement {
                lesson_id: lesson_c,
                time_block_id: tb_d1_p1,
                room_id: room_c,
                teacher_id: teacher_c,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_teacher.insert((teacher_b, tb_d1_p0));
        state.used_teacher.insert((teacher_c, tb_d1_p1));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_class.insert((class_b, tb_d1_p0));
        state.used_class.insert((class_a, tb_d1_p1));
        state.used_room.insert((room_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d1_p0));
        state.used_room.insert((room_c, tb_d1_p1));
        state.lessons_by_class_day.insert((class_a, 0), 1);
        state.lessons_by_class_day.insert((class_b, 1), 1);
        state.lessons_by_class_day.insert((class_a, 1), 1);
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let weights = ConstraintWeights::default();
        let lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
        let pinned: HashSet<LessonId> = HashSet::new();
        let mut class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        class_max_lessons_per_day.insert(class_a, 1);
        let placements_before = placements.clone();
        let accepted = try_swap_move(
            &problem,
            &idx,
            0,
            1,
            &lesson_lookup,
            &tb_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &class_max_lessons_per_day,
            &lahc_list,
            0,
        );
        assert!(!accepted, "daily-cap-breach swap must reject");
        assert_eq!(placements, placements_before);
    }

    // -- try_home_room_repair_move unit tests --

    /// Build a Problem with two classes, two rooms, one TB-grid (2 days x 3
    /// positions), one subject, two lessons. Class A's home_room is `room_a`;
    /// class B's home_room is `room_b`. Each lesson has 1 hour, block_size 1,
    /// no group. Room suitabilities are wide-open (every room suits the
    /// subject). The caller passes the home_room for each class via the
    /// `home_room_a` / `home_room_b` args (use `Some(...)` or `None`).
    #[allow(clippy::type_complexity)]
    fn home_room_problem(
        home_room_a: Option<RoomId>,
        home_room_b: Option<RoomId>,
    ) -> (
        Problem,
        SchoolClassId,
        SchoolClassId,
        TeacherId,
        TeacherId,
        SubjectId,
        RoomId,
        RoomId,
        LessonId,
        LessonId,
    ) {
        use crate::types::{Room, SchoolClass, Subject, Teacher, TeacherQualification};
        let class_a = SchoolClassId(lahc_uuid(50));
        let class_b = SchoolClassId(lahc_uuid(51));
        let teacher_a = TeacherId(lahc_uuid(20));
        let teacher_b = TeacherId(lahc_uuid(21));
        let subject = SubjectId(lahc_uuid(40));
        let room_a = RoomId(lahc_uuid(30));
        let room_b = RoomId(lahc_uuid(31));
        let lesson_a = LessonId(lahc_uuid(60));
        let lesson_b = LessonId(lahc_uuid(61));
        let lesson_a_obj = Lesson {
            id: lesson_a,
            school_class_ids: vec![class_a],
            subject_id: subject,
            teacher_candidates: vec![teacher_a],
            teacher_pin: Some(teacher_a),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        let lesson_b_obj = Lesson {
            id: lesson_b,
            school_class_ids: vec![class_b],
            subject_id: subject,
            teacher_candidates: vec![teacher_b],
            teacher_pin: Some(teacher_b),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        // Build TBs manually so we get a 2x3 grid.
        let mut time_blocks = Vec::new();
        let mut tb_idx: u32 = 0;
        for d in 0..2u8 {
            for p in 0..3u8 {
                time_blocks.push(TimeBlock {
                    id: TimeBlockId(lahc_uuid((100 + tb_idx) as u8)),
                    day_of_week: d,
                    position: p,
                    kind: TimeBlockKind::Lesson,
                });
                tb_idx += 1;
            }
        }
        let problem = Problem {
            time_blocks,
            teachers: vec![
                Teacher {
                    id: teacher_a,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: teacher_b,
                    max_hours_per_week: 40,
                    reserve_hours_per_week: 0,
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
                    id: class_a,
                    home_room_id: home_room_a,
                    max_lessons_per_day: None,
                    class_teacher_id: None,
                },
                SchoolClass {
                    id: class_b,
                    home_room_id: home_room_b,
                    max_lessons_per_day: None,
                    class_teacher_id: None,
                },
            ],
            lessons: vec![lesson_a_obj, lesson_b_obj],
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
        (
            problem, class_a, class_b, teacher_a, teacher_b, subject, room_a, room_b, lesson_a,
            lesson_b,
        )
    }

    /// Permissive lahc_list: any canonical delta accepts.
    fn permissive_lahc_list() -> Vec<u32> {
        vec![u32::MAX; LAHC_LIST_LEN]
    }

    #[test]
    fn try_home_room_repair_move_accepts_room_free_path() {
        // Class A has home_room = room_a. Place lesson A in room_b at d0p0,
        // while room_a is free at d0p0 -> move should land lesson A in room_a.
        let (
            problem,
            class_a,
            _class_b,
            teacher_a,
            _teacher_b,
            subject,
            room_a,
            room_b,
            lesson_a,
            _lesson_b,
        ) = home_room_problem(Some(/* room_a placeholder */ RoomId(lahc_uuid(30))), None);
        let _ = subject;
        let _ = class_a;
        let _ = teacher_a;
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let mut placements = vec![Placement {
            lesson_id: lesson_a,
            time_block_id: tb_d0_p0,
            room_id: room_b,
            teacher_id: teacher_a,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        // canonical pre = 5 (1 home_room miss * 5).
        state.canonical_score = 5;
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();
        let lahc_list = permissive_lahc_list();
        let mut room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        room_order.sort_unstable_by_key(|&i| problem.rooms[i].id.0);
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(accepted, "room-free home_room repair must accept");
        assert_eq!(placements[0].room_id, room_a);
        assert!(state.used_room.contains(&(room_a, tb_d0_p0)));
        assert!(!state.used_room.contains(&(room_b, tb_d0_p0)));
    }

    #[test]
    fn try_home_room_repair_move_accepts_room_occupied_single_collision_swap() {
        // P: lesson_a in room_b @ d0p0; Q: lesson_b in room_a @ d0p0. Same TB.
        // class_a's home_room is room_a; class_b's home_room is None.
        // Subject is suitable in both rooms. Expect swap: P -> room_a, Q -> room_b.
        let (
            problem,
            class_a,
            class_b,
            teacher_a,
            teacher_b,
            _subject,
            room_a,
            room_b,
            lesson_a,
            lesson_b,
        ) = home_room_problem(Some(RoomId(lahc_uuid(30))), None);
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let mut placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_d0_p0,
                room_id: room_b,
                teacher_id: teacher_a,
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_d0_p0,
                room_id: room_a,
                teacher_id: teacher_b,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_teacher.insert((teacher_b, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_class.insert((class_b, tb_d0_p0));
        state.used_room.insert((room_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        // Pre: lesson_a in non-home (room_b) -> 5; lesson_b's class has no
        // home_room so its contribution is 0. canonical_score = 5.
        state.canonical_score = 5;
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();
        let lahc_list = permissive_lahc_list();
        let room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(accepted, "single-collision swap must accept");
        assert_eq!(placements[0].room_id, room_a);
        assert_eq!(placements[1].room_id, room_b);
    }

    #[test]
    fn try_home_room_repair_move_rejects_when_already_in_home_room() {
        // Lesson A already in room_a (which is home_room). No-op reject.
        let (
            problem,
            class_a,
            _class_b,
            teacher_a,
            _teacher_b,
            _subject,
            room_a,
            _room_b,
            lesson_a,
            _lesson_b,
        ) = home_room_problem(Some(RoomId(lahc_uuid(30))), None);
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let mut placements = vec![Placement {
            lesson_id: lesson_a,
            time_block_id: tb_d0_p0,
            room_id: room_a,
            teacher_id: teacher_a,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_room.insert((room_a, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();
        let lahc_list = permissive_lahc_list();
        let room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        let placements_before = placements.clone();
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(!accepted, "no-op repair must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_home_room_repair_move_rejects_blocked_room() {
        // home_room (room_a) is in room_blocked_times at d0p0.
        use crate::types::RoomBlockedTime;
        let (
            mut problem,
            class_a,
            _class_b,
            teacher_a,
            _teacher_b,
            _subject,
            room_a,
            room_b,
            lesson_a,
            _lesson_b,
        ) = home_room_problem(Some(RoomId(lahc_uuid(30))), None);
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        problem.room_blocked_times.push(RoomBlockedTime {
            room_id: room_a,
            time_block_id: tb_d0_p0,
        });
        let mut placements = vec![Placement {
            lesson_id: lesson_a,
            time_block_id: tb_d0_p0,
            room_id: room_b,
            teacher_id: teacher_a,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();
        let lahc_list = permissive_lahc_list();
        let room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        let placements_before = placements.clone();
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(!accepted, "blocked home_room must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_home_room_repair_move_rejects_grouped_lesson() {
        // Lesson A's lesson_group_id is Some -> reject.
        let (
            mut problem,
            class_a,
            _class_b,
            teacher_a,
            _teacher_b,
            _subject,
            room_a,
            room_b,
            lesson_a,
            _lesson_b,
        ) = home_room_problem(Some(RoomId(lahc_uuid(30))), None);
        let group_id = LessonGroupId(lahc_uuid(70));
        problem.lessons[0].lesson_group_id = Some(group_id);
        let _ = room_a;
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let mut placements = vec![Placement {
            lesson_id: lesson_a,
            time_block_id: tb_d0_p0,
            room_id: room_b,
            teacher_id: teacher_a,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();
        let lahc_list = permissive_lahc_list();
        let room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        let placements_before = placements.clone();
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(!accepted, "grouped-lesson repair must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_home_room_repair_move_rejects_pinned_lesson() {
        // lesson_a is in pinned set -> reject.
        let (
            problem,
            class_a,
            _class_b,
            teacher_a,
            _teacher_b,
            _subject,
            _room_a,
            room_b,
            lesson_a,
            _lesson_b,
        ) = home_room_problem(Some(RoomId(lahc_uuid(30))), None);
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let mut placements = vec![Placement {
            lesson_id: lesson_a,
            time_block_id: tb_d0_p0,
            room_id: room_b,
            teacher_id: teacher_a,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let mut pinned: HashSet<LessonId> = HashSet::new();
        pinned.insert(lesson_a);
        let lahc_list = permissive_lahc_list();
        let room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        let placements_before = placements.clone();
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(!accepted, "pinned-lesson repair must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_home_room_repair_move_rejects_no_home_room() {
        // class_a has no home_room (None) -> reject.
        let (
            problem,
            class_a,
            _class_b,
            teacher_a,
            _teacher_b,
            _subject,
            _room_a,
            room_b,
            lesson_a,
            _lesson_b,
        ) = home_room_problem(None, None);
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let mut placements = vec![Placement {
            lesson_id: lesson_a,
            time_block_id: tb_d0_p0,
            room_id: room_b,
            teacher_id: teacher_a,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();
        let lahc_list = permissive_lahc_list();
        let room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        let placements_before = placements.clone();
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(!accepted, "no-home-room repair must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_home_room_repair_move_rejects_block_size_gt_1() {
        // lesson_a.preferred_block_size = 2 -> reject.
        let (
            mut problem,
            class_a,
            _class_b,
            teacher_a,
            _teacher_b,
            _subject,
            _room_a,
            room_b,
            lesson_a,
            _lesson_b,
        ) = home_room_problem(Some(RoomId(lahc_uuid(30))), None);
        problem.lessons[0].preferred_block_size = 2;
        problem.lessons[0].hours_per_week = 2;
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let mut placements = vec![Placement {
            lesson_id: lesson_a,
            time_block_id: tb_d0_p0,
            room_id: room_b,
            teacher_id: teacher_a,
        }];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();
        let lahc_list = permissive_lahc_list();
        let room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        let placements_before = placements.clone();
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(!accepted, "block_size>1 repair must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_home_room_repair_move_rejects_subject_unsuitable_in_collision_swap() {
        // (B) path: Q (lesson_b) has a subject that is not suitable in P's
        // old room (room_b). Concretely: a second subject for Q, with the
        // room suitability table marking that subject as suitable only in
        // room_a (Q's current room, the home_room). Swap proposal would put
        // Q in room_b, which is not suitable.
        use crate::types::{RoomSubjectSuitability, Subject};
        let (
            mut problem,
            class_a,
            class_b,
            teacher_a,
            teacher_b,
            subject_main,
            room_a,
            room_b,
            lesson_a,
            lesson_b,
        ) = home_room_problem(Some(RoomId(lahc_uuid(30))), None);
        // Add a second subject only suitable in room_a.
        let subject_locked = SubjectId(lahc_uuid(41));
        problem.subjects.push(Subject {
            id: subject_locked,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        });
        // room_a entry: suits both subjects; room_b entry: suits only subject_main.
        problem
            .room_subject_suitabilities
            .push(RoomSubjectSuitability {
                room_id: room_a,
                subject_id: subject_main,
            });
        problem
            .room_subject_suitabilities
            .push(RoomSubjectSuitability {
                room_id: room_a,
                subject_id: subject_locked,
            });
        problem
            .room_subject_suitabilities
            .push(RoomSubjectSuitability {
                room_id: room_b,
                subject_id: subject_main,
            });
        // Repoint lesson_b to subject_locked + add qualification for teacher_b.
        problem.lessons[1].subject_id = subject_locked;
        problem
            .teacher_qualifications
            .push(crate::types::TeacherQualification {
                teacher_id: teacher_b,
                subject_id: subject_locked,
            });
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let mut placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_d0_p0,
                room_id: room_b,
                teacher_id: teacher_a,
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_d0_p0,
                room_id: room_a,
                teacher_id: teacher_b,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_teacher.insert((teacher_b, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_class.insert((class_b, tb_d0_p0));
        state.used_room.insert((room_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();
        let lahc_list = permissive_lahc_list();
        let room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        let placements_before = placements.clone();
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(!accepted, "subject-unsuitable collision swap must reject");
        assert_eq!(placements, placements_before);
    }

    #[test]
    fn try_home_room_repair_move_rejects_when_lahc_accept_threshold_disallows() {
        // Construct a case where the delta is +5 (worse) and lahc_list[iter % L] = 0.
        // P is in home_room (room_a)... wait, that's the no-op reject case.
        // To get a +5 delta, we need a swap that PUTS a class out of its home.
        // Setup: lesson_a's class home_room = room_a; lesson_b's class home_room = room_b.
        // P (lesson_a) in room_b (miss), Q (lesson_b) in room_a (miss). pre = 5 + 5 = 10.
        // Post swap: P in room_a (hit, 0), Q in room_b (hit, 0). post = 0.
        // delta = -10 (better). Always accepts. To force a worsening swap:
        // Setup: lesson_a's class home_room = room_a; lesson_b's class home_room = room_a.
        // P (lesson_a) in room_a (hit, 0), Q (lesson_b) in room_b (miss, 5). pre = 5.
        // But P is already in home_room -> no-op reject before we even propose.
        // Workable shape: a "swap into home_room for lesson_a, out of home_room for lesson_b
        // where the canonical worsens overall". Set lesson_a home=room_a, lesson_b home=room_b.
        // P (lesson_a) in room_b -> miss 5; Q (lesson_b) in room_a -> miss 5. pre = 10.
        // Swap: P -> room_a (hit, 0), Q -> room_b (hit, 0). post = 0. delta = -10 (better).
        // Hmm, can't easily build a +delta in a 2-class symmetric setup.
        //
        // Easier: use the room-free path and a very tight lahc_list[iter%L] = 0.
        // pre = 5 (lesson_a in room_b, home is room_a, miss). post = 0 (lesson_a in room_a, hit).
        // delta = -5. new_canonical = 0. 0 <= 0 -> accepts.
        //
        // To force a reject on threshold alone, we need a case where the move
        // makes canonical WORSE. The hint is in the kernel: home_room_repair
        // can't make a class's own home_room contribution worse (only better
        // or equal), so the only "worsening" comes from (B) swaps where Q's
        // class's home_room moves further away. That requires Q's class
        // home_room to be in P's NEW room... but P's new room IS the home
        // room of P. So Q's class home_room == P's home_room means Q is
        // already in its own home_room (currently in room_a == Q's home).
        // After swap, Q lands in room_b, away from its home_room. The Q class
        // contribution rises from 0 -> 5. P contribution falls from 5 -> 0.
        // Net delta = 0 (symmetric improve/regress). For a true +delta, the
        // weight scaling has to be asymmetric per class - but
        // weights.prefer_home_room is global. So the symmetric case nets to
        // zero.
        //
        // The cleanest approach: pin lahc_list[iter%L] to 0 and prove that
        // an equal-canonical swap (delta = 0, post = pre = 5) STILL accepts
        // (LAHC accept uses `<=`). Conversely, use lahc_list[iter%L] = 4
        // (one less than the current canonical), and the +0-delta swap
        // would propose new_canonical = 5 which is > 4, reject.
        //
        // For room-free path: pre = 5, delta = -5, post = 0. accept iff 0 <= lahc_list[iter%L].
        // Setting lahc_list[iter%L] = 0 still accepts (0 <= 0). To force a reject,
        // make the move RAISE canonical somehow. Can't with home_room only.
        //
        // Simplest construction: weights.prefer_home_room = 0 (axis disabled).
        // Then delta is always 0; post = pre = 0; accept iff 0 <= lahc_list[iter%L].
        // Setting lahc_list[iter%L] = 0 still accepts.
        //
        // To make an *unambiguous* RED on the accept-threshold branch, we
        // configure: weights.prefer_home_room = 5, lesson_a in room_b (miss),
        // pre = 5. We RAISE state.canonical_score to a non-trivial value 5,
        // and set lahc_list[iter%L] to LESS THAN the delta-applied value.
        // Move delta = -5, new_canonical = 0. 0 <= lahc_list[iter%L] = 0?
        // YES (still accepts). Make lahc_list[iter%L] = some value 0 cannot
        // beat... not possible with unsigned ints and a 0 lower bound.
        //
        // So the threshold-disallows path requires a SYNTHETIC pre-canonical.
        // Use pre = 5, lahc_list[iter%L] = 0, move delta = +5 (somehow).
        // Achievable via the (B) path with asymmetric setup: P's class has
        // home = room_a (current room_b -> 5 miss). Q's class has home = NONE.
        // Pre: P contributes 5, Q contributes 0; canonical = 5.
        // Post swap: P contributes 0, Q contributes 0 (no home); canonical = 0.
        // delta = -5. Always accepts.
        //
        // The cleanest threshold test: pre = 0 (no misses), and propose a swap
        // that would WORSEN by hitting an unmovable Q's class home_room. But
        // P needs to currently NOT be in its home_room to be even eligible.
        // The (B) swap path only swaps when P is in a non-home_room and home
        // is occupied by Q. Pre: P (lesson_a, home=room_a) in room_b (miss=5).
        // Q (lesson_b, home=room_c) in room_a (miss=5). Post: P in room_a (0),
        // Q in room_b (miss=5 if home=room_c). canonical_delta = 0 - 5 + 5 - 5 = -5.
        // Still better.
        //
        // Definitive threshold test: weights.prefer_home_room = 5, P in
        // room_b (home=room_a, miss=5). pre = 5. lahc_list[iter%L] = u32::MAX.
        // delta = -5. new_canonical = 0. accept. But we want REJECT.
        //
        // Set lahc_list[iter%L] = 0 minus delta = 0 - (-5) = 5; new_canonical
        // = 0 <= 5 accepts. To get a reject: new_canonical > lahc_list[iter%L].
        // That requires new_canonical > 0, meaning the move makes things
        // WORSE. The only way with the (A) path is impossible: the move
        // removes a non-zero home_room miss.
        //
        // Resolution: the threshold test pins the symmetric (B) case where
        // the proposed delta is +5 by constructing Q's class with
        // home_room = room_b (Q's NEW post-swap room). Then Q's contribution
        // pre = 5 (Q in room_a, home=room_b), post = 0 (Q in room_b, home=room_b).
        // P's contribution pre = 5 (P in room_b, home=room_a), post = 0
        // (P in room_a, home=room_a). delta = -10. always better. Hmm.
        //
        // Wait - to construct a +delta, set Q's class home_room = ROOM_A
        // (same as P's home_room). pre: P in room_b (miss), Q in room_a (hit).
        // P contributes 5, Q contributes 0. canonical = 5.
        // post swap: P in room_a (hit, 0), Q in room_b (miss, 5).
        // delta = -5 + 5 = 0. new_canonical = 5. lahc_list[iter%L] = 4. reject.
        //
        // This is the construction. Q's class shares P's home_room.
        let (
            mut problem,
            class_a,
            class_b,
            teacher_a,
            teacher_b,
            _subject,
            room_a,
            room_b,
            lesson_a,
            lesson_b,
        ) = home_room_problem(Some(RoomId(lahc_uuid(30))), Some(RoomId(lahc_uuid(30))));
        // Force class_b's home to be room_a too (already set, but be explicit).
        problem.school_classes[1].home_room_id = Some(room_a);
        let tb_d0_p0 = tb_at(&problem, 0, 0);
        let mut placements = vec![
            Placement {
                lesson_id: lesson_a,
                time_block_id: tb_d0_p0,
                room_id: room_b,
                teacher_id: teacher_a,
            },
            Placement {
                lesson_id: lesson_b,
                time_block_id: tb_d0_p0,
                room_id: room_a,
                teacher_id: teacher_b,
            },
        ];
        let mut state = crate::solve::GreedyState::new();
        state.used_teacher.insert((teacher_a, tb_d0_p0));
        state.used_teacher.insert((teacher_b, tb_d0_p0));
        state.used_class.insert((class_a, tb_d0_p0));
        state.used_class.insert((class_b, tb_d0_p0));
        state.used_room.insert((room_a, tb_d0_p0));
        state.used_room.insert((room_b, tb_d0_p0));
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        };
        state.canonical_score = 5; // P in room_b (miss=5), Q in room_a (hit=0).
        let idx = crate::index::Indexed::new(&problem);
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        let subject_lookup: HashMap<SubjectId, &Subject> =
            problem.subjects.iter().map(|s| (s.id, s)).collect();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let pinned: HashSet<LessonId> = HashSet::new();
        // lahc_list[iter%L] = 4. new_canonical after swap = 0 - 5 + 5 + 0 = 5.
        // 5 <= 4 ? NO -> reject.
        let mut lahc_list = vec![u32::MAX; LAHC_LIST_LEN];
        lahc_list[0] = 4;
        let room_order: Vec<usize> = (0..problem.rooms.len()).collect();
        let placements_before = placements.clone();
        let canonical_before = state.canonical_score;
        let accepted = try_home_room_repair_move(
            &problem,
            &idx,
            0,
            &lesson_lookup,
            &tb_lookup,
            &subject_lookup,
            &home_room_lookup,
            &weights,
            &mut placements,
            &mut state,
            &pinned,
            &lahc_list,
            0,
            &room_order,
        );
        assert!(!accepted, "lahc-threshold-disallowed swap must reject");
        assert_eq!(placements, placements_before);
        assert_eq!(state.canonical_score, canonical_before);
    }
}
