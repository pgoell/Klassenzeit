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

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::types::{
    Lesson, Placement, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
    TimeBlock, TimeBlockKind,
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
