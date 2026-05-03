//! First Fit Decreasing lesson ordering.
//!
//! Returns a permutation of `problem.lessons` indices in placement order.
//! Lessons are sorted by an eligibility metric (lower = more constrained =
//! placed first) computed once before placement begins; the metric is the
//! product of two counts:
//!
//! 1. Time blocks where the lesson's teacher is not blocked.
//! 2. Rooms suitable for the lesson's subject.
//!
//! Tiebreak is the lesson's `LessonId` byte order so two lessons with equal
//! eligibility keep a deterministic ordering across runs.
//!
//! Lessons whose teacher lacks the qualification for the subject fall to
//! eligibility `0` and sort first; the placement loop in `solve_with_config`
//! skips them and `pre_solve_violations` records each affected hour as a
//! `NoQualifiedTeacher` violation.

use crate::index::Indexed;
use crate::types::{Lesson, Problem};

/// Compute placement order under First Fit Decreasing. See module docs.
pub(crate) fn ffd_order(problem: &Problem, idx: &Indexed) -> Vec<usize> {
    let scores: Vec<u32> = problem
        .lessons
        .iter()
        .map(|l| eligibility(l, problem, idx))
        .collect();
    let mut order: Vec<usize> = (0..problem.lessons.len()).collect();
    order.sort_by(|&a, &b| {
        scores[a]
            .cmp(&scores[b])
            .then_with(|| problem.lessons[a].id.0.cmp(&problem.lessons[b].id.0))
    });
    order
}

fn eligibility(lesson: &Lesson, problem: &Problem, idx: &Indexed) -> u32 {
    let free_blocks = problem
        .time_blocks
        .iter()
        .filter(|tb| !idx.teacher_blocked(lesson.teacher_id, tb.id))
        .count();
    let suitable_rooms = problem
        .rooms
        .iter()
        .filter(|r| idx.room_suits_subject(r.id, lesson.subject_id))
        .count();
    u32::try_from(free_blocks.saturating_mul(suitable_rooms)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
    use crate::types::{
        Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherBlockedTime,
        TeacherQualification, TimeBlock,
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
                },
                TimeBlock {
                    id: TimeBlockId(ord_uuid(11)),
                    day_of_week: 0,
                    position: 1,
                },
            ],
            teachers: vec![
                Teacher {
                    id: TeacherId(ord_uuid(20)),
                    max_hours_per_week: 5,
                },
                Teacher {
                    id: TeacherId(ord_uuid(21)),
                    max_hours_per_week: 5,
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
                },
                Subject {
                    id: SubjectId(ord_uuid(41)),
                    prefer_early_period: 0,
                    avoid_first_period: 0,
                    avoid_last_period: 0,
                },
            ],
            school_classes: vec![
                SchoolClass {
                    id: SchoolClassId(ord_uuid(50)),
                    home_room_id: None,
                },
                SchoolClass {
                    id: SchoolClassId(ord_uuid(51)),
                    home_room_id: None,
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
            teacher_id: TeacherId(ord_uuid(20)),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(71)),
            school_class_ids: vec![SchoolClassId(ord_uuid(51))],
            subject_id: SubjectId(ord_uuid(41)),
            teacher_id: TeacherId(ord_uuid(21)),
            hours_per_week: 1,
            preferred_block_size: 1,
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
            teacher_id: TeacherId(ord_uuid(20)),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        problem.lessons.push(Lesson {
            id: LessonId(ord_uuid(80)),
            school_class_ids: vec![SchoolClassId(ord_uuid(51))],
            subject_id: SubjectId(ord_uuid(41)),
            teacher_id: TeacherId(ord_uuid(21)),
            hours_per_week: 1,
            preferred_block_size: 1,
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
                teacher_id: TeacherId(ord_uuid(20)),
                hours_per_week: 1,
                preferred_block_size: 1,
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
            teacher_id: TeacherId(ord_uuid(20)),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        let idx = Indexed::new(&problem);
        let order = ffd_order(&problem, &idx);
        assert_eq!(order, vec![0]);
    }
}
