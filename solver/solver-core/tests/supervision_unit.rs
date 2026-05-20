//! Unit coverage for `compute_supervision_spread` / `compute_supervision_full`.
//!
//! These tests pin the count-only and full-collection entry points side by
//! side so a future delta-incremental optimisation cannot diverge from the
//! canonical full-pass result. Fixtures are constructed inline so the test
//! shape is decoupled from `tests/common::feasible_problem` (which mints
//! teaching slots only).

use solver_core::{
    ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId},
    supervision::{compute_supervision_full, compute_supervision_spread},
    types::{
        Lesson, Placement, Problem, Room, RoomSubjectSuitability, SchoolClass, Subject, Teacher,
        TeacherQualification, TimeBlock, TimeBlockKind, ViolationKind,
    },
};
use uuid::Uuid;

fn tid(n: u128) -> TeacherId {
    TeacherId(Uuid::from_u128(n))
}
fn tb_id(n: u128) -> TimeBlockId {
    TimeBlockId(Uuid::from_u128(n))
}
fn rid(n: u128) -> RoomId {
    RoomId(Uuid::from_u128(n))
}
fn sid(n: u128) -> SubjectId {
    SubjectId(Uuid::from_u128(n))
}
fn cid(n: u128) -> SchoolClassId {
    SchoolClassId(Uuid::from_u128(n))
}
fn lid(n: u128) -> LessonId {
    LessonId(Uuid::from_u128(n))
}

fn empty_subject(n: u128) -> Subject {
    Subject {
        id: sid(n),
        prefer_early_period: 0,
        avoid_first_period: 0,
        avoid_last_period: 0,
        prefer_late_period: 0,
        max_hours_per_day: 8,
    }
}

fn empty_class(n: u128) -> SchoolClass {
    SchoolClass {
        id: cid(n),
        home_room_id: None,
        max_lessons_per_day: None,
        class_teacher_id: None,
    }
}

fn teacher(n: u128) -> Teacher {
    Teacher {
        id: tid(n),
        max_hours_per_week: 40,
        reserve_hours_per_week: 0,
    }
}

fn make_problem(
    time_blocks: Vec<TimeBlock>,
    teachers: Vec<Teacher>,
    rooms: Vec<Room>,
    subjects: Vec<Subject>,
    school_classes: Vec<SchoolClass>,
    lessons: Vec<Lesson>,
    teacher_qualifications: Vec<TeacherQualification>,
) -> Problem {
    Problem {
        time_blocks,
        teachers,
        rooms,
        subjects,
        school_classes,
        lessons,
        teacher_qualifications,
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: Vec::<RoomSubjectSuitability>::new(),
        pinned_placements: vec![],
        pre_first_slot_grace_minutes: 0,
    }
}

fn lesson_tb(id: u128, day: u8, position: u8, kind: TimeBlockKind) -> TimeBlock {
    TimeBlock {
        id: tb_id(id),
        day_of_week: day,
        position,
        kind,
    }
}

fn placement(lesson: u128, time_block: u128, room: u128, teacher: u128) -> Placement {
    Placement {
        lesson_id: lid(lesson),
        time_block_id: tb_id(time_block),
        room_id: rid(room),
        teacher_id: tid(teacher),
    }
}

#[test]
fn no_breaks_yields_empty_assignments_and_zero_spread() {
    let problem = make_problem(
        vec![lesson_tb(1, 0, 0, TimeBlockKind::Lesson)],
        vec![teacher(10), teacher(11)],
        vec![Room { id: rid(100) }],
        vec![empty_subject(50)],
        vec![empty_class(20)],
        vec![],
        vec![],
    );
    let placements = vec![placement(900, 1, 100, 10)];
    let (assignments, violations, spread) = compute_supervision_full(&problem, &placements);
    assert!(assignments.is_empty());
    assert!(violations.is_empty());
    assert_eq!(spread, 0);
    assert_eq!(compute_supervision_spread(&problem, &placements), 0);
}

#[test]
fn one_break_with_one_adjacent_teacher_assigns_that_teacher() {
    // position 0: lesson by teacher 10; position 1: Hofpause; teacher 10 is
    // free at position 1 (no placement) AND has a lesson at position 0, so
    // teacher 10 is the unique eligible supervisor.
    let problem = make_problem(
        vec![
            lesson_tb(1, 0, 0, TimeBlockKind::Lesson),
            lesson_tb(2, 0, 1, TimeBlockKind::Break),
        ],
        vec![teacher(10), teacher(11)],
        vec![Room { id: rid(100) }],
        vec![empty_subject(50)],
        vec![empty_class(20)],
        vec![],
        vec![],
    );
    let placements = vec![placement(900, 1, 100, 10)];
    let (assignments, violations, spread) = compute_supervision_full(&problem, &placements);
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].time_block_id, tb_id(2));
    assert_eq!(assignments[0].teacher_id, tid(10));
    assert!(violations.is_empty());
    // Only one teacher supervises, so max - min over the supervising pool = 0.
    assert_eq!(spread, 0);
}

#[test]
fn one_break_with_no_eligible_teacher_emits_supervision_gap() {
    // Break at (day=0, position=3) with no adjacent placements at position
    // 2 or 4. Eligible set is empty; expect a SupervisionGap violation.
    let problem = make_problem(
        vec![
            lesson_tb(1, 0, 0, TimeBlockKind::Lesson),
            lesson_tb(2, 0, 3, TimeBlockKind::Break),
        ],
        vec![teacher(10), teacher(11)],
        vec![Room { id: rid(100) }],
        vec![empty_subject(50)],
        vec![empty_class(20)],
        vec![],
        vec![],
    );
    let placements = vec![placement(900, 1, 100, 10)];
    let (assignments, violations, spread) = compute_supervision_full(&problem, &placements);
    assert!(assignments.is_empty());
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].kind, ViolationKind::SupervisionGap);
    let reason = violations[0]
        .reason
        .as_deref()
        .expect("supervision gap violation must carry a reason");
    assert!(reason.contains("day=0"), "reason was: {reason}");
    assert!(reason.contains("position=3"), "reason was: {reason}");
    assert!(reason.contains("candidates=0"), "reason was: {reason}");
    assert_eq!(spread, 0);
}

#[test]
fn teacher_busy_at_break_slot_is_not_eligible() {
    // Teacher 10 teaches at position 0 AND at position 1 (the break slot).
    // Despite adjacency at position 0, teacher 10 is busy AT the break slot
    // and must not be eligible. No other teacher is adjacent, so the slot
    // has no eligible supervisor.
    let problem = make_problem(
        vec![
            lesson_tb(1, 0, 0, TimeBlockKind::Lesson),
            lesson_tb(2, 0, 1, TimeBlockKind::Break),
        ],
        vec![teacher(10), teacher(11)],
        vec![Room { id: rid(100) }, Room { id: rid(101) }],
        vec![empty_subject(50)],
        vec![empty_class(20)],
        vec![],
        vec![],
    );
    // Teacher 10 placed at the break slot (modeled as ordinary placement so
    // the occupancy index sees teacher 10 as busy at (day=0, position=1)).
    let placements = vec![placement(900, 1, 100, 10), placement(901, 2, 101, 10)];
    let (assignments, violations, _spread) = compute_supervision_full(&problem, &placements);
    assert!(assignments.is_empty());
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].kind, ViolationKind::SupervisionGap);
}

#[test]
fn multiple_breaks_balance_load() {
    // Two breaks; both teachers 10 and 11 are equally adjacent and free at
    // each break. Min-load greedy plus tie-break on smallest TeacherId
    // assigns the first break to teacher 10 (count 0 -> 1) and the second
    // break to teacher 11 (count 0 < count 1).
    let problem = make_problem(
        vec![
            lesson_tb(1, 0, 0, TimeBlockKind::Lesson), // teacher 10 lesson
            lesson_tb(2, 0, 1, TimeBlockKind::Break),
            lesson_tb(3, 0, 2, TimeBlockKind::Lesson), // teacher 11 lesson
            lesson_tb(4, 1, 0, TimeBlockKind::Lesson), // teacher 10 lesson
            lesson_tb(5, 1, 1, TimeBlockKind::Break),
            lesson_tb(6, 1, 2, TimeBlockKind::Lesson), // teacher 11 lesson
        ],
        vec![teacher(10), teacher(11)],
        vec![Room { id: rid(100) }, Room { id: rid(101) }],
        vec![empty_subject(50)],
        vec![empty_class(20)],
        vec![],
        vec![],
    );
    let placements = vec![
        placement(900, 1, 100, 10),
        placement(901, 3, 100, 11),
        placement(902, 4, 101, 10),
        placement(903, 6, 101, 11),
    ];
    let (assignments, violations, spread) = compute_supervision_full(&problem, &placements);
    assert_eq!(assignments.len(), 2, "expected two supervisor assignments");
    assert!(violations.is_empty());
    // Day 0 break (position 1): teachers 10 and 11 both eligible; both at
    // count 0; tiebreak on smallest TeacherId picks teacher 10.
    assert_eq!(assignments[0].time_block_id, tb_id(2));
    assert_eq!(assignments[0].teacher_id, tid(10));
    // Day 1 break: teacher 10 is now at count 1, teacher 11 at 0; min-load
    // picks teacher 11.
    assert_eq!(assignments[1].time_block_id, tb_id(5));
    assert_eq!(assignments[1].teacher_id, tid(11));
    // Both supervisors at count 1; spread = 0.
    assert_eq!(spread, 0);
}

#[test]
fn end_of_day_break_eligibility_uses_pos_minus_one_only() {
    // Break is the last position of the day. There is no position+1, so
    // adjacency must use position-1 only. Teacher 10 has a lesson at
    // position-1 and is eligible.
    let problem = make_problem(
        vec![
            lesson_tb(1, 0, 0, TimeBlockKind::Lesson),
            lesson_tb(2, 0, 1, TimeBlockKind::Break),
        ],
        vec![teacher(10), teacher(11)],
        vec![Room { id: rid(100) }],
        vec![empty_subject(50)],
        vec![empty_class(20)],
        vec![],
        vec![],
    );
    let placements = vec![placement(900, 1, 100, 10)];
    let (assignments, violations, _spread) = compute_supervision_full(&problem, &placements);
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].teacher_id, tid(10));
    assert!(violations.is_empty());
}

#[test]
fn compute_supervision_spread_matches_compute_supervision_full_spread() {
    // Three breaks across two days. Teachers 10, 11, 12 are all adjacent
    // and free for every break, producing a deterministic 1/1/1 load.
    let problem = make_problem(
        vec![
            lesson_tb(1, 0, 0, TimeBlockKind::Lesson),
            lesson_tb(2, 0, 1, TimeBlockKind::Break),
            lesson_tb(3, 0, 2, TimeBlockKind::Lesson),
            lesson_tb(4, 0, 3, TimeBlockKind::Break),
            lesson_tb(5, 1, 0, TimeBlockKind::Lesson),
            lesson_tb(6, 1, 1, TimeBlockKind::Break),
            lesson_tb(7, 1, 2, TimeBlockKind::Lesson),
        ],
        vec![teacher(10), teacher(11), teacher(12)],
        vec![
            Room { id: rid(100) },
            Room { id: rid(101) },
            Room { id: rid(102) },
        ],
        vec![empty_subject(50)],
        vec![empty_class(20)],
        vec![],
        vec![],
    );
    // Teachers 10, 11, 12 each teach at position 0 (lesson) and position 2
    // (lesson) on both days, so every break has all three eligible.
    let placements = vec![
        // Day 0 position 0: teachers 10/11/12 all teaching different
        // classes are unrealistic; use one teacher per placement and rely
        // on adjacency only requiring "at least one placement at adjacent
        // position". We place teacher 10 at position 0, teachers 11 & 12
        // at position 2 to make all three adjacent to position 1.
        placement(900, 1, 100, 10),
        placement(901, 3, 101, 11),
        placement(902, 3, 102, 12),
        // Day 0 position 3 break: needs adjacency to position 2 (above)
        // or position 4 (absent). Teachers 11 & 12 are adjacent via
        // position 2. Teacher 10 is NOT adjacent (only taught position 0).
        // Day 1: teacher 10 at position 0, teachers 11 & 12 at position 2.
        placement(903, 5, 100, 10),
        placement(904, 7, 101, 11),
        placement(905, 7, 102, 12),
    ];
    let (_, _, full_spread) = compute_supervision_full(&problem, &placements);
    let count_only = compute_supervision_spread(&problem, &placements);
    assert_eq!(
        full_spread, count_only,
        "spread-only and full-pass entry points must agree on spread"
    );
}
