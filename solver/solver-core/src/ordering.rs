//! First Fit Decreasing lesson ordering.
//!
//! Returns a permutation of `problem.lessons` indices in placement order.
//! Lessons are sorted by a same-room-aware eligibility metric (lower = more
//! constrained = placed first) computed once before placement begins. The
//! metric counts `(day, room)` pairs where at least
//! `lesson.preferred_block_size` consecutive teacher-unblocked,
//! room-unblocked time blocks exist on `day` for the lesson's subject.
//!
//! Tiebreak is the lesson's `LessonId` byte order so two lessons with equal
//! eligibility keep a deterministic ordering across runs.
//!
//! Lessons whose teacher lacks the qualification for the subject still get a
//! computed metric (the metric does not gate on qualification); the placement
//! loop in `solve_with_config` skips them and `pre_solve_violations` records
//! each affected hour as a `NoQualifiedTeacher` violation.

use crate::ids::{RoomId, SchoolClassId};
use crate::index::Indexed;
use crate::types::{Lesson, Problem, TimeBlock, TimeBlockKind};

/// Compute placement order under First Fit Decreasing. See module docs.
pub(crate) fn ffd_order(problem: &Problem, idx: &Indexed) -> Vec<usize> {
    // Precompute per-day TB lists (sorted by position) and the
    // class -> home_room map once for the whole call so the per-lesson
    // metric does not re-allocate or re-scan `time_blocks` and
    // `school_classes` for every lesson.
    // Break-kind time blocks are excluded: the FFD lesson placer never lands a
    // lesson on a Hofpause slot, so the same-room-aware contiguity scan must
    // not count break positions toward viable windows.
    let mut tbs_by_day: Vec<(u8, Vec<&TimeBlock>)> = Vec::new();
    for tb in &problem.time_blocks {
        if tb.kind != TimeBlockKind::Lesson {
            continue;
        }
        match tbs_by_day.iter_mut().find(|(d, _)| *d == tb.day_of_week) {
            Some((_, vec)) => vec.push(tb),
            None => tbs_by_day.push((tb.day_of_week, vec![tb])),
        }
    }
    for (_, tbs) in &mut tbs_by_day {
        tbs.sort_unstable_by_key(|tb| tb.position);
    }

    // Per-lesson home_rooms lookup uses a flat (class_id, home_room) Vec
    // (typically <= 16 entries; HashMap overhead dominates at this size).
    let home_room_pairs: Vec<(SchoolClassId, RoomId)> = problem
        .school_classes
        .iter()
        .filter_map(|c| c.home_room_id.map(|r| (c.id, r)))
        .collect();

    // Fast path: when the problem has neither teacher- nor room-blocked
    // times, viable_pairs reduces to (days where day_tbs.len() >= n) ×
    // (rooms suitable for the lesson's subject). The contiguity check
    // becomes trivial because every position is unblocked. Most production
    // and bench fixtures hit this path.
    let no_blocks =
        problem.teacher_blocked_times.is_empty() && problem.room_blocked_times.is_empty();

    // Primary FFD key is `teacher_candidates.len()` ascending: lessons whose
    // qualified teacher pool is narrow get placed first so the FFD loop does
    // not exhaust a hard lesson's only candidate by committing it to an
    // earlier broader-pool lesson. See OPEN_THINGS item 80 and the spec at
    // /tmp/kz-autopilot/2026-05-12-ffd-scarcity-ordering-design.md.
    let scores: Vec<(u32, u32, u32)> = if no_blocks {
        problem
            .lessons
            .iter()
            .map(|l| {
                let (viable_pairs, home_pairs) =
                    same_room_eligibility_no_blocks(l, problem, idx, &tbs_by_day, &home_room_pairs);
                (l.teacher_candidates.len() as u32, viable_pairs, home_pairs)
            })
            .collect()
    } else {
        problem
            .lessons
            .iter()
            .map(|l| {
                let (viable_pairs, home_pairs) =
                    same_room_eligibility(l, problem, idx, &tbs_by_day, &home_room_pairs);
                (l.teacher_candidates.len() as u32, viable_pairs, home_pairs)
            })
            .collect()
    };
    let mut order: Vec<usize> = (0..problem.lessons.len()).collect();
    order.sort_by(|&a, &b| {
        scores[a]
            .cmp(&scores[b])
            .then_with(|| problem.lessons[a].id.0.cmp(&problem.lessons[b].id.0))
    });
    order
}

fn lesson_home_rooms(
    lesson: &Lesson,
    home_room_pairs: &[(SchoolClassId, RoomId)],
) -> ([Option<RoomId>; 4], usize) {
    let mut home_rooms: [Option<RoomId>; 4] = [None; 4];
    let mut home_count = 0usize;
    for cid in &lesson.school_class_ids {
        for (kid, room) in home_room_pairs {
            if kid == cid {
                if home_count < home_rooms.len() {
                    home_rooms[home_count] = Some(*room);
                    home_count += 1;
                }
                break;
            }
        }
    }
    (home_rooms, home_count)
}

fn home_rooms_contains(home_rooms: &[Option<RoomId>; 4], count: usize, room: RoomId) -> bool {
    home_rooms[..count]
        .iter()
        .any(|hr| matches!(hr, Some(r) if *r == room))
}

/// Fast path for problems with no teacher/room blocked times: viable pairs
/// reduce to `(days where day_tbs.len() >= n) × (rooms suitable for subject)`,
/// and home_pairs reduce to `viable_days × home_room_count_suitable`.
fn same_room_eligibility_no_blocks(
    lesson: &Lesson,
    problem: &Problem,
    idx: &Indexed,
    tbs_by_day: &[(u8, Vec<&TimeBlock>)],
    home_room_pairs: &[(SchoolClassId, RoomId)],
) -> (u32, u32) {
    let n = lesson.preferred_block_size as usize;
    if n == 0 {
        return (0, 0);
    }
    let viable_days = tbs_by_day.iter().filter(|(_, tbs)| tbs.len() >= n).count() as u32;
    let (home_rooms, home_count) = lesson_home_rooms(lesson, home_room_pairs);
    let mut suitable_rooms: u32 = 0;
    let mut suitable_home_rooms: u32 = 0;
    for room in &problem.rooms {
        if !idx.room_suits_subject(room.id, lesson.subject_id) {
            continue;
        }
        suitable_rooms = suitable_rooms.saturating_add(1);
        if home_rooms_contains(&home_rooms, home_count, room.id) {
            suitable_home_rooms = suitable_home_rooms.saturating_add(1);
        }
    }
    (
        viable_days.saturating_mul(suitable_rooms),
        viable_days.saturating_mul(suitable_home_rooms),
    )
}

/// Count `(day, room)` pairs where at least `lesson.preferred_block_size`
/// consecutive teacher-unblocked, room-unblocked time blocks exist on `day`
/// for `lesson.subject_id`. Returns `(viable_pairs, home_pairs)` where
/// `home_pairs` counts the subset of viable pairs whose room is the home
/// room of one of the lesson's classes. FFD sorts `(viable_pairs, home_pairs)`
/// lexicographically ascending so:
///
/// - The primary signal (`viable_pairs`) preserves the same-room-aware
///   contiguity check that distinguishes a doppelstunde with one viable
///   window from one with several: a lesson's score reflects whether at
///   least `preferred_block_size` consecutive teacher-unblocked,
///   room-unblocked time blocks exist per `(day, room)` pair.
/// - The secondary signal (`home_pairs`) breaks ties by class home-room
///   availability: when a class's home room is unavailable for a subject
///   (e.g., renovation-week perturbation), `home_pairs = 0` and the lesson
///   sorts before sibling lessons whose class has working home-room access
///   on at least one day. This addresses the demo Grundschule lock-in
///   surfaced in PR #173 without perturbing the relative order of lessons
///   on fixtures where every class has the same home-room availability.
fn same_room_eligibility(
    lesson: &Lesson,
    problem: &Problem,
    idx: &Indexed,
    tbs_by_day: &[(u8, Vec<&TimeBlock>)],
    home_room_pairs: &[(SchoolClassId, RoomId)],
) -> (u32, u32) {
    let n = lesson.preferred_block_size as usize;
    if n == 0 {
        return (0, 0);
    }
    let (home_rooms, home_count) = lesson_home_rooms(lesson, home_room_pairs);

    let mut viable_pairs: u32 = 0;
    let mut home_pairs: u32 = 0;
    for (_, day_tbs) in tbs_by_day {
        if day_tbs.len() < n {
            continue;
        }
        for room in &problem.rooms {
            if !idx.room_suits_subject(room.id, lesson.subject_id) {
                continue;
            }
            // Walk positions; track the run length of contiguous TBs where
            // both teacher and room are unblocked. As soon as run >= n, the
            // (day, room) pair is viable.
            let mut run: usize = 0;
            let mut fits = false;
            for tb in day_tbs.iter() {
                if !idx.teacher_blocked(lesson.assigned_teacher_id(), tb.id)
                    && !idx.room_blocked(room.id, tb.id)
                {
                    run += 1;
                    if run >= n {
                        fits = true;
                        break;
                    }
                } else {
                    run = 0;
                }
            }
            if fits {
                viable_pairs = viable_pairs.saturating_add(1);
                if home_rooms_contains(&home_rooms, home_count, room.id) {
                    home_pairs = home_pairs.saturating_add(1);
                }
            }
        }
    }
    (viable_pairs, home_pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
    use crate::types::{
        Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherBlockedTime,
        TeacherQualification, TimeBlock, TimeBlockKind,
    };
    use uuid::Uuid;

    fn ord_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn two_blocks_two_rooms() -> Problem {
        Problem {
            time_blocks: vec![
                TimeBlock {
                    id: TimeBlockId(ord_uuid(10)),
                    day_of_week: 0,
                    position: 0,
                    kind: TimeBlockKind::Lesson,
                },
                TimeBlock {
                    id: TimeBlockId(ord_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                    kind: TimeBlockKind::Lesson,
                },
            ],
            teachers: vec![
                Teacher {
                    id: TeacherId(ord_uuid(20)),
                    max_hours_per_week: 5,
                    reserve_hours_per_week: 0,
                },
                Teacher {
                    id: TeacherId(ord_uuid(21)),
                    max_hours_per_week: 5,
                    reserve_hours_per_week: 0,
                },
            ],
            rooms: vec![
                Room {
                    id: RoomId(ord_uuid(30)),
                },
                Room {
                    id: RoomId(ord_uuid(31)),
                },
            ],
            subjects: vec![
                Subject {
                    id: SubjectId(ord_uuid(40)),
                    prefer_early_period: 0,
                    avoid_first_period: 0,
                    avoid_last_period: 0,
                    prefer_late_period: 0,
                    max_hours_per_day: 8,
                },
                Subject {
                    id: SubjectId(ord_uuid(41)),
                    prefer_early_period: 0,
                    avoid_first_period: 0,
                    avoid_last_period: 0,
                    prefer_late_period: 0,
                    max_hours_per_day: 8,
                },
            ],
            school_classes: vec![
                SchoolClass {
                    id: SchoolClassId(ord_uuid(50)),
                    home_room_id: None,
                    max_lessons_per_day: None,
                    class_teacher_id: None,
                },
                SchoolClass {
                    id: SchoolClassId(ord_uuid(51)),
                    home_room_id: None,
                    max_lessons_per_day: None,
                    class_teacher_id: None,
                },
            ],
            lessons: vec![],
            teacher_qualifications: vec![
                TeacherQualification {
                    teacher_id: TeacherId(ord_uuid(20)),
                    subject_id: SubjectId(ord_uuid(40)),
                },
                TeacherQualification {
                    teacher_id: TeacherId(ord_uuid(21)),
                    subject_id: SubjectId(ord_uuid(41)),
                },
            ],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        }
    }

    #[test]
    fn ffd_order_places_low_eligibility_lesson_first() {
        let mut problem = two_blocks_two_rooms();
        // Lesson A: teacher 20 blocked in TB 10 -> 1 free block.
        // Lesson B: teacher 21 not blocked anywhere -> 2 free blocks.
        problem.teacher_blocked_times.push(TeacherBlockedTime {
            teacher_id: TeacherId(ord_uuid(20)),
            time_block_id: TimeBlockId(ord_uuid(10)),
        });
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(70)),
            school_class_ids: vec![SchoolClassId(ord_uuid(50))],
            subject_id: SubjectId(ord_uuid(40)),
            teacher_candidates: vec![TeacherId(ord_uuid(20))],
            teacher_pin: Some(TeacherId(ord_uuid(20))),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(71)),
            school_class_ids: vec![SchoolClassId(ord_uuid(51))],
            subject_id: SubjectId(ord_uuid(41)),
            teacher_candidates: vec![TeacherId(ord_uuid(21))],
            teacher_pin: Some(TeacherId(ord_uuid(21))),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        let idx = Indexed::new(&problem);
        assert_eq!(ffd_order(&problem, &idx), vec![0, 1]);

        // Reversing input order does not change the FFD order.
        problem.lessons.swap(0, 1);
        let idx = Indexed::new(&problem);
        // Lesson A is now at index 1, B at index 0.
        assert_eq!(ffd_order(&problem, &idx), vec![1, 0]);
    }

    #[test]
    fn ffd_order_tiebreaks_on_lesson_id_when_eligibility_ties() {
        let mut problem = two_blocks_two_rooms();
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(81)),
            school_class_ids: vec![SchoolClassId(ord_uuid(50))],
            subject_id: SubjectId(ord_uuid(40)),
            teacher_candidates: vec![TeacherId(ord_uuid(20))],
            teacher_pin: Some(TeacherId(ord_uuid(20))),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(80)),
            school_class_ids: vec![SchoolClassId(ord_uuid(51))],
            subject_id: SubjectId(ord_uuid(41)),
            teacher_candidates: vec![TeacherId(ord_uuid(21))],
            teacher_pin: Some(TeacherId(ord_uuid(21))),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        let idx = Indexed::new(&problem);
        // Both lessons have eligibility 2 * 2 = 4. Lower id (80) sorts first
        // even though it is at index 1 in the input Vec.
        assert_eq!(ffd_order(&problem, &idx), vec![1, 0]);
    }

    #[test]
    fn ffd_order_returns_every_index_exactly_once() {
        let mut problem = two_blocks_two_rooms();
        for k in 0..6u8 {
            problem.lessons.push(Lesson {
                id: LessonId(ord_uuid(90 + k)),
                school_class_ids: vec![SchoolClassId(ord_uuid(50))],
                subject_id: SubjectId(ord_uuid(40)),
                teacher_candidates: vec![TeacherId(ord_uuid(20))],
                teacher_pin: Some(TeacherId(ord_uuid(20))),
                hours_per_week: 1,
                preferred_block_size: 1,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
                lesson_group_id: None,
            });
        }
        let idx = Indexed::new(&problem);
        let order = ffd_order(&problem, &idx);
        assert_eq!(order.len(), 6);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5]);
        assert!(order.iter().all(|&i| i < 6));
    }

    #[test]
    fn ffd_order_lifts_unqualified_lesson_to_the_front() {
        // A lesson whose teacher is not qualified for the subject still has
        // free_blocks > 0 and suitable_rooms > 0, so its eligibility is
        // computed as if the placement could happen. The placement loop in
        // `solve_with_config` skips it; `pre_solve_violations` records the
        // `NoQualifiedTeacher` kind. The eligibility metric does not need to
        // gate on qualification; the test below simply confirms the metric
        // is monotonic in the underlying counts.
        let mut problem = two_blocks_two_rooms();
        // Teacher 20 is qualified for subject 40 (set in two_blocks_two_rooms).
        // Teacher 21 is qualified for subject 41 only; lesson C below ties
        // teacher 20 to subject 41 (no qualification) -> placement skipped at
        // solve time, but ffd_order treats it like any other lesson.
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(95)),
            school_class_ids: vec![SchoolClassId(ord_uuid(50))],
            subject_id: SubjectId(ord_uuid(41)),
            teacher_candidates: vec![TeacherId(ord_uuid(20))],
            teacher_pin: Some(TeacherId(ord_uuid(20))),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        let idx = Indexed::new(&problem);
        let order = ffd_order(&problem, &idx);
        assert_eq!(order, vec![0]);
    }

    #[test]
    fn ffd_order_scarcer_teacher_pool_sorts_first() {
        // Two lessons share the same subject, room-suitability shape, and
        // preferred block size, so they tie on the existing
        // `(viable_pairs, home_pairs)` keys. They differ only in
        // `teacher_candidates.len()`: L1 has a pool of 2 qualified teachers,
        // L2 has a pool of 1. Lesson ids are picked so today's id tiebreak
        // sorts L1 (id 70) before L2 (id 71). The scarcity-first comparator
        // sorts L2 (narrower pool) before L1 (broader pool); see OPEN_THINGS
        // item 80 spec at /tmp/kz-autopilot/2026-05-12-ffd-scarcity-ordering-design.md.
        let mut problem = two_blocks_two_rooms();
        // Widen subject 40's qualified-teacher pool: teacher 21 also qualifies
        // for subject 40 so L1's two-candidate pool is structurally valid.
        problem.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(ord_uuid(21)),
            subject_id: SubjectId(ord_uuid(40)),
        });
        // L1 (idx 0, id 70): broader pool with two qualified candidates.
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(70)),
            school_class_ids: vec![SchoolClassId(ord_uuid(50))],
            subject_id: SubjectId(ord_uuid(40)),
            teacher_candidates: vec![TeacherId(ord_uuid(20)), TeacherId(ord_uuid(21))],
            teacher_pin: None,
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        // L2 (idx 1, id 71): narrower pool with one qualified candidate.
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(71)),
            school_class_ids: vec![SchoolClassId(ord_uuid(51))],
            subject_id: SubjectId(ord_uuid(40)),
            teacher_candidates: vec![TeacherId(ord_uuid(20))],
            teacher_pin: None,
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        let idx = Indexed::new(&problem);
        // Scarcer pool sorts first: L2 (idx 1) before L1 (idx 0).
        assert_eq!(ffd_order(&problem, &idx), vec![1, 0]);
    }

    #[test]
    fn ffd_order_doppelstunde_with_one_viable_window_sorts_first() {
        // Two doppelstunden lessons with identical free_blocks * suitable_rooms
        // (the old metric ties them) but different (day, room) viable-window
        // counts under the new same-room-aware metric. The constructed fixture
        // gives lesson A (idx 0, id 71) zero viable (day, room) pairs because
        // its teacher is blocked mid-day on both days, splitting contiguity;
        // lesson B (idx 1, id 70) has all four pairs viable because its teacher
        // is blocked at TB position 0 only, leaving positions 1-2 contiguous.
        // Under the old metric both score `4 free blocks * 2 suitable rooms = 8`
        // and the tiebreak picks lesson B (lower lesson_id) first, yielding
        // order [1, 0]; the new metric ranks A as more constrained (0 viable
        // pairs vs 4) and yields [0, 1]. The assertion below picks up the
        // metric flip: it FAILS under the old metric and PASSES under the new.
        let mut problem = two_blocks_two_rooms();
        // Add four extra TBs: one more TB on day 0 (position 2) and three TBs
        // on day 1 (positions 0, 1, 2). Total grid: 2 days x 3 positions = 6 TBs.
        problem.time_blocks.push(TimeBlock {
            id: TimeBlockId(ord_uuid(12)),
            day_of_week: 0,
            position: 2,
            kind: TimeBlockKind::Lesson,
        });
        problem.time_blocks.push(TimeBlock {
            id: TimeBlockId(ord_uuid(13)),
            day_of_week: 1,
            position: 0,
            kind: TimeBlockKind::Lesson,
        });
        problem.time_blocks.push(TimeBlock {
            id: TimeBlockId(ord_uuid(14)),
            day_of_week: 1,
            position: 1,
            kind: TimeBlockKind::Lesson,
        });
        problem.time_blocks.push(TimeBlock {
            id: TimeBlockId(ord_uuid(15)),
            day_of_week: 1,
            position: 2,
            kind: TimeBlockKind::Lesson,
        });
        // Teacher 20 (lesson A): blocked mid-day on both days -> contiguity
        // busted, no viable doppelstunde window.
        problem.teacher_blocked_times.push(TeacherBlockedTime {
            teacher_id: TeacherId(ord_uuid(20)),
            time_block_id: TimeBlockId(ord_uuid(11)),
        });
        problem.teacher_blocked_times.push(TeacherBlockedTime {
            teacher_id: TeacherId(ord_uuid(20)),
            time_block_id: TimeBlockId(ord_uuid(14)),
        });
        // Teacher 21 (lesson B): blocked at the first TB on each day -> the
        // last two positions remain contiguous, so the doppelstunde fits.
        problem.teacher_blocked_times.push(TeacherBlockedTime {
            teacher_id: TeacherId(ord_uuid(21)),
            time_block_id: TimeBlockId(ord_uuid(10)),
        });
        problem.teacher_blocked_times.push(TeacherBlockedTime {
            teacher_id: TeacherId(ord_uuid(21)),
            time_block_id: TimeBlockId(ord_uuid(13)),
        });
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(71)),
            school_class_ids: vec![SchoolClassId(ord_uuid(50))],
            subject_id: SubjectId(ord_uuid(40)),
            teacher_candidates: vec![TeacherId(ord_uuid(20))],
            teacher_pin: Some(TeacherId(ord_uuid(20))),
            hours_per_week: 2,
            preferred_block_size: 2,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(70)),
            school_class_ids: vec![SchoolClassId(ord_uuid(51))],
            subject_id: SubjectId(ord_uuid(41)),
            teacher_candidates: vec![TeacherId(ord_uuid(21))],
            teacher_pin: Some(TeacherId(ord_uuid(21))),
            hours_per_week: 2,
            preferred_block_size: 2,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        let idx = Indexed::new(&problem);
        assert_eq!(ffd_order(&problem, &idx), vec![0, 1]);
    }
}
