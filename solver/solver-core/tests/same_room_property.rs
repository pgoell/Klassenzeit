//! Hard-constraint property: for every solved problem, no `(class, day, subject)`
//! triple has placements in two different rooms.
//!
//! Two scenarios are covered:
//! 1. A targeted minimal fixture that forces a room hop without the constraint
//!    (low-id room blocked at position 0 so FFD must use the high-id room
//!    there; at position 1 the low-id room is free again and the FFD greedy
//!    would naturally pick it). The constraint forces the second placement to
//!    stay in the high-id room.
//! 2. A Grundschule-shaped fixture mirroring `tests/grundschule_smoke.rs`,
//!    asserted under non-trivial soft weights so the LAHC pass also runs.

use std::collections::HashMap;

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    solve_with_config,
    types::{
        ConstraintWeights, Lesson, Problem, Room, RoomBlockedTime, RoomSubjectSuitability,
        SchoolClass, Solution, SolveConfig, Subject, Teacher, TeacherQualification, TimeBlock,
        ViolationKind,
    },
};
use uuid::Uuid;

fn same_room_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

/// Assert no `(class, day, subject)` triple uses more than one room across the
/// solved placements.
fn assert_no_room_hopping(problem: &Problem, solution: &Solution) {
    let mut groups: HashMap<(SchoolClassId, u8, SubjectId), RoomId> = HashMap::new();
    for placement in &solution.placements {
        let lesson = problem
            .lessons
            .iter()
            .find(|l| l.id == placement.lesson_id)
            .expect("placement lesson exists");
        let tb = problem
            .time_blocks
            .iter()
            .find(|t| t.id == placement.time_block_id)
            .expect("placement time_block exists");
        for class in &lesson.school_class_ids {
            let key = (*class, tb.day_of_week, lesson.subject_id);
            let entry = groups.entry(key).or_insert(placement.room_id);
            assert_eq!(
                *entry, placement.room_id,
                "room hop within day for class {:?} day {} subject {:?}: rooms {:?} and {:?}",
                class, tb.day_of_week, lesson.subject_id, entry, placement.room_id
            );
        }
    }
}

/// Minimal fixture: one class, two hours of one subject on day 0, two rooms
/// where the lowest-id room is blocked at position 0 only. Without the
/// constraint, FFD would place hour 0 in the high-id room (r1) then hour 1 in
/// the low-id room (r0) on the same day, producing a room hop.
fn forced_hop_problem() -> Problem {
    let r0 = RoomId(same_room_uuid(30));
    let r1 = RoomId(same_room_uuid(31));
    let class = SchoolClassId(same_room_uuid(50));
    let subject = SubjectId(same_room_uuid(40));
    let teacher = TeacherId(same_room_uuid(20));
    let lesson = LessonId(same_room_uuid(60));
    let tb0 = TimeBlockId(same_room_uuid(10));
    let tb1 = TimeBlockId(same_room_uuid(11));

    Problem {
        time_blocks: vec![
            TimeBlock {
                id: tb0,
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: tb1,
                day_of_week: 0,
                position: 1,
            },
        ],
        teachers: vec![Teacher {
            id: teacher,
            max_hours_per_week: 10,
        }],
        rooms: vec![Room { id: r0 }, Room { id: r1 }],
        subjects: vec![Subject {
            id: subject,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
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
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: teacher,
            subject_id: subject,
        }],
        teacher_blocked_times: vec![],
        // Only r0 is blocked at position 0; r0 is the lowest-id room so FFD
        // would pick it everywhere else by default.
        room_blocked_times: vec![RoomBlockedTime {
            room_id: r0,
            time_block_id: tb0,
        }],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

#[test]
fn no_room_hopping_within_day_for_one_subject_minimal_forced_hop() {
    let problem = forced_hop_problem();
    let config = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 1,
            teacher_gap: 1,
            ..ConstraintWeights::default()
        },
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve");
    // Both hours must be placed (the constraint sticks them in the same room
    // rather than dropping a placement; r1 is feasible at both positions).
    assert_eq!(
        solution.placements.len(),
        2,
        "expected both hours placed; got {:?}",
        solution.placements
    );
    assert_no_room_hopping(&problem, &solution);
    // All Deutsch placements must share the high-id room (r1) since r0 is
    // blocked at position 0.
    let r1 = RoomId(same_room_uuid(31));
    for placement in &solution.placements {
        assert_eq!(
            placement.room_id, r1,
            "expected r1 for both placements; got {:?}",
            placement
        );
    }
}

/// Grundschule-shaped Problem mirroring `tests/grundschule_smoke.rs`. Two
/// classes, 5 weekdays × 5 periods, 5 rooms with the gym restricted to Sport.
fn same_room_grundschule() -> Problem {
    let time_blocks: Vec<TimeBlock> = (0..25)
        .map(|i| TimeBlock {
            id: TimeBlockId(same_room_uuid(100 + i)),
            day_of_week: i / 5,
            position: i % 5,
        })
        .collect();

    let teachers: Vec<Teacher> = (0..8)
        .map(|i| Teacher {
            id: TeacherId(same_room_uuid(30 + i)),
            max_hours_per_week: 28,
        })
        .collect();

    let rooms: Vec<Room> = (0..5)
        .map(|i| Room {
            id: RoomId(same_room_uuid(50 + i)),
        })
        .collect();

    let subject_ids: Vec<SubjectId> = (0..8).map(|i| SubjectId(same_room_uuid(60 + i))).collect();
    let subjects: Vec<Subject> = subject_ids
        .iter()
        .map(|id| Subject {
            id: *id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
        })
        .collect();

    let classes: Vec<SchoolClass> = (0..2)
        .map(|i| SchoolClass {
            id: SchoolClassId(same_room_uuid(70 + i)),
            home_room_id: Some(RoomId(same_room_uuid(50 + i))),
        })
        .collect();

    // Same Stundentafeln as grundschule_smoke.rs.
    let hours_per_class: [[u8; 8]; 2] = [[6, 5, 2, 0, 2, 1, 2, 3], [5, 5, 4, 2, 2, 1, 2, 3]];

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
                id: LessonId(same_room_uuid(200 + lesson_idx)),
                school_class_ids: vec![class.id],
                subject_id: subject.id,
                teacher_id: teacher.id,
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
fn no_room_hopping_within_day_for_one_subject_demo_grundschule() {
    let problem = same_room_grundschule();
    // Non-trivial weights including prefer_home_room so the LAHC pass exercises
    // moves that could re-introduce room hops.
    let config = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_home_room: 5,
            ..ConstraintWeights::default()
        },
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve");
    assert_no_room_hopping(&problem, &solution);
}

/// Demo-Grundschule-shaped fixture sized to reproduce the FFD lock-in flake
/// described in `docs/superpowers/OPEN_THINGS.md` (active sprint, diagnostic
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
fn ffd_lock_in_grundschule() -> Problem {
    // Time blocks: 5 days, 7 periods each. id base 100.
    let time_blocks: Vec<TimeBlock> = (0..35u8)
        .map(|i| TimeBlock {
            id: TimeBlockId(same_room_uuid(100 + i)),
            day_of_week: i / 7,
            position: i % 7,
        })
        .collect();

    // Rooms 50..56. Klassenraeume 50..53 are academic-suitable; 54 = TH (Sport),
    // 55 = MU-Raum, 56 = KU-Raum.
    let rooms: Vec<Room> = (0..7u8)
        .map(|i| Room {
            id: RoomId(same_room_uuid(50 + i)),
        })
        .collect();
    let klassenraum_ids = [rooms[0].id, rooms[1].id, rooms[2].id, rooms[3].id];
    let turnhalle = rooms[4].id;
    let musikraum = rooms[5].id;
    let kunstraum = rooms[6].id;

    // Classes 70..73 = 1a..4a. home_room_id = own Klassenraum.
    let classes: Vec<SchoolClass> = (0..4u8)
        .map(|i| SchoolClass {
            id: SchoolClassId(same_room_uuid(70 + i)),
            home_room_id: Some(klassenraum_ids[i as usize]),
        })
        .collect();

    // Subjects 80..88: D M SU E ETH KU MU SP FOe.
    let d = SubjectId(same_room_uuid(80));
    let m = SubjectId(same_room_uuid(81));
    let su = SubjectId(same_room_uuid(82));
    let e_subj = SubjectId(same_room_uuid(83));
    let eth = SubjectId(same_room_uuid(84));
    let ku = SubjectId(same_room_uuid(85));
    let mu = SubjectId(same_room_uuid(86));
    let sp = SubjectId(same_room_uuid(87));
    let foe = SubjectId(same_room_uuid(88));
    let subjects: Vec<Subject> = vec![
        Subject {
            id: d,
            prefer_early_period: 1,
            avoid_first_period: 0,
            avoid_last_period: 1,
            prefer_late_period: 0,
        },
        Subject {
            id: m,
            prefer_early_period: 1,
            avoid_first_period: 0,
            avoid_last_period: 1,
            prefer_late_period: 0,
        },
        Subject {
            id: su,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
        },
        Subject {
            id: e_subj,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
        },
        Subject {
            id: eth,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
        },
        Subject {
            id: ku,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
        },
        Subject {
            id: mu,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
        },
        Subject {
            id: sp,
            prefer_early_period: 0,
            avoid_first_period: 1,
            avoid_last_period: 0,
            prefer_late_period: 0,
        },
        Subject {
            id: foe,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
        },
    ];

    // Teachers 30..35: MUE SCH WEB FIS BEC HOF.
    let mue = TeacherId(same_room_uuid(30));
    let sch = TeacherId(same_room_uuid(31));
    let web = TeacherId(same_room_uuid(32));
    let fis = TeacherId(same_room_uuid(33));
    let bec = TeacherId(same_room_uuid(34));
    let hof = TeacherId(same_room_uuid(35));
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
            id: LessonId(same_room_uuid(200 + (i as u8))),
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

/// Asserts the FFD lock-in failure mode the active sprint's diagnostic phase
/// is built around: a Grundschule-shaped Problem produces at least one
/// `ViolationKind::NoSuitableRoom` under greedy-only solving with the
/// production active-default weights. The fixture pins teachers per the
/// builder's docstring and removes Klassenraum 4a from the academic pool
/// (`room_blocked_times`) for every slot to simulate the real-world
/// "renovation week" scenario; this is the smallest perturbation that
/// reliably trips the lock-in given that `prefer_home_room=5` in the active
/// defaults dominates greedy scoring otherwise.
///
/// Locked triple observed at this commit: `(class 4a, day 0, FÖ) -> klassenraum 1a`.
/// FFD locks 4a's first FÖ hour into a sibling Klassenraum because 4a's home
/// room is unavailable, then 4a's second FÖ hour cannot place because every
/// academically-suitable room across day 0 is either locked to 4a-FÖ at the
/// wrong position (klassenraum 1a) or busy with a sibling class's lesson.
///
/// When the active sprint's item 4 lands (Path A / B / C from item 3), that
/// PR renames this test to `ffd_does_not_lock_in_on_demo_grundschule` and
/// flips the assertion to `assert!(solution.violations.is_empty())`. The
/// rename is the visible signal that the regression became a guarantee.
#[test]
fn ffd_locks_in_on_demo_grundschule_and_returns_no_suitable_room() {
    let problem = ffd_lock_in_grundschule();
    let config = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_early_period: 1,
            avoid_first_period: 1,
            prefer_home_room: 5,
            avoid_last_period: 1,
            prefer_late_period: 1,
            class_day_balance: 5,
        },
        deadline: None, // greedy only; LAHC cannot escape the lock-in
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve");
    let no_suitable: Vec<_> = solution
        .violations
        .iter()
        .filter(|v| matches!(v.kind, ViolationKind::NoSuitableRoom))
        .collect();
    assert!(
        !no_suitable.is_empty(),
        "expected at least one NoSuitableRoom violation; got {:?}",
        solution.violations
    );
    let first = no_suitable.first().expect("non-empty by previous assert");
    let lesson = problem
        .lessons
        .iter()
        .find(|l| l.id == first.lesson_id)
        .expect("violation lesson exists in fixture");
    let foe_subject = SubjectId(same_room_uuid(88));
    assert_eq!(
        lesson.subject_id, foe_subject,
        "expected first NoSuitableRoom to be on subject FÖ; got {:?}; lock-in pattern may have shifted, update the test docstring",
        lesson.subject_id,
    );
}
