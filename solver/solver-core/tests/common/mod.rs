//! Test-only fixtures shared between integration and property tests.

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    types::{
        Lesson, Problem, Room, RoomSubjectSuitability, SchoolClass, Subject, Teacher,
        TeacherQualification, TimeBlock, TimeBlockKind,
    },
};
use uuid::Uuid;

pub fn common_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

pub fn common_big_uuid(hi: u8, lo: u8) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = hi;
    bytes[15] = lo;
    Uuid::from_bytes(bytes)
}

/// Build a feasible problem with `classes` classes, `teachers` teachers, `rooms`
/// rooms, and exactly `blocks` time blocks distributed across 5 weekdays.
/// Every teacher qualifies for every subject and is available everywhere;
/// rooms have no suitability filter; no teacher/room blocked times.
#[allow(dead_code)] // Reason: used only by tests/properties.rs; other integration tests may ignore this
pub fn feasible_problem(
    classes: u8,
    teachers: u8,
    rooms: u8,
    blocks: u8,
    subjects: u8,
    hours_per_lesson: u8,
) -> Problem {
    let time_blocks: Vec<TimeBlock> = (0..blocks)
        .map(|i| TimeBlock {
            id: TimeBlockId(common_uuid(200 + i)),
            day_of_week: i / 5,
            position: i % 5,
            kind: TimeBlockKind::Lesson,
        })
        .collect();

    let teachers_vec: Vec<Teacher> = (0..teachers)
        .map(|i| Teacher {
            id: TeacherId(common_uuid(50 + i)),
            max_hours_per_week: 255,
            reserve_hours_per_week: 0,
        })
        .collect();

    let rooms_vec: Vec<Room> = (0..rooms)
        .map(|i| Room {
            id: RoomId(common_uuid(100 + i)),
        })
        .collect();
    let subjects_vec: Vec<Subject> = (0..subjects)
        .map(|i| Subject {
            id: SubjectId(common_uuid(150 + i)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        })
        .collect();
    let classes_vec: Vec<SchoolClass> = (0..classes)
        .map(|i| SchoolClass {
            id: SchoolClassId(common_uuid(20 + i)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        })
        .collect();

    let mut lessons = Vec::new();
    let mut quals = Vec::new();
    for (c_idx, class) in classes_vec.iter().enumerate() {
        for (s_idx, subject) in subjects_vec.iter().enumerate() {
            let teacher = &teachers_vec[(c_idx + s_idx) % teachers_vec.len()];
            lessons.push(Lesson {
                id: LessonId(common_big_uuid((c_idx as u8) + 1, s_idx as u8)),
                school_class_ids: vec![class.id],
                subject_id: subject.id,
                teacher_candidates: vec![teacher.id],
                teacher_pin: Some(teacher.id),
                hours_per_week: hours_per_lesson,
                preferred_block_size: 1,
                pre_buffer_minutes: 0,
                post_buffer_minutes: 0,
                lesson_group_id: None,
            });
        }
    }
    for teacher in &teachers_vec {
        for subject in &subjects_vec {
            quals.push(TeacherQualification {
                teacher_id: teacher.id,
                subject_id: subject.id,
            });
        }
    }

    Problem {
        time_blocks,
        teachers: teachers_vec,
        rooms: rooms_vec,
        subjects: subjects_vec,
        school_classes: classes_vec,
        lessons,
        teacher_qualifications: quals,
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: Vec::<RoomSubjectSuitability>::new(),
        pinned_placements: vec![],
        pre_first_slot_grace_minutes: 0,
    }
}
