//! Property test for the Placement.teacher_id contract introduced in
//! OPEN_THINGS items 64 + 65. Every Placement must carry a teacher_id that
//! is in the corresponding Lesson's teacher_candidates (or matches
//! teacher_pin if set).

use proptest::prelude::*;
use solver_core::{
    solve_with_config, Lesson, LessonId, Problem, Room, RoomId, SchoolClass, SchoolClassId,
    SolveConfig, Subject, SubjectId, Teacher, TeacherId, TeacherQualification, TimeBlock,
    TimeBlockId, TimeBlockKind, PRODUCTION_ACTIVE_WEIGHTS,
};
use uuid::Uuid;

fn pick_teacher_id(n: u8) -> TeacherId {
    let mut bytes = [0u8; 16];
    bytes[0] = n;
    TeacherId(Uuid::from_bytes(bytes))
}

fn pinned_problem_with_n_candidates(num_candidates: u8, pin_idx: u8) -> Problem {
    let class_id = SchoolClassId(Uuid::from_bytes([100u8; 16]));
    let subj_id = SubjectId(Uuid::from_bytes([200u8; 16]));
    let room_id = RoomId(Uuid::from_bytes([150u8; 16]));
    let tb_id = TimeBlockId(Uuid::from_bytes([50u8; 16]));
    let candidates: Vec<TeacherId> = (1..=num_candidates).map(pick_teacher_id).collect();
    let teachers: Vec<Teacher> = candidates
        .iter()
        .map(|tid| Teacher {
            id: *tid,
            max_hours_per_week: 40,
            reserve_hours_per_week: 0,
        })
        .collect();
    let pin = candidates[pin_idx as usize];
    // Reorder candidates so the pin appears first (matches build_problem_json
    // contract).
    let mut ordered_candidates = vec![pin];
    for c in &candidates {
        if *c != pin {
            ordered_candidates.push(*c);
        }
    }
    let lesson_id_v = LessonId(Uuid::from_bytes([42u8; 16]));
    Problem {
        time_blocks: vec![TimeBlock {
            id: tb_id,
            day_of_week: 0,
            position: 0,
            kind: TimeBlockKind::Lesson,
        }],
        teachers,
        rooms: vec![Room { id: room_id }],
        subjects: vec![Subject {
            id: subj_id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass {
            id: class_id,
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        }],
        lessons: vec![Lesson {
            id: lesson_id_v,
            school_class_ids: vec![class_id],
            subject_id: subj_id,
            teacher_candidates: ordered_candidates,
            teacher_pin: Some(pin),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        teacher_qualifications: candidates
            .iter()
            .map(|tid| TeacherQualification {
                teacher_id: *tid,
                subject_id: subj_id,
            })
            .collect(),
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

proptest! {
    #[test]
    fn every_placement_teacher_id_is_in_candidates_or_matches_pin(
        num_candidates in 1u8..=4u8,
        pin_idx_raw in 0u8..=3u8,
    ) {
        // Clamp the pin index into [0, num_candidates).
        let pin_idx = pin_idx_raw % num_candidates;
        let problem = pinned_problem_with_n_candidates(num_candidates, pin_idx);
        let cfg = SolveConfig {
            weights: PRODUCTION_ACTIVE_WEIGHTS,
            seed: 1,
            deadline: None,
            max_iterations: Some(0),
            ..SolveConfig::default()
        };
        let solution = solve_with_config(&problem, &cfg).expect("solve must not error");
        prop_assert!(!solution.placements.is_empty(), "expected at least one placement");
        for placement in &solution.placements {
            let lesson = problem
                .lessons
                .iter()
                .find(|l| l.id == placement.lesson_id)
                .expect("placement references unknown lesson");
            prop_assert!(
                lesson.teacher_candidates.contains(&placement.teacher_id),
                "Placement.teacher_id {:?} not in candidates {:?}",
                placement.teacher_id,
                lesson.teacher_candidates
            );
            if let Some(pin) = lesson.teacher_pin {
                prop_assert_eq!(placement.teacher_id, pin);
            }
        }
    }
}
