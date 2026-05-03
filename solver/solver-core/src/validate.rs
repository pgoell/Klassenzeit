//! Structural validation and the pre-solve cross-entity check.
//!
//! `validate_structural` returns `Err(Error::Input)` on malformed input (unknown
//! references, duplicate IDs, `hours_per_week == 0`, empty `time_blocks` or
//! `rooms`). `pre_solve_violations` takes a structurally-valid `Problem` and
//! emits `NoQualifiedTeacher` violations for every lesson whose teacher lacks
//! the subject qualification.

use std::collections::HashSet;

use crate::error::Error;
use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use crate::types::{Problem, Violation, ViolationKind};

/// Validate a `Problem` against purely structural rules: non-empty core
/// collections, unique IDs, known references, `hours_per_week > 0`.
pub fn validate_structural(problem: &Problem) -> Result<(), Error> {
    if problem.time_blocks.is_empty() {
        return Err(Error::Input("problem has no time_blocks".into()));
    }
    if problem.rooms.is_empty() {
        return Err(Error::Input("problem has no rooms".into()));
    }

    let time_block_ids: HashSet<TimeBlockId> =
        collect_unique(problem.time_blocks.iter().map(|tb| tb.id), "time_blocks")?;
    let teacher_ids: HashSet<TeacherId> =
        collect_unique(problem.teachers.iter().map(|t| t.id), "teachers")?;
    let room_ids: HashSet<RoomId> = collect_unique(problem.rooms.iter().map(|r| r.id), "rooms")?;
    let subject_ids: HashSet<SubjectId> =
        collect_unique(problem.subjects.iter().map(|s| s.id), "subjects")?;
    let class_ids: HashSet<SchoolClassId> = collect_unique(
        problem.school_classes.iter().map(|c| c.id),
        "school_classes",
    )?;
    let _lesson_ids: HashSet<LessonId> =
        collect_unique(problem.lessons.iter().map(|l| l.id), "lessons")?;

    for lesson in &problem.lessons {
        if lesson.hours_per_week == 0 {
            return Err(Error::Input(format!(
                "lesson {} has hours_per_week = 0",
                lesson.id.0
            )));
        }
        if lesson.preferred_block_size == 0 {
            return Err(Error::Input(format!(
                "lesson {} has preferred_block_size = 0",
                lesson.id.0
            )));
        }
        if lesson.hours_per_week % lesson.preferred_block_size != 0 {
            return Err(Error::Input(format!(
                "lesson {}: hours_per_week ({}) is not divisible by preferred_block_size ({})",
                lesson.id.0, lesson.hours_per_week, lesson.preferred_block_size
            )));
        }
        if !teacher_ids.contains(&lesson.teacher_id) {
            return Err(Error::Input(format!(
                "lesson {} references unknown teacher {}",
                lesson.id.0, lesson.teacher_id.0
            )));
        }
        if !subject_ids.contains(&lesson.subject_id) {
            return Err(Error::Input(format!(
                "lesson {} references unknown subject {}",
                lesson.id.0, lesson.subject_id.0
            )));
        }
        if lesson.school_class_ids.is_empty() {
            return Err(Error::Input(format!(
                "lesson {} has empty school_class_ids",
                lesson.id.0
            )));
        }
        let mut seen_classes: HashSet<SchoolClassId> = HashSet::new();
        for class_id in &lesson.school_class_ids {
            if !seen_classes.insert(*class_id) {
                return Err(Error::Input(format!(
                    "lesson {} has duplicate school_class {} in school_class_ids",
                    lesson.id.0, class_id.0
                )));
            }
            if !class_ids.contains(class_id) {
                return Err(Error::Input(format!(
                    "lesson {} references unknown school_class {}",
                    lesson.id.0, class_id.0
                )));
            }
        }
    }
    for q in &problem.teacher_qualifications {
        if !teacher_ids.contains(&q.teacher_id) {
            return Err(Error::Input(format!(
                "teacher_qualification references unknown teacher {}",
                q.teacher_id.0
            )));
        }
        if !subject_ids.contains(&q.subject_id) {
            return Err(Error::Input(format!(
                "teacher_qualification references unknown subject {}",
                q.subject_id.0
            )));
        }
    }
    for b in &problem.teacher_blocked_times {
        if !teacher_ids.contains(&b.teacher_id) {
            return Err(Error::Input(format!(
                "teacher_blocked_time references unknown teacher {}",
                b.teacher_id.0
            )));
        }
        if !time_block_ids.contains(&b.time_block_id) {
            return Err(Error::Input(format!(
                "teacher_blocked_time references unknown time_block {}",
                b.time_block_id.0
            )));
        }
    }
    for b in &problem.room_blocked_times {
        if !room_ids.contains(&b.room_id) {
            return Err(Error::Input(format!(
                "room_blocked_time references unknown room {}",
                b.room_id.0
            )));
        }
        if !time_block_ids.contains(&b.time_block_id) {
            return Err(Error::Input(format!(
                "room_blocked_time references unknown time_block {}",
                b.time_block_id.0
            )));
        }
    }
    for s in &problem.room_subject_suitabilities {
        if !room_ids.contains(&s.room_id) {
            return Err(Error::Input(format!(
                "room_subject_suitability references unknown room {}",
                s.room_id.0
            )));
        }
        if !subject_ids.contains(&s.subject_id) {
            return Err(Error::Input(format!(
                "room_subject_suitability references unknown subject {}",
                s.subject_id.0
            )));
        }
    }

    use crate::ids::LessonGroupId;
    let mut groups: std::collections::HashMap<LessonGroupId, Vec<&crate::types::Lesson>> =
        std::collections::HashMap::new();
    for lesson in &problem.lessons {
        if let Some(group_id) = lesson.lesson_group_id {
            groups.entry(group_id).or_default().push(lesson);
        }
    }
    for (group_id, members) in &groups {
        if members.len() < 2 {
            continue;
        }
        let first = &members[0];
        for member in &members[1..] {
            if member.hours_per_week != first.hours_per_week {
                return Err(Error::Input(format!(
                    "lesson group {} members disagree on hours_per_week: {} vs {}",
                    group_id.0, first.hours_per_week, member.hours_per_week
                )));
            }
            if member.preferred_block_size != first.preferred_block_size {
                return Err(Error::Input(format!(
                    "lesson group {} members disagree on preferred_block_size: {} vs {}",
                    group_id.0, first.preferred_block_size, member.preferred_block_size
                )));
            }
        }
        let mut seen_teachers: HashSet<TeacherId> = HashSet::new();
        for member in members {
            if !seen_teachers.insert(member.teacher_id) {
                return Err(Error::Input(format!(
                    "lesson group {} has duplicate teacher {}",
                    group_id.0, member.teacher_id.0
                )));
            }
        }
    }
    Ok(())
}

fn collect_unique<Id, I>(iter: I, kind: &'static str) -> Result<HashSet<Id>, Error>
where
    Id: std::hash::Hash + Eq + Copy + std::fmt::Display,
    I: IntoIterator<Item = Id>,
{
    let mut set = HashSet::new();
    for id in iter {
        if !set.insert(id) {
            return Err(Error::Input(format!("duplicate id {id} in {kind}")));
        }
    }
    Ok(set)
}

/// Scan lessons for teacher / subject pairs that are not in
/// `teacher_qualifications` and record one `NoQualifiedTeacher` violation per
/// hour on the affected lesson.
pub fn pre_solve_violations(problem: &Problem) -> Vec<Violation> {
    let mut qualified: HashSet<(TeacherId, SubjectId)> = HashSet::new();
    for q in &problem.teacher_qualifications {
        qualified.insert((q.teacher_id, q.subject_id));
    }

    let mut out = Vec::new();
    for lesson in &problem.lessons {
        if qualified.contains(&(lesson.teacher_id, lesson.subject_id)) {
            continue;
        }
        let n = lesson.preferred_block_size;
        let block_count = lesson.hours_per_week / n;
        for block_index in 0..block_count {
            out.push(Violation {
                kind: ViolationKind::NoQualifiedTeacher,
                lesson_id: lesson.id,
                hour_index: block_index * n,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Lesson, Problem, Room, RoomSubjectSuitability, SchoolClass, Subject, Teacher,
        TeacherQualification, TimeBlock,
    };
    use uuid::Uuid;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn minimal_problem() -> Problem {
        let tb = TimeBlock {
            id: TimeBlockId(uuid(1)),
            day_of_week: 0,
            position: 0,
        };
        let teacher = Teacher {
            id: TeacherId(uuid(2)),
            max_hours_per_week: 10,
        };
        let room = Room {
            id: RoomId(uuid(3)),
        };
        let subject = Subject {
            id: SubjectId(uuid(4)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
        };
        let class = SchoolClass {
            id: SchoolClassId(uuid(5)),
            home_room_id: None,
        };
        let lesson = Lesson {
            id: LessonId(uuid(6)),
            school_class_ids: vec![class.id],
            subject_id: subject.id,
            teacher_id: teacher.id,
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        };
        Problem {
            time_blocks: vec![tb],
            teachers: vec![teacher],
            rooms: vec![room],
            subjects: vec![subject],
            school_classes: vec![class],
            lessons: vec![lesson],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: TeacherId(uuid(2)),
                subject_id: SubjectId(uuid(4)),
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        }
    }

    #[test]
    fn minimal_problem_is_structurally_valid() {
        validate_structural(&minimal_problem()).unwrap();
    }

    #[test]
    fn empty_time_blocks_is_input_error() {
        let mut p = minimal_problem();
        p.time_blocks.clear();
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("time_blocks")));
    }

    #[test]
    fn empty_rooms_is_input_error() {
        let mut p = minimal_problem();
        p.rooms.clear();
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("rooms")));
    }

    #[test]
    fn duplicate_teacher_id_is_input_error() {
        let mut p = minimal_problem();
        p.teachers.push(p.teachers[0].clone());
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("duplicate id")));
    }

    #[test]
    fn lesson_with_zero_hours_is_input_error() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 0;
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("hours_per_week")));
    }

    #[test]
    fn lesson_with_zero_block_size_is_input_error() {
        let mut p = minimal_problem();
        p.lessons[0].preferred_block_size = 0;
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("preferred_block_size")));
    }

    #[test]
    fn lesson_with_non_divisible_hours_is_input_error() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 3;
        p.lessons[0].preferred_block_size = 2;
        let err = validate_structural(&p).unwrap_err();
        assert!(
            matches!(err, Error::Input(msg) if msg.contains("divisible by preferred_block_size"))
        );
    }

    #[test]
    fn block_size_one_with_any_hours_is_valid() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 7;
        p.lessons[0].preferred_block_size = 1;
        validate_structural(&p).unwrap();
    }

    #[test]
    fn block_size_two_with_even_hours_is_valid() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 4;
        p.lessons[0].preferred_block_size = 2;
        validate_structural(&p).unwrap();
    }

    #[test]
    fn unknown_teacher_ref_is_input_error() {
        let mut p = minimal_problem();
        p.lessons[0].teacher_id = TeacherId(uuid(99));
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("unknown teacher")));
    }

    #[test]
    fn validate_structural_rejects_empty_school_class_ids() {
        let mut p = minimal_problem();
        p.lessons[0].school_class_ids.clear();
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("empty school_class_ids")));
    }

    #[test]
    fn validate_structural_rejects_duplicate_school_class_ids() {
        let mut p = minimal_problem();
        let class_id = p.lessons[0].school_class_ids[0];
        p.lessons[0].school_class_ids.push(class_id);
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("duplicate school_class")));
    }

    #[test]
    fn validate_structural_rejects_unknown_school_class_id_in_set() {
        let mut p = minimal_problem();
        p.lessons[0].school_class_ids.push(SchoolClassId(uuid(99)));
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("unknown school_class")));
    }

    #[test]
    fn unknown_room_suitability_ref_is_input_error() {
        let mut p = minimal_problem();
        p.room_subject_suitabilities.push(RoomSubjectSuitability {
            room_id: RoomId(uuid(99)),
            subject_id: SubjectId(uuid(4)),
        });
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("unknown room")));
    }

    #[test]
    fn pre_solve_emits_no_violations_when_all_teachers_qualified() {
        let violations = pre_solve_violations(&minimal_problem());
        assert!(violations.is_empty());
    }

    #[test]
    fn pre_solve_emits_violations_per_hour_for_unqualified_teacher() {
        let mut p = minimal_problem();
        p.teacher_qualifications.clear();
        p.lessons[0].hours_per_week = 3;
        let violations = pre_solve_violations(&p);
        assert_eq!(violations.len(), 3);
        assert!(violations
            .iter()
            .all(|v| v.kind == ViolationKind::NoQualifiedTeacher));
        assert_eq!(
            violations.iter().map(|v| v.hour_index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    fn two_member_group_problem() -> Problem {
        use crate::ids::LessonGroupId;
        let group_id = LessonGroupId(uuid(99));
        let mut p = minimal_problem();
        p.school_classes.push(SchoolClass {
            id: SchoolClassId(uuid(7)),
            home_room_id: None,
        });
        p.subjects.push(Subject {
            id: SubjectId(uuid(8)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
        });
        p.teachers.push(Teacher {
            id: TeacherId(uuid(9)),
            max_hours_per_week: 10,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(uuid(9)),
            subject_id: SubjectId(uuid(8)),
        });
        p.lessons[0].lesson_group_id = Some(group_id);
        p.lessons.push(Lesson {
            id: LessonId(uuid(10)),
            school_class_ids: vec![SchoolClassId(uuid(7))],
            subject_id: SubjectId(uuid(8)),
            teacher_id: TeacherId(uuid(9)),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: Some(group_id),
        });
        p
    }

    #[test]
    fn validate_structural_accepts_group_with_consistent_invariants() {
        validate_structural(&two_member_group_problem()).unwrap();
    }

    #[test]
    fn validate_structural_accepts_single_member_group() {
        use crate::ids::LessonGroupId;
        let mut p = minimal_problem();
        p.lessons[0].lesson_group_id = Some(LessonGroupId(uuid(99)));
        validate_structural(&p).unwrap();
    }

    #[test]
    fn validate_structural_rejects_group_members_with_different_hours_per_week() {
        let mut p = two_member_group_problem();
        p.lessons[1].hours_per_week = 2;
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("hours_per_week")));
    }

    #[test]
    fn validate_structural_rejects_group_members_with_different_block_size() {
        let mut p = two_member_group_problem();
        p.lessons[0].hours_per_week = 2;
        p.lessons[0].preferred_block_size = 2;
        p.lessons[1].hours_per_week = 2;
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("preferred_block_size")));
    }

    #[test]
    fn validate_structural_rejects_group_with_duplicate_teacher() {
        let mut p = two_member_group_problem();
        p.lessons[1].teacher_id = p.lessons[0].teacher_id;
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("duplicate teacher")));
    }
}
