//! First Fit Decreasing greedy timetable solver. Sorts lessons by
//! eligibility (most constrained first) via `ordering::ffd_order`, then
//! commits the first hard-constraint-satisfying (time block, room) for each
//! lesson-hour. Placement failures become typed violations
//! (`TeacherOverCapacity`, `NoFreeTimeBlock`, `NoSuitableRoom`) inside
//! `Solution`; `Err(Error::Input)` is reserved for structural input errors.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::error::Error;
use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use crate::index::Indexed;
use crate::types::{
    ConstraintWeights, Lesson, Placement, Problem, Solution, SolveConfig, SolveStats, TimeBlock,
    Violation, ViolationKind,
};
use crate::validate::{
    pre_solve_violations, validate_class_subject_teacher_uniformity, validate_daily_caps,
    validate_no_double_booking, validate_no_room_hopping, validate_placement_teacher_in_candidates,
    validate_structural,
};

#[cfg(feature = "solver-trace")]
use crate::trace;

/// Solve the timetable problem using lowest-delta greedy placement followed
/// by a 200ms LAHC local-search pass. Active default soft-constraint weights
/// are `class_gap = teacher_gap = 10`, `prefer_home_room = class_day_balance
///   = 5`, and `prefer_early_period = avoid_first_period = avoid_last_period
///   = prefer_late_period = 1`. The gap weights dominate so the optimiser
/// treats compaction as the primary objective; `prefer_home_room` and
/// `class_day_balance` form a mid tier that shapes which feasible compact
/// schedules are preferred; the per-period preferences are tiebreakers.
/// `prefer_late_period` is non-zero so the per-subject opt-in (e.g. FOe)
/// has any effect at all. Callers wanting greedy-only behaviour (no LAHC
/// pass) construct their own [`SolveConfig`] with `deadline: None` and call
/// [`solve_with_config`] directly.
pub fn solve(problem: &Problem) -> Result<Solution, Error> {
    let active_default = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_early_period: 1,
            avoid_first_period: 1,
            prefer_home_room: 5,
            avoid_last_period: 1,
            prefer_late_period: 1,
            class_day_balance: 5,
            prefer_class_teacher: 5,
        },
        deadline: Some(Duration::from_millis(200)),
        ..SolveConfig::default()
    };
    solve_with_config(problem, &active_default)
}

/// Solve the timetable problem with explicit configuration. Iterates lessons
/// in FFD order; for each lesson-hour, picks the hard-feasible
/// `(time_block, room)` candidate that minimises the running soft-score
/// (sum of weighted gap-hours per `(class, day)` and `(teacher, day)`
/// partition). Wrapper over [`solve_with_config_stats`] that discards the
/// timing probes; production callers (the no-config `solve()`,
/// `solve_json_with_config`, the solver-py binding) keep the byte-identical
/// signature.
pub fn solve_with_config(problem: &Problem, config: &SolveConfig) -> Result<Solution, Error> {
    solve_with_config_stats(problem, config).map(|(sol, _)| sol)
}

/// Like [`solve_with_config`], plus optional timing probes ([`SolveStats`])
/// recorded against the function entry's wall-clock origin. Today only the
/// bake-off bench (`solver-bench`) consumes the stats; production callers go
/// through [`solve_with_config`] which discards them.
pub fn solve_with_config_stats(
    problem: &Problem,
    config: &SolveConfig,
) -> Result<(Solution, SolveStats), Error> {
    let solve_start = Instant::now();
    let mut stats = SolveStats::default();
    validate_structural(problem)?;

    let (seed_placements, pinned, mut pin_violations) = validate_pins(problem);

    let idx = Indexed::new(problem);
    let mut solution = Solution {
        placements: seed_placements,
        violations: {
            let mut v = pre_solve_violations(problem);
            v.append(&mut pin_violations);
            v
        },
        soft_score: 0,
    };

    let mut state = GreedyState::new();
    use crate::ids::LessonGroupId;
    let mut placed_groups: HashSet<LessonGroupId> = HashSet::new();
    let mut group_members: HashMap<LessonGroupId, Vec<usize>> = HashMap::new();
    for (i, lesson) in problem.lessons.iter().enumerate() {
        if let Some(group_id) = lesson.lesson_group_id {
            group_members.entry(group_id).or_default().push(i);
        }
    }
    let teacher_max: HashMap<TeacherId, u8> = problem
        .teachers
        .iter()
        .map(|t| (t.id, t.max_hours_per_week))
        .collect();
    let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = problem
        .school_classes
        .iter()
        .filter_map(|c| c.max_lessons_per_day.map(|cap| (c.id, cap)))
        .collect();

    // Seed greedy bookkeeping from surviving pinned placements so the FFD
    // loop's existing conflict checks treat pinned slots as occupied.
    seed_greedy_state_from_pins(problem, &solution.placements, &mut state);

    // Iterate time-blocks in (day, position) order and rooms in id order so
    // the lowest-delta picker can prune later candidates whose tiebreak rank
    // they could no longer beat. Sorting once amortises across all placements.
    let mut tb_order: Vec<usize> = (0..problem.time_blocks.len()).collect();
    tb_order.sort_unstable_by_key(|&i| {
        let tb = &problem.time_blocks[i];
        (tb.day_of_week, tb.position, tb.id.0)
    });
    let mut room_order: Vec<usize> = (0..problem.rooms.len()).collect();
    room_order.sort_unstable_by_key(|&i| problem.rooms[i].id.0);
    let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
        .school_classes
        .iter()
        .map(|c| (c.id, c.home_room_id))
        .collect();
    // Item 68 precompute pair: `class_teacher_lookup` maps every class to
    // its (optional) `class_teacher_id`; `subject_qualified_teachers`
    // maps each subject to the set of teachers qualified for it. Both
    // are read by `try_place_block` to score the prefer_class_teacher
    // axis at placement time. Building once here amortises across every
    // FFD lesson and every R&R recreate call.
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

    let order = crate::ordering::ffd_order(problem, &idx);
    for &lesson_idx in &order {
        let lesson = &problem.lessons[lesson_idx];
        // Pinned lessons keep their seeded placement; FFD must not retry them.
        if pinned.contains(&lesson.id) {
            continue;
        }
        // Skip placements for lessons with pre-solve violations; `pre_solve_violations`
        // already recorded one violation per hour.
        if !idx.teacher_qualified(lesson.assigned_teacher_id(), lesson.subject_id) {
            continue;
        }

        if let Some(group_id) = lesson.lesson_group_id {
            if !placed_groups.insert(group_id) {
                continue;
            }
            let member_indices = group_members.get(&group_id).cloned().unwrap_or_default();
            if member_indices.len() < 2 {
                placed_groups.remove(&group_id);
            } else {
                let unqualified_member = member_indices.iter().any(|&mi| {
                    let m = &problem.lessons[mi];
                    !idx.teacher_qualified(m.assigned_teacher_id(), m.subject_id)
                });
                let n = lesson.preferred_block_size;
                let block_count = lesson.hours_per_week / n;
                for block_index in 0..block_count {
                    let placed = if unqualified_member {
                        false
                    } else {
                        try_place_group(
                            problem,
                            &member_indices,
                            n,
                            &idx,
                            &teacher_max,
                            &class_max_lessons_per_day,
                            &config.weights,
                            &mut state,
                            &mut solution.placements,
                            &tb_order,
                            &room_order,
                            &max_position_per_day,
                            days,
                        )
                    };
                    if !placed {
                        for &mi in &member_indices {
                            let member = &problem.lessons[mi];
                            if !idx
                                .teacher_qualified(member.assigned_teacher_id(), member.subject_id)
                            {
                                continue;
                            }
                            solution.violations.push(Violation {
                                kind: ViolationKind::LessonGroupSplit,
                                lesson_id: member.id,
                                hour_index: block_index * n,
                                reason: None,
                            });
                        }
                    }
                }
                continue;
            }
        }

        let n = lesson.preferred_block_size;
        let block_count = lesson.hours_per_week / n;
        for block_index in 0..block_count {
            let placed = try_place_block(
                problem,
                lesson,
                n,
                &idx,
                &teacher_max,
                &class_max_lessons_per_day,
                &config.weights,
                &home_room_lookup,
                &class_teacher_lookup,
                &subject_qualified_teachers,
                &mut state,
                &mut solution.placements,
                &tb_order,
                &room_order,
                &max_position_per_day,
                days,
            );
            if !placed {
                solution.violations.push(Violation {
                    kind: unplaced_kind(
                        problem,
                        lesson,
                        &idx,
                        &teacher_max,
                        &state.used_teacher,
                        &state.used_class,
                        &state.hours_by_teacher,
                    ),
                    lesson_id: lesson.id,
                    hour_index: block_index * n,
                    reason: None,
                });
            }
        }
    }

    // FFD-already-feasible probe: if greedy produced a feasible, soft-score-zero
    // schedule, both ttf and tto are recorded as Some(0.0) before LAHC runs.
    // If feasibility holds but search_score_slice > 0, ttf alone is set so the
    // LAHC loop's running-best probe captures any improvement.
    let placements_expected: usize = problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as usize)
        .sum();
    let greedy_feasible =
        solution.violations.is_empty() && solution.placements.len() == placements_expected;
    if greedy_feasible {
        stats.time_to_first_feasible_ms = Some(0.0);
        if state.search_score_slice == 0 {
            stats.time_to_optimal_ms = Some(0.0);
        }
    }

    // Initialise the canonical-objective tracker. Greedy persists the slice
    // into state.search_score_slice via try_place_block; the canonical adds
    // home_room + class_day_balance over the slice. Set once here so LAHC
    // can maintain canonical incrementally per move.
    state.canonical_score =
        crate::score::score_solution(problem, &solution.placements, &config.weights);

    crate::lahc::run(
        problem,
        &idx,
        config,
        &mut solution.placements,
        &mut state,
        &pinned,
        &class_max_lessons_per_day,
        &mut stats,
        solve_start,
    );

    // Post-solve hard-constraint sanity check. A failure here is a solver bug.
    validate_no_room_hopping(problem, &solution.placements)?;
    validate_no_double_booking(problem, &solution.placements)?;
    validate_daily_caps(problem, &solution.placements)?;
    validate_placement_teacher_in_candidates(problem, &solution.placements)?;
    validate_class_subject_teacher_uniformity(problem, &solution.placements)?;

    // state.search_score_slice is the LAHC running slice (class_gap +
    // teacher_gap + subject_pref). Solution.soft_score is the full weighted
    // cost on the final placements, including prefer_home_room and
    // class_day_balance, so consumers compare every backend on the same
    // number.
    solution.soft_score =
        crate::score::score_solution(problem, &solution.placements, &config.weights);
    debug_assert_eq!(
        solution.soft_score,
        crate::score::score_solution(problem, &solution.placements, &config.weights),
        "Solution.soft_score must equal score_solution(problem, placements, weights) for every backend (item 51)",
    );
    Ok((solution, stats))
}

/// Mutable bookkeeping shared across all lesson-hour placements during one
/// greedy solve. Hard-constraint sets prevent double-booking; partition maps
/// and `search_score_slice` enable O(1) candidate scoring without reiterating
/// placed lessons.
pub(crate) struct GreedyState {
    pub(crate) used_teacher: HashSet<(TeacherId, TimeBlockId)>,
    pub(crate) used_class: HashSet<(SchoolClassId, TimeBlockId)>,
    pub(crate) used_room: HashSet<(RoomId, TimeBlockId)>,
    pub(crate) hours_by_teacher: HashMap<TeacherId, u8>,
    pub(crate) class_positions: HashMap<(SchoolClassId, u8), Vec<u8>>,
    pub(crate) teacher_positions: HashMap<(TeacherId, u8), Vec<u8>>,
    /// Hard same-room invariant: every accepted placement records the room a
    /// `(class, day_of_week, subject)` triple was first placed in plus a
    /// reference count. Subsequent placements for the same triple must reuse
    /// the same room. Across days the room can change. Pinned placements seed
    /// the map before FFD runs and LAHC reads it to reject moves that would
    /// introduce a hop. The count tracks how many placements share the
    /// triple so LAHC can remove the lock when its last placement leaves the
    /// triple.
    pub(crate) locked_room: HashMap<(SchoolClassId, u8, SubjectId), (RoomId, u32)>,
    /// Per-`(class, day, subject)` cap-aware hour counter; mirrors the
    /// running total compared against `Subject.max_hours_per_day`. Updated
    /// in `try_place_block`'s accept path and decremented in the row-removal
    /// helper used by `rr_ruin_block` and `kempe_rollback`.
    pub(crate) subject_hours_by_class_day: HashMap<(SchoolClassId, u8, SubjectId), u8>,
    /// Per-`(class, day)` cap-aware lesson counter; mirrors the running
    /// total compared against `SchoolClass.max_lessons_per_day` (when set).
    /// Maintained in lockstep with the existing per-class bookkeeping.
    pub(crate) lessons_by_class_day: HashMap<(SchoolClassId, u8), u8>,
    /// Solver-driven teacher-uniformity lock (item 66): the first
    /// placement of a `(school_class, subject)` pair pins the teacher
    /// every subsequent placement of the same pair must reuse. Populated
    /// by `try_place_block` at commit time; read by `try_place_block`'s
    /// candidate loop to short-circuit to a singleton iter when a lock
    /// already exists. R&R recreate clears entries whose pair has no
    /// remaining placements after the destroy phase. Change moves leave
    /// the map untouched (the move keeps the same lesson hence the same
    /// pair). Item 68.
    pub(crate) class_subject_teacher: HashMap<(SchoolClassId, SubjectId), TeacherId>,
    /// Running LAHC search slice: `class_gap + teacher_gap + subject_pref`.
    /// Maintained by greedy's `try_place_block` persist site, by Change-move
    /// delta, by Kempe snapshot+delta, and by R&R via
    /// `running_slice_from_placements`. Greedy's picker persist contract
    /// stores `slice_score` here; LAHC reads the slice for the non-negative
    /// debug_assert in `try_change_move`.
    pub(crate) search_score_slice: u32,
    /// Running canonical objective: `score_solution(problem, placements,
    /// weights)`. Initialised at the end of greedy in
    /// `solve_with_config_stats` before LAHC dispatch. Maintained in
    /// lockstep with `search_score_slice` across the LAHC Change move
    /// (incremental delta), R&R (full recompute), and Kempe (snapshot
    /// plus delta). Drives LAHC's accept criterion, `time_to_optimal_ms`
    /// probe, early-exit predicate, and running-best snapshot.
    pub(crate) canonical_score: u32,
}

impl GreedyState {
    pub(crate) fn new() -> Self {
        Self {
            used_teacher: HashSet::new(),
            used_class: HashSet::new(),
            used_room: HashSet::new(),
            hours_by_teacher: HashMap::new(),
            class_positions: HashMap::new(),
            teacher_positions: HashMap::new(),
            locked_room: HashMap::new(),
            subject_hours_by_class_day: HashMap::new(),
            lessons_by_class_day: HashMap::new(),
            class_subject_teacher: HashMap::new(),
            search_score_slice: 0,
            canonical_score: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BlockCandidate {
    outer_pos: usize,
    day: u8,
    start_pos: u8,
    end_pos: u8,
    room_id: RoomId,
    /// Teacher chosen for this candidate from the lesson's
    /// `teacher_candidates` (item 68). When the lesson has a `teacher_pin`
    /// or any member `(class, subject)` already has a `class_subject_teacher`
    /// lock, the teacher iteration collapses to that one teacher.
    teacher_id: TeacherId,
    /// Slice-only running cost (`class_gap + teacher_gap + subject_pref`)
    /// post-place. Persisted to `state.search_score_slice` so the slice
    /// contract LAHC's Change move and R&R post-recreate
    /// `running_slice_from_placements` rely on stays intact.
    slice_score: u32,
    /// Total cost = `slice_score + home_room_penalty(room_id) +
    /// class_day_balance + prefer_class_teacher_cost`. Used for candidate
    /// ranking and pruning. NOT persisted to `state.search_score_slice`.
    /// The canonical score is recomputed via `score::score_solution`
    /// after the FFD pass and after every R&R recreate, so we do not
    /// thread the per-candidate prefer_class_teacher breakdown through
    /// the BlockCandidate.
    total_score: u32,
}

/// Stack-allocated iterator over a lesson's eligible teachers per
/// `try_place_block` call. The hot loop must not allocate per call (per
/// `solver/CLAUDE.md` no-allocation rule); this enum sidesteps `Box<dyn
/// Iterator>` and `Vec` collection.
///
/// `Singleton` covers the pinned-lesson case (`teacher_pin = Some(t)`)
/// and the lock-already-set case (any member class has a
/// `class_subject_teacher` entry). `Multi` covers the free pick case
/// over `lesson.teacher_candidates`.
enum TeacherCandidates<'a> {
    Singleton([TeacherId; 1]),
    Multi(&'a [TeacherId]),
}

impl<'a> TeacherCandidates<'a> {
    fn iter(&self) -> std::slice::Iter<'_, TeacherId> {
        match self {
            TeacherCandidates::Singleton(arr) => arr.iter(),
            TeacherCandidates::Multi(slice) => slice.iter(),
        }
    }
}

/// Cost contribution of one `(class, subject)` pair when a placement
/// would freshly populate `state.class_subject_teacher`. Mirrors the
/// closed-form term in `score::score_solution`: zero unless the class
/// has a `class_teacher_id` qualified for the subject AND the chosen
/// teacher is not the class teacher.
///
/// Returns `weights.prefer_class_teacher` per qualifying pair, else 0.
/// Saturating arithmetic at the call site sums across member classes.
fn prefer_class_teacher_lock_cost(
    class: SchoolClassId,
    subject_id: SubjectId,
    chosen_teacher: TeacherId,
    class_teacher_lookup: &HashMap<SchoolClassId, Option<TeacherId>>,
    subject_qualified_teachers: &HashMap<SubjectId, HashSet<TeacherId>>,
    weights: &ConstraintWeights,
) -> u32 {
    if weights.prefer_class_teacher == 0 {
        return 0;
    }
    let Some(Some(klt)) = class_teacher_lookup.get(&class) else {
        return 0;
    };
    let Some(qualified) = subject_qualified_teachers.get(&subject_id) else {
        return 0;
    };
    if !qualified.contains(klt) || chosen_teacher == *klt {
        return 0;
    }
    weights.prefer_class_teacher
}

/// Gap-count after inserting positions `start..=end` (inclusive) into a sorted
/// slice. Caller guarantees `[start, end]` is disjoint from `positions`.
/// Allocation-free: reads `v.first()` and `v.last()`, computes the new span
/// and length, and returns the gap-count without copying the slice.
fn gap_count_after_window_insert(positions: Option<&Vec<u8>>, start: u8, end: u8) -> u32 {
    let n_added = u32::from(end - start + 1);
    let Some(v) = positions else {
        return 0;
    };
    if v.is_empty() {
        return 0;
    }
    let v_min = *v.first().unwrap();
    let v_max = *v.last().unwrap();
    let new_min = v_min.min(start);
    let new_max = v_max.max(end);
    let len_after = u32::try_from(v.len())
        .unwrap_or(u32::MAX)
        .saturating_add(n_added);
    let span = u32::from(new_max - new_min) + 1;
    span.saturating_sub(len_after)
}

#[allow(clippy::too_many_arguments)] // Reason: internal helper; refactoring to a struct hurts clarity more than it helps
pub(crate) fn try_place_block(
    problem: &Problem,
    lesson: &Lesson,
    n: u8,
    idx: &Indexed,
    teacher_max: &HashMap<TeacherId, u8>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    weights: &ConstraintWeights,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    class_teacher_lookup: &HashMap<SchoolClassId, Option<TeacherId>>,
    subject_qualified_teachers: &HashMap<SubjectId, HashSet<TeacherId>>,
    state: &mut GreedyState,
    placements: &mut Vec<Placement>,
    tb_order: &[usize],
    room_order: &[usize],
    max_position_per_day: &HashMap<u8, u8>,
    days: u8,
) -> bool {
    let class_ids: &[SchoolClassId] = &lesson.school_class_ids;
    let subject = problem
        .subjects
        .iter()
        .find(|s| s.id == lesson.subject_id)
        .expect("validate_structural ensures every lesson.subject_id resolves");
    let n_usize = n as usize;

    // Item 68 teacher candidate selection. Three cases:
    // 1. Lock already exists for any member `(class, subject)` pair: the
    //    candidates collapse to the locked teacher. All member-class
    //    locks must agree; disagreement is a hard infeasibility (caller
    //    upstream should never construct this state).
    // 2. `lesson.teacher_pin` is set: collapse to `[pin]`.
    // 3. Otherwise: iterate `lesson.teacher_candidates`.
    let mut existing_lock: Option<TeacherId> = None;
    for class in class_ids {
        if let Some(t) = state
            .class_subject_teacher
            .get(&(*class, lesson.subject_id))
        {
            match existing_lock {
                None => existing_lock = Some(*t),
                Some(prev) if prev != *t => {
                    // Disagreement among member-class locks. Cannot
                    // satisfy uniformity; treat as no candidate.
                    return false;
                }
                _ => {}
            }
        }
    }
    let teacher_candidates: TeacherCandidates<'_> = if let Some(t) = existing_lock {
        TeacherCandidates::Singleton([t])
    } else if let Some(pin) = lesson.teacher_pin {
        TeacherCandidates::Singleton([pin])
    } else {
        TeacherCandidates::Multi(&lesson.teacher_candidates)
    };

    let mut best: Option<BlockCandidate> = None;
    'outer: for outer_pos in 0..tb_order.len() {
        if outer_pos + n_usize > tb_order.len() {
            break;
        }
        let first_tb = &problem.time_blocks[tb_order[outer_pos]];

        // Window contiguity: every position in the window must sit on the
        // same day at first_tb.position + k. Since tb_order is sorted by
        // (day, position, id), a non-contiguous neighbour means this start
        // cannot anchor an n-block window.
        for k in 1..n_usize {
            let nb = &problem.time_blocks[tb_order[outer_pos + k]];
            if nb.day_of_week != first_tb.day_of_week
                || nb.position != first_tb.position + (k as u8)
            {
                #[cfg(feature = "solver-trace")]
                trace::ffd_trace(
                    lesson.id,
                    first_tb.day_of_week,
                    first_tb.position,
                    None,
                    "non_contiguous_window",
                );
                continue 'outer;
            }
        }

        // Class-side hard-feasibility (teacher-independent: every member
        // class must be free in every position of the window). Hoisted
        // out of the teacher loop because the candidates iterator does
        // not affect class busy-ness.
        for k in 0..n_usize {
            let tb = &problem.time_blocks[tb_order[outer_pos + k]];
            for class in class_ids {
                if state.used_class.contains(&(*class, tb.id)) {
                    #[cfg(feature = "solver-trace")]
                    trace::ffd_trace(
                        lesson.id,
                        first_tb.day_of_week,
                        first_tb.position,
                        None,
                        "class_busy",
                    );
                    continue 'outer;
                }
            }
        }
        // Per-day caps: reject windows that would push any member class past
        // `Subject.max_hours_per_day` or past `SchoolClass.max_lessons_per_day`
        // (when set). One block contributes `n` to subject hours and `1` to
        // class lessons. Teacher-independent, hoisted.
        let subject_cap = subject.max_hours_per_day;
        for class in class_ids {
            let key = (*class, first_tb.day_of_week, lesson.subject_id);
            let current_hours = state
                .subject_hours_by_class_day
                .get(&key)
                .copied()
                .unwrap_or(0);
            if current_hours.saturating_add(n) > subject_cap {
                #[cfg(feature = "solver-trace")]
                trace::ffd_trace(
                    lesson.id,
                    first_tb.day_of_week,
                    first_tb.position,
                    None,
                    "subject_daily_cap",
                );
                continue 'outer;
            }
            if let Some(cap) = class_max_lessons_per_day.get(class).copied() {
                let lessons_today = state
                    .lessons_by_class_day
                    .get(&(*class, first_tb.day_of_week))
                    .copied()
                    .unwrap_or(0);
                if lessons_today.saturating_add(1) > cap {
                    #[cfg(feature = "solver-trace")]
                    trace::ffd_trace(
                        lesson.id,
                        first_tb.day_of_week,
                        first_tb.position,
                        None,
                        "class_daily_lesson_cap",
                    );
                    continue 'outer;
                }
            }
        }

        // Class-side score (teacher-independent), plus subject_pref. Hoisted
        // out of the teacher loop because neither depends on the chosen
        // teacher.
        let start_pos = first_tb.position;
        let end_pos = start_pos + n - 1;
        let mut class_delta_sum: i64 = 0;
        for class in class_ids {
            let class_partition = state.class_positions.get(&(*class, first_tb.day_of_week));
            let class_old = match class_partition {
                Some(p) => crate::score::gap_count(p),
                None => 0,
            };
            let class_new = gap_count_after_window_insert(class_partition, start_pos, end_pos);
            class_delta_sum += i64::from(class_new) - i64::from(class_old);
        }
        let max_pos = max_position_per_day
            .get(&first_tb.day_of_week)
            .copied()
            .unwrap_or(end_pos);
        let mut subject_pref = 0u32;
        for k in 0..n_usize {
            let tb = &problem.time_blocks[tb_order[outer_pos + k]];
            subject_pref = subject_pref.saturating_add(crate::score::subject_preference_score(
                subject, tb, max_pos, weights,
            ));
        }
        let class_delta_w = class_delta_sum.saturating_mul(i64::from(weights.class_gap));

        // Same-room hard constraint: if any member class already has this
        // subject placed on this day, every member class must agree on that
        // room. Disagreement would force two different rooms; skip the
        // window. A consistent shared lock pins the candidate room.
        // Teacher-independent, hoisted.
        let day = first_tb.day_of_week;
        let mut shared_lock: Option<RoomId> = None;
        let mut lock_conflict = false;
        for class in class_ids {
            if let Some(&(locked, _)) = state.locked_room.get(&(*class, day, lesson.subject_id)) {
                match shared_lock {
                    None => shared_lock = Some(locked),
                    Some(prev) if prev != locked => {
                        lock_conflict = true;
                        break;
                    }
                    _ => {}
                }
            }
        }
        if lock_conflict {
            #[cfg(feature = "solver-trace")]
            trace::ffd_trace(
                lesson.id,
                first_tb.day_of_week,
                first_tb.position,
                None,
                "locked_room_conflict",
            );
            continue;
        }

        // Pick the feasible room that minimises `home_room_penalty(room)`.
        // Teacher-independent (room suitability/blocking/usage do not
        // depend on which teacher is chosen). Hoisted out of the teacher
        // loop. Strict `<` plus `room_order`'s id-sorted iteration means
        // the lowest-id room wins on a penalty tie. Early-break when
        // penalty == 0: a home-room match is unbeatable and later rooms
        // are id-greater (only-tied-or-worse on the penalty/id tiebreak).
        let mut best_room: Option<(RoomId, u32)> = None;
        'rooms: for &room_idx in room_order {
            let room = &problem.rooms[room_idx];
            if let Some(locked) = shared_lock {
                if room.id != locked {
                    #[cfg(feature = "solver-trace")]
                    trace::ffd_trace(
                        lesson.id,
                        first_tb.day_of_week,
                        first_tb.position,
                        Some(room.id),
                        "locked_room_mismatch",
                    );
                    continue;
                }
            }
            if !idx.room_suits_subject(room.id, lesson.subject_id) {
                #[cfg(feature = "solver-trace")]
                trace::ffd_trace(
                    lesson.id,
                    first_tb.day_of_week,
                    first_tb.position,
                    Some(room.id),
                    "room_unsuitable",
                );
                continue;
            }
            for k in 0..n_usize {
                let tb = &problem.time_blocks[tb_order[outer_pos + k]];
                if state.used_room.contains(&(room.id, tb.id)) {
                    #[cfg(feature = "solver-trace")]
                    trace::ffd_trace(
                        lesson.id,
                        first_tb.day_of_week,
                        first_tb.position,
                        Some(room.id),
                        "room_busy",
                    );
                    continue 'rooms;
                }
                if idx.room_blocked(room.id, tb.id) {
                    #[cfg(feature = "solver-trace")]
                    trace::ffd_trace(
                        lesson.id,
                        first_tb.day_of_week,
                        first_tb.position,
                        Some(room.id),
                        "room_blocked",
                    );
                    continue 'rooms;
                }
            }
            let penalty =
                crate::score::home_room_penalty(lesson, home_room_lookup, room.id, weights);
            let take = match best_room {
                None => true,
                Some((_, best_penalty)) => penalty < best_penalty,
            };
            if take {
                best_room = Some((room.id, penalty));
                if penalty == 0 {
                    // Home-room match found at lowest-id; later rooms are
                    // id-greater and at-best-tied on penalty, so they cannot
                    // strictly beat this candidate.
                    break;
                }
            }
        }
        let Some((room_id, room_penalty)) = best_room else {
            continue;
        };

        // Class-day-balance contribution to candidate ranking (item 54). Sum
        // the per-class post-place L1 cost across every member class of this
        // lesson, then weight. Teacher-independent, hoisted.
        let balance_post: u32 = if weights.class_day_balance == 0 {
            0
        } else {
            let mut acc: u32 = 0;
            for class in class_ids {
                acc = acc.saturating_add(crate::score::class_day_balance_cost_for_class_after_add(
                    *class,
                    days,
                    &state.class_positions,
                    first_tb.day_of_week,
                    n,
                ));
            }
            weights.class_day_balance.saturating_mul(acc)
        };

        // Item 68 teacher candidate iteration. The class-side score and
        // the room scan above are teacher-independent; only teacher
        // hard-feasibility (used_teacher, teacher_blocked, teacher
        // capacity), the teacher-gap delta, and the
        // prefer_class_teacher cost vary per teacher. The candidate
        // iterator is stack-allocated (`TeacherCandidates::Singleton` for
        // pinned/locked, `Multi` borrowed slice for the free pick) so
        // the hot loop stays allocation-free.
        for &candidate_teacher in teacher_candidates.iter() {
            // Hard-feasibility for every position in the window for THIS
            // teacher: not currently busy and not blocked at any of the
            // window's tbs.
            let mut teacher_blocked_in_window = false;
            for k in 0..n_usize {
                let tb = &problem.time_blocks[tb_order[outer_pos + k]];
                if state.used_teacher.contains(&(candidate_teacher, tb.id))
                    || idx.teacher_blocked(candidate_teacher, tb.id)
                {
                    teacher_blocked_in_window = true;
                    break;
                }
            }
            if teacher_blocked_in_window {
                #[cfg(feature = "solver-trace")]
                trace::ffd_trace(
                    lesson.id,
                    first_tb.day_of_week,
                    first_tb.position,
                    None,
                    "teacher_busy",
                );
                continue;
            }

            // Teacher capacity (per-week max).
            let current = state
                .hours_by_teacher
                .get(&candidate_teacher)
                .copied()
                .unwrap_or(0);
            let max = teacher_max.get(&candidate_teacher).copied().unwrap_or(0);
            if current.saturating_add(n) > max {
                #[cfg(feature = "solver-trace")]
                trace::ffd_trace(
                    lesson.id,
                    first_tb.day_of_week,
                    first_tb.position,
                    None,
                    "teacher_over_capacity",
                );
                continue;
            }

            // Teacher-side score delta.
            let teacher_partition = state
                .teacher_positions
                .get(&(candidate_teacher, first_tb.day_of_week));
            let teacher_old = match teacher_partition {
                Some(p) => crate::score::gap_count(p),
                None => 0,
            };
            let teacher_new = gap_count_after_window_insert(teacher_partition, start_pos, end_pos);
            let teacher_delta_w = (i64::from(teacher_new) - i64::from(teacher_old))
                .saturating_mul(i64::from(weights.teacher_gap));

            let new_signed = i64::from(state.search_score_slice)
                .saturating_add(class_delta_w)
                .saturating_add(teacher_delta_w)
                .saturating_add(i64::from(subject_pref));
            let slice_score = u32::try_from(new_signed.max(0)).unwrap_or(u32::MAX);

            // prefer_class_teacher cost contribution: sum across member
            // classes whose `(class, subject)` lock would be FRESHLY set
            // by this placement. Member classes whose lock already
            // exists do not add a new contribution (the existing entry
            // is already in `state.canonical_score`).
            let mut prefer_class_teacher_cost: u32 = 0;
            if weights.prefer_class_teacher != 0 && existing_lock.is_none() {
                // existing_lock None means no member class currently has
                // a lock (else we collapsed to Singleton above); each
                // member class is freshly locked by this placement.
                for class in class_ids {
                    prefer_class_teacher_cost =
                        prefer_class_teacher_cost.saturating_add(prefer_class_teacher_lock_cost(
                            *class,
                            lesson.subject_id,
                            candidate_teacher,
                            class_teacher_lookup,
                            subject_qualified_teachers,
                            weights,
                        ));
                }
            }

            let total_score = slice_score
                .saturating_add(room_penalty)
                .saturating_add(balance_post)
                .saturating_add(prefer_class_teacher_cost);

            // Pruning: skip if this triple's total cannot beat the current
            // best total. Strict `>=` keeps FIRST-walked candidate on tie
            // (cross-window strict-`<` rule, item 60).
            if let Some(b) = &best {
                if total_score >= b.total_score {
                    #[cfg(feature = "solver-trace")]
                    trace::ffd_trace(
                        lesson.id,
                        first_tb.day_of_week,
                        first_tb.position,
                        None,
                        "score_pruned",
                    );
                    continue;
                }
            }

            #[cfg(feature = "solver-trace")]
            trace::ffd_trace(
                lesson.id,
                first_tb.day_of_week,
                first_tb.position,
                Some(room_id),
                "window_candidate",
            );
            best = Some(BlockCandidate {
                outer_pos,
                day: first_tb.day_of_week,
                start_pos,
                end_pos,
                room_id,
                teacher_id: candidate_teacher,
                slice_score,
                total_score,
            });
        }

        // Early exit at the window level: if the current best landed at
        // this window with `total_score == state.search_score_slice`
        // (no extra cost beyond the running slice baseline), no later
        // window can strictly improve it: `tb_order`'s sort means later
        // windows have weakly larger (day, position) tiebreak rank, and
        // strict `<` keeps the FIRST-walked candidate on tie.
        if let Some(b) = &best {
            if b.outer_pos == outer_pos && b.total_score == state.search_score_slice {
                break;
            }
        }
    }

    let Some(c) = best else {
        #[cfg(feature = "solver-trace")]
        {
            let kind = unplaced_kind(
                problem,
                lesson,
                idx,
                teacher_max,
                &state.used_teacher,
                &state.used_class,
                &state.hours_by_teacher,
            );
            let reason = match kind {
                ViolationKind::NoSuitableRoom => "unplaced_no_suitable_room",
                ViolationKind::NoFreeTimeBlock => "unplaced_no_free_time_block",
                ViolationKind::TeacherOverCapacity => "unplaced_teacher_over_capacity",
                _ => "unplaced",
            };
            trace::ffd_trace(lesson.id, 0, 0, None, reason);
        }
        return false;
    };

    let chosen_teacher = c.teacher_id;
    for k in 0..n_usize {
        let tb = &problem.time_blocks[tb_order[c.outer_pos + k]];
        placements.push(Placement {
            lesson_id: lesson.id,
            time_block_id: tb.id,
            room_id: c.room_id,
            teacher_id: chosen_teacher,
        });
        state.used_teacher.insert((chosen_teacher, tb.id));
        for class in class_ids {
            state.used_class.insert((*class, tb.id));
        }
        state.used_room.insert((c.room_id, tb.id));
    }
    *state.hours_by_teacher.entry(chosen_teacher).or_insert(0) += n;

    for class in class_ids {
        let class_part = state.class_positions.entry((*class, c.day)).or_default();
        for pos in c.start_pos..=c.end_pos {
            let ins = class_part.binary_search(&pos).unwrap_or_else(|i| i);
            class_part.insert(ins, pos);
        }
        // Increment the same-room lock by `n` (one per placed hour). The
        // lock's room must already match `c.room_id` since the room picker
        // honoured `shared_lock`.
        let entry = state
            .locked_room
            .entry((*class, c.day, lesson.subject_id))
            .or_insert((c.room_id, 0));
        entry.1 += u32::from(n);
        // Item 66: set the per-(class, subject) teacher lock so any later
        // FFD or R&R placement of the same pair reuses this teacher.
        // `or_insert(chosen_teacher)` is correct because the candidate
        // selection above already collapsed to the locked teacher when an
        // entry exists; this only inserts on first placement of a pair.
        state
            .class_subject_teacher
            .entry((*class, lesson.subject_id))
            .or_insert(chosen_teacher);
        // Per-day cap counters: subject hours add `n` (period span); class
        // lessons add `1` (one block per accepted call). Decremented in
        // `rr_remove_row_bookkeeping` per row (n=1 each call).
        *state
            .subject_hours_by_class_day
            .entry((*class, c.day, lesson.subject_id))
            .or_insert(0) += n;
        *state
            .lessons_by_class_day
            .entry((*class, c.day))
            .or_insert(0) += 1;
    }
    let teacher_part = state
        .teacher_positions
        .entry((chosen_teacher, c.day))
        .or_default();
    for pos in c.start_pos..=c.end_pos {
        let ins = teacher_part.binary_search(&pos).unwrap_or_else(|i| i);
        teacher_part.insert(ins, pos);
    }
    state.search_score_slice = c.slice_score;
    // Canonical-score maintenance for the prefer_class_teacher axis. The
    // search slice excludes home_room / class_day_balance / prefer_class_teacher;
    // every other axis lives on the canonical-only side. Greedy entry to LAHC
    // initialises `state.canonical_score = score_solution(...)` after the FFD
    // pass, so during FFD we only need to keep `state.search_score_slice` in
    // step. The post-FFD canonical recompute (`solve_with_config_stats`)
    // captures the prefer_class_teacher contribution from `class_subject_teacher`
    // via `score_solution`, so no per-call delta on `state.canonical_score`
    // is required during FFD. Item 68.
    #[cfg(feature = "solver-trace")]
    trace::ffd_trace(lesson.id, c.day, c.start_pos, Some(c.room_id), "placed");
    true
}

fn unplaced_kind(
    problem: &Problem,
    lesson: &Lesson,
    idx: &Indexed,
    teacher_max: &HashMap<TeacherId, u8>,
    used_teacher: &HashSet<(TeacherId, TimeBlockId)>,
    used_class: &HashSet<(SchoolClassId, TimeBlockId)>,
    hours_by_teacher: &HashMap<TeacherId, u8>,
) -> ViolationKind {
    let current = hours_by_teacher
        .get(&lesson.assigned_teacher_id())
        .copied()
        .unwrap_or(0);
    let max = teacher_max
        .get(&lesson.assigned_teacher_id())
        .copied()
        .unwrap_or(0);
    if current >= max {
        return ViolationKind::TeacherOverCapacity;
    }

    let any_slot_open = problem.time_blocks.iter().any(|tb| {
        !used_teacher.contains(&(lesson.assigned_teacher_id(), tb.id))
            && !idx.teacher_blocked(lesson.assigned_teacher_id(), tb.id)
            && lesson
                .school_class_ids
                .iter()
                .all(|class| !used_class.contains(&(*class, tb.id)))
    });
    if !any_slot_open {
        return ViolationKind::NoFreeTimeBlock;
    }
    ViolationKind::NoSuitableRoom
}

#[allow(clippy::too_many_arguments)] // Reason: internal helper; refactoring to a struct hurts clarity more than it helps
fn try_place_group(
    problem: &Problem,
    member_indices: &[usize],
    n: u8,
    idx: &Indexed,
    teacher_max: &HashMap<TeacherId, u8>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    weights: &ConstraintWeights,
    state: &mut GreedyState,
    placements: &mut Vec<Placement>,
    tb_order: &[usize],
    room_order: &[usize],
    max_position_per_day: &HashMap<u8, u8>,
    days: u8,
) -> bool {
    let n_usize = n as usize;
    let members: Vec<&Lesson> = member_indices
        .iter()
        .map(|&i| &problem.lessons[i])
        .collect();
    let mut seen_classes: HashSet<SchoolClassId> = HashSet::new();
    let mut class_set: Vec<SchoolClassId> = Vec::new();
    for member in &members {
        for &class in &member.school_class_ids {
            if seen_classes.insert(class) {
                class_set.push(class);
            }
        }
    }

    #[derive(Debug, Clone)]
    struct GroupCandidate {
        outer_pos: usize,
        day: u8,
        start_pos: u8,
        end_pos: u8,
        rooms: Vec<RoomId>,
        score: u32,
    }
    let mut best: Option<GroupCandidate> = None;

    'outer: for outer_pos in 0..tb_order.len() {
        if outer_pos + n_usize > tb_order.len() {
            break;
        }
        let first_tb = &problem.time_blocks[tb_order[outer_pos]];

        for k in 1..n_usize {
            let nb = &problem.time_blocks[tb_order[outer_pos + k]];
            if nb.day_of_week != first_tb.day_of_week
                || nb.position != first_tb.position + (k as u8)
            {
                continue 'outer;
            }
        }

        for k in 0..n_usize {
            let tb = &problem.time_blocks[tb_order[outer_pos + k]];
            for member in &members {
                if state
                    .used_teacher
                    .contains(&(member.assigned_teacher_id(), tb.id))
                    || idx.teacher_blocked(member.assigned_teacher_id(), tb.id)
                {
                    continue 'outer;
                }
            }
            for class in &class_set {
                if state.used_class.contains(&(*class, tb.id)) {
                    continue 'outer;
                }
            }
        }
        for member in &members {
            let current = state
                .hours_by_teacher
                .get(&member.assigned_teacher_id())
                .copied()
                .unwrap_or(0);
            let max = teacher_max
                .get(&member.assigned_teacher_id())
                .copied()
                .unwrap_or(0);
            if current.saturating_add(n) > max {
                continue 'outer;
            }
        }

        // Per-day caps gate. Each member contributes n hours of its subject
        // to its classes; the shared class set adds 1 lesson per member to
        // each class. Reject windows that would push any class past its
        // cap on any subject or past `max_lessons_per_day` (when set).
        {
            let day_of_week = first_tb.day_of_week;
            let mut subject_cap_violated = false;
            for member in &members {
                let member_subject = problem
                    .subjects
                    .iter()
                    .find(|s| s.id == member.subject_id)
                    .expect("validate_structural ensures member subject_id resolves");
                for class in &member.school_class_ids {
                    let key = (*class, day_of_week, member.subject_id);
                    let current_hours = state
                        .subject_hours_by_class_day
                        .get(&key)
                        .copied()
                        .unwrap_or(0);
                    if current_hours.saturating_add(n) > member_subject.max_hours_per_day {
                        subject_cap_violated = true;
                        break;
                    }
                }
                if subject_cap_violated {
                    break;
                }
            }
            if subject_cap_violated {
                continue 'outer;
            }
            let mut class_cap_violated = false;
            for class in &class_set {
                if let Some(cap) = class_max_lessons_per_day.get(class).copied() {
                    let lessons_today = state
                        .lessons_by_class_day
                        .get(&(*class, day_of_week))
                        .copied()
                        .unwrap_or(0);
                    let added = u8::try_from(members.len()).unwrap_or(u8::MAX);
                    if lessons_today.saturating_add(added) > cap {
                        class_cap_violated = true;
                        break;
                    }
                }
            }
            if class_cap_violated {
                continue 'outer;
            }
        }

        // Same-room hard constraint per (class, day, subject) for each
        // member: if any class already has the member's subject placed on
        // this day, the chosen room must match; disagreement across that
        // member's classes makes the window infeasible.
        let day = first_tb.day_of_week;
        let mut chosen: Vec<RoomId> = Vec::with_capacity(members.len());
        let mut taken: HashSet<RoomId> = HashSet::new();
        let mut all_assigned = true;
        for member in &members {
            let mut shared_lock: Option<RoomId> = None;
            let mut lock_conflict = false;
            for class in &member.school_class_ids {
                if let Some(&(locked, _)) = state.locked_room.get(&(*class, day, member.subject_id))
                {
                    match shared_lock {
                        None => shared_lock = Some(locked),
                        Some(prev) if prev != locked => {
                            lock_conflict = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            if lock_conflict {
                all_assigned = false;
                break;
            }

            let mut picked: Option<RoomId> = None;
            'rooms: for &room_idx in room_order {
                let room = &problem.rooms[room_idx];
                if taken.contains(&room.id) {
                    continue;
                }
                if let Some(locked) = shared_lock {
                    if room.id != locked {
                        continue;
                    }
                }
                if !idx.room_suits_subject(room.id, member.subject_id) {
                    continue;
                }
                for k in 0..n_usize {
                    let tb = &problem.time_blocks[tb_order[outer_pos + k]];
                    if state.used_room.contains(&(room.id, tb.id))
                        || idx.room_blocked(room.id, tb.id)
                    {
                        continue 'rooms;
                    }
                }
                picked = Some(room.id);
                break;
            }
            match picked {
                Some(r) => {
                    taken.insert(r);
                    chosen.push(r);
                }
                None => {
                    all_assigned = false;
                    break;
                }
            }
        }
        if !all_assigned {
            continue;
        }

        let start_pos = first_tb.position;
        let end_pos = start_pos + n - 1;
        let mut class_delta_sum: i64 = 0;
        for class in &class_set {
            let class_partition = state.class_positions.get(&(*class, first_tb.day_of_week));
            let class_old = match class_partition {
                Some(p) => crate::score::gap_count(p),
                None => 0,
            };
            let class_new = gap_count_after_window_insert(class_partition, start_pos, end_pos);
            class_delta_sum += i64::from(class_new) - i64::from(class_old);
        }
        let mut teacher_delta_sum: i64 = 0;
        for member in &members {
            let teacher_partition = state
                .teacher_positions
                .get(&(member.assigned_teacher_id(), first_tb.day_of_week));
            let teacher_old = match teacher_partition {
                Some(p) => crate::score::gap_count(p),
                None => 0,
            };
            let teacher_new = gap_count_after_window_insert(teacher_partition, start_pos, end_pos);
            teacher_delta_sum += i64::from(teacher_new) - i64::from(teacher_old);
        }
        let max_pos = max_position_per_day
            .get(&first_tb.day_of_week)
            .copied()
            .unwrap_or(end_pos);
        let mut subject_pref = 0u32;
        for member in &members {
            let subject = problem
                .subjects
                .iter()
                .find(|s| s.id == member.subject_id)
                .expect("validate_structural ensures member subject_id resolves");
            for k in 0..n_usize {
                let tb = &problem.time_blocks[tb_order[outer_pos + k]];
                subject_pref = subject_pref.saturating_add(crate::score::subject_preference_score(
                    subject, tb, max_pos, weights,
                ));
            }
        }
        let class_delta_w = class_delta_sum.saturating_mul(i64::from(weights.class_gap));
        let teacher_delta_w = teacher_delta_sum.saturating_mul(i64::from(weights.teacher_gap));
        let new_signed = i64::from(state.search_score_slice)
            .saturating_add(class_delta_w)
            .saturating_add(teacher_delta_w)
            .saturating_add(i64::from(subject_pref));
        let slice_score = u32::try_from(new_signed.max(0)).unwrap_or(u32::MAX);

        // Class-day-balance contribution for this group window (item 54). One
        // group placement adds `n` lessons to every class in `class_set`
        // simultaneously, so the per-class cost stacks across the shared set.
        let balance_post: u32 = if weights.class_day_balance == 0 {
            0
        } else {
            let mut acc: u32 = 0;
            for class in &class_set {
                acc = acc.saturating_add(crate::score::class_day_balance_cost_for_class_after_add(
                    *class,
                    days,
                    &state.class_positions,
                    first_tb.day_of_week,
                    n,
                ));
            }
            weights.class_day_balance.saturating_mul(acc)
        };
        let score = slice_score.saturating_add(balance_post);

        if let Some(b) = &best {
            if score >= b.score {
                continue;
            }
        }

        best = Some(GroupCandidate {
            outer_pos,
            day: first_tb.day_of_week,
            start_pos,
            end_pos,
            rooms: chosen,
            score,
        });

        if score == state.search_score_slice {
            break;
        }
    }

    let Some(c) = best else {
        return false;
    };

    for (member_pos, member) in members.iter().enumerate() {
        let room_id = c.rooms[member_pos];
        let member_teacher = member.assigned_teacher_id();
        for k in 0..n_usize {
            let tb = &problem.time_blocks[tb_order[c.outer_pos + k]];
            placements.push(Placement {
                lesson_id: member.id,
                time_block_id: tb.id,
                room_id,
                teacher_id: member_teacher,
            });
            state.used_teacher.insert((member_teacher, tb.id));
            state.used_room.insert((room_id, tb.id));
        }
        *state
            .hours_by_teacher
            .entry(member.assigned_teacher_id())
            .or_insert(0) += n;
    }
    for k in 0..n_usize {
        let tb = &problem.time_blocks[tb_order[c.outer_pos + k]];
        for class in &class_set {
            state.used_class.insert((*class, tb.id));
        }
    }
    for class in &class_set {
        let part = state.class_positions.entry((*class, c.day)).or_default();
        for pos in c.start_pos..=c.end_pos {
            let ins = part.binary_search(&pos).unwrap_or_else(|i| i);
            part.insert(ins, pos);
        }
    }
    for member in &members {
        let part = state
            .teacher_positions
            .entry((member.assigned_teacher_id(), c.day))
            .or_default();
        for pos in c.start_pos..=c.end_pos {
            let ins = part.binary_search(&pos).unwrap_or_else(|i| i);
            part.insert(ins, pos);
        }
    }
    for (member_pos, member) in members.iter().enumerate() {
        let room_id = c.rooms[member_pos];
        let member_teacher = member.assigned_teacher_id();
        for class in &member.school_class_ids {
            let entry = state
                .locked_room
                .entry((*class, c.day, member.subject_id))
                .or_insert((room_id, 0));
            entry.1 += u32::from(n);
            // Item 66: lesson-group co-placement seeds the per-(class,
            // subject) teacher lock per member. Each lesson-group member
            // is a distinct subject (per-Jahrgang Religion trio: RK / RE
            // / ETH), so members do not collide on the same (class,
            // subject) key.
            state
                .class_subject_teacher
                .entry((*class, member.subject_id))
                .or_insert(member_teacher);
            *state
                .subject_hours_by_class_day
                .entry((*class, c.day, member.subject_id))
                .or_insert(0) += n;
        }
    }
    // Each lesson-group member is one lesson; bump per-class lesson count
    // by the number of members whose class set includes the class.
    for class in &class_set {
        let added = members
            .iter()
            .filter(|m| m.school_class_ids.contains(class))
            .count();
        let added_u8 = u8::try_from(added).unwrap_or(u8::MAX);
        *state
            .lessons_by_class_day
            .entry((*class, c.day))
            .or_insert(0) += added_u8;
    }
    state.search_score_slice = c.score;
    true
}

/// Walk `problem.pinned_placements`, drop any malformed entry (recording one
/// `PinnedConflict` violation per drop), and return:
/// 1. `seed_placements` to feed directly into `Solution.placements`,
/// 2. `pinned` lesson-id set for FFD to skip,
/// 3. `pin_violations` to merge into `Solution.violations`.
///
/// Reason codes: `unknown_lesson`, `unknown_time_block`, `unknown_room`,
/// `duplicate_slot`, `block_size_mismatch`. Bad pins do not abort the solve.
fn validate_pins(problem: &Problem) -> (Vec<Placement>, HashSet<LessonId>, Vec<Violation>) {
    let lessons_by_id: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let time_blocks_by_id: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
    let room_ids: HashSet<RoomId> = problem.rooms.iter().map(|r| r.id).collect();

    let mut violations: Vec<Violation> = Vec::new();
    let mut surviving_per_lesson: HashMap<LessonId, Vec<&crate::types::PinnedPlacement>> =
        HashMap::new();
    let mut taken_slots: HashSet<(TimeBlockId, RoomId)> = HashSet::new();

    let push_violation = |violations: &mut Vec<Violation>, lesson_id: LessonId, reason: &str| {
        violations.push(Violation {
            kind: ViolationKind::PinnedConflict,
            lesson_id,
            hour_index: 0,
            reason: Some(reason.to_string()),
        });
    };

    // First pass: per-entry validation (id existence + duplicate-slot).
    for pin in &problem.pinned_placements {
        if !lessons_by_id.contains_key(&pin.lesson_id) {
            push_violation(&mut violations, pin.lesson_id, "unknown_lesson");
            continue;
        }
        if !time_blocks_by_id.contains_key(&pin.time_block_id) {
            push_violation(&mut violations, pin.lesson_id, "unknown_time_block");
            continue;
        }
        if !room_ids.contains(&pin.room_id) {
            push_violation(&mut violations, pin.lesson_id, "unknown_room");
            continue;
        }
        if !taken_slots.insert((pin.time_block_id, pin.room_id)) {
            push_violation(&mut violations, pin.lesson_id, "duplicate_slot");
            continue;
        }
        surviving_per_lesson
            .entry(pin.lesson_id)
            .or_default()
            .push(pin);
    }

    // Second pass: per-lesson block-shape validation. The pin set for a
    // lesson must cover exactly `hours_per_week` hours, partitioned into
    // `hours_per_week / preferred_block_size` blocks. Each block is a run
    // of `preferred_block_size` time-blocks with consecutive `position`
    // values on the same `day_of_week`, sharing one `room_id`.
    let mut seed: Vec<Placement> = Vec::new();
    let mut pinned_set: HashSet<LessonId> = HashSet::new();
    for (lesson_id, pins) in surviving_per_lesson {
        let lesson = lessons_by_id[&lesson_id];
        let hours = lesson.hours_per_week as usize;
        let n = lesson.preferred_block_size as usize;

        // Full pinning required: a partial pin set leaves the lesson in
        // limbo (FFD would skip it because it's "pinned" but its remaining
        // hours never get placed). Reject partial pins as block_size_mismatch.
        if pins.len() != hours {
            push_violation(&mut violations, lesson_id, "block_size_mismatch");
            for pin in &pins {
                taken_slots.remove(&(pin.time_block_id, pin.room_id));
            }
            continue;
        }

        // Group pins by day_of_week, sort by position within each day.
        let mut by_day: HashMap<u8, Vec<&crate::types::PinnedPlacement>> = HashMap::new();
        for pin in &pins {
            let tb = time_blocks_by_id[&pin.time_block_id];
            by_day.entry(tb.day_of_week).or_default().push(pin);
        }
        for day_pins in by_day.values_mut() {
            day_pins.sort_by_key(|p| time_blocks_by_id[&p.time_block_id].position);
        }

        // Walk each day's pins in chunks of `n`. Each chunk must be
        // (a) consecutive positions and (b) same room_id throughout.
        let mut shape_ok = true;
        'outer: for day_pins in by_day.values() {
            if day_pins.len() % n != 0 {
                shape_ok = false;
                break;
            }
            for chunk in day_pins.chunks(n) {
                let first_tb = time_blocks_by_id[&chunk[0].time_block_id];
                let same_room = chunk.iter().all(|p| p.room_id == chunk[0].room_id);
                let consecutive = chunk.iter().enumerate().all(|(i, p)| {
                    let tb = time_blocks_by_id[&p.time_block_id];
                    tb.day_of_week == first_tb.day_of_week
                        && tb.position == first_tb.position + (i as u8)
                });
                if !same_room || !consecutive {
                    shape_ok = false;
                    break 'outer;
                }
            }
        }

        if !shape_ok {
            push_violation(&mut violations, lesson_id, "block_size_mismatch");
            // Surrender the slots so they can be reused; the lesson stays
            // un-pinned and FFD will place it normally.
            for pin in &pins {
                taken_slots.remove(&(pin.time_block_id, pin.room_id));
            }
            continue;
        }

        let pin_teacher = lesson.assigned_teacher_id();
        for pin in &pins {
            seed.push(Placement {
                lesson_id: pin.lesson_id,
                time_block_id: pin.time_block_id,
                room_id: pin.room_id,
                teacher_id: pin_teacher,
            });
        }
        pinned_set.insert(lesson_id);
    }

    (seed, pinned_set, violations)
}

/// Replay seeded placements into greedy bookkeeping so the FFD loop's
/// conflict checks treat pinned slots as occupied. Mirrors the bookkeeping
/// updates in `try_place_block`.
fn seed_greedy_state_from_pins(
    problem: &Problem,
    placements: &[Placement],
    state: &mut GreedyState,
) {
    let lessons_by_id: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let tb_by_id: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
    for pl in placements {
        let lesson = lessons_by_id[&pl.lesson_id];
        let tb = tb_by_id[&pl.time_block_id];
        state
            .used_teacher
            .insert((lesson.assigned_teacher_id(), tb.id));
        state.used_room.insert((pl.room_id, tb.id));
        for class in &lesson.school_class_ids {
            state.used_class.insert((*class, tb.id));
        }
        *state
            .hours_by_teacher
            .entry(lesson.assigned_teacher_id())
            .or_insert(0) += 1;
        for class in &lesson.school_class_ids {
            let part = state
                .class_positions
                .entry((*class, tb.day_of_week))
                .or_default();
            let ins = part.binary_search(&tb.position).unwrap_or_else(|i| i);
            // Lesson-group co-placement (e.g. the per-Jahrgang Religion
            // RK/RE/ETH trio) seeds the same (class, day, position) once per
            // member lesson. Dedup the insert so the partition stays unique
            // and `gap_count` (which assumes a sorted dedup'd slice) holds.
            if part.get(ins).copied() != Some(tb.position) {
                part.insert(ins, tb.position);
            }
            // Seed the same-room lock from authoritative pins so FFD's room
            // picker sees an existing room for this triple. One placement per
            // pinned hour increments the count by 1.
            let entry = state
                .locked_room
                .entry((*class, tb.day_of_week, lesson.subject_id))
                .or_insert((pl.room_id, 0));
            entry.1 += 1;
            // Item 66: seed the per-(class, subject) teacher lock from
            // pins so subsequent FFD placements of the same pair pick the
            // pinned teacher and the canonical score stays in lockstep
            // with `score::score_solution`.
            state
                .class_subject_teacher
                .entry((*class, lesson.subject_id))
                .or_insert(pl.teacher_id);
            // Per-day cap counters: each pinned row contributes 1 hour and
            // 1 lesson to the matching keys.
            *state
                .subject_hours_by_class_day
                .entry((*class, tb.day_of_week, lesson.subject_id))
                .or_insert(0) += 1;
            *state
                .lessons_by_class_day
                .entry((*class, tb.day_of_week))
                .or_insert(0) += 1;
        }
        let part = state
            .teacher_positions
            .entry((lesson.assigned_teacher_id(), tb.day_of_week))
            .or_default();
        let ins = part.binary_search(&tb.position).unwrap_or_else(|i| i);
        if part.get(ins).copied() != Some(tb.position) {
            part.insert(ins, tb.position);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
    use crate::types::{
        Lesson, PinnedPlacement, Problem, Room, RoomBlockedTime, RoomSubjectSuitability,
        SchoolClass, Subject, Teacher, TeacherBlockedTime, TeacherQualification, TimeBlock,
    };
    use uuid::Uuid;

    fn solve_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    /// Greedy-only invocation. Active default `solve()` adds a 200ms LAHC pass
    /// that this module's structural unit tests do not benefit from; using a
    /// fresh `SolveConfig` with `deadline: None` keeps these tests fast.
    fn greedy_solve(problem: &Problem) -> Result<Solution, Error> {
        solve_with_config(
            problem,
            &SolveConfig {
                weights: ConstraintWeights {
                    class_gap: 1,
                    teacher_gap: 1,
                    ..ConstraintWeights::default()
                },
                ..SolveConfig::default()
            },
        )
    }

    fn base_problem() -> Problem {
        Problem {
            time_blocks: vec![
                TimeBlock {
                    id: TimeBlockId(solve_uuid(10)),
                    day_of_week: 0,
                    position: 0,
                },
                TimeBlock {
                    id: TimeBlockId(solve_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                },
            ],
            teachers: vec![Teacher {
                id: TeacherId(solve_uuid(20)),
                max_hours_per_week: 10,
            }],
            rooms: vec![Room {
                id: RoomId(solve_uuid(30)),
            }],
            subjects: vec![Subject {
                id: SubjectId(solve_uuid(40)),
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
                max_hours_per_day: 8,
            }],
            school_classes: vec![SchoolClass {
                id: SchoolClassId(solve_uuid(50)),
                home_room_id: None,
                max_lessons_per_day: None,
                class_teacher_id: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(solve_uuid(60)),
                school_class_ids: vec![SchoolClassId(solve_uuid(50))],
                subject_id: SubjectId(solve_uuid(40)),
                teacher_candidates: vec![TeacherId(solve_uuid(20))],
                teacher_pin: Some(TeacherId(solve_uuid(20))),
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: TeacherId(solve_uuid(20)),
                subject_id: SubjectId(solve_uuid(40)),
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        }
    }

    #[test]
    fn single_hour_places_into_first_slot_and_room() {
        let s = greedy_solve(&base_problem()).unwrap();
        assert_eq!(s.placements.len(), 1);
        assert_eq!(s.placements[0].time_block_id, TimeBlockId(solve_uuid(10)));
        assert_eq!(s.placements[0].room_id, RoomId(solve_uuid(30)));
        assert!(s.violations.is_empty());
    }

    #[test]
    fn unqualified_teacher_emits_violation_and_skips_placement() {
        let mut p = base_problem();
        p.teacher_qualifications.clear();
        let s = greedy_solve(&p).unwrap();
        assert!(s.placements.is_empty());
        assert_eq!(s.violations.len(), 1);
        assert_eq!(s.violations[0].kind, ViolationKind::NoQualifiedTeacher);
    }

    #[test]
    fn teacher_blocked_time_prevents_placement_there() {
        let mut p = base_problem();
        p.teacher_blocked_times.push(TeacherBlockedTime {
            teacher_id: TeacherId(solve_uuid(20)),
            time_block_id: TimeBlockId(solve_uuid(10)),
        });
        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 1);
        assert_eq!(s.placements[0].time_block_id, TimeBlockId(solve_uuid(11)));
    }

    #[test]
    fn room_unsuitable_for_subject_is_skipped() {
        let mut p = base_problem();
        // Mark the sole room as suitable only for an unrelated subject, but add that
        // subject to keep validation happy. Room now suits no subject we place.
        p.subjects.push(Subject {
            id: SubjectId(solve_uuid(41)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        });
        p.room_subject_suitabilities.push(RoomSubjectSuitability {
            room_id: RoomId(solve_uuid(30)),
            subject_id: SubjectId(solve_uuid(41)),
        });
        let s = greedy_solve(&p).unwrap();
        assert!(s.placements.is_empty());
        assert_eq!(s.violations.len(), 1);
        assert_eq!(s.violations[0].kind, ViolationKind::NoSuitableRoom);
    }

    #[test]
    fn room_blocked_time_pushes_placement_to_next_slot() {
        let mut p = base_problem();
        p.room_blocked_times.push(RoomBlockedTime {
            room_id: RoomId(solve_uuid(30)),
            time_block_id: TimeBlockId(solve_uuid(10)),
        });
        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 1);
        assert_eq!(s.placements[0].time_block_id, TimeBlockId(solve_uuid(11)));
    }

    #[test]
    fn teacher_max_hours_cap_emits_teacher_over_capacity() {
        let mut p = base_problem();
        p.teachers[0].max_hours_per_week = 0;
        let s = greedy_solve(&p).unwrap();
        assert!(s.placements.is_empty());
        assert_eq!(s.violations.len(), 1);
        assert_eq!(s.violations[0].kind, ViolationKind::TeacherOverCapacity);
    }

    #[test]
    fn no_free_time_block_when_class_slots_are_filled_blocks_second_lesson() {
        let mut p = base_problem();
        // base_problem has 2 time_blocks. Add a second subject + lesson whose teacher is
        // qualified for both subjects, then block the teacher in time_block 1 to leave only
        // time_block 0 free; the first lesson takes block 0, the second cannot place.
        p.subjects.push(Subject {
            id: SubjectId(solve_uuid(41)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(solve_uuid(20)),
            subject_id: SubjectId(solve_uuid(41)),
        });
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(61)),
            school_class_ids: vec![SchoolClassId(solve_uuid(50))],
            subject_id: SubjectId(solve_uuid(41)),
            teacher_candidates: vec![TeacherId(solve_uuid(20))],
            teacher_pin: Some(TeacherId(solve_uuid(20))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        p.teacher_blocked_times.push(TeacherBlockedTime {
            teacher_id: TeacherId(solve_uuid(20)),
            time_block_id: TimeBlockId(solve_uuid(11)),
        });
        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 1);
        assert_eq!(s.violations.len(), 1);
        assert_eq!(s.violations[0].kind, ViolationKind::NoFreeTimeBlock);
    }

    #[test]
    fn two_lessons_in_same_class_do_not_double_book_slot() {
        let mut p = base_problem();
        p.subjects.push(Subject {
            id: SubjectId(solve_uuid(41)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(solve_uuid(20)),
            subject_id: SubjectId(solve_uuid(41)),
        });
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(61)),
            school_class_ids: vec![SchoolClassId(solve_uuid(50))],
            subject_id: SubjectId(solve_uuid(41)),
            teacher_candidates: vec![TeacherId(solve_uuid(20))],
            teacher_pin: Some(TeacherId(solve_uuid(20))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 2);
        assert_ne!(s.placements[0].time_block_id, s.placements[1].time_block_id);
    }

    #[test]
    fn two_rooms_used_in_parallel_for_different_classes_in_same_slot() {
        let mut p = base_problem();
        // second class with its own lesson
        p.school_classes.push(SchoolClass {
            id: SchoolClassId(solve_uuid(51)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        });
        p.teachers.push(Teacher {
            id: TeacherId(solve_uuid(21)),
            max_hours_per_week: 10,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(solve_uuid(21)),
            subject_id: SubjectId(solve_uuid(40)),
        });
        p.rooms.push(Room {
            id: RoomId(solve_uuid(31)),
        });
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(61)),
            school_class_ids: vec![SchoolClassId(solve_uuid(51))],
            subject_id: SubjectId(solve_uuid(40)),
            teacher_candidates: vec![TeacherId(solve_uuid(21))],
            teacher_pin: Some(TeacherId(solve_uuid(21))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 2);
        // both placements happened in the first slot but in different rooms
        assert_eq!(s.placements[0].time_block_id, s.placements[1].time_block_id);
        assert_ne!(s.placements[0].room_id, s.placements[1].room_id);
    }

    #[test]
    fn structural_error_returns_err_input() {
        let mut p = base_problem();
        p.time_blocks.clear();
        let err = greedy_solve(&p).unwrap_err();
        assert!(matches!(err, Error::Input(_)));
    }

    #[test]
    fn lowest_delta_picks_gap_minimising_slot_for_class() {
        // Lesson A is forced to position 3; lesson B (unconstrained second teacher)
        // should pick position 2 under lowest-delta to minimise class-gap, not
        // position 0 (which first-fit would pick).
        let mut p = base_problem();
        p.time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(solve_uuid(10)),
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(11)),
                day_of_week: 0,
                position: 1,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(12)),
                day_of_week: 0,
                position: 2,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(13)),
                day_of_week: 0,
                position: 3,
            },
        ];
        for tb_id in [10u8, 11, 12] {
            p.teacher_blocked_times.push(TeacherBlockedTime {
                teacher_id: TeacherId(solve_uuid(20)),
                time_block_id: TimeBlockId(solve_uuid(tb_id)),
            });
        }
        p.subjects.push(Subject {
            id: SubjectId(solve_uuid(41)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        });
        p.teachers.push(Teacher {
            id: TeacherId(solve_uuid(21)),
            max_hours_per_week: 10,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(solve_uuid(21)),
            subject_id: SubjectId(solve_uuid(41)),
        });
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(61)),
            school_class_ids: vec![SchoolClassId(solve_uuid(50))],
            subject_id: SubjectId(solve_uuid(41)),
            teacher_candidates: vec![TeacherId(solve_uuid(21))],
            teacher_pin: Some(TeacherId(solve_uuid(21))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });

        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 2);
        let lesson_a = s
            .placements
            .iter()
            .find(|x| x.lesson_id == LessonId(solve_uuid(60)))
            .unwrap();
        assert_eq!(lesson_a.time_block_id, TimeBlockId(solve_uuid(13)));
        let lesson_b = s
            .placements
            .iter()
            .find(|x| x.lesson_id == LessonId(solve_uuid(61)))
            .unwrap();
        assert_eq!(lesson_b.time_block_id, TimeBlockId(solve_uuid(12)));
        assert_eq!(s.soft_score, 0);
    }

    #[test]
    fn lowest_delta_picks_gap_minimising_slot_for_teacher() {
        // Two classes share teacher 20. Lesson A places at the lowest free slot;
        // lesson B (different class, same teacher) should pick the slot adjacent
        // to A under lowest-delta, not the lowest-index free slot.
        let mut p = base_problem();
        p.time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(solve_uuid(10)),
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(11)),
                day_of_week: 0,
                position: 1,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(12)),
                day_of_week: 0,
                position: 2,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(13)),
                day_of_week: 0,
                position: 3,
            },
        ];
        for tb_id in [10u8, 11] {
            p.teacher_blocked_times.push(TeacherBlockedTime {
                teacher_id: TeacherId(solve_uuid(20)),
                time_block_id: TimeBlockId(solve_uuid(tb_id)),
            });
        }
        p.school_classes.push(SchoolClass {
            id: SchoolClassId(solve_uuid(51)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        });
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(61)),
            school_class_ids: vec![SchoolClassId(solve_uuid(51))],
            subject_id: SubjectId(solve_uuid(40)),
            teacher_candidates: vec![TeacherId(solve_uuid(20))],
            teacher_pin: Some(TeacherId(solve_uuid(20))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        p.teachers[0].max_hours_per_week = 10;

        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 2);
        let lesson_a = s
            .placements
            .iter()
            .find(|x| x.lesson_id == LessonId(solve_uuid(60)))
            .unwrap();
        let lesson_b = s
            .placements
            .iter()
            .find(|x| x.lesson_id == LessonId(solve_uuid(61)))
            .unwrap();
        let pos_a = p
            .time_blocks
            .iter()
            .find(|tb| tb.id == lesson_a.time_block_id)
            .unwrap()
            .position;
        let pos_b = p
            .time_blocks
            .iter()
            .find(|tb| tb.id == lesson_b.time_block_id)
            .unwrap()
            .position;
        assert_eq!(
            pos_a.abs_diff(pos_b),
            1,
            "lessons should be adjacent under lowest-delta teacher-gap"
        );
        assert_eq!(s.soft_score, 0);
    }

    #[test]
    fn greedy_avoids_position_zero_for_avoid_first_subject_when_alternative_exists() {
        let mut p = base_problem();
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(solve_uuid(12)),
            day_of_week: 0,
            position: 2,
        });
        // Mark the only subject as avoid_first.
        p.subjects[0].avoid_first_period = 1;
        // Active default solve(p) uses weight 1 for each axis; lesson should
        // place at position 1 (the lowest-id non-zero alternative), not 0.
        let s = solve_with_config(
            &p,
            &SolveConfig {
                weights: ConstraintWeights {
                    class_gap: 1,
                    teacher_gap: 1,
                    prefer_early_period: 1,
                    avoid_first_period: 1,
                    prefer_home_room: 0,
                    avoid_last_period: 0,
                    prefer_late_period: 0,
                    class_day_balance: 0,
                    prefer_class_teacher: 0,
                },
                ..SolveConfig::default()
            },
        )
        .unwrap();
        assert_eq!(s.placements.len(), 1);
        assert_ne!(
            s.placements[0].time_block_id,
            TimeBlockId(solve_uuid(10)),
            "expected the avoid-first subject to skip position 0"
        );
    }

    #[test]
    fn greedy_avoids_max_position_for_avoid_last_subject_when_alternative_exists() {
        // Single class, single teacher, single subject flagged avoid_last_period.
        // Three time-blocks on day 0 (positions 0, 1, 2; max = 2). Two hours to
        // place; both placements must avoid position 2 (the day's max).
        let mut p = base_problem();
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(solve_uuid(12)),
            day_of_week: 0,
            position: 2,
        });
        p.subjects[0].avoid_last_period = 1;
        p.lessons[0].hours_per_week = 2;
        let s = solve_with_config(
            &p,
            &SolveConfig {
                weights: ConstraintWeights {
                    avoid_last_period: 1,
                    ..ConstraintWeights::default()
                },
                ..SolveConfig::default()
            },
        )
        .unwrap();
        assert_eq!(s.placements.len(), 2);
        let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
            p.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
        for placement in &s.placements {
            let tb = tb_lookup[&placement.time_block_id];
            assert_ne!(
                tb.position, 2,
                "greedy should avoid max-position TB; got {:?}",
                placement
            );
        }
    }

    #[test]
    fn block_lesson_places_n_consecutive_positions_in_one_room() {
        let mut p = base_problem();
        p.time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(solve_uuid(10)),
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(11)),
                day_of_week: 0,
                position: 1,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(12)),
                day_of_week: 0,
                position: 2,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(13)),
                day_of_week: 0,
                position: 3,
            },
        ];
        p.lessons[0].hours_per_week = 2;
        p.lessons[0].preferred_block_size = 2;

        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 2);
        let mut positions: Vec<u8> = s
            .placements
            .iter()
            .map(|pl| {
                p.time_blocks
                    .iter()
                    .find(|tb| tb.id == pl.time_block_id)
                    .unwrap()
                    .position
            })
            .collect();
        positions.sort_unstable();
        assert_eq!(
            positions[1] - positions[0],
            1,
            "positions must be consecutive"
        );
        assert_eq!(s.placements[0].room_id, s.placements[1].room_id);
    }

    #[test]
    fn block_lesson_does_not_cross_day_boundary() {
        let mut p = base_problem();
        p.time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(solve_uuid(10)),
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(11)),
                day_of_week: 0,
                position: 1,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(12)),
                day_of_week: 1,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(13)),
                day_of_week: 1,
                position: 1,
            },
        ];
        p.teacher_blocked_times.push(TeacherBlockedTime {
            teacher_id: TeacherId(solve_uuid(20)),
            time_block_id: TimeBlockId(solve_uuid(10)),
        });
        p.lessons[0].hours_per_week = 2;
        p.lessons[0].preferred_block_size = 2;

        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 2, "block must place on day 1");
        let days: Vec<u8> = s
            .placements
            .iter()
            .map(|pl| {
                p.time_blocks
                    .iter()
                    .find(|tb| tb.id == pl.time_block_id)
                    .unwrap()
                    .day_of_week
            })
            .collect();
        assert!(days.iter().all(|&d| d == days[0]), "all positions same day");
    }

    #[test]
    fn block_lesson_emits_one_violation_per_failed_block() {
        let mut p = base_problem();
        p.time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(solve_uuid(10)),
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(11)),
                day_of_week: 0,
                position: 1,
            },
        ];
        p.lessons[0].hours_per_week = 4;
        p.lessons[0].preferred_block_size = 2;
        p.teachers[0].max_hours_per_week = 4;

        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 2, "first block places");
        assert_eq!(
            s.violations.len(),
            1,
            "exactly one violation per failed block"
        );
        assert_eq!(
            s.violations[0].hour_index, 2,
            "second block starts at hour 2"
        );
    }

    #[test]
    fn multi_class_lesson_blocks_each_class_independently() {
        // Single time block, single room. The multi-class lesson covers
        // classes 50 and 51 simultaneously. A second lesson, single-class for
        // class 51, must fail to place because class 51's only candidate slot
        // is now booked by the multi-class lesson. The greedy must record
        // that booking against every member class, not just the first.
        let mut p = base_problem();
        p.time_blocks = vec![TimeBlock {
            id: TimeBlockId(solve_uuid(10)),
            day_of_week: 0,
            position: 0,
        }];
        p.school_classes.push(SchoolClass {
            id: SchoolClassId(solve_uuid(51)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        });
        p.teachers.push(Teacher {
            id: TeacherId(solve_uuid(21)),
            max_hours_per_week: 10,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(solve_uuid(21)),
            subject_id: SubjectId(solve_uuid(40)),
        });
        p.rooms.push(Room {
            id: RoomId(solve_uuid(31)),
        });
        // Make lesson 60 multi-class (classes 50 + 51).
        p.lessons[0].school_class_ids =
            vec![SchoolClassId(solve_uuid(50)), SchoolClassId(solve_uuid(51))];
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(61)),
            school_class_ids: vec![SchoolClassId(solve_uuid(51))],
            subject_id: SubjectId(solve_uuid(40)),
            teacher_candidates: vec![TeacherId(solve_uuid(21))],
            teacher_pin: Some(TeacherId(solve_uuid(21))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });

        let s = greedy_solve(&p).unwrap();
        let placed_60: Vec<_> = s
            .placements
            .iter()
            .filter(|pl| pl.lesson_id == LessonId(solve_uuid(60)))
            .collect();
        let placed_61: Vec<_> = s
            .placements
            .iter()
            .filter(|pl| pl.lesson_id == LessonId(solve_uuid(61)))
            .collect();
        assert_eq!(placed_60.len(), 1, "multi-class lesson places once");
        assert_eq!(
            placed_61.len(),
            0,
            "single-class lesson cannot share class 51's only slot"
        );
        assert_eq!(s.violations.len(), 1);
        assert_eq!(s.violations[0].lesson_id, LessonId(solve_uuid(61)));
        assert_eq!(s.violations[0].kind, ViolationKind::NoFreeTimeBlock);
    }

    fn two_member_group_base_problem() -> Problem {
        use crate::ids::LessonGroupId;
        let mut p = base_problem();
        p.time_blocks = vec![TimeBlock {
            id: TimeBlockId(solve_uuid(10)),
            day_of_week: 0,
            position: 0,
        }];
        p.school_classes.push(SchoolClass {
            id: SchoolClassId(solve_uuid(51)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        });
        p.teachers.push(Teacher {
            id: TeacherId(solve_uuid(21)),
            max_hours_per_week: 10,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(solve_uuid(21)),
            subject_id: SubjectId(solve_uuid(40)),
        });
        p.rooms.push(Room {
            id: RoomId(solve_uuid(31)),
        });
        let group_id = LessonGroupId(solve_uuid(70));
        p.lessons[0].lesson_group_id = Some(group_id);
        // Each group member serves a distinct class; co-placement at one TB
        // is the group invariant (typical Religion / Ethik split).
        p.lessons[0].school_class_ids = vec![SchoolClassId(solve_uuid(50))];
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(61)),
            school_class_ids: vec![SchoolClassId(solve_uuid(51))],
            subject_id: SubjectId(solve_uuid(40)),
            teacher_candidates: vec![TeacherId(solve_uuid(21))],
            teacher_pin: Some(TeacherId(solve_uuid(21))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: Some(group_id),
        });
        p
    }

    #[test]
    fn lesson_group_atomic_places_two_members_at_one_tb_with_distinct_rooms() {
        let p = two_member_group_base_problem();
        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 2, "both members place");
        assert_eq!(
            s.placements[0].time_block_id, s.placements[1].time_block_id,
            "members co-place at the same TB"
        );
        assert_ne!(
            s.placements[0].room_id, s.placements[1].room_id,
            "members occupy distinct rooms"
        );
        assert!(s.violations.is_empty());
    }

    #[test]
    fn lesson_group_emits_violation_per_member_when_no_slot_fits() {
        use crate::ids::LessonGroupId;
        let mut p = two_member_group_base_problem();
        p.rooms.truncate(1);
        let s = greedy_solve(&p).unwrap();
        assert!(
            s.placements.is_empty(),
            "no placements when group cannot atomically place"
        );
        let split: Vec<_> = s
            .violations
            .iter()
            .filter(|v| v.kind == ViolationKind::LessonGroupSplit)
            .collect();
        assert_eq!(split.len(), 2, "one LessonGroupSplit per member");
        assert_eq!(split[0].hour_index, 0);
        let lesson_ids: HashSet<LessonId> = split.iter().map(|v| v.lesson_id).collect();
        assert_eq!(lesson_ids.len(), 2);
        let _ = LessonGroupId(solve_uuid(70));
    }

    #[test]
    fn lesson_group_with_two_hours_places_into_two_distinct_tbs() {
        let mut p = two_member_group_base_problem();
        p.time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(solve_uuid(10)),
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(11)),
                day_of_week: 0,
                position: 1,
            },
        ];
        p.lessons[0].hours_per_week = 2;
        p.lessons[1].hours_per_week = 2;
        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 4);
        let tbs: HashSet<TimeBlockId> = s.placements.iter().map(|pl| pl.time_block_id).collect();
        assert_eq!(tbs.len(), 2, "group occupies two distinct TBs");
    }

    #[test]
    fn lesson_group_blocked_by_non_group_class_use() {
        use crate::ids::LessonGroupId;
        let mut p = two_member_group_base_problem();
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(solve_uuid(11)),
            day_of_week: 0,
            position: 1,
        });
        p.subjects.push(Subject {
            id: SubjectId(solve_uuid(41)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        });
        p.teachers.push(Teacher {
            id: TeacherId(solve_uuid(22)),
            max_hours_per_week: 10,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(solve_uuid(22)),
            subject_id: SubjectId(solve_uuid(41)),
        });
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(62)),
            school_class_ids: vec![SchoolClassId(solve_uuid(50))],
            subject_id: SubjectId(solve_uuid(41)),
            teacher_candidates: vec![TeacherId(solve_uuid(22))],
            teacher_pin: Some(TeacherId(solve_uuid(22))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        let s = greedy_solve(&p).unwrap();
        assert_eq!(s.placements.len(), 3, "all three lessons place");
        let group_tb = s.placements.iter().find(|pl| {
            pl.lesson_id == LessonId(solve_uuid(60)) || pl.lesson_id == LessonId(solve_uuid(61))
        });
        let non_group_tb = s
            .placements
            .iter()
            .find(|pl| pl.lesson_id == LessonId(solve_uuid(62)))
            .unwrap();
        assert_ne!(
            group_tb.unwrap().time_block_id,
            non_group_tb.time_block_id,
            "group does not collide with non-group class booking"
        );
        let _ = LessonGroupId(solve_uuid(70));
    }

    #[test]
    fn lesson_group_with_unqualified_member_does_not_place() {
        let mut p = two_member_group_base_problem();
        p.teacher_qualifications
            .retain(|q| q.teacher_id != TeacherId(solve_uuid(21)));
        let s = greedy_solve(&p).unwrap();
        let split: Vec<_> = s
            .violations
            .iter()
            .filter(|v| v.kind == ViolationKind::LessonGroupSplit)
            .collect();
        let unqual: Vec<_> = s
            .violations
            .iter()
            .filter(|v| v.kind == ViolationKind::NoQualifiedTeacher)
            .collect();
        assert_eq!(split.len(), 1, "qualified member gets LessonGroupSplit");
        assert_eq!(
            unqual.len(),
            1,
            "unqualified member keeps NoQualifiedTeacher"
        );
        assert_eq!(split[0].lesson_id, LessonId(solve_uuid(60)));
        assert_eq!(unqual[0].lesson_id, LessonId(solve_uuid(61)));
        assert!(s.placements.is_empty());
    }

    #[test]
    fn solve_skips_ffd_for_pinned_lesson() {
        // Two lessons share one teacher and class across two same-day TBs.
        // Lesson 0 is pinned to (TB0, Room0); lesson 1 is free and must
        // still place into the remaining slot.
        let mut p = base_problem();
        p.subjects.push(Subject {
            id: SubjectId(solve_uuid(41)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(solve_uuid(20)),
            subject_id: SubjectId(solve_uuid(41)),
        });
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(61)),
            school_class_ids: vec![SchoolClassId(solve_uuid(50))],
            subject_id: SubjectId(solve_uuid(41)),
            teacher_candidates: vec![TeacherId(solve_uuid(20))],
            teacher_pin: Some(TeacherId(solve_uuid(20))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        // Pin lesson 0 to TB1 (position 1) so without the pin FFD would pick
        // TB0 first; the pin must override that.
        p.pinned_placements.push(PinnedPlacement {
            lesson_id: LessonId(solve_uuid(60)),
            time_block_id: TimeBlockId(solve_uuid(11)),
            room_id: RoomId(solve_uuid(30)),
        });

        let solution = greedy_solve(&p).unwrap();

        let pinned_lesson_id = LessonId(solve_uuid(60));
        let pinned_in_solution = solution
            .placements
            .iter()
            .find(|pl| pl.lesson_id == pinned_lesson_id)
            .expect("pinned lesson must appear");
        assert_eq!(
            pinned_in_solution.time_block_id,
            TimeBlockId(solve_uuid(11))
        );
        assert_eq!(pinned_in_solution.room_id, RoomId(solve_uuid(30)));

        let free_lesson_id = LessonId(solve_uuid(61));
        assert!(
            solution
                .placements
                .iter()
                .any(|pl| pl.lesson_id == free_lesson_id),
            "free lesson must also be placed"
        );
        assert!(
            solution.violations.is_empty(),
            "no violations expected for valid pin"
        );
    }

    #[test]
    fn solve_emits_pinned_conflict_for_unknown_lesson_id() {
        let mut p = base_problem();
        let bogus_lesson_id = LessonId(Uuid::from_u128(0xDEAD_BEEF));
        p.pinned_placements.push(PinnedPlacement {
            lesson_id: bogus_lesson_id,
            time_block_id: TimeBlockId(solve_uuid(10)),
            room_id: RoomId(solve_uuid(30)),
        });

        let solution = greedy_solve(&p).unwrap();

        let pin_violations: Vec<_> = solution
            .violations
            .iter()
            .filter(|v| {
                matches!(v.kind, ViolationKind::PinnedConflict)
                    && v.lesson_id == bogus_lesson_id
                    && v.reason.as_deref() == Some("unknown_lesson")
            })
            .collect();
        assert_eq!(
            pin_violations.len(),
            1,
            "expected one PinnedConflict for unknown_lesson"
        );
        assert!(
            !solution.placements.is_empty(),
            "valid lesson is still placed"
        );
    }

    #[test]
    fn greedy_packs_prefer_early_subject_into_lower_positions_when_multiple_hours() {
        // Two-hour lesson of a prefer-early subject across a four-block day.
        // With prefer_early weight = 1, positions 0 and 1 should win over
        // 2 and 3 because their cumulative position cost (0+1=1) beats
        // (0+2=2) or any later combination.
        let mut p = base_problem();
        p.time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(solve_uuid(10)),
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(11)),
                day_of_week: 0,
                position: 1,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(12)),
                day_of_week: 0,
                position: 2,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(13)),
                day_of_week: 0,
                position: 3,
            },
        ];
        p.lessons[0].hours_per_week = 2;
        p.subjects[0].prefer_early_period = 1;
        let s = solve_with_config(
            &p,
            &SolveConfig {
                weights: ConstraintWeights {
                    class_gap: 1,
                    teacher_gap: 1,
                    prefer_early_period: 1,
                    avoid_first_period: 1,
                    prefer_home_room: 0,
                    avoid_last_period: 0,
                    prefer_late_period: 0,
                    class_day_balance: 0,
                    prefer_class_teacher: 0,
                },
                ..SolveConfig::default()
            },
        )
        .unwrap();
        assert_eq!(s.placements.len(), 2);
        let positions: Vec<u8> = s
            .placements
            .iter()
            .map(|pl| {
                p.time_blocks
                    .iter()
                    .find(|tb| tb.id == pl.time_block_id)
                    .unwrap()
                    .position
            })
            .collect();
        assert_eq!(
            positions
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>(),
            std::collections::HashSet::from([0u8, 1u8])
        );
    }

    #[test]
    fn solve_accepts_multi_block_pinned_lesson() {
        // One lesson with hours_per_week = 4, preferred_block_size = 2.
        // Two Doppelstunden: (Mon pos 0+1) and (Tue pos 0+1), same room.
        // The full 4-pin set must seed verbatim with no PinnedConflict.
        let mut p = base_problem();
        p.time_blocks = vec![
            TimeBlock {
                id: TimeBlockId(solve_uuid(10)),
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(11)),
                day_of_week: 0,
                position: 1,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(12)),
                day_of_week: 1,
                position: 0,
            },
            TimeBlock {
                id: TimeBlockId(solve_uuid(13)),
                day_of_week: 1,
                position: 1,
            },
        ];
        p.lessons = vec![Lesson {
            id: LessonId(solve_uuid(60)),
            school_class_ids: vec![SchoolClassId(solve_uuid(50))],
            subject_id: SubjectId(solve_uuid(40)),
            teacher_candidates: vec![TeacherId(solve_uuid(20))],
            teacher_pin: Some(TeacherId(solve_uuid(20))),
            hours_per_week: 4,
            preferred_block_size: 2,
            lesson_group_id: None,
        }];
        p.pinned_placements = vec![
            PinnedPlacement {
                lesson_id: LessonId(solve_uuid(60)),
                time_block_id: TimeBlockId(solve_uuid(10)),
                room_id: RoomId(solve_uuid(30)),
            },
            PinnedPlacement {
                lesson_id: LessonId(solve_uuid(60)),
                time_block_id: TimeBlockId(solve_uuid(11)),
                room_id: RoomId(solve_uuid(30)),
            },
            PinnedPlacement {
                lesson_id: LessonId(solve_uuid(60)),
                time_block_id: TimeBlockId(solve_uuid(12)),
                room_id: RoomId(solve_uuid(30)),
            },
            PinnedPlacement {
                lesson_id: LessonId(solve_uuid(60)),
                time_block_id: TimeBlockId(solve_uuid(13)),
                room_id: RoomId(solve_uuid(30)),
            },
        ];

        let solution = greedy_solve(&p).unwrap();

        assert!(
            solution.violations.is_empty(),
            "no violations expected for valid multi-block pin set; got {:?}",
            solution.violations
        );
        assert_eq!(
            solution.placements.len(),
            4,
            "all four pinned hours must seed as placements"
        );
        let lesson_id = LessonId(solve_uuid(60));
        let pinned_tb_ids: std::collections::HashSet<TimeBlockId> = p
            .pinned_placements
            .iter()
            .map(|pp| pp.time_block_id)
            .collect();
        for tb_id in pinned_tb_ids {
            assert!(
                solution.placements.iter().any(|pl| {
                    pl.lesson_id == lesson_id
                        && pl.time_block_id == tb_id
                        && pl.room_id == RoomId(solve_uuid(30))
                }),
                "pin for tb {tb_id:?} must appear verbatim in placements"
            );
        }
    }

    #[test]
    fn solve_with_config_stats_returns_zero_ttf_when_greedy_is_feasible() {
        // base_problem has one lesson, one feasible (tb, room) pair; FFD greedy
        // produces a feasible, soft-score-zero solution before LAHC runs. ttf
        // and tto must both report Some(0.0) without spending wall-clock in LAHC.
        let cfg = SolveConfig {
            weights: ConstraintWeights {
                class_gap: 1,
                teacher_gap: 1,
                ..ConstraintWeights::default()
            },
            ..SolveConfig::default()
        };
        let (sol, stats) = solve_with_config_stats(&base_problem(), &cfg).unwrap();
        assert!(sol.violations.is_empty());
        assert_eq!(sol.placements.len(), 1);
        assert_eq!(stats.time_to_first_feasible_ms, Some(0.0));
        assert_eq!(stats.time_to_optimal_ms, Some(0.0));
    }

    #[test]
    fn solve_with_config_stats_records_running_best_improvement() {
        // A multi-lesson, multi-day problem with a non-trivial deadline so
        // LAHC has both placements to shuffle and time to find improvements.
        // Whenever the run reaches feasibility (it does here on FFD greedy),
        // ttf must be set, tto must also be set, and tto >= ttf.
        let mut p = base_problem();
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(solve_uuid(12)),
            day_of_week: 1,
            position: 0,
        });
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(solve_uuid(13)),
            day_of_week: 1,
            position: 1,
        });
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(61)),
            school_class_ids: vec![SchoolClassId(solve_uuid(50))],
            subject_id: SubjectId(solve_uuid(40)),
            teacher_candidates: vec![TeacherId(solve_uuid(20))],
            teacher_pin: Some(TeacherId(solve_uuid(20))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        p.lessons.push(Lesson {
            id: LessonId(solve_uuid(62)),
            school_class_ids: vec![SchoolClassId(solve_uuid(50))],
            subject_id: SubjectId(solve_uuid(40)),
            teacher_candidates: vec![TeacherId(solve_uuid(20))],
            teacher_pin: Some(TeacherId(solve_uuid(20))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        let cfg = SolveConfig {
            weights: ConstraintWeights {
                class_gap: 1,
                teacher_gap: 1,
                ..ConstraintWeights::default()
            },
            seed: 7,
            deadline: Some(Duration::from_millis(20)),
            ..SolveConfig::default()
        };
        let (sol, stats) = solve_with_config_stats(&p, &cfg).unwrap();
        assert!(sol.violations.is_empty());
        assert!(stats.time_to_first_feasible_ms.is_some());
        let ttf = stats.time_to_first_feasible_ms.unwrap();
        let tto = stats
            .time_to_optimal_ms
            .expect("tto set when run is feasible");
        assert!(tto + 1e-6 >= ttf, "tto {tto} < ttf {ttf}");
    }

    #[test]
    fn solve_with_config_stats_returns_none_when_unfeasible() {
        // No qualified teacher means greedy emits a violation and never
        // reaches feasibility; LAHC has no placements to move. Both stats
        // fields must stay None.
        let mut p = base_problem();
        p.teacher_qualifications.clear();
        let cfg = SolveConfig {
            weights: ConstraintWeights {
                class_gap: 1,
                teacher_gap: 1,
                ..ConstraintWeights::default()
            },
            seed: 1,
            deadline: Some(Duration::from_millis(5)),
            ..SolveConfig::default()
        };
        let (sol, stats) = solve_with_config_stats(&p, &cfg).unwrap();
        assert!(sol.placements.is_empty());
        assert_eq!(sol.violations.len(), 1);
        assert_eq!(stats.time_to_first_feasible_ms, None);
        assert_eq!(stats.time_to_optimal_ms, None);
    }

    #[test]
    fn try_place_block_room_picker_minimises_home_room_penalty() {
        fn id(n: u8) -> Uuid {
            Uuid::from_bytes([n; 16])
        }

        // Two rooms: R0 (id=30) and R1 (id=31). R0 is the class's home room.
        // The lesson's class has home_room_id=R0. The picker MUST pick R0
        // because home_room_penalty(R0) = 0 vs home_room_penalty(R1) = w.
        let class_id = SchoolClassId(id(1));
        let teacher_id = TeacherId(id(2));
        let subject_id = SubjectId(id(3));
        let r0 = RoomId(id(30));
        let r1 = RoomId(id(31));
        let tb_id = TimeBlockId(id(40));
        let lesson_id = LessonId(id(50));

        let problem = Problem {
            time_blocks: vec![TimeBlock {
                id: tb_id,
                day_of_week: 0,
                position: 0,
            }],
            teachers: vec![Teacher {
                id: teacher_id,
                max_hours_per_week: 10,
            }],
            rooms: vec![Room { id: r0 }, Room { id: r1 }],
            school_classes: vec![SchoolClass {
                id: class_id,
                max_lessons_per_day: None,
                class_teacher_id: None,
                home_room_id: Some(r0),
            }],
            subjects: vec![Subject {
                id: subject_id,
                max_hours_per_day: 4,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_early_period: 0,
                prefer_late_period: 0,
            }],
            lessons: vec![Lesson {
                id: lesson_id,
                subject_id,
                teacher_candidates: vec![teacher_id],
                teacher_pin: Some(teacher_id),
                school_class_ids: vec![class_id],
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id,
                subject_id,
            }],
            room_subject_suitabilities: vec![],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            pinned_placements: vec![],
        };

        let idx = crate::index::Indexed::new(&problem);
        let mut state = GreedyState::new();
        let mut placements: Vec<Placement> = Vec::new();
        let teacher_max: HashMap<TeacherId, u8> = problem
            .teachers
            .iter()
            .map(|t| (t.id, t.max_hours_per_week))
            .collect();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let weights = ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_early_period: 0,
            avoid_first_period: 0,
            prefer_home_room: 100,
            avoid_last_period: 0,
            prefer_late_period: 0,
            class_day_balance: 0,
            prefer_class_teacher: 0,
        };
        let tb_order: Vec<usize> = vec![0];
        // room_order intentionally orders R1 first so the picker would pick
        // R1 under id-order iteration. The picker MUST pick R0 by penalty.
        let room_order: Vec<usize> = vec![1, 0];
        let max_position_per_day: HashMap<u8, u8> = HashMap::from([(0, 0)]);

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
        let placed = try_place_block(
            &problem,
            &problem.lessons[0],
            1,
            &idx,
            &teacher_max,
            &class_max_lessons_per_day,
            &weights,
            &home_room_lookup,
            &class_teacher_lookup,
            &subject_qualified_teachers,
            &mut state,
            &mut placements,
            &tb_order,
            &room_order,
            &max_position_per_day,
            1,
        );

        assert!(placed, "lesson should be placed");
        assert_eq!(placements.len(), 1);
        assert_eq!(
            placements[0].room_id, r0,
            "picker should choose home room (R0) over non-home room (R1) regardless of room_order"
        );
    }

    #[test]
    fn try_place_block_room_picker_falls_back_to_id_order_when_no_home_room_advantage() {
        fn fallback_id(n: u8) -> Uuid {
            Uuid::from_bytes([n; 16])
        }

        // Same setup as the home-room test, but the class has no home room.
        // With `room_order = [1, 0]` (R1 first, R0 second) and no home-room
        // advantage on either room, the picker's strict `<` does not flip
        // when R0's penalty 0 ties R1's penalty 0. The first feasible room
        // (R1) wins. This pins the determinism contract: callers wanting
        // lowest-id-wins-on-tie must pass `room_order` already sorted by id
        // (the canonical FFD greedy caller does).
        let class_id = SchoolClassId(fallback_id(1));
        let teacher_id = TeacherId(fallback_id(2));
        let subject_id = SubjectId(fallback_id(3));
        let r0 = RoomId(fallback_id(30));
        let r1 = RoomId(fallback_id(31));
        let tb_id = TimeBlockId(fallback_id(40));
        let lesson_id = LessonId(fallback_id(50));

        let problem = Problem {
            time_blocks: vec![TimeBlock {
                id: tb_id,
                day_of_week: 0,
                position: 0,
            }],
            teachers: vec![Teacher {
                id: teacher_id,
                max_hours_per_week: 10,
            }],
            rooms: vec![Room { id: r0 }, Room { id: r1 }],
            school_classes: vec![SchoolClass {
                id: class_id,
                max_lessons_per_day: None,
                class_teacher_id: None,
                home_room_id: None,
            }],
            subjects: vec![Subject {
                id: subject_id,
                max_hours_per_day: 4,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_early_period: 0,
                prefer_late_period: 0,
            }],
            lessons: vec![Lesson {
                id: lesson_id,
                subject_id,
                teacher_candidates: vec![teacher_id],
                teacher_pin: Some(teacher_id),
                school_class_ids: vec![class_id],
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id,
                subject_id,
            }],
            room_subject_suitabilities: vec![],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            pinned_placements: vec![],
        };

        let idx = crate::index::Indexed::new(&problem);
        let mut state = GreedyState::new();
        let mut placements: Vec<Placement> = Vec::new();
        let teacher_max: HashMap<TeacherId, u8> = problem
            .teachers
            .iter()
            .map(|t| (t.id, t.max_hours_per_week))
            .collect();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
            .school_classes
            .iter()
            .map(|c| (c.id, c.home_room_id))
            .collect();
        let weights = ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_early_period: 0,
            avoid_first_period: 0,
            prefer_home_room: 100,
            avoid_last_period: 0,
            prefer_late_period: 0,
            class_day_balance: 0,
            prefer_class_teacher: 0,
        };
        let tb_order: Vec<usize> = vec![0];
        // Walk R1 first to check the picker still considers R0 and prefers
        // it by tiebreak only when penalties differ. With no home-room
        // advantage, penalties tie at 0; strict `<` keeps R1.
        let room_order: Vec<usize> = vec![1, 0];
        let max_position_per_day: HashMap<u8, u8> = HashMap::from([(0, 0)]);

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
        let placed = try_place_block(
            &problem,
            &problem.lessons[0],
            1,
            &idx,
            &teacher_max,
            &class_max_lessons_per_day,
            &weights,
            &home_room_lookup,
            &class_teacher_lookup,
            &subject_qualified_teachers,
            &mut state,
            &mut placements,
            &tb_order,
            &room_order,
            &max_position_per_day,
            1,
        );

        assert!(placed, "lesson should be placed");
        assert_eq!(placements.len(), 1);
        assert_eq!(
            placements[0].room_id, r1,
            "with no home-room advantage and room_order=[R1, R0], picker keeps R1"
        );
    }

    /// FFD greedy's window picker must respond to `weights.class_day_balance`
    /// (item 54). Build a 1-class, 4-day fixture with three lessons pinned
    /// onto day 0 (positions 0, 1, 2) and one lesson pinned onto day 1
    /// (position 0). Pre-FFD per-class day counts are 3/1/0/0. FFD places one
    /// remaining lesson; eligible windows are day 1 pos 1, day 2 pos 0, and
    /// day 3 pos 0.
    ///
    /// Baseline (`class_day_balance == 0`): every weight is zero, so every
    /// candidate scores 0; the picker hits the early-exit at the first
    /// feasible window (`total_score == state.search_score_slice == 0`),
    /// landing lesson_e on day 1 (tb_d1_p1, the earliest non-busy tb in
    /// `tb_order`). Balance-on (`class_day_balance == 5`): no early-exit
    /// fires (totals are non-zero), so the picker walks every feasible
    /// window. Day 1 yields 3/2/0/0 with per-class L1 cost 5 (total = 25);
    /// day 2 yields 3/1/1/0 with cost 3 (total = 15); day 3 yields 3/1/0/1
    /// with cost 3 (total = 15). The picker's pruning rule fires only when
    /// the slice lower bound is at least the current best total; with
    /// `slice_score = 0` for every window the rule never fires. Under the
    /// strict-`<` cross-window comparison (item 60), the picker walks day 1
    /// first (best total = 25), then day 2 (15 < 25, becomes best), then
    /// day 3 (15 < 15 is false, day 2 keeps the lead). The contract the
    /// test pins is that balance-on lands deterministically on day 2: the
    /// FIRST-walked window of the cost-3 tier, mirroring the room-scan's
    /// "lowest-id wins on tie" rule via `tb_order`'s
    /// `(day_of_week, position, tb_id)` sort.
    #[test]
    fn try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight() {
        let class_id = SchoolClassId(solve_uuid(1));
        let teacher_id = TeacherId(solve_uuid(2));
        let room_id = RoomId(solve_uuid(3));
        let subject_id = SubjectId(solve_uuid(4));
        let lesson_a = LessonId(solve_uuid(10)); // pinned day 0 pos 0
        let lesson_b = LessonId(solve_uuid(11)); // pinned day 0 pos 1
        let lesson_c = LessonId(solve_uuid(12)); // pinned day 0 pos 2
        let lesson_d = LessonId(solve_uuid(13)); // pinned day 1 pos 0
        let lesson_e = LessonId(solve_uuid(14)); // FFD-placed
        let tb_d0_p0 = TimeBlockId(solve_uuid(20));
        let tb_d0_p1 = TimeBlockId(solve_uuid(21));
        let tb_d0_p2 = TimeBlockId(solve_uuid(22));
        let tb_d1_p0 = TimeBlockId(solve_uuid(23));
        let tb_d1_p1 = TimeBlockId(solve_uuid(24));
        let tb_d2 = TimeBlockId(solve_uuid(25));
        let tb_d3 = TimeBlockId(solve_uuid(26));
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
                    id: tb_d0_p2,
                    day_of_week: 0,
                    position: 2,
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
                TimeBlock {
                    id: tb_d2,
                    day_of_week: 2,
                    position: 0,
                },
                TimeBlock {
                    id: tb_d3,
                    day_of_week: 3,
                    position: 0,
                },
            ],
            teachers: vec![Teacher {
                id: teacher_id,
                max_hours_per_week: 30,
            }],
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
                class_teacher_id: None,
            }],
            lessons: vec![
                Lesson {
                    id: lesson_a,
                    school_class_ids: vec![class_id],
                    subject_id,
                    teacher_candidates: vec![teacher_id],
                    teacher_pin: Some(teacher_id),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_b,
                    school_class_ids: vec![class_id],
                    subject_id,
                    teacher_candidates: vec![teacher_id],
                    teacher_pin: Some(teacher_id),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_c,
                    school_class_ids: vec![class_id],
                    subject_id,
                    teacher_candidates: vec![teacher_id],
                    teacher_pin: Some(teacher_id),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_d,
                    school_class_ids: vec![class_id],
                    subject_id,
                    teacher_candidates: vec![teacher_id],
                    teacher_pin: Some(teacher_id),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
                Lesson {
                    id: lesson_e,
                    school_class_ids: vec![class_id],
                    subject_id,
                    teacher_candidates: vec![teacher_id],
                    teacher_pin: Some(teacher_id),
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                },
            ],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id,
                subject_id,
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![
                PinnedPlacement {
                    lesson_id: lesson_a,
                    time_block_id: tb_d0_p0,
                    room_id,
                },
                PinnedPlacement {
                    lesson_id: lesson_b,
                    time_block_id: tb_d0_p1,
                    room_id,
                },
                PinnedPlacement {
                    lesson_id: lesson_c,
                    time_block_id: tb_d0_p2,
                    room_id,
                },
                PinnedPlacement {
                    lesson_id: lesson_d,
                    time_block_id: tb_d1_p0,
                    room_id,
                },
            ],
        };
        // class_day_balance == 0 baseline: with every weight zero the picker
        // keeps the first feasible window (lowest-tb-id tiebreak). The first
        // non-busy window in tb_order is tb_d1_p1 on day 1.
        let cfg_balance_off = SolveConfig {
            weights: ConstraintWeights::default(),
            deadline: None,
            ..SolveConfig::default()
        };
        let sol_off = solve_with_config(&problem, &cfg_balance_off)
            .expect("baseline solve must succeed on the tiny fixture");
        let placement_off_e = sol_off
            .placements
            .iter()
            .find(|p| p.lesson_id == lesson_e)
            .expect("FFD must place lesson_e on the baseline solve");
        assert_eq!(
            placement_off_e.time_block_id, tb_d1_p1,
            "baseline (class_day_balance=0): lesson_e expected on day 1 (lowest-tb-id feasible)"
        );

        // class_day_balance > 0: pre-FFD counts are 3/1/0/0. Day 1 candidate
        // gives 3/2/0/0 (cost 5); day 2 candidate gives 3/1/1/0 (cost 3);
        // day 3 candidate gives 3/1/0/1 (cost 3). The picker walks every
        // candidate (no early-exit fires when totals are non-zero) and the
        // BlockCandidate assignment is unconditional; the last non-pruned
        // window wins, which is day 3.
        let cfg_balance_on = SolveConfig {
            weights: ConstraintWeights {
                class_day_balance: 5,
                ..ConstraintWeights::default()
            },
            deadline: None,
            ..SolveConfig::default()
        };
        let sol_on = solve_with_config(&problem, &cfg_balance_on)
            .expect("balance-on solve must succeed on the tiny fixture");
        let placement_on_e = sol_on
            .placements
            .iter()
            .find(|p| p.lesson_id == lesson_e)
            .expect("FFD must place lesson_e on the balance-on solve");
        assert_ne!(
            placement_on_e.time_block_id, tb_d1_p1,
            "balance-on (class_day_balance=5): picker must NOT pile lesson_e onto day 1; \
             expected the FIRST-walked L1-spread-minimising candidate (day 2 under strict `<`)"
        );
        // Verify the chosen day is exactly day 2 (the FIRST-walked window of
        // the cost-3 tier). Day 1 baseline yields 3/2/0/0 (cost 5); day 2
        // and day 3 both yield cost 3, but strict `<` resolves the tie to
        // day 2 because tb_order is sorted by (day, position, tb_id).
        let chosen_tb = placement_on_e.time_block_id;
        let chosen_day = problem
            .time_blocks
            .iter()
            .find(|tb| tb.id == chosen_tb)
            .expect("chosen tb must resolve")
            .day_of_week;
        assert_eq!(
            chosen_day, 2,
            "balance-on: picker must land lesson_e on day 2 (FIRST-walked of the tied cost-3 candidates under strict `<`); \
             actual day = {chosen_day}"
        );
    }

    #[test]
    fn try_place_block_picker_skips_busy_teacher_candidate() {
        // 2-teacher 1-subject 1-class problem. Pre-bind state.used_teacher
        // for (T1, TB_0) so the picker MUST choose T2 at TB_0 when called on
        // a lesson with teacher_candidates = [T1, T2].
        //
        // Item 74: under unpinned candidates, this is the exact mechanism
        // the FFD picker must guard. The line-839 check inside
        // `try_place_block` iterates each candidate and skips any that is
        // already busy at any TB in the window; this test pins that
        // contract so future picker edits cannot regress it.
        let day = 0u8;
        let tb_ids: Vec<TimeBlockId> = (0..5).map(|i| TimeBlockId(solve_uuid(100 + i))).collect();
        let time_blocks: Vec<TimeBlock> = tb_ids
            .iter()
            .enumerate()
            .map(|(i, id)| TimeBlock {
                id: *id,
                day_of_week: day,
                position: i as u8,
            })
            .collect();

        let t1 = TeacherId(solve_uuid(30));
        let t2 = TeacherId(solve_uuid(31));
        let teachers = vec![
            Teacher {
                id: t1,
                max_hours_per_week: 28,
            },
            Teacher {
                id: t2,
                max_hours_per_week: 28,
            },
        ];

        let room = Room {
            id: RoomId(solve_uuid(50)),
        };
        let subject = Subject {
            id: SubjectId(solve_uuid(60)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        let class = SchoolClass {
            id: SchoolClassId(solve_uuid(70)),
            home_room_id: Some(room.id),
            max_lessons_per_day: None,
            class_teacher_id: None,
        };

        let lesson = Lesson {
            id: LessonId(solve_uuid(200)),
            school_class_ids: vec![class.id],
            subject_id: subject.id,
            teacher_candidates: vec![t1, t2],
            teacher_pin: None,
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };

        let problem = Problem {
            time_blocks: time_blocks.clone(),
            teachers: teachers.clone(),
            rooms: vec![room.clone()],
            subjects: vec![subject.clone()],
            school_classes: vec![class.clone()],
            lessons: vec![lesson.clone()],
            teacher_qualifications: vec![
                TeacherQualification {
                    teacher_id: t1,
                    subject_id: subject.id,
                },
                TeacherQualification {
                    teacher_id: t2,
                    subject_id: subject.id,
                },
            ],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };

        let idx = crate::index::Indexed::new(&problem);
        let weights = crate::PRODUCTION_ACTIVE_WEIGHTS;
        let mut state = GreedyState::new();
        // Pre-bind T1 at TB_0 to simulate a previously placed lesson.
        state.used_teacher.insert((t1, tb_ids[0]));
        state.hours_by_teacher.insert(t1, 1);

        let teacher_max: HashMap<TeacherId, u8> = teachers
            .iter()
            .map(|t| (t.id, t.max_hours_per_week))
            .collect();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> =
            std::iter::once((class.id, Some(room.id))).collect();
        let class_teacher_lookup: HashMap<SchoolClassId, Option<TeacherId>> =
            std::iter::once((class.id, None)).collect();
        let mut subject_qualified_teachers: HashMap<SubjectId, HashSet<TeacherId>> = HashMap::new();
        subject_qualified_teachers
            .entry(subject.id)
            .or_default()
            .extend([t1, t2]);
        let tb_order: Vec<usize> = (0..time_blocks.len()).collect();
        let room_order: Vec<usize> = vec![0];
        let max_position_per_day: HashMap<u8, u8> = std::iter::once((day, 4)).collect();

        let mut placements: Vec<Placement> = Vec::new();
        let placed = try_place_block(
            &problem,
            &lesson,
            1,
            &idx,
            &teacher_max,
            &class_max_lessons_per_day,
            &weights,
            &home_room_lookup,
            &class_teacher_lookup,
            &subject_qualified_teachers,
            &mut state,
            &mut placements,
            &tb_order,
            &room_order,
            &max_position_per_day,
            1,
        );

        assert!(placed, "picker must place the lesson");
        assert_eq!(placements.len(), 1, "exactly one placement for n=1 lesson");
        // The picker is free to land on TB_0 with T2 (T1 busy, T2 free) or to
        // pick a later TB; either way it must NOT pick T1 at TB_0 because the
        // (T1, TB_0) pair is already in state.used_teacher.
        let placement = &placements[0];
        if placement.time_block_id == tb_ids[0] {
            assert_eq!(
                placement.teacher_id, t2,
                "picker landed at TB_0 but T1 is already busy there; \
                 picker chose teacher_id={:?}",
                placement.teacher_id,
            );
        }
        // In every case, the (teacher, tb) pair must not duplicate the
        // pre-existing (T1, TB_0) entry: that would be the double-book
        // item 74 is hunting.
        assert!(
            !(placement.teacher_id == t1 && placement.time_block_id == tb_ids[0]),
            "picker must not double-book (T1, TB_0); state already holds that pair"
        );
    }

    #[test]
    fn try_place_block_picker_does_not_pick_locked_teacher_when_busy() {
        // Same shape as `try_place_block_picker_skips_busy_teacher_candidate`,
        // but with state.class_subject_teacher pre-locked to T1 for the
        // (class, subject) pair AND state.used_teacher pre-bound to
        // (T1, TB_0). The lock collapses teacher_candidates to Singleton([T1]);
        // the picker must therefore skip TB_0 (T1 busy) and place at a later
        // TB rather than producing a double-book.
        let day = 0u8;
        let tb_ids: Vec<TimeBlockId> = (0..5).map(|i| TimeBlockId(solve_uuid(100 + i))).collect();
        let time_blocks: Vec<TimeBlock> = tb_ids
            .iter()
            .enumerate()
            .map(|(i, id)| TimeBlock {
                id: *id,
                day_of_week: day,
                position: i as u8,
            })
            .collect();

        let t1 = TeacherId(solve_uuid(30));
        let t2 = TeacherId(solve_uuid(31));
        let teachers = vec![
            Teacher {
                id: t1,
                max_hours_per_week: 28,
            },
            Teacher {
                id: t2,
                max_hours_per_week: 28,
            },
        ];

        let room = Room {
            id: RoomId(solve_uuid(50)),
        };
        let subject = Subject {
            id: SubjectId(solve_uuid(60)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        let class = SchoolClass {
            id: SchoolClassId(solve_uuid(70)),
            home_room_id: Some(room.id),
            max_lessons_per_day: None,
            class_teacher_id: None,
        };

        let lesson = Lesson {
            id: LessonId(solve_uuid(200)),
            school_class_ids: vec![class.id],
            subject_id: subject.id,
            teacher_candidates: vec![t1, t2],
            teacher_pin: None,
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };

        let problem = Problem {
            time_blocks: time_blocks.clone(),
            teachers: teachers.clone(),
            rooms: vec![room.clone()],
            subjects: vec![subject.clone()],
            school_classes: vec![class.clone()],
            lessons: vec![lesson.clone()],
            teacher_qualifications: vec![
                TeacherQualification {
                    teacher_id: t1,
                    subject_id: subject.id,
                },
                TeacherQualification {
                    teacher_id: t2,
                    subject_id: subject.id,
                },
            ],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };

        let idx = crate::index::Indexed::new(&problem);
        let weights = crate::PRODUCTION_ACTIVE_WEIGHTS;
        let mut state = GreedyState::new();
        // Lock the (class, subject) pair to T1 and pre-bind T1 at TB_0.
        state
            .class_subject_teacher
            .insert((class.id, subject.id), t1);
        state.used_teacher.insert((t1, tb_ids[0]));
        state.hours_by_teacher.insert(t1, 1);

        let teacher_max: HashMap<TeacherId, u8> = teachers
            .iter()
            .map(|t| (t.id, t.max_hours_per_week))
            .collect();
        let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = HashMap::new();
        let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> =
            std::iter::once((class.id, Some(room.id))).collect();
        let class_teacher_lookup: HashMap<SchoolClassId, Option<TeacherId>> =
            std::iter::once((class.id, None)).collect();
        let mut subject_qualified_teachers: HashMap<SubjectId, HashSet<TeacherId>> = HashMap::new();
        subject_qualified_teachers
            .entry(subject.id)
            .or_default()
            .extend([t1, t2]);
        let tb_order: Vec<usize> = (0..time_blocks.len()).collect();
        let room_order: Vec<usize> = vec![0];
        let max_position_per_day: HashMap<u8, u8> = std::iter::once((day, 4)).collect();

        let mut placements: Vec<Placement> = Vec::new();
        let placed = try_place_block(
            &problem,
            &lesson,
            1,
            &idx,
            &teacher_max,
            &class_max_lessons_per_day,
            &weights,
            &home_room_lookup,
            &class_teacher_lookup,
            &subject_qualified_teachers,
            &mut state,
            &mut placements,
            &tb_order,
            &room_order,
            &max_position_per_day,
            1,
        );

        assert!(placed, "picker must place the lesson at a non-TB_0 window");
        assert_eq!(placements.len(), 1, "exactly one placement for n=1 lesson");
        let placement = &placements[0];
        assert_eq!(
            placement.teacher_id, t1,
            "lock collapsed candidates to Singleton([T1]); picker must honour the lock"
        );
        assert_ne!(
            placement.time_block_id, tb_ids[0],
            "picker must skip TB_0 because the locked teacher T1 is already busy there; \
             producing a placement at TB_0 would be the item 74 double-book"
        );
    }
}
