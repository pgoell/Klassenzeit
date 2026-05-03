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
/// `placements` and the partition / used-* state in place. Caller-owned
/// `current_score` is updated to reflect the post-LAHC running total.
#[allow(clippy::too_many_arguments)] // Reason: internal helper; bundling args into a struct hurts clarity more than it helps
pub(crate) fn run(
    problem: &Problem,
    idx: &Indexed,
    config: &SolveConfig,
    placements: &mut [Placement],
    class_positions: &mut HashMap<(SchoolClassId, u8), Vec<u8>>,
    teacher_positions: &mut HashMap<(TeacherId, u8), Vec<u8>>,
    used_teacher: &mut HashSet<(TeacherId, TimeBlockId)>,
    used_class: &mut HashSet<(SchoolClassId, TimeBlockId)>,
    used_room: &mut HashSet<(RoomId, TimeBlockId)>,
    current_score: &mut u32,
) {
    let Some(deadline) = config.deadline else {
        return;
    };
    if placements.is_empty() {
        return;
    }
    let start = Instant::now();
    let mut rng = SmallRng::seed_from_u64(config.seed);
    let mut lahc_list = vec![*current_score; LAHC_LIST_LEN];
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

    let mut iter: u64 = 0;
    while iter < max_iter && start.elapsed() < deadline {
        // Always consume two random draws per iteration so the RNG sequence
        // is invariant across feasibility branches; this is what the
        // determinism property test relies on.
        let placement_idx = rng.random_range(0..placements.len());
        let new_tb_idx = rng.random_range(0..problem.time_blocks.len());

        if try_change_move(
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
            class_positions,
            teacher_positions,
            used_teacher,
            used_class,
            used_room,
            current_score,
            &lahc_list,
            iter,
        ) {
            // accepted; current_score already updated by try_change_move
        }

        iter += 1;
        lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = *current_score;
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
    class_positions: &mut HashMap<(SchoolClassId, u8), Vec<u8>>,
    teacher_positions: &mut HashMap<(TeacherId, u8), Vec<u8>>,
    used_teacher: &mut HashSet<(TeacherId, TimeBlockId)>,
    used_class: &mut HashSet<(SchoolClassId, TimeBlockId)>,
    used_room: &mut HashSet<(RoomId, TimeBlockId)>,
    current_score: &mut u32,
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
    let old_tb = tb_lookup[&p.time_block_id].clone();
    let new_tb = problem.time_blocks[new_tb_idx].clone();

    if new_tb.id == old_tb.id {
        return false;
    }

    let class_ids: &[SchoolClassId] = &lesson.school_class_ids;
    let teacher = lesson.teacher_id;

    if used_teacher.contains(&(teacher, new_tb.id)) {
        return false;
    }
    for class in class_ids {
        if used_class.contains(&(*class, new_tb.id)) {
            return false;
        }
    }
    if idx.teacher_blocked(teacher, new_tb.id) {
        return false;
    }

    let Some(new_room_id) = pick_room(
        problem,
        idx,
        lesson.subject_id,
        p.room_id,
        new_tb.id,
        used_room,
    ) else {
        return false;
    };

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
        class_positions,
        teacher_positions,
        weights,
    ) + subject_pref_delta;

    let new_score_signed = i64::from(*current_score) + delta;
    debug_assert!(
        new_score_signed >= 0,
        "running score must remain non-negative; current_score={} delta={}",
        *current_score,
        delta
    );
    let new_score = u32::try_from(new_score_signed.max(0)).unwrap_or(u32::MAX);

    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let accept = new_score <= *current_score || new_score <= prior;
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
        placements,
        class_positions,
        teacher_positions,
        used_teacher,
        used_class,
        used_room,
    );
    *current_score = new_score;
    true
}

/// Pick a room for the Change move's destination tb. Prefers reusing
/// `old_room_id`; falls back to the lowest-id hard-feasible room. Returns
/// `None` if no room is feasible.
fn pick_room(
    problem: &Problem,
    idx: &Indexed,
    subject_id: crate::ids::SubjectId,
    old_room_id: RoomId,
    new_tb_id: TimeBlockId,
    used_room: &HashSet<(RoomId, TimeBlockId)>,
) -> Option<RoomId> {
    let old_room_feasible = idx.room_suits_subject(old_room_id, subject_id)
        && !idx.room_blocked(old_room_id, new_tb_id)
        && !used_room.contains(&(old_room_id, new_tb_id));
    if old_room_feasible {
        return Some(old_room_id);
    }
    let mut best: Option<RoomId> = None;
    for room in &problem.rooms {
        if !idx.room_suits_subject(room.id, subject_id) {
            continue;
        }
        if idx.room_blocked(room.id, new_tb_id) {
            continue;
        }
        if used_room.contains(&(room.id, new_tb_id)) {
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
/// update the partition maps, swap the used-* set entries. For multi-class
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
    placements: &mut [Placement],
    class_positions: &mut HashMap<(SchoolClassId, u8), Vec<u8>>,
    teacher_positions: &mut HashMap<(TeacherId, u8), Vec<u8>>,
    used_teacher: &mut HashSet<(TeacherId, TimeBlockId)>,
    used_class: &mut HashSet<(SchoolClassId, TimeBlockId)>,
    used_room: &mut HashSet<(RoomId, TimeBlockId)>,
) {
    placements[placement_idx] = Placement {
        lesson_id: old_p.lesson_id,
        time_block_id: new_tb.id,
        room_id: new_room_id,
    };

    for class in class_ids {
        if let Some(part) = class_positions.get_mut(&(*class, old_tb.day_of_week)) {
            if let Ok(i) = part.binary_search(&old_tb.position) {
                part.remove(i);
            }
            if part.is_empty() {
                class_positions.remove(&(*class, old_tb.day_of_week));
            }
        }
        let part = class_positions
            .entry((*class, new_tb.day_of_week))
            .or_default();
        let ins = part.binary_search(&new_tb.position).unwrap_or_else(|i| i);
        if part.get(ins).copied() != Some(new_tb.position) {
            part.insert(ins, new_tb.position);
        }
    }

    if let Some(part) = teacher_positions.get_mut(&(teacher, old_tb.day_of_week)) {
        if let Ok(i) = part.binary_search(&old_tb.position) {
            part.remove(i);
        }
        if part.is_empty() {
            teacher_positions.remove(&(teacher, old_tb.day_of_week));
        }
    }
    let part = teacher_positions
        .entry((teacher, new_tb.day_of_week))
        .or_default();
    let ins = part.binary_search(&new_tb.position).unwrap_or_else(|i| i);
    if part.get(ins).copied() != Some(new_tb.position) {
        part.insert(ins, new_tb.position);
    }

    used_teacher.remove(&(teacher, old_tb.id));
    used_teacher.insert((teacher, new_tb.id));
    for class in class_ids {
        used_class.remove(&(*class, old_tb.id));
        used_class.insert((*class, new_tb.id));
    }
    used_room.remove(&(old_p.room_id, old_tb.id));
    used_room.insert((new_room_id, new_tb.id));
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
        let mut class_positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        class_positions.insert((class, 0), vec_part(&[0]));
        let mut teacher_positions: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
        teacher_positions.insert((teacher, 0), vec_part(&[0]));
        let mut used_teacher: HashSet<(TeacherId, TimeBlockId)> = HashSet::new();
        used_teacher.insert((teacher, old_tb.id));
        let mut used_class: HashSet<(SchoolClassId, TimeBlockId)> = HashSet::new();
        used_class.insert((class, old_tb.id));
        let mut used_room: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
        used_room.insert((old_room, old_tb.id));

        let old_tb_id = old_tb.id;
        let new_tb_id = new_tb.id;
        apply_change_move(
            0,
            &placements[0].clone(),
            old_tb,
            new_tb,
            new_room,
            &[class],
            teacher,
            &mut placements,
            &mut class_positions,
            &mut teacher_positions,
            &mut used_teacher,
            &mut used_class,
            &mut used_room,
        );

        assert_eq!(placements[0].time_block_id, new_tb_id);
        assert_eq!(placements[0].room_id, new_room);
        assert_eq!(class_positions.get(&(class, 0)), Some(&vec_part(&[1])));
        assert_eq!(teacher_positions.get(&(teacher, 0)), Some(&vec_part(&[1])));
        assert!(used_teacher.contains(&(teacher, new_tb_id)));
        assert!(!used_teacher.contains(&(teacher, old_tb_id)));
        assert!(used_class.contains(&(class, new_tb_id)));
        assert!(used_room.contains(&(new_room, new_tb_id)));
        assert!(!used_room.contains(&(old_room, old_tb_id)));
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
        let mut class_positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        class_positions.insert((class, 0), vec_part(&[0]));
        let mut teacher_positions: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
        teacher_positions.insert((teacher, 0), vec_part(&[0]));
        let mut used_teacher: HashSet<(TeacherId, TimeBlockId)> = HashSet::new();
        used_teacher.insert((teacher, tb_zero));
        let mut used_class: HashSet<(SchoolClassId, TimeBlockId)> = HashSet::new();
        used_class.insert((class, tb_zero));
        let mut used_room: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
        used_room.insert((room, tb_zero));
        let mut current_score: u32 = 1; // avoid_first penalty active at position 0

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
        };

        run(
            &problem,
            &idx,
            &config,
            &mut placements,
            &mut class_positions,
            &mut teacher_positions,
            &mut used_teacher,
            &mut used_class,
            &mut used_room,
            &mut current_score,
        );

        assert_eq!(placements.len(), 1);
        assert_eq!(
            placements[0].time_block_id, tb_one,
            "LAHC should move the avoid-first lesson off position 0"
        );
        assert_eq!(current_score, 0);
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
        let mut class_positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        class_positions.insert((class, 0), vec_part(&[0, 1]));
        let mut teacher_positions: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
        teacher_positions.insert((teacher, 0), vec_part(&[0, 1]));
        let mut used_teacher: HashSet<(TeacherId, TimeBlockId)> = HashSet::new();
        used_teacher.insert((teacher, tb_zero));
        used_teacher.insert((teacher, tb_one));
        let mut used_class: HashSet<(SchoolClassId, TimeBlockId)> = HashSet::new();
        used_class.insert((class, tb_zero));
        used_class.insert((class, tb_one));
        let mut used_room: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
        used_room.insert((room, tb_zero));
        used_room.insert((room, tb_one));
        let mut current_score: u32 = 1; // avoid_first penalty active at position 0

        let config = SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            seed: 0,
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2000),
        };

        run(
            &problem,
            &idx,
            &config,
            &mut placements,
            &mut class_positions,
            &mut teacher_positions,
            &mut used_teacher,
            &mut used_class,
            &mut used_room,
            &mut current_score,
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
        let mut class_positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
        class_positions.insert((class_a, 0), vec_part(&[0]));
        class_positions.insert((class_b, 0), vec_part(&[0]));
        let mut teacher_positions: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
        teacher_positions.insert((teacher_a, 0), vec_part(&[0]));
        teacher_positions.insert((teacher_b, 0), vec_part(&[0]));
        let mut used_teacher: HashSet<(TeacherId, TimeBlockId)> = HashSet::new();
        used_teacher.insert((teacher_a, tb_zero));
        used_teacher.insert((teacher_b, tb_zero));
        let mut used_class: HashSet<(SchoolClassId, TimeBlockId)> = HashSet::new();
        used_class.insert((class_a, tb_zero));
        used_class.insert((class_b, tb_zero));
        let mut used_room: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
        used_room.insert((room_a, tb_zero));
        used_room.insert((room_b, tb_zero));
        let mut current_score: u32 = 2;

        let config = SolveConfig {
            weights: ConstraintWeights {
                avoid_first_period: 1,
                ..ConstraintWeights::default()
            },
            seed: 0,
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2000),
        };

        run(
            &problem,
            &idx,
            &config,
            &mut placements,
            &mut class_positions,
            &mut teacher_positions,
            &mut used_teacher,
            &mut used_class,
            &mut used_room,
            &mut current_score,
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
            pick_room(&problem, &idx, subject, old_room, new_tb, &used),
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
            pick_room(&problem, &idx, subject, old_room, new_tb, &used),
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
            pick_room(&problem, &idx, subject, old_room, new_tb, &used),
            None
        );
    }
}
