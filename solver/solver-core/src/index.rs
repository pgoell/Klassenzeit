//! Private index of `Problem` relations. Built once at the top of `solve`.
//! Each predicate is an O(1) hashmap / hashset probe.

use std::collections::{HashMap, HashSet};

use crate::ids::{RoomId, SubjectId, TeacherId, TimeBlockId};
use crate::types::Problem;

pub(crate) struct Indexed {
    teacher_subject: HashMap<TeacherId, HashSet<SubjectId>>,
    teacher_blocked: HashSet<(TeacherId, TimeBlockId)>,
    /// Absence of key means "room has no suitability filter → suits every subject".
    /// Presence of key with an empty set means "room suits zero subjects".
    room_subject: HashMap<RoomId, HashSet<SubjectId>>,
    room_blocked: HashSet<(RoomId, TimeBlockId)>,
}

impl Indexed {
    pub(crate) fn new(problem: &Problem) -> Self {
        let mut teacher_subject: HashMap<TeacherId, HashSet<SubjectId>> = HashMap::new();
        for q in &problem.teacher_qualifications {
            teacher_subject
                .entry(q.teacher_id)
                .or_default()
                .insert(q.subject_id);
        }

        let mut teacher_blocked: HashSet<(TeacherId, TimeBlockId)> = HashSet::new();
        for b in &problem.teacher_blocked_times {
            teacher_blocked.insert((b.teacher_id, b.time_block_id));
        }

        let mut room_subject: HashMap<RoomId, HashSet<SubjectId>> = HashMap::new();
        for s in &problem.room_subject_suitabilities {
            room_subject
                .entry(s.room_id)
                .or_default()
                .insert(s.subject_id);
        }

        let mut room_blocked: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
        for b in &problem.room_blocked_times {
            room_blocked.insert((b.room_id, b.time_block_id));
        }

        Self {
            teacher_subject,
            teacher_blocked,
            room_subject,
            room_blocked,
        }
    }

    pub(crate) fn teacher_qualified(&self, teacher: TeacherId, subject: SubjectId) -> bool {
        self.teacher_subject
            .get(&teacher)
            .is_some_and(|s| s.contains(&subject))
    }

    pub(crate) fn teacher_blocked(&self, teacher: TeacherId, tb: TimeBlockId) -> bool {
        self.teacher_blocked.contains(&(teacher, tb))
    }

    /// True when room has no suitability entries (suits all) or explicitly lists the subject.
    pub(crate) fn room_suits_subject(&self, room: RoomId, subject: SubjectId) -> bool {
        match self.room_subject.get(&room) {
            None => true,
            Some(set) => set.contains(&subject),
        }
    }

    pub(crate) fn room_blocked(&self, room: RoomId, tb: TimeBlockId) -> bool {
        self.room_blocked.contains(&(room, tb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
    use crate::types::{
        Lesson, Problem, Room, RoomBlockedTime, RoomSubjectSuitability, SchoolClass, Subject,
        Teacher, TeacherBlockedTime, TeacherQualification, TimeBlock,
    };
    use uuid::Uuid;

    fn u(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn problem() -> Problem {
        Problem {
            time_blocks: vec![TimeBlock {
                id: TimeBlockId(u(1)),
                day_of_week: 0,
                position: 0,
            }],
            teachers: vec![Teacher {
                id: TeacherId(u(2)),
                max_hours_per_week: 10,
            }],
            rooms: vec![Room { id: RoomId(u(3)) }, Room { id: RoomId(u(4)) }],
            subjects: vec![
                Subject {
                    id: SubjectId(u(5)),
                    prefer_early_periods: false,
                    avoid_first_period: false,
                },
                Subject {
                    id: SubjectId(u(6)),
                    prefer_early_periods: false,
                    avoid_first_period: false,
                },
            ],
            school_classes: vec![SchoolClass {
                id: SchoolClassId(u(7)),
                home_room_id: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(u(8)),
                school_class_ids: vec![SchoolClassId(u(7))],
                subject_id: SubjectId(u(5)),
                teacher_id: TeacherId(u(2)),
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: TeacherId(u(2)),
                subject_id: SubjectId(u(5)),
            }],
            teacher_blocked_times: vec![TeacherBlockedTime {
                teacher_id: TeacherId(u(2)),
                time_block_id: TimeBlockId(u(1)),
            }],
            room_blocked_times: vec![RoomBlockedTime {
                room_id: RoomId(u(3)),
                time_block_id: TimeBlockId(u(1)),
            }],
            room_subject_suitabilities: vec![RoomSubjectSuitability {
                room_id: RoomId(u(3)),
                subject_id: SubjectId(u(5)),
            }],
        }
    }

    #[test]
    fn teacher_qualified_hits_and_misses() {
        let idx = Indexed::new(&problem());
        assert!(idx.teacher_qualified(TeacherId(u(2)), SubjectId(u(5))));
        assert!(!idx.teacher_qualified(TeacherId(u(2)), SubjectId(u(6))));
        assert!(!idx.teacher_qualified(TeacherId(u(99)), SubjectId(u(5))));
    }

    #[test]
    fn teacher_blocked_matches_pair() {
        let idx = Indexed::new(&problem());
        assert!(idx.teacher_blocked(TeacherId(u(2)), TimeBlockId(u(1))));
        assert!(!idx.teacher_blocked(TeacherId(u(2)), TimeBlockId(u(99))));
    }

    #[test]
    fn room_with_entries_suits_only_listed_subjects() {
        let idx = Indexed::new(&problem());
        assert!(idx.room_suits_subject(RoomId(u(3)), SubjectId(u(5))));
        assert!(!idx.room_suits_subject(RoomId(u(3)), SubjectId(u(6))));
    }

    #[test]
    fn room_with_no_entries_suits_all_subjects() {
        let idx = Indexed::new(&problem());
        assert!(idx.room_suits_subject(RoomId(u(4)), SubjectId(u(5))));
        assert!(idx.room_suits_subject(RoomId(u(4)), SubjectId(u(6))));
    }

    #[test]
    fn room_blocked_matches_pair() {
        let idx = Indexed::new(&problem());
        assert!(idx.room_blocked(RoomId(u(3)), TimeBlockId(u(1))));
        assert!(!idx.room_blocked(RoomId(u(4)), TimeBlockId(u(1))));
    }
}
