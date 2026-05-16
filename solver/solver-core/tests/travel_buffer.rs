//! Unit tests for the `validate_travel_buffer` post-condition validator
//! (ADR 0044 / item Schwimmunterricht). The validator's contract:
//!
//! For every placement whose lesson has `pre_buffer_minutes > 0`, the
//! preceding slot on the same day must either be a `kind == Break`
//! TimeBlock or carry no placement for the lesson's class AND no
//! placement for the lesson's teacher. Lessons with
//! `pre_buffer_minutes > 0` cannot be placed at day-position 0.
//!
//! Symmetric for `post_buffer_minutes > 0`: the slot at
//! `pos + preferred_block_size` (the first slot AFTER the lesson's block)
//! must be a break or free; otherwise validation fails. Lessons cannot
//! be placed when no such following slot exists (last position of day).
//!
//! Zero buffers no-op: a lesson with `pre_buffer_minutes == 0 &&
//! post_buffer_minutes == 0` never triggers a check.
//!
//! Self-conflict avoidance: the second slot of a Doppelstunde is occupied
//! by the lesson itself, not by a foreign placement. The validator must
//! skip same-`lesson_id` conflicts.

use std::time::Duration;

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::solve_with_config;
use solver_core::types::{
    ConstraintWeights, Lesson, PinKind, PinnedPlacement, Placement, Problem, Room, SchoolClass,
    SolveConfig, Subject, Teacher, TeacherQualification, TimeBlock, TimeBlockKind,
};
use solver_core::validate::validate_travel_buffer;
use uuid::Uuid;

fn tb_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([1, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
}

fn lesson_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([2, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
}

fn class_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([3, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
}

fn teacher_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([4, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
}

fn subject_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([5, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
}

fn room_uuid(n: u8) -> Uuid {
    Uuid::from_bytes([6, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
}

/// One day with five positions on day_of_week = 0. Position `break_pos`
/// (when `Some`) is marked as `TimeBlockKind::Break`; the rest are
/// `TimeBlockKind::Lesson`.
fn build_five_position_problem(break_pos: Option<u8>) -> Problem {
    let time_blocks: Vec<TimeBlock> = (0..5)
        .map(|p| TimeBlock {
            id: TimeBlockId(tb_uuid(p)),
            day_of_week: 0,
            position: p,
            kind: if Some(p) == break_pos {
                TimeBlockKind::Break
            } else {
                TimeBlockKind::Lesson
            },
        })
        .collect();

    let teacher_a = Teacher {
        id: TeacherId(teacher_uuid(1)),
        max_hours_per_week: 30,
        reserve_hours_per_week: 0,
    };
    let teacher_b = Teacher {
        id: TeacherId(teacher_uuid(2)),
        max_hours_per_week: 30,
        reserve_hours_per_week: 0,
    };

    let subject_schwimm = Subject {
        id: SubjectId(subject_uuid(1)),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: 0,
        max_hours_per_day: 8,
    };
    let subject_math = Subject {
        id: SubjectId(subject_uuid(2)),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: 0,
        max_hours_per_day: 8,
    };

    let class_a = SchoolClass {
        id: SchoolClassId(class_uuid(1)),
        home_room_id: None,
        max_lessons_per_day: None,
        class_teacher_id: None,
    };
    let class_b = SchoolClass {
        id: SchoolClassId(class_uuid(2)),
        home_room_id: None,
        max_lessons_per_day: None,
        class_teacher_id: None,
    };

    let room_one = Room {
        id: RoomId(room_uuid(1)),
    };
    let room_two = Room {
        id: RoomId(room_uuid(2)),
    };

    // Schwimm lesson: buffered (pre + post). Class A, Teacher A.
    let lesson_schwimm = Lesson {
        id: LessonId(lesson_uuid(1)),
        school_class_ids: vec![class_a.id],
        subject_id: subject_schwimm.id,
        teacher_candidates: vec![teacher_a.id],
        teacher_pin: Some(teacher_a.id),
        hours_per_week: 1,
        preferred_block_size: 1,
        pre_buffer_minutes: 15,
        post_buffer_minutes: 15,
        lesson_group_id: None,
    };
    // Adjacent lesson on same class (math, teacher B). Single-hour, no buffer.
    let lesson_math_a = Lesson {
        id: LessonId(lesson_uuid(2)),
        school_class_ids: vec![class_a.id],
        subject_id: subject_math.id,
        teacher_candidates: vec![teacher_b.id],
        teacher_pin: Some(teacher_b.id),
        hours_per_week: 1,
        preferred_block_size: 1,
        pre_buffer_minutes: 0,
        post_buffer_minutes: 0,
        lesson_group_id: None,
    };
    // Different class, but shares the schwimm teacher.
    let lesson_math_b = Lesson {
        id: LessonId(lesson_uuid(3)),
        school_class_ids: vec![class_b.id],
        subject_id: subject_math.id,
        teacher_candidates: vec![teacher_a.id],
        teacher_pin: Some(teacher_a.id),
        hours_per_week: 1,
        preferred_block_size: 1,
        pre_buffer_minutes: 0,
        post_buffer_minutes: 0,
        lesson_group_id: None,
    };

    Problem {
        time_blocks,
        teachers: vec![teacher_a.clone(), teacher_b.clone()],
        rooms: vec![room_one, room_two],
        subjects: vec![subject_schwimm.clone(), subject_math.clone()],
        school_classes: vec![class_a, class_b],
        lessons: vec![lesson_schwimm, lesson_math_a, lesson_math_b],
        teacher_qualifications: vec![
            TeacherQualification {
                teacher_id: teacher_a.id,
                subject_id: subject_schwimm.id,
            },
            TeacherQualification {
                teacher_id: teacher_a.id,
                subject_id: subject_math.id,
            },
            TeacherQualification {
                teacher_id: teacher_b.id,
                subject_id: subject_math.id,
            },
        ],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

#[test]
fn test_validate_travel_buffer_accepts_break_before() {
    // Position 0 = break, position 1 = schwimm (pre-buffered). The break
    // adjacency on the pre side satisfies the contract.
    let problem = build_five_position_problem(Some(0));
    let placements = vec![Placement {
        lesson_id: problem.lessons[0].id,
        time_block_id: problem.time_blocks[1].id,
        room_id: problem.rooms[0].id,
        teacher_id: problem.teachers[0].id,
    }];
    validate_travel_buffer(&problem, &placements).unwrap();
}

#[test]
fn test_validate_travel_buffer_accepts_free_class_before() {
    // No break anywhere. Schwimm lesson at position 2; nothing precedes it
    // on day 0. Class A is free at position 1, teacher A is free at
    // position 1: validator passes.
    let problem = build_five_position_problem(None);
    let placements = vec![Placement {
        lesson_id: problem.lessons[0].id,
        time_block_id: problem.time_blocks[2].id,
        room_id: problem.rooms[0].id,
        teacher_id: problem.teachers[0].id,
    }];
    validate_travel_buffer(&problem, &placements).unwrap();
}

#[test]
fn test_validate_travel_buffer_rejects_class_busy_before() {
    // Math lesson for class A at position 0; schwimm at position 1 for
    // class A. Pre-buffer side has a class A placement at position 0,
    // not a break: validator rejects.
    let problem = build_five_position_problem(None);
    let placements = vec![
        Placement {
            lesson_id: problem.lessons[1].id, // math_a, class A, teacher B
            time_block_id: problem.time_blocks[0].id,
            room_id: problem.rooms[1].id,
            teacher_id: problem.teachers[1].id,
        },
        Placement {
            lesson_id: problem.lessons[0].id, // schwimm
            time_block_id: problem.time_blocks[1].id,
            room_id: problem.rooms[0].id,
            teacher_id: problem.teachers[0].id,
        },
    ];
    let err = validate_travel_buffer(&problem, &placements).unwrap_err();
    let solver_core::Error::Input(msg) = err else {
        panic!("expected Error::Input, got {err:?}")
    };
    assert!(
        msg.contains("TravelBufferConflict"),
        "expected TravelBufferConflict in {msg}"
    );
    assert!(msg.contains("side=pre"), "expected side=pre in {msg}");
}

#[test]
fn test_validate_travel_buffer_rejects_teacher_busy_before() {
    // Math lesson for class B taught by teacher A at position 0; schwimm
    // at position 1 for class A taught by teacher A. Class A is free at
    // position 0 but teacher A is busy at position 0: validator rejects
    // (teacher-side conflict).
    let problem = build_five_position_problem(None);
    let placements = vec![
        Placement {
            lesson_id: problem.lessons[2].id, // math_b, class B, teacher A
            time_block_id: problem.time_blocks[0].id,
            room_id: problem.rooms[1].id,
            teacher_id: problem.teachers[0].id,
        },
        Placement {
            lesson_id: problem.lessons[0].id, // schwimm, class A, teacher A
            time_block_id: problem.time_blocks[1].id,
            room_id: problem.rooms[0].id,
            teacher_id: problem.teachers[0].id,
        },
    ];
    let err = validate_travel_buffer(&problem, &placements).unwrap_err();
    let solver_core::Error::Input(msg) = err else {
        panic!("expected Error::Input, got {err:?}")
    };
    assert!(
        msg.contains("TravelBufferConflict"),
        "expected TravelBufferConflict in {msg}"
    );
    assert!(msg.contains("side=pre"), "expected side=pre in {msg}");
    assert!(msg.contains("teacher="), "expected teacher= in {msg}");
}

#[test]
fn test_validate_travel_buffer_rejects_first_slot_of_day() {
    // Schwimm lesson at position 0 of day 0: nothing precedes it. A
    // pre-buffered lesson at the first position fails by construction.
    let problem = build_five_position_problem(None);
    let placements = vec![Placement {
        lesson_id: problem.lessons[0].id,
        time_block_id: problem.time_blocks[0].id,
        room_id: problem.rooms[0].id,
        teacher_id: problem.teachers[0].id,
    }];
    let err = validate_travel_buffer(&problem, &placements).unwrap_err();
    let solver_core::Error::Input(msg) = err else {
        panic!("expected Error::Input, got {err:?}")
    };
    assert!(
        msg.contains("TravelBufferConflict"),
        "expected TravelBufferConflict in {msg}"
    );
    assert!(msg.contains("side=pre"), "expected side=pre in {msg}");
}

#[test]
fn test_validate_travel_buffer_accepts_break_after() {
    // Schwimm at position 1; position 2 = break. Post-buffer side is a
    // break: validator accepts. Position 0 (pre) is a lesson slot but
    // also free, so pre side is satisfied too.
    let problem = build_five_position_problem(Some(2));
    let placements = vec![Placement {
        lesson_id: problem.lessons[0].id,
        time_block_id: problem.time_blocks[1].id,
        room_id: problem.rooms[0].id,
        teacher_id: problem.teachers[0].id,
    }];
    // Add a placement at position 0 in a DIFFERENT class to make pre
    // side trivially free for class A and teacher A.
    // Actually class B and teacher B free at position 0 means class A,
    // teacher A free at position 0 -> pre side passes.
    validate_travel_buffer(&problem, &placements).unwrap();
}

#[test]
fn test_validate_travel_buffer_accepts_free_class_after() {
    // Schwimm at position 1; positions 0, 2, 3, 4 all empty. Post side
    // at position 2 is free for class A and teacher A: passes.
    let problem = build_five_position_problem(None);
    let placements = vec![Placement {
        lesson_id: problem.lessons[0].id,
        time_block_id: problem.time_blocks[1].id,
        room_id: problem.rooms[0].id,
        teacher_id: problem.teachers[0].id,
    }];
    validate_travel_buffer(&problem, &placements).unwrap();
}

#[test]
fn test_validate_travel_buffer_rejects_class_busy_after() {
    // Schwimm for class A at position 1; math for class A at position 2.
    // Post-buffer side has a class A placement at position 2, not a
    // break: validator rejects.
    let problem = build_five_position_problem(None);
    let placements = vec![
        Placement {
            lesson_id: problem.lessons[0].id, // schwimm
            time_block_id: problem.time_blocks[1].id,
            room_id: problem.rooms[0].id,
            teacher_id: problem.teachers[0].id,
        },
        Placement {
            lesson_id: problem.lessons[1].id, // math_a, class A, teacher B
            time_block_id: problem.time_blocks[2].id,
            room_id: problem.rooms[1].id,
            teacher_id: problem.teachers[1].id,
        },
    ];
    let err = validate_travel_buffer(&problem, &placements).unwrap_err();
    let solver_core::Error::Input(msg) = err else {
        panic!("expected Error::Input, got {err:?}")
    };
    assert!(
        msg.contains("TravelBufferConflict"),
        "expected TravelBufferConflict in {msg}"
    );
    assert!(msg.contains("side=post"), "expected side=post in {msg}");
}

#[test]
fn test_validate_travel_buffer_rejects_teacher_busy_after() {
    // Schwimm at position 1 (class A, teacher A); math_b at position 2
    // (class B, teacher A). Class A is free at position 2 but teacher A
    // is busy at position 2: validator rejects (teacher-side).
    let problem = build_five_position_problem(None);
    let placements = vec![
        Placement {
            lesson_id: problem.lessons[0].id, // schwimm
            time_block_id: problem.time_blocks[1].id,
            room_id: problem.rooms[0].id,
            teacher_id: problem.teachers[0].id,
        },
        Placement {
            lesson_id: problem.lessons[2].id, // math_b, class B, teacher A
            time_block_id: problem.time_blocks[2].id,
            room_id: problem.rooms[1].id,
            teacher_id: problem.teachers[0].id,
        },
    ];
    let err = validate_travel_buffer(&problem, &placements).unwrap_err();
    let solver_core::Error::Input(msg) = err else {
        panic!("expected Error::Input, got {err:?}")
    };
    assert!(
        msg.contains("TravelBufferConflict"),
        "expected TravelBufferConflict in {msg}"
    );
    assert!(msg.contains("side=post"), "expected side=post in {msg}");
    assert!(msg.contains("teacher="), "expected teacher= in {msg}");
}

#[test]
fn test_validate_travel_buffer_rejects_last_slot_of_day() {
    // Schwimm at position 4 (last slot); nothing follows on day 0.
    // post_buffer_minutes > 0 with no following slot: validator rejects.
    let problem = build_five_position_problem(None);
    let placements = vec![Placement {
        lesson_id: problem.lessons[0].id,
        time_block_id: problem.time_blocks[4].id,
        room_id: problem.rooms[0].id,
        teacher_id: problem.teachers[0].id,
    }];
    // Need to NOT trip the pre-side check on position 4: class A free at
    // position 3 (no placements there), teacher A free at 3 -> pre OK.
    // Then the post side: position 5 does not exist -> reject.
    let err = validate_travel_buffer(&problem, &placements).unwrap_err();
    let solver_core::Error::Input(msg) = err else {
        panic!("expected Error::Input, got {err:?}")
    };
    assert!(
        msg.contains("TravelBufferConflict"),
        "expected TravelBufferConflict in {msg}"
    );
    assert!(msg.contains("side=post"), "expected side=post in {msg}");
}

#[test]
fn test_validate_travel_buffer_zero_buffers_no_op() {
    // Math_a has pre_buffer_minutes == post_buffer_minutes == 0. Even
    // when placed at position 0 (first slot) with adjacent class
    // occupancy, the validator must pass: zero buffers no-op.
    let problem = build_five_position_problem(None);
    let placements = vec![
        Placement {
            lesson_id: problem.lessons[1].id, // math_a, unbuffered
            time_block_id: problem.time_blocks[0].id,
            room_id: problem.rooms[0].id,
            teacher_id: problem.teachers[1].id,
        },
        Placement {
            lesson_id: problem.lessons[2].id, // math_b, unbuffered
            time_block_id: problem.time_blocks[1].id,
            room_id: problem.rooms[1].id,
            teacher_id: problem.teachers[0].id,
        },
    ];
    validate_travel_buffer(&problem, &placements).unwrap();
}

// -- Move-site pruning tests (Task 4). The FFD picker, the LAHC Change move,
//    the LAHC Swap move, and the Kempe-chain destination must each reject a
//    placement that would violate the travel-buffer constraint at apply time.
//    The post-condition validator runs in `solve_with_config_stats`'s tail and
//    promotes a missed prune into `Err(Error::Input)`; the tests therefore
//    assert solver success on inputs that force each move type to encounter
//    a buffered lesson.

/// Build a one-day problem with `lesson_count` `pre_buffer_minutes=15`
/// lessons of `hours_per_week=1, preferred_block_size=1`. Five Lesson-kind
/// positions on day 0. FFD's naive lowest-position pick would place the
/// first buffered lesson at pos 0; pruning forces pos >= 1.
fn build_ffd_buffered_problem() -> Problem {
    let time_blocks: Vec<TimeBlock> = (0..5)
        .map(|p| TimeBlock {
            id: TimeBlockId(tb_uuid(p)),
            day_of_week: 0,
            position: p,
            kind: TimeBlockKind::Lesson,
        })
        .collect();
    let teacher = Teacher {
        id: TeacherId(teacher_uuid(1)),
        max_hours_per_week: 30,
        reserve_hours_per_week: 0,
    };
    let subject = Subject {
        id: SubjectId(subject_uuid(1)),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: 0,
        max_hours_per_day: 8,
    };
    let class_one = SchoolClass {
        id: SchoolClassId(class_uuid(1)),
        home_room_id: None,
        max_lessons_per_day: None,
        class_teacher_id: None,
    };
    let room = Room {
        id: RoomId(room_uuid(1)),
    };
    // One buffered lesson (pre side only) for class A.
    let lesson = Lesson {
        id: LessonId(lesson_uuid(1)),
        school_class_ids: vec![class_one.id],
        subject_id: subject.id,
        teacher_candidates: vec![teacher.id],
        teacher_pin: Some(teacher.id),
        hours_per_week: 1,
        preferred_block_size: 1,
        pre_buffer_minutes: 15,
        post_buffer_minutes: 0,
        lesson_group_id: None,
    };
    Problem {
        time_blocks,
        teachers: vec![teacher.clone()],
        rooms: vec![room],
        subjects: vec![subject.clone()],
        school_classes: vec![class_one],
        lessons: vec![lesson],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: teacher.id,
            subject_id: subject.id,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

#[test]
fn test_ffd_skips_first_slot_for_buffered_lesson() {
    // FFD-only (deadline=None). Without the pruning at try_place_block, FFD
    // picks pos 0 (lowest position in tb_order) and the tail validator
    // rejects the run with Err(Error::Input). With pruning, FFD walks to
    // pos 1 and the run succeeds. Assert success plus pos >= 1.
    let problem = build_ffd_buffered_problem();
    let cfg = SolveConfig::default();
    let solution = solve_with_config(&problem, &cfg)
        .expect("FFD must place the buffered lesson at a buffer-legal slot");
    assert_eq!(solution.placements.len(), 1);
    let tb_id = solution.placements[0].time_block_id;
    let tb = problem
        .time_blocks
        .iter()
        .find(|t| t.id == tb_id)
        .expect("tb resolves");
    assert!(
        tb.position >= 1,
        "buffered lesson placed at pos {} but pre-buffer forbids pos 0",
        tb.position
    );
}

/// Build a tiny problem that, after FFD, has one unpinned buffered lesson
/// the LAHC loop can shuffle. The placement floor leaves the lesson with
/// candidate moves on day 0; without pruning at the move site, an accepted
/// move to a forbidden slot becomes the running-best and the tail validator
/// rejects the final solution.
fn build_lahc_move_site_problem() -> Problem {
    // Five positions on day 0, all Lesson-kind. One buffered lesson
    // (schwimm, pre+post). One companion lesson on the same class so the
    // FFD seed deterministically places schwimm at a legal slot and LAHC
    // has somewhere to move it. Companion is hard-pinned at pos 0 so the
    // only LAHC move target is schwimm itself; the buffered lesson cannot
    // legally sit at pos 1 (pre violates against companion at pos 0).
    let time_blocks: Vec<TimeBlock> = (0..5)
        .map(|p| TimeBlock {
            id: TimeBlockId(tb_uuid(p)),
            day_of_week: 0,
            position: p,
            kind: TimeBlockKind::Lesson,
        })
        .collect();
    let teacher_a = Teacher {
        id: TeacherId(teacher_uuid(1)),
        max_hours_per_week: 30,
        reserve_hours_per_week: 0,
    };
    let teacher_b = Teacher {
        id: TeacherId(teacher_uuid(2)),
        max_hours_per_week: 30,
        reserve_hours_per_week: 0,
    };
    let subject_schwimm = Subject {
        id: SubjectId(subject_uuid(1)),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: 0,
        max_hours_per_day: 8,
    };
    let subject_math = Subject {
        id: SubjectId(subject_uuid(2)),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: 0,
        max_hours_per_day: 8,
    };
    let class_one = SchoolClass {
        id: SchoolClassId(class_uuid(1)),
        home_room_id: None,
        max_lessons_per_day: None,
        class_teacher_id: None,
    };
    let room_one = Room {
        id: RoomId(room_uuid(1)),
    };
    let room_two = Room {
        id: RoomId(room_uuid(2)),
    };
    let lesson_schwimm = Lesson {
        id: LessonId(lesson_uuid(1)),
        school_class_ids: vec![class_one.id],
        subject_id: subject_schwimm.id,
        teacher_candidates: vec![teacher_a.id],
        teacher_pin: Some(teacher_a.id),
        hours_per_week: 1,
        preferred_block_size: 1,
        pre_buffer_minutes: 15,
        post_buffer_minutes: 15,
        lesson_group_id: None,
    };
    let lesson_math = Lesson {
        id: LessonId(lesson_uuid(2)),
        school_class_ids: vec![class_one.id],
        subject_id: subject_math.id,
        teacher_candidates: vec![teacher_b.id],
        teacher_pin: Some(teacher_b.id),
        hours_per_week: 1,
        preferred_block_size: 1,
        pre_buffer_minutes: 0,
        post_buffer_minutes: 0,
        lesson_group_id: None,
    };
    // Pin math at pos 0 so FFD must seat schwimm at pos >= 2 (post-buffer
    // also forbids pos 4-last and pre-buffer forbids pos 0..=1 because of
    // class A's math at pos 0). Schwimm will land at pos 2 or 3. LAHC then
    // explores moves on the unpinned schwimm placement; a move to pos 1
    // (adjacent to math at pos 0) violates pre-buffer, a move to pos 4
    // violates post-buffer.
    let pinned = vec![PinnedPlacement {
        lesson_id: lesson_math.id,
        time_block_id: time_blocks[0].id,
        room_id: room_two.id,
        teacher_id: Some(teacher_b.id),
        kind: PinKind::Hard,
    }];
    Problem {
        time_blocks,
        teachers: vec![teacher_a.clone(), teacher_b.clone()],
        rooms: vec![room_one, room_two],
        subjects: vec![subject_schwimm.clone(), subject_math.clone()],
        school_classes: vec![class_one],
        lessons: vec![lesson_schwimm, lesson_math],
        teacher_qualifications: vec![
            TeacherQualification {
                teacher_id: teacher_a.id,
                subject_id: subject_schwimm.id,
            },
            TeacherQualification {
                teacher_id: teacher_b.id,
                subject_id: subject_math.id,
            },
        ],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: pinned,
    }
}

#[test]
fn test_lahc_change_move_rejects_buffer_violation() {
    // LAHC Change-only (rr/kempe periods both None). With class_gap weighted,
    // an accepted move that pulls schwimm next to the pinned math lesson
    // improves canonical and becomes running-best; without pruning at
    // try_change_move_n1, the tail validator rejects with Err(Error::Input).
    let problem = build_lahc_move_site_problem();
    let cfg = SolveConfig {
        deadline: Some(Duration::from_millis(50)),
        seed: 0,
        weights: ConstraintWeights {
            class_gap: 10,
            ..ConstraintWeights::default()
        },
        max_iterations: Some(1024),
        lahc_rr_period: None,
        lahc_kempe_period: None,
        lahc_rr_k: 5,
        lahc_kempe_max_chain: 8,
    };
    let solution = solve_with_config(&problem, &cfg)
        .expect("LAHC Change move must respect travel-buffer at every accepted move");
    // Double-check via the validator on the returned placements.
    validate_travel_buffer(&problem, &solution.placements)
        .expect("returned solution must satisfy travel-buffer");
}

#[test]
fn test_lahc_swap_move_rejects_buffer_violation() {
    // Swap fires on the 6th selector of the change_rng draw; we widen the
    // iteration budget so the bias toward Change (5:1) still produces swap
    // attempts across multiple seeds. Without pruning at try_swap_move, an
    // accepted swap can park schwimm at a buffer-violating slot.
    let problem = build_lahc_move_site_problem();
    for seed in 0..4u64 {
        let cfg = SolveConfig {
            deadline: Some(Duration::from_millis(50)),
            seed,
            weights: ConstraintWeights {
                class_gap: 10,
                ..ConstraintWeights::default()
            },
            max_iterations: Some(2048),
            lahc_rr_period: None,
            lahc_kempe_period: None,
            lahc_rr_k: 5,
            lahc_kempe_max_chain: 8,
        };
        let solution = solve_with_config(&problem, &cfg)
            .unwrap_or_else(|e| panic!("seed {seed}: LAHC Swap must respect travel-buffer: {e:?}"));
        validate_travel_buffer(&problem, &solution.placements)
            .unwrap_or_else(|e| panic!("seed {seed}: validator rejected returned solution: {e:?}"));
    }
}

#[test]
fn test_lahc_kempe_chain_rejects_buffer_violation() {
    // Enable Kempe (period=1 so every iteration is a Kempe attempt). The
    // buffered schwimm lesson is one of two anchors; chain destinations
    // include slots that would violate its pre/post buffer. Without pruning
    // in the chain build/apply path, an accepted chain leaves the buffered
    // lesson at a violating slot and the tail validator rejects.
    let problem = build_lahc_move_site_problem();
    for seed in 0..4u64 {
        let cfg = SolveConfig {
            deadline: Some(Duration::from_millis(50)),
            seed,
            weights: ConstraintWeights {
                class_gap: 10,
                ..ConstraintWeights::default()
            },
            max_iterations: Some(2048),
            lahc_rr_period: None,
            lahc_kempe_period: Some(1),
            lahc_rr_k: 5,
            lahc_kempe_max_chain: 8,
        };
        let solution = solve_with_config(&problem, &cfg).unwrap_or_else(|e| {
            panic!("seed {seed}: LAHC Kempe must respect travel-buffer: {e:?}")
        });
        validate_travel_buffer(&problem, &solution.placements)
            .unwrap_or_else(|e| panic!("seed {seed}: validator rejected returned solution: {e:?}"));
    }
}
