//! Hofpause (Break) supervision rota: min-load greedy.
//!
//! For each [`TimeBlock`] with `kind == Break`, iterated in
//! `(day_of_week, position)` order for determinism, pick the eligible teacher
//! with the lowest running supervision count (ties broken by smallest
//! [`TeacherId`]). A teacher is eligible iff they are free at the break slot
//! AND have at least one placement at the immediately preceding or following
//! position on the same day.
//!
//! Two entry points share an inner helper:
//!
//! - [`compute_supervision_spread`] is the LAHC-time count-only path. It
//!   walks the same algorithm but skips assignment + violation allocation.
//!   Returns the spread (`max - min` over teachers with at least one
//!   supervision, zero when the supervising pool is empty).
//! - [`compute_supervision_full`] is the post-solve finalisation path. Returns
//!   the full `(assignments, violations, spread)` triple so the [`Solution`]
//!   build site can attach the rota and any `SupervisionGap` rows.
//!
//! Both paths produce identical spread values on the same fixture; the count-
//! only path exists so the hot-path scorer does not allocate a `Vec<_>`.
//!
//! [`Solution`]: crate::types::Solution

use std::collections::{HashMap, HashSet};

use crate::ids::{TeacherId, TimeBlockId};
use crate::types::{
    Placement, Problem, SupervisionAssignment, TimeBlockKind, Violation, ViolationKind,
};

/// Post-solve supervision pass that returns the full assignment vector,
/// supervision-gap violations, and the load spread.
///
/// See module-level docs for the algorithm. The `spread` value matches what
/// [`compute_supervision_spread`] returns on the same `(problem, placements)`.
pub fn compute_supervision_full(
    problem: &Problem,
    placements: &[Placement],
) -> (Vec<SupervisionAssignment>, Vec<Violation>, u32) {
    compute_inner(problem, placements, true)
}

/// Hot-path supervision spread used by `score_solution`. Skips assignment +
/// violation allocation; only the running count vector is built. Returns
/// `max - min` over teachers with at least one assignment, zero when the
/// supervising pool is empty.
pub fn compute_supervision_spread(problem: &Problem, placements: &[Placement]) -> u32 {
    compute_inner(problem, placements, false).2
}

/// Shared helper for the two entry points. When `collect` is `false`, the
/// returned assignment + violation vectors are empty regardless of outcome;
/// the spread is still computed.
fn compute_inner(
    problem: &Problem,
    placements: &[Placement],
    collect: bool,
) -> (Vec<SupervisionAssignment>, Vec<Violation>, u32) {
    // Index time blocks by (day, position) so we can look up adjacency
    // positions quickly. Multiple blocks at the same (day, position) are
    // unusual but we keep the indirection cheap.
    let mut tb_position: HashMap<TimeBlockId, (u8, u8)> = HashMap::new();
    for tb in &problem.time_blocks {
        tb_position.insert(tb.id, (tb.day_of_week, tb.position));
    }

    // Occupancy: which teachers are placed at (day, position). One placement
    // can have one teacher; multiple placements per slot (different classes
    // / lesson groups) accumulate. Used both for "free at the break slot"
    // and for "has lesson at adjacent position".
    let mut occupancy: HashMap<(u8, u8), HashSet<TeacherId>> = HashMap::new();
    for pl in placements {
        if let Some(&(day, position)) = tb_position.get(&pl.time_block_id) {
            occupancy
                .entry((day, position))
                .or_default()
                .insert(pl.teacher_id);
        }
    }

    // Collect break slots in deterministic (day, position) order.
    let mut break_slots: Vec<(u8, u8, TimeBlockId)> = problem
        .time_blocks
        .iter()
        .filter(|tb| tb.kind == TimeBlockKind::Break)
        .map(|tb| (tb.day_of_week, tb.position, tb.id))
        .collect();
    break_slots.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));

    // Running supervision count per teacher. Only teachers that get assigned
    // appear in the map; absent => count zero.
    let mut counts: HashMap<TeacherId, u32> = HashMap::new();
    let mut assignments: Vec<SupervisionAssignment> = Vec::new();
    let mut violations: Vec<Violation> = Vec::new();

    // Pre-sort teachers by id so the eligibility scan order is deterministic
    // when we compute argmin tiebreaks.
    let mut teachers_sorted: Vec<TeacherId> = problem.teachers.iter().map(|t| t.id).collect();
    teachers_sorted.sort();

    for (day, position, tb_id) in &break_slots {
        let busy_here = occupancy.get(&(*day, *position));
        // Adjacency lookups: position - 1 (if any) and position + 1.
        let prev = position
            .checked_sub(1)
            .and_then(|p| occupancy.get(&(*day, p)));
        let next = position
            .checked_add(1)
            .and_then(|p| occupancy.get(&(*day, p)));

        // Build the eligible set: free at slot AND adjacent same-day lesson.
        let mut eligible: Vec<TeacherId> = Vec::new();
        for t in &teachers_sorted {
            if busy_here.is_some_and(|set| set.contains(t)) {
                continue;
            }
            let adjacent =
                prev.is_some_and(|set| set.contains(t)) || next.is_some_and(|set| set.contains(t));
            if adjacent {
                eligible.push(*t);
            }
        }

        if eligible.is_empty() {
            if collect {
                violations.push(Violation {
                    kind: ViolationKind::SupervisionGap,
                    // No lesson is responsible for a supervision gap; the
                    // sentinel `Uuid::nil()` keeps the wire shape consistent
                    // with other variants and is matched by the reason
                    // string for diagnostics.
                    lesson_id: crate::ids::LessonId(uuid::Uuid::nil()),
                    hour_index: 0,
                    reason: Some(format!("day={day} position={position} candidates=0")),
                });
            }
            continue;
        }

        // Min-load greedy: pick the eligible teacher with the lowest running
        // count; ties resolved by smallest TeacherId. `teachers_sorted` is
        // ascending by id, so iterating it preserves the tiebreak.
        let chosen = eligible
            .iter()
            .copied()
            .min_by_key(|t| (*counts.get(t).unwrap_or(&0), *t))
            .expect("eligible non-empty");
        *counts.entry(chosen).or_insert(0) += 1;
        if collect {
            assignments.push(SupervisionAssignment {
                time_block_id: *tb_id,
                teacher_id: chosen,
            });
        }
    }

    // Spread over the supervising pool (count > 0). Empty pool => 0.
    let supervising_counts: Vec<u32> = counts.values().copied().filter(|c| *c > 0).collect();
    let spread = if supervising_counts.is_empty() {
        0
    } else {
        let max = *supervising_counts.iter().max().unwrap();
        let min = *supervising_counts.iter().min().unwrap();
        max - min
    };

    (assignments, violations, spread)
}
