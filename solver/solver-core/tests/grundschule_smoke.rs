//! Grundschule-shaped smoke test. Two classes (one grade-1/2 Pflichtstunden at 21
//! hours, one grade-3/4 at 25 hours) across 5 weekdays × 5 periods = 25 time
//! blocks. 8 teachers, 5 rooms including one "gym" limited to sports. The greedy
//! must place every hour with zero violations.

use std::collections::HashSet;

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    solve,
    types::{
        Lesson, Problem, Room, RoomSubjectSuitability, SchoolClass, Subject, Teacher,
        TeacherQualification, TimeBlock,
    },
};
use uuid::Uuid;

fn grundschule_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

fn grundschule() -> Problem {
    // 5 weekdays, 5 periods per day = 25 time blocks
    let time_blocks: Vec<TimeBlock> = (0..25)
        .map(|i| TimeBlock {
            id: TimeBlockId(grundschule_uuid(100 + i)),
            day_of_week: i / 5,
            position: i % 5,
        })
        .collect();

    // 8 teachers, generous caps
    let teachers: Vec<Teacher> = (0..8)
        .map(|i| Teacher {
            id: TeacherId(grundschule_uuid(30 + i)),
            max_hours_per_week: 28,
            reserve_hours_per_week: 0,
        })
        .collect();

    // 5 rooms: 2 regular classrooms, 1 music, 1 art, 1 gym
    let rooms: Vec<Room> = (0..5)
        .map(|i| Room {
            id: RoomId(grundschule_uuid(50 + i)),
        })
        .collect();

    // 7 subjects: Deutsch, Mathe, Sachunterricht, Fremdsprache, Religion, Musik, Kunst, Sport
    // (actually 8; renumbered below)
    let subject_ids: Vec<SubjectId> = (0..8)
        .map(|i| SubjectId(grundschule_uuid(60 + i)))
        .collect();
    let subjects: Vec<Subject> = subject_ids
        .iter()
        .map(|id| Subject {
            id: *id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        })
        .collect();

    // 2 classes: class 1/2 (index 0), class 3/4 (index 1)
    let classes: Vec<SchoolClass> = (0..2)
        .map(|i| SchoolClass {
            id: SchoolClassId(grundschule_uuid(70 + i)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        })
        .collect();

    // Stundentafeln (hessische Grundschule, abgespeckt)
    // Klasse 1/2: Deutsch 6, Mathe 5, Sachunterricht 2, Religion 2, Kunst 1, Musik 1, Werken 1, Sport 3 = 21
    // Klasse 3/4: Deutsch 5, Mathe 5, Sachunterricht 4, Fremdsprache 2, Religion 2, Kunst 1, Musik 1, Werken 1, Sport 3 = 24; +1 Förder für 25
    // For solver determinism we size to 21 and 24 respectively; stories within ±2 of the real figures.
    let hours_per_class: [[u8; 8]; 2] = [
        // Deutsch, Mathe, Sachunterricht, Fremdsprache, Religion, Musik, Kunst, Sport
        [6, 5, 2, 0, 2, 1, 2, 3],
        [5, 5, 4, 2, 2, 1, 2, 3],
    ];

    // One teacher per (class, subject) pair, round-robin.
    let mut lessons = Vec::new();
    let mut quals = Vec::new();
    let mut lesson_idx = 0u8;
    for (c_idx, class) in classes.iter().enumerate() {
        for (s_idx, subject) in subjects.iter().enumerate() {
            let hours = hours_per_class[c_idx][s_idx];
            if hours == 0 {
                continue;
            }
            let teacher = &teachers[(c_idx * 4 + s_idx) % teachers.len()];
            lessons.push(Lesson {
                id: LessonId(grundschule_uuid(200 + lesson_idx)),
                school_class_ids: vec![class.id],
                subject_id: subject.id,
                teacher_candidates: vec![teacher.id],
                teacher_pin: Some(teacher.id),
                hours_per_week: hours,
                preferred_block_size: 1,
                lesson_group_id: None,
            });
            lesson_idx += 1;
            quals.push(TeacherQualification {
                teacher_id: teacher.id,
                subject_id: subject.id,
            });
        }
    }

    // Gym (room index 4) suits only Sport (subject index 7). Others suit all.
    let sport_subject = subject_ids[7];
    let gym = rooms[4].id;
    let suits: Vec<RoomSubjectSuitability> = vec![RoomSubjectSuitability {
        room_id: gym,
        subject_id: sport_subject,
    }];

    Problem {
        time_blocks,
        teachers,
        rooms,
        subjects,
        school_classes: classes,
        lessons,
        teacher_qualifications: quals,
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: suits,
        pinned_placements: vec![],
    }
}

#[test]
fn grundschule_shape_places_every_hour_with_zero_violations() {
    let problem = grundschule();
    let expected_hours: u32 = problem
        .lessons
        .iter()
        .map(|l| u32::from(l.hours_per_week))
        .sum();
    let solution = solve(&problem).unwrap();
    assert!(
        solution.violations.is_empty(),
        "expected zero violations, got {:?}",
        solution.violations
    );
    assert_eq!(solution.placements.len() as u32, expected_hours);

    // Basic room no-double-booking sanity (the property test covers the rest)
    let mut seen: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
    for pl in &solution.placements {
        assert!(
            seen.insert((pl.room_id, pl.time_block_id)),
            "room double-book"
        );
    }
}
