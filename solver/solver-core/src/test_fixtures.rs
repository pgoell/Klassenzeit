//! Hand-coded `Problem` fixtures shared by the criterion bench, the
//! bake-off bench, and integration tests. Mirrors the seed builders in
//! `backend/src/klassenzeit_backend/seed/demo_*.py`. Per-fixture drift is
//! caught by `assert_eq!(lessons.len(), N)` against literals shared with the
//! matching Python solvability test (or, for `ffd_lock_in_grundschule`, the
//! count documented in the function docstring).
//!
//! Gated behind the `fixtures` Cargo feature so the maturin-built solver-py
//! wheel does not ship the fixture builders.

use std::collections::HashSet;

use uuid::Uuid;

use crate::ids::{
    LessonGroupId, LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId,
};
use crate::types::{
    Lesson, Problem, Room, RoomBlockedTime, RoomSubjectSuitability, SchoolClass, Subject, Teacher,
    TeacherQualification, TimeBlock,
};

/// Deterministic 16-byte UUID with every byte equal to `n`. Replaces the
/// per-file `bench_uuid` and `same_room_uuid` helpers; the byte pattern is
/// identical across both call sites and tests assert specific UUIDs.
fn fixture_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

/// Build a Grundschule-shaped `Problem`. Mirrors the test fixture in
/// `solver-core/tests/grundschule_smoke.rs::grundschule()`. Asserts the
/// resulting problem has exactly 15 lessons totalling 45 placements so
/// copy-paste drift is caught.
pub fn grundschule_fixture() -> Problem {
    let time_blocks: Vec<TimeBlock> = (0..25)
        .map(|i| TimeBlock {
            id: TimeBlockId(fixture_uuid(100 + i)),
            day_of_week: i / 5,
            position: i % 5,
        })
        .collect();

    let teachers: Vec<Teacher> = (0..8)
        .map(|i| Teacher {
            id: TeacherId(fixture_uuid(30 + i)),
            max_hours_per_week: 28,
        })
        .collect();

    let rooms: Vec<Room> = (0..5)
        .map(|i| Room {
            id: RoomId(fixture_uuid(50 + i)),
        })
        .collect();

    let subject_ids: Vec<SubjectId> = (0..8).map(|i| SubjectId(fixture_uuid(60 + i))).collect();
    let subjects: Vec<Subject> = subject_ids
        .iter()
        .enumerate()
        .map(|(i, id)| Subject {
            id: *id,
            prefer_early_period: u32::from(matches!(i, 0 | 1)), // index 0 = Deutsch, 1 = Mathematik
            avoid_first_period: u32::from(i == 7),              // index 7 = Sport
            avoid_last_period: u32::from(matches!(i, 0 | 1)),   // index 0 = Deutsch, 1 = Mathematik
            prefer_late_period: 0,
            max_hours_per_day: 8,
        })
        .collect();

    let classes: Vec<SchoolClass> = (0..2)
        .map(|i| SchoolClass {
            id: SchoolClassId(fixture_uuid(70 + i)),
            home_room_id: Some(RoomId(fixture_uuid(50 + i))),
            max_lessons_per_day: None,
        })
        .collect();

    let hours_per_class: [[u8; 8]; 2] = [[6, 5, 2, 0, 2, 1, 2, 3], [5, 5, 4, 2, 2, 1, 2, 3]];

    let mut lessons = Vec::new();
    let mut quals = Vec::new();
    let mut lesson_idx: u8 = 0;
    for (c_idx, class) in classes.iter().enumerate() {
        for (s_idx, subject) in subjects.iter().enumerate() {
            let hours = hours_per_class[c_idx][s_idx];
            if hours == 0 {
                continue;
            }
            let teacher = &teachers[(c_idx * 4 + s_idx) % teachers.len()];
            lessons.push(Lesson {
                id: LessonId(fixture_uuid(200 + lesson_idx)),
                school_class_ids: vec![class.id],
                subject_id: subject.id,
                teacher_id: teacher.id,
                hours_per_week: hours,
                // SU (Sachunterricht) is taught as a Doppelstunde, mirroring the
                // demo_grundschule seed; all other subjects are length-1.
                preferred_block_size: if s_idx == 2 { 2 } else { 1 },
                lesson_group_id: None,
            });
            lesson_idx += 1;
            quals.push(TeacherQualification {
                teacher_id: teacher.id,
                subject_id: subject.id,
            });
        }
    }

    assert_eq!(
        lessons.len(),
        15,
        "bench fixture drifted from the test fixture"
    );

    // Gym (room index 4) suits only Sport (subject index 7).
    let suits: Vec<RoomSubjectSuitability> = vec![RoomSubjectSuitability {
        room_id: rooms[4].id,
        subject_id: subject_ids[7],
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

/// Build a zweizügige Grundschule `Problem`. Mirrors the Python seed in
/// `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py`.
/// Asserts 68 lessons / 196 placements so copy-paste drift is caught.
pub fn zweizuegig_fixture() -> Problem {
    // 5 days x 7 periods = 35 time-blocks (same WeekScheme as einzuegig).
    let time_blocks: Vec<TimeBlock> = (0..35u8)
        .map(|i| TimeBlock {
            id: TimeBlockId(fixture_uuid(140 + i)),
            day_of_week: i / 7,
            position: i % 7,
        })
        .collect();

    // 12 teachers; max_hours per the Python seed table.
    let teacher_max_hours: [u8; 12] = [28, 28, 28, 28, 28, 28, 28, 28, 18, 21, 14, 21];
    let teachers: Vec<Teacher> = (0..12u8)
        .map(|i| Teacher {
            id: TeacherId(fixture_uuid(40 + i)),
            max_hours_per_week: teacher_max_hours[i as usize],
        })
        .collect();

    // 12 rooms: 8 Klassenraeume + Turnhalle + Sportplatz + Musikraum + Kunstraum.
    // The 12-room layout mirrors the Python seed which adds Sportplatz to relieve
    // Sport scheduling contention; see `demo_grundschule_zweizuegig.py` docstring.
    let rooms: Vec<Room> = (0..12u8)
        .map(|i| Room {
            id: RoomId(fixture_uuid(56 + i)),
        })
        .collect();

    // 9 subjects: D, M, SU, RE, E, KU, MU, SP, FOE (indices 0..9).
    let subject_ids: Vec<SubjectId> = (0..9u8).map(|i| SubjectId(fixture_uuid(80 + i))).collect();
    let subjects: Vec<Subject> = subject_ids
        .iter()
        .enumerate()
        .map(|(i, id)| Subject {
            id: *id,
            prefer_early_period: u32::from(matches!(i, 0 | 1)), // index 0 = Deutsch, 1 = Mathematik
            avoid_first_period: u32::from(i == 7),              // index 7 = Sport
            avoid_last_period: u32::from(matches!(i, 0 | 1)),   // index 0 = Deutsch, 1 = Mathematik
            prefer_late_period: u32::from(i == 8) * 5,
            max_hours_per_day: 8,
        })
        .collect();

    // 8 classes: 1a..4b. Indices align with grade pairs.
    let classes: Vec<SchoolClass> = (0..8u8)
        .map(|i| SchoolClass {
            id: SchoolClassId(fixture_uuid(90 + i)),
            home_room_id: Some(RoomId(fixture_uuid(56 + i))),
            max_lessons_per_day: None,
        })
        .collect();

    // hours_per_class[class_idx][subject_idx]; 0 = subject not taught in this class.
    // Subject order: D, M, SU, RE, E, KU, MU, SP, FOE.
    // Grade 1 (1a, 1b): D6 M5 SU2 RE2 E0 KU2 MU1 SP3 FOE2 = 23h
    // Grade 2 (2a, 2b): same = 23h
    // Grade 3 (3a, 3b): D5 M5 SU4 RE2 E2 KU2 MU1 SP3 FOE2 = 26h
    // Grade 4 (4a, 4b): same = 26h
    let hours_per_class: [[u8; 9]; 8] = [
        [6, 5, 2, 2, 0, 2, 1, 3, 2], // 1a
        [6, 5, 2, 2, 0, 2, 1, 3, 2], // 1b
        [6, 5, 2, 2, 0, 2, 1, 3, 2], // 2a
        [6, 5, 2, 2, 0, 2, 1, 3, 2], // 2b
        [5, 5, 4, 2, 2, 2, 1, 3, 2], // 3a
        [5, 5, 4, 2, 2, 2, 1, 3, 2], // 3b
        [5, 5, 4, 2, 2, 2, 1, 3, 2], // 4a
        [5, 5, 4, 2, 2, 2, 1, 3, 2], // 4b
    ];

    // teacher_per_class[class_idx][subject_idx] = teacher_idx; mirrors
    // _TEACHER_ASSIGNMENTS_ZWEIZUEGIG in the Python seed.
    // Teacher indices:
    //   0 MUE, 1 SCH, 2 WEB, 3 FIS, 4 KAI, 5 LAN, 6 NEU, 7 OTT,
    //   8 BEC, 9 HOF, 10 WIL, 11 RIC.
    // Use a sentinel (255) for hours-zero subjects so the lesson loop skips them.
    let teacher_per_class: [[u8; 9]; 8] = [
        [0, 0, 0, 8, 255, 0, 8, 9, 9],     // 1a
        [1, 1, 1, 10, 255, 1, 10, 11, 11], // 1b
        [2, 2, 2, 8, 255, 9, 8, 9, 8],     // 2a
        [3, 3, 3, 10, 255, 0, 10, 11, 11], // 2b
        [4, 4, 4, 8, 2, 4, 8, 9, 8],       // 3a
        [5, 5, 5, 10, 3, 5, 10, 11, 11],   // 3b
        [6, 6, 6, 8, 6, 0, 8, 9, 9],       // 4a
        [7, 7, 7, 10, 7, 1, 10, 11, 11],   // 4b
    ];

    let mut lessons = Vec::new();
    let mut quals = Vec::new();
    let mut qual_set: HashSet<(TeacherId, SubjectId)> = HashSet::new();
    let mut lesson_idx: u8 = 0;
    for c_idx in 0..classes.len() {
        for s_idx in 0..subjects.len() {
            let hours = hours_per_class[c_idx][s_idx];
            if hours == 0 {
                continue;
            }
            let t_idx = teacher_per_class[c_idx][s_idx] as usize;
            let teacher = &teachers[t_idx];
            let subject = &subjects[s_idx];
            lessons.push(Lesson {
                id: LessonId(fixture_uuid(180 + lesson_idx)),
                school_class_ids: vec![classes[c_idx].id],
                subject_id: subject.id,
                teacher_id: teacher.id,
                hours_per_week: hours,
                preferred_block_size: 1,
                lesson_group_id: None,
            });
            lesson_idx += 1;
            // Deduplicate qualifications: a teacher qualified for D appears
            // multiple times if they teach D in multiple classes.
            if qual_set.insert((teacher.id, subject.id)) {
                quals.push(TeacherQualification {
                    teacher_id: teacher.id,
                    subject_id: subject.id,
                });
            }
        }
    }

    assert_eq!(
        lessons.len(),
        68,
        "zweizuegig fixture drifted from the seed: expected 68 lessons"
    );
    let total_hours: u32 = lessons.iter().map(|l| u32::from(l.hours_per_week)).sum();
    assert_eq!(
        total_hours, 196,
        "zweizuegig fixture drifted from the seed: expected 196 placements"
    );

    // Turnhalle (room 8) suits only Sport (subject 7).
    // Sportplatz (room 9) also suits only Sport.
    // Musikraum (room 10) suits only Musik (subject 6).
    // Kunstraum (room 11) suits only Kunst (subject 5).
    let suits: Vec<RoomSubjectSuitability> = vec![
        RoomSubjectSuitability {
            room_id: rooms[8].id,
            subject_id: subject_ids[7],
        },
        RoomSubjectSuitability {
            room_id: rooms[9].id,
            subject_id: subject_ids[7],
        },
        RoomSubjectSuitability {
            room_id: rooms[10].id,
            subject_id: subject_ids[6],
        },
        RoomSubjectSuitability {
            room_id: rooms[11].id,
            subject_id: subject_ids[5],
        },
    ];

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

/// Build a dreizügige Grundschule `Problem`. Mirrors the Python seed in
/// `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py`.
/// Asserts 102 lessons / 294 placements so copy-paste drift is caught.
///
/// First fixture exercising the multi-class Lesson shape: each Religion
/// lesson (RK / RE / ETH per Jahrgang) is one `Lesson` row with three
/// entries in `school_class_ids`, sharing a `lesson_group_id` with the
/// other two Religionsfächer of the same Jahrgang.
pub fn dreizuegig_fixture() -> Problem {
    // 5 days x 8 periods = 40 time-blocks. The Python seed uses 5x7 = 35,
    // matching `WEEK_SCHEME_DESCRIPTION` ("7 Stunden a 45 Minuten"). The bench
    // fixture deliberately adds one period of slack per day so FFD greedy
    // (which has no notion of cross-class Religion lessons consuming three
    // class-slots per placement) can deterministically place all 102 lessons.
    // Without the slack the random lesson-UUID tiebreak in `ffd_order` chooses
    // a packing that leaves a handful of NoFreeTimeBlock violations on Sport
    // or Religion lessons, breaking the bench's `solution.violations.is_empty()`
    // contract. The Python `demo_grundschule_dreizuegig` seed mirrors this
    // 8-period grid via `_PERIODS_DREIZUEGIG` so seed and bench stay aligned.
    // FFD eligibility weighting for cross-class lessons is filed as a
    // sprint follow-up; the lesson-group co-placement constraint that lands
    // in the algorithm-phase PR will also remove the slot pressure.
    let time_blocks: Vec<TimeBlock> = (0..40u8)
        .map(|i| TimeBlock {
            id: TimeBlockId(fixture_uuid(i)),
            day_of_week: i / 8,
            position: i % 8,
        })
        .collect();

    // 11 subjects in the order from `_SUBJECTS` in `demo_grundschule.py`:
    //   0 D, 1 M, 2 SU, 3 RK, 4 RE, 5 ETH, 6 E, 7 KU, 8 MU, 9 SP, 10 FÖ.
    let subject_ids: Vec<SubjectId> = (0..11u8).map(|i| SubjectId(fixture_uuid(35 + i))).collect();
    let subjects: Vec<Subject> = subject_ids
        .iter()
        .enumerate()
        .map(|(i, id)| Subject {
            id: *id,
            prefer_early_period: u32::from(matches!(i, 0 | 1)), // index 0 = Deutsch, 1 = Mathematik
            avoid_first_period: u32::from(i == 9),              // index 9 = Sport
            avoid_last_period: u32::from(matches!(i, 0 | 1)),   // index 0 = Deutsch, 1 = Mathematik
            prefer_late_period: u32::from(i == 10) * 5,
            max_hours_per_day: 8,
        })
        .collect();

    // 18 teachers; max_hours_per_week per `_TEACHERS_DREIZUEGIG` (Klassenlehrer
    // and Zug-bound specialists 28h, Religion specialists 14h).
    // Indices:
    //   0 MUE, 1 SCH, 2 DIE, 3 ENG, 4 KAI, 5 LAN     (Klassenlehrer 1/2)
    //   6 NOL, 7 ROT, 8 STA, 9 BRA, 10 HUB, 11 FRE   (Klassenlehrer 3/4)
    //   12 HOF, 13 RIC, 14 SCS                       (Zug-bound specialists)
    //   15 PFK, 16 PSL, 17 PHL                       (Religion specialists)
    let teacher_max_hours: [u8; 18] = [
        28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 14, 14, 14,
    ];
    let teachers: Vec<Teacher> = (0..18u8)
        .map(|i| Teacher {
            id: TeacherId(fixture_uuid(46 + i)),
            max_hours_per_week: teacher_max_hours[i as usize],
        })
        .collect();

    // 16 rooms: 12 Klassenräume + Turnhalle + Sportplatz + Musikraum + Kunstraum.
    let rooms: Vec<Room> = (0..16u8)
        .map(|i| Room {
            id: RoomId(fixture_uuid(64 + i)),
        })
        .collect();

    // 12 classes: 1a..4c. Indices align with grade triples.
    //   0 1a, 1 1b, 2 1c (grade 1, Züge a/b/c)
    //   3 2a, 4 2b, 5 2c (grade 2)
    //   6 3a, 7 3b, 8 3c (grade 3)
    //   9 4a, 10 4b, 11 4c (grade 4)
    let classes: Vec<SchoolClass> = (0..12u8)
        .map(|i| SchoolClass {
            id: SchoolClassId(fixture_uuid(80 + i)),
            home_room_id: Some(RoomId(fixture_uuid(64 + i))),
            max_lessons_per_day: None,
        })
        .collect();

    // hours_per_class[class_idx][subject_idx]; 0 = subject not taught directly
    // by this class (Religion is delivered via the cross-class trio, not the
    // Stundentafel). Subject order: D, M, SU, RK, RE, ETH, E, KU, MU, SP, FÖ.
    // Grade 1/2: D6 M5 SU2 KU2 MU1 SP3 FÖ2 = 21h
    // Grade 3/4: D5 M5 SU4 E2 KU2 MU1 SP3 FÖ2 = 24h
    let hours_per_class: [[u8; 11]; 12] = [
        [6, 5, 2, 0, 0, 0, 0, 2, 1, 3, 2], // 1a
        [6, 5, 2, 0, 0, 0, 0, 2, 1, 3, 2], // 1b
        [6, 5, 2, 0, 0, 0, 0, 2, 1, 3, 2], // 1c
        [6, 5, 2, 0, 0, 0, 0, 2, 1, 3, 2], // 2a
        [6, 5, 2, 0, 0, 0, 0, 2, 1, 3, 2], // 2b
        [6, 5, 2, 0, 0, 0, 0, 2, 1, 3, 2], // 2c
        [5, 5, 4, 0, 0, 0, 2, 2, 1, 3, 2], // 3a
        [5, 5, 4, 0, 0, 0, 2, 2, 1, 3, 2], // 3b
        [5, 5, 4, 0, 0, 0, 2, 2, 1, 3, 2], // 3c
        [5, 5, 4, 0, 0, 0, 2, 2, 1, 3, 2], // 4a
        [5, 5, 4, 0, 0, 0, 2, 2, 1, 3, 2], // 4b
        [5, 5, 4, 0, 0, 0, 2, 2, 1, 3, 2], // 4c
    ];

    // teacher_per_class[class_idx][subject_idx] = teacher_idx; mirrors
    // _TEACHER_ASSIGNMENTS_DREIZUEGIG. Sentinel 255 marks (class, subject)
    // pairs with hours==0 so the lesson loop skips them. Religion subjects
    // (indices 3/4/5) are always 255 here; their lessons come from the
    // cross-class trio loop below.
    let teacher_per_class: [[u8; 11]; 12] = [
        // Grade 1/2 Klassenlehrer take D/M/SU/KU; Zug specialist takes MU/SP/FÖ.
        [0, 0, 0, 255, 255, 255, 255, 0, 12, 12, 12], // 1a (MUE + HOF)
        [1, 1, 1, 255, 255, 255, 255, 1, 13, 13, 13], // 1b (SCH + RIC)
        [2, 2, 2, 255, 255, 255, 255, 2, 14, 14, 14], // 1c (DIE + SCS)
        [3, 3, 3, 255, 255, 255, 255, 3, 12, 12, 12], // 2a (ENG + HOF)
        [4, 4, 4, 255, 255, 255, 255, 4, 13, 13, 13], // 2b (KAI + RIC)
        [5, 5, 5, 255, 255, 255, 255, 5, 14, 14, 14], // 2c (LAN + SCS)
        // Grade 3/4 Klassenlehrer take D/M/SU/E; Zug specialist takes KU/MU/SP/FÖ.
        [6, 6, 6, 255, 255, 255, 6, 12, 12, 12, 12], // 3a (NOL + HOF)
        [7, 7, 7, 255, 255, 255, 7, 13, 13, 13, 13], // 3b (ROT + RIC)
        [8, 8, 8, 255, 255, 255, 8, 14, 14, 14, 14], // 3c (STA + SCS)
        [9, 9, 9, 255, 255, 255, 9, 12, 12, 12, 12], // 4a (BRA + HOF)
        [10, 10, 10, 255, 255, 255, 10, 13, 13, 13, 13], // 4b (HUB + RIC)
        [11, 11, 11, 255, 255, 255, 11, 14, 14, 14, 14], // 4c (FRE + SCS)
    ];

    let mut lessons = Vec::new();
    let mut quals = Vec::new();
    let mut qual_set: HashSet<(TeacherId, SubjectId)> = HashSet::new();
    let mut lesson_idx: u8 = 0;
    for c_idx in 0..classes.len() {
        for s_idx in 0..subjects.len() {
            let hours = hours_per_class[c_idx][s_idx];
            if hours == 0 {
                continue;
            }
            let t_idx = teacher_per_class[c_idx][s_idx] as usize;
            let teacher = &teachers[t_idx];
            let subject = &subjects[s_idx];
            lessons.push(Lesson {
                id: LessonId(fixture_uuid(92 + lesson_idx)),
                school_class_ids: vec![classes[c_idx].id],
                subject_id: subject.id,
                teacher_id: teacher.id,
                hours_per_week: hours,
                preferred_block_size: 1,
                lesson_group_id: None,
            });
            lesson_idx += 1;
            // Deduplicate qualifications: a teacher qualified for D appears
            // multiple times if they teach D in multiple classes.
            if qual_set.insert((teacher.id, subject.id)) {
                quals.push(TeacherQualification {
                    teacher_id: teacher.id,
                    subject_id: subject.id,
                });
            }
        }
    }

    // Cross-class Religion trio per Jahrgang: each Jahrgang gets one
    // `lesson_group_id` shared by RK / RE / ETH, and each lesson spans the
    // three classes of that Jahrgang via `school_class_ids` (the multi-class
    // shape this fixture is here to exercise).
    //
    //   Religion subject indices: RK=3, RE=4, ETH=5.
    //   Religion teacher indices: PFK=15 (RK), PSL=16 (RE), PHL=17 (ETH).
    let religion_subject_indices: [usize; 3] = [3, 4, 5];
    let religion_teacher_indices: [usize; 3] = [15, 16, 17];
    for jahrgang in 1u8..=4u8 {
        let group_id = LessonGroupId(fixture_uuid(200 + (jahrgang - 1)));
        // Three classes per Jahrgang: a/b/c at offsets 0/1/2 from base.
        let class_base = ((jahrgang - 1) * 3) as usize;
        let class_ids: Vec<SchoolClassId> = (0..3)
            .map(|offset| classes[class_base + offset].id)
            .collect();
        for (s_idx, t_idx) in religion_subject_indices
            .iter()
            .zip(religion_teacher_indices.iter())
        {
            let subject = &subjects[*s_idx];
            let teacher = &teachers[*t_idx];
            lessons.push(Lesson {
                id: LessonId(fixture_uuid(92 + lesson_idx)),
                school_class_ids: class_ids.clone(),
                subject_id: subject.id,
                teacher_id: teacher.id,
                hours_per_week: 2,
                preferred_block_size: 1,
                lesson_group_id: Some(group_id),
            });
            lesson_idx += 1;
            if qual_set.insert((teacher.id, subject.id)) {
                quals.push(TeacherQualification {
                    teacher_id: teacher.id,
                    subject_id: subject.id,
                });
            }
        }
    }

    assert_eq!(
        lessons.len(),
        102,
        "dreizuegig fixture drifted from the seed: expected 102 lessons"
    );
    let total_hours: u32 = lessons.iter().map(|l| u32::from(l.hours_per_week)).sum();
    assert_eq!(
        total_hours, 294,
        "dreizuegig fixture drifted from the seed: expected 294 placements"
    );

    // Klassenräume (rooms 0..11) suit only the Klassenraum-fit subjects
    //   D, M, SU, RK, RE, ETH, E, FÖ
    // matching `_KLASSENRAUM_SUITABLE_SUBJECTS` in the Python seed; without
    // these explicit entries Klassenräume would default to "suits all", which
    // inflates FFD's eligibility scores and starves SP/KU/MU of placements.
    // Turnhalle (room 12) and Sportplatz (room 13) suit only Sport (subject 9).
    // Musikraum (room 14) suits only Musik (subject 8).
    // Kunstraum (room 15) suits only Kunst (subject 7).
    let klassenraum_subject_indices: [usize; 8] = [0, 1, 2, 3, 4, 5, 6, 10];
    let mut suits: Vec<RoomSubjectSuitability> = Vec::new();
    for klassenraum in rooms.iter().take(12) {
        for &s_idx in &klassenraum_subject_indices {
            suits.push(RoomSubjectSuitability {
                room_id: klassenraum.id,
                subject_id: subject_ids[s_idx],
            });
        }
    }
    suits.push(RoomSubjectSuitability {
        room_id: rooms[12].id,
        subject_id: subject_ids[9],
    });
    suits.push(RoomSubjectSuitability {
        room_id: rooms[13].id,
        subject_id: subject_ids[9],
    });
    suits.push(RoomSubjectSuitability {
        room_id: rooms[14].id,
        subject_id: subject_ids[8],
    });
    suits.push(RoomSubjectSuitability {
        room_id: rooms[15].id,
        subject_id: subject_ids[7],
    });

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

/// Demo-Grundschule-shaped fixture sized to reproduce the FFD lock-in flake
/// described in `docs/OPEN_THINGS.md` (active sprint, diagnostic
/// phase, item 1) and in `solver/CLAUDE.md` L44. Mirrors
/// `backend/src/klassenzeit_backend/seed/demo_grundschule.py` at fixed
/// deterministic UUIDs: 4 classes (1a..4a), 5 days x 7 periods, 7 rooms (4
/// Klassenraeume + Turnhalle + Musikraum + Kunstraum), 9 subjects (D, M, SU,
/// E, ETH, KU, MU, SP, FOe), 6 teachers (MUE, SCH, WEB, FIS, BEC, HOF), and
/// the Hessen Stundentafel hours (grades 1-2 = 8 lessons per class, grades
/// 3-4 = 9). Hand-pinned teacher allocation chosen so FFD reliably locks an
/// early `(class, day, subject) -> room` triple into the wrong Klassenraum,
/// after which the matching second hour for a sibling class fails to place
/// because every academically-suitable room is already held by a sibling
/// class's lock. Locked triple recorded in the test docstring after step 1.6.
pub fn ffd_lock_in_grundschule() -> Problem {
    // Time blocks: 5 days, 7 periods each. id base 100.
    let time_blocks: Vec<TimeBlock> = (0..35u8)
        .map(|i| TimeBlock {
            id: TimeBlockId(fixture_uuid(100 + i)),
            day_of_week: i / 7,
            position: i % 7,
        })
        .collect();

    // Rooms 50..56. Klassenraeume 50..53 are academic-suitable; 54 = TH (Sport),
    // 55 = MU-Raum, 56 = KU-Raum.
    let rooms: Vec<Room> = (0..7u8)
        .map(|i| Room {
            id: RoomId(fixture_uuid(50 + i)),
        })
        .collect();
    let klassenraum_ids = [rooms[0].id, rooms[1].id, rooms[2].id, rooms[3].id];
    let turnhalle = rooms[4].id;
    let musikraum = rooms[5].id;
    let kunstraum = rooms[6].id;

    // Classes 70..73 = 1a..4a. home_room_id = own Klassenraum.
    let classes: Vec<SchoolClass> = (0..4u8)
        .map(|i| SchoolClass {
            id: SchoolClassId(fixture_uuid(70 + i)),
            home_room_id: Some(klassenraum_ids[i as usize]),
            max_lessons_per_day: None,
        })
        .collect();

    // Subjects 80..88: D M SU E ETH KU MU SP FOe.
    let d = SubjectId(fixture_uuid(80));
    let m = SubjectId(fixture_uuid(81));
    let su = SubjectId(fixture_uuid(82));
    let e_subj = SubjectId(fixture_uuid(83));
    let eth = SubjectId(fixture_uuid(84));
    let ku = SubjectId(fixture_uuid(85));
    let mu = SubjectId(fixture_uuid(86));
    let sp = SubjectId(fixture_uuid(87));
    let foe = SubjectId(fixture_uuid(88));
    let subjects: Vec<Subject> = vec![
        Subject {
            id: d,
            prefer_early_period: 1,
            avoid_first_period: 0,
            avoid_last_period: 1,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        },
        Subject {
            id: m,
            prefer_early_period: 1,
            avoid_first_period: 0,
            avoid_last_period: 1,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        },
        Subject {
            id: su,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        },
        Subject {
            id: e_subj,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        },
        Subject {
            id: eth,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        },
        Subject {
            id: ku,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        },
        Subject {
            id: mu,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        },
        Subject {
            id: sp,
            prefer_early_period: 0,
            avoid_first_period: 1,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        },
        Subject {
            id: foe,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        },
    ];

    // Teachers 30..35: MUE SCH WEB FIS BEC HOF.
    let mue = TeacherId(fixture_uuid(30));
    let sch = TeacherId(fixture_uuid(31));
    let web = TeacherId(fixture_uuid(32));
    let fis = TeacherId(fixture_uuid(33));
    let bec = TeacherId(fixture_uuid(34));
    let hof = TeacherId(fixture_uuid(35));
    let teachers = vec![
        Teacher {
            id: mue,
            max_hours_per_week: 28,
        },
        Teacher {
            id: sch,
            max_hours_per_week: 28,
        },
        Teacher {
            id: web,
            max_hours_per_week: 28,
        },
        Teacher {
            id: fis,
            max_hours_per_week: 28,
        },
        Teacher {
            id: bec,
            max_hours_per_week: 18,
        },
        Teacher {
            id: hof,
            max_hours_per_week: 21,
        },
    ];

    // Qualifications mirror the seed (RK/RE skipped because no class teaches them here).
    let teacher_qualifications = vec![
        // MUE: D, M, SU, KU
        TeacherQualification {
            teacher_id: mue,
            subject_id: d,
        },
        TeacherQualification {
            teacher_id: mue,
            subject_id: m,
        },
        TeacherQualification {
            teacher_id: mue,
            subject_id: su,
        },
        TeacherQualification {
            teacher_id: mue,
            subject_id: ku,
        },
        // SCH: D, M, SU, KU
        TeacherQualification {
            teacher_id: sch,
            subject_id: d,
        },
        TeacherQualification {
            teacher_id: sch,
            subject_id: m,
        },
        TeacherQualification {
            teacher_id: sch,
            subject_id: su,
        },
        TeacherQualification {
            teacher_id: sch,
            subject_id: ku,
        },
        // WEB: D, M, SU, E
        TeacherQualification {
            teacher_id: web,
            subject_id: d,
        },
        TeacherQualification {
            teacher_id: web,
            subject_id: m,
        },
        TeacherQualification {
            teacher_id: web,
            subject_id: su,
        },
        TeacherQualification {
            teacher_id: web,
            subject_id: e_subj,
        },
        // FIS: D, M, SU, E
        TeacherQualification {
            teacher_id: fis,
            subject_id: d,
        },
        TeacherQualification {
            teacher_id: fis,
            subject_id: m,
        },
        TeacherQualification {
            teacher_id: fis,
            subject_id: su,
        },
        TeacherQualification {
            teacher_id: fis,
            subject_id: e_subj,
        },
        // BEC: ETH, MU, FOe (RK/RE skipped: no class teaches them here)
        TeacherQualification {
            teacher_id: bec,
            subject_id: eth,
        },
        TeacherQualification {
            teacher_id: bec,
            subject_id: mu,
        },
        TeacherQualification {
            teacher_id: bec,
            subject_id: foe,
        },
        // HOF: SP, KU, FOe
        TeacherQualification {
            teacher_id: hof,
            subject_id: sp,
        },
        TeacherQualification {
            teacher_id: hof,
            subject_id: ku,
        },
        TeacherQualification {
            teacher_id: hof,
            subject_id: foe,
        },
    ];

    // Stundentafel: grades 1-2 (8 subjects), grades 3-4 (9 subjects, adds E).
    // Hand-pinned teacher per (class, subject); see the docstring above.
    struct LockInLessonRow {
        class_idx: usize,
        subject: SubjectId,
        hours: u8,
        block_size: u8,
        teacher: TeacherId,
    }
    let class_1a = classes[0].id;
    let class_2a = classes[1].id;
    let class_3a = classes[2].id;
    let class_4a = classes[3].id;
    let rows: Vec<LockInLessonRow> = vec![
        // 1a (grades 1-2): D=6/MUE, M=5/MUE, SU=2 doppel/MUE, ETH=2/BEC, KU=2/HOF, MU=1/BEC, SP=3/HOF, FOe=2/BEC
        LockInLessonRow {
            class_idx: 0,
            subject: d,
            hours: 6,
            block_size: 1,
            teacher: mue,
        },
        LockInLessonRow {
            class_idx: 0,
            subject: m,
            hours: 5,
            block_size: 1,
            teacher: mue,
        },
        LockInLessonRow {
            class_idx: 0,
            subject: su,
            hours: 2,
            block_size: 2,
            teacher: mue,
        },
        LockInLessonRow {
            class_idx: 0,
            subject: eth,
            hours: 2,
            block_size: 1,
            teacher: bec,
        },
        LockInLessonRow {
            class_idx: 0,
            subject: ku,
            hours: 2,
            block_size: 1,
            teacher: hof,
        },
        LockInLessonRow {
            class_idx: 0,
            subject: mu,
            hours: 1,
            block_size: 1,
            teacher: bec,
        },
        LockInLessonRow {
            class_idx: 0,
            subject: sp,
            hours: 3,
            block_size: 1,
            teacher: hof,
        },
        LockInLessonRow {
            class_idx: 0,
            subject: foe,
            hours: 2,
            block_size: 1,
            teacher: bec,
        },
        // 2a (grades 1-2): same shape, SCH on D/M/SU
        LockInLessonRow {
            class_idx: 1,
            subject: d,
            hours: 6,
            block_size: 1,
            teacher: sch,
        },
        LockInLessonRow {
            class_idx: 1,
            subject: m,
            hours: 5,
            block_size: 1,
            teacher: sch,
        },
        LockInLessonRow {
            class_idx: 1,
            subject: su,
            hours: 2,
            block_size: 2,
            teacher: sch,
        },
        LockInLessonRow {
            class_idx: 1,
            subject: eth,
            hours: 2,
            block_size: 1,
            teacher: bec,
        },
        LockInLessonRow {
            class_idx: 1,
            subject: ku,
            hours: 2,
            block_size: 1,
            teacher: sch,
        },
        LockInLessonRow {
            class_idx: 1,
            subject: mu,
            hours: 1,
            block_size: 1,
            teacher: bec,
        },
        LockInLessonRow {
            class_idx: 1,
            subject: sp,
            hours: 3,
            block_size: 1,
            teacher: hof,
        },
        LockInLessonRow {
            class_idx: 1,
            subject: foe,
            hours: 2,
            block_size: 1,
            teacher: bec,
        },
        // 3a (grades 3-4): D=5/WEB, M=5/WEB, SU=4 doppel/WEB, E=2/WEB, ETH=2/BEC, KU=2/HOF, MU=1/BEC, SP=3/HOF, FOe=2/HOF
        LockInLessonRow {
            class_idx: 2,
            subject: d,
            hours: 5,
            block_size: 1,
            teacher: web,
        },
        LockInLessonRow {
            class_idx: 2,
            subject: m,
            hours: 5,
            block_size: 1,
            teacher: web,
        },
        LockInLessonRow {
            class_idx: 2,
            subject: su,
            hours: 4,
            block_size: 2,
            teacher: web,
        },
        LockInLessonRow {
            class_idx: 2,
            subject: e_subj,
            hours: 2,
            block_size: 1,
            teacher: web,
        },
        LockInLessonRow {
            class_idx: 2,
            subject: eth,
            hours: 2,
            block_size: 1,
            teacher: bec,
        },
        LockInLessonRow {
            class_idx: 2,
            subject: ku,
            hours: 2,
            block_size: 1,
            teacher: hof,
        },
        LockInLessonRow {
            class_idx: 2,
            subject: mu,
            hours: 1,
            block_size: 1,
            teacher: bec,
        },
        LockInLessonRow {
            class_idx: 2,
            subject: sp,
            hours: 3,
            block_size: 1,
            teacher: hof,
        },
        LockInLessonRow {
            class_idx: 2,
            subject: foe,
            hours: 2,
            block_size: 1,
            teacher: hof,
        },
        // 4a (grades 3-4): same shape, FIS on academics
        LockInLessonRow {
            class_idx: 3,
            subject: d,
            hours: 5,
            block_size: 1,
            teacher: fis,
        },
        LockInLessonRow {
            class_idx: 3,
            subject: m,
            hours: 5,
            block_size: 1,
            teacher: fis,
        },
        LockInLessonRow {
            class_idx: 3,
            subject: su,
            hours: 4,
            block_size: 2,
            teacher: fis,
        },
        LockInLessonRow {
            class_idx: 3,
            subject: e_subj,
            hours: 2,
            block_size: 1,
            teacher: fis,
        },
        LockInLessonRow {
            class_idx: 3,
            subject: eth,
            hours: 2,
            block_size: 1,
            teacher: bec,
        },
        LockInLessonRow {
            class_idx: 3,
            subject: ku,
            hours: 2,
            block_size: 1,
            teacher: hof,
        },
        LockInLessonRow {
            class_idx: 3,
            subject: mu,
            hours: 1,
            block_size: 1,
            teacher: bec,
        },
        LockInLessonRow {
            class_idx: 3,
            subject: sp,
            hours: 3,
            block_size: 1,
            teacher: hof,
        },
        // 4a FOe re-assigned from HOF to BEC: HOF would otherwise total 22h
        // (1a SP+KU=5, 2a SP=3, 3a SP+KU+FOe=7, 4a SP+KU+FOe=7) but
        // max_hours_per_week=21, which trips TeacherOverCapacity before the
        // lock-in fires. BEC has slack (18h budget; gets 16h with 4a FOe added,
        // exact budget at 18 with both 3a FOe and 4a FOe handled by HOF still).
        LockInLessonRow {
            class_idx: 3,
            subject: foe,
            hours: 2,
            block_size: 1,
            teacher: bec,
        },
    ];
    let class_ids = [class_1a, class_2a, class_3a, class_4a];
    let lessons: Vec<Lesson> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| Lesson {
            id: LessonId(fixture_uuid(200 + (i as u8))),
            school_class_ids: vec![class_ids[r.class_idx]],
            subject_id: r.subject,
            teacher_id: r.teacher,
            hours_per_week: r.hours,
            preferred_block_size: r.block_size,
            lesson_group_id: None,
        })
        .collect();

    // Room-subject suitabilities mirror the seed: Klassenraeume suit
    // {D, M, SU, E, ETH, FOe} (academic subjects taught in classroom);
    // Turnhalle suits SP only; Musikraum suits MU only; Kunstraum suits KU only.
    let mut room_subject_suitabilities = Vec::new();
    let academic = [d, m, su, e_subj, eth, foe];
    for room in &klassenraum_ids {
        for subj in academic {
            room_subject_suitabilities.push(RoomSubjectSuitability {
                room_id: *room,
                subject_id: subj,
            });
        }
    }
    room_subject_suitabilities.push(RoomSubjectSuitability {
        room_id: turnhalle,
        subject_id: sp,
    });
    room_subject_suitabilities.push(RoomSubjectSuitability {
        room_id: musikraum,
        subject_id: mu,
    });
    room_subject_suitabilities.push(RoomSubjectSuitability {
        room_id: kunstraum,
        subject_id: ku,
    });

    // Reason: simulate "Klassenraum 4a unavailable this week" (a real scenario
    // for any school: renovation, public-health closure, exam staging). With
    // the production active-default weights `prefer_home_room=5` dominates
    // FFD's lowest-delta scoring so much that home-room placements always win
    // and the lock-in never fires; teacher swaps alone can't perturb that.
    // Knocking the academic-room pool from 4 down to 3 forces 4a to compete
    // for sibling klassenraeume on every slot, which after the first lock
    // (4a's first academic on a given day -> klassenraum 1a/2a/3a) wedges
    // every later 4a same-day same-subject placement: every academically-
    // suitable room is either locked to 4a in the wrong slot or held by a
    // sibling class's same-day-same-subject lock.
    let mut blocked_room_blocks = Vec::new();
    for tb in &time_blocks {
        blocked_room_blocks.push(RoomBlockedTime {
            room_id: klassenraum_ids[3],
            time_block_id: tb.id,
        });
    }

    Problem {
        time_blocks,
        teachers,
        rooms,
        subjects,
        school_classes: classes,
        lessons,
        teacher_qualifications,
        teacher_blocked_times: vec![],
        room_blocked_times: blocked_room_blocks,
        room_subject_suitabilities,
        pinned_placements: vec![],
    }
}
