//! Property test pinning the per-(class, subject) teacher uniformity
//! contract enforced at placement time by `try_place_block`'s
//! `class_subject_teacher` lock-map check (item 66 placement-time, item
//! 68 algorithm phase). Random `Problem`s with multiple lessons per
//! `(class, subject)` pair must solve to either an infeasible result OR
//! a `Solution` whose every `(class, subject)` pair maps to one
//! teacher across all placements.
//!
//! Generator is intentionally simple: one class, two subjects, multiple
//! lessons per subject. Each lesson advertises every teacher as a
//! candidate (no pin) so the solver picks freely; the only invariant
//! the test pins is uniformity, not the picker's choice.

use proptest::prelude::*;
use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::types::{
    ConstraintWeights, Lesson, Problem, Room, SchoolClass, Solution, SolveConfig, Subject, Teacher,
    TeacherQualification, TimeBlock, TimeBlockKind,
};
use solver_core::{solve_with_config, PRODUCTION_ACTIVE_WEIGHTS};
use std::collections::HashMap;
use uuid::Uuid;

/// Build a stable `Uuid` from a `u32` seed by splatting it across the
/// last 4 bytes; the Uuid newtype stays globally unique across this
/// integration test crate.
fn teacher_uniformity_uuid_from(n: u32) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[12..16].copy_from_slice(&n.to_be_bytes());
    Uuid::from_bytes(bytes)
}

prop_compose! {
    fn teacher_uniformity_problem()(
        n_teachers in 2usize..=4usize,
        n_lessons_per_subject in 2usize..=3usize,
        n_days in 2u8..=3u8,
        slots_per_day in 3u8..=5u8,
    ) -> Problem {
        let class = SchoolClass {
            id: SchoolClassId(teacher_uniformity_uuid_from(1)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        };
        let class_id = class.id;

        // Two subjects so we exercise multiple `(class, subject)` pairs;
        // both go through the lock map independently.
        let subject_a = Subject {
            id: SubjectId(teacher_uniformity_uuid_from(10)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        let subject_b = Subject {
            id: SubjectId(teacher_uniformity_uuid_from(11)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };

        let teachers: Vec<Teacher> = (0..n_teachers)
            .map(|i| Teacher {
                id: TeacherId(teacher_uniformity_uuid_from(100 + i as u32)),
                max_hours_per_week: 40,
                reserve_hours_per_week: 0,
            })
            .collect();
        let teacher_ids: Vec<TeacherId> = teachers.iter().map(|t| t.id).collect();

        // Every teacher is qualified for both subjects so the solver is
        // free to pick; the lock map is what should constrain the choice
        // to one teacher per pair.
        let mut teacher_qualifications: Vec<TeacherQualification> = Vec::new();
        for t in &teachers {
            for s in [&subject_a, &subject_b] {
                teacher_qualifications.push(TeacherQualification {
                    teacher_id: t.id,
                    subject_id: s.id,
                });
            }
        }

        let rooms: Vec<Room> = (0..2)
            .map(|i| Room {
                id: RoomId(teacher_uniformity_uuid_from(500 + i)),
            })
            .collect();

        let mut time_blocks: Vec<TimeBlock> = Vec::new();
        let mut tb_idx = 0u32;
        for d in 0..n_days {
            for p in 0..slots_per_day {
                time_blocks.push(TimeBlock {
                    id: TimeBlockId(teacher_uniformity_uuid_from(2000 + tb_idx)),
                    day_of_week: d,
                    position: p,
                    kind: TimeBlockKind::Lesson,
                });
                tb_idx += 1;
            }
        }

        // Build lessons: `n_lessons_per_subject` lessons for subject_a,
        // same for subject_b. Every lesson advertises every teacher as a
        // candidate; no `teacher_pin` so the solver picks. Each lesson
        // is one hour, single-period.
        let mut lessons: Vec<Lesson> = Vec::new();
        for subject in [&subject_a, &subject_b] {
            for _j in 0..n_lessons_per_subject {
                lessons.push(Lesson {
                    id: LessonId(teacher_uniformity_uuid_from(
                        3000 + (lessons.len() as u32),
                    )),
                    school_class_ids: vec![class_id],
                    subject_id: subject.id,
                    teacher_candidates: teacher_ids.clone(),
                    teacher_pin: None,
                    hours_per_week: 1,
                    preferred_block_size: 1,
                    lesson_group_id: None,
                });
            }
        }

        Problem {
            time_blocks,
            teachers,
            rooms,
            subjects: vec![subject_a, subject_b],
            school_classes: vec![class],
            lessons,
            teacher_qualifications,
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        }
    }
}

prop_compose! {
    fn teacher_uniformity_weights()(
        class_gap in 0u32..=10,
        teacher_gap in 0u32..=10,
        prefer_class_teacher in 0u32..=20,
    ) -> ConstraintWeights {
        ConstraintWeights {
            class_gap,
            teacher_gap,
            prefer_class_teacher,
            ..ConstraintWeights::default()
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        ..ProptestConfig::default()
    })]

    /// For any random Problem with multiple lessons per `(class,
    /// subject)` pair, every placement of a given pair must use the same
    /// teacher (item 66 placement-time uniformity invariant). The
    /// lesson-shaped property is structural; production weights and
    /// random small weights both satisfy it because the
    /// `class_subject_teacher` lock collapses the candidate iterator to
    /// a singleton on every placement after the first one for the pair.
    #[test]
    fn solution_has_uniform_teacher_per_class_subject_pair(
        problem in teacher_uniformity_problem(),
        weights in teacher_uniformity_weights(),
    ) {
        let cfg = SolveConfig {
            weights,
            seed: 1,
            deadline: None,
            max_iterations: Some(0),
            ..SolveConfig::default()
        };
        let solution: Solution = solve_with_config(&problem, &cfg).expect("solve must not error");

        let mut pair_to_teacher: HashMap<(SchoolClassId, SubjectId), TeacherId> = HashMap::new();
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        for placement in &solution.placements {
            let lesson = lesson_lookup
                .get(&placement.lesson_id)
                .expect("placement must reference a known lesson");
            for class_id in &lesson.school_class_ids {
                let pair = (*class_id, lesson.subject_id);
                if let Some(&existing) = pair_to_teacher.get(&pair) {
                    prop_assert_eq!(
                        existing,
                        placement.teacher_id,
                        "split teacher in pair {:?}: existing={:?} new={:?}",
                        pair,
                        existing,
                        placement.teacher_id
                    );
                } else {
                    pair_to_teacher.insert(pair, placement.teacher_id);
                }
            }
        }
    }

    /// Same uniformity invariant but with the production active weights
    /// to exercise the LAHC pass with deadline; LAHC moves must
    /// preserve the per-(class, subject) teacher across Change moves
    /// and R&R recreates.
    #[test]
    fn solution_has_uniform_teacher_under_production_weights(
        problem in teacher_uniformity_problem(),
    ) {
        let cfg = SolveConfig {
            weights: PRODUCTION_ACTIVE_WEIGHTS,
            seed: 1,
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(200),
            ..SolveConfig::default()
        };
        let solution: Solution = solve_with_config(&problem, &cfg).expect("solve must not error");

        let mut pair_to_teacher: HashMap<(SchoolClassId, SubjectId), TeacherId> = HashMap::new();
        let lesson_lookup: HashMap<LessonId, &Lesson> =
            problem.lessons.iter().map(|l| (l.id, l)).collect();
        for placement in &solution.placements {
            let lesson = lesson_lookup
                .get(&placement.lesson_id)
                .expect("placement must reference a known lesson");
            for class_id in &lesson.school_class_ids {
                let pair = (*class_id, lesson.subject_id);
                if let Some(&existing) = pair_to_teacher.get(&pair) {
                    prop_assert_eq!(
                        existing,
                        placement.teacher_id,
                        "LAHC introduced split teacher in pair {:?}: existing={:?} new={:?}",
                        pair,
                        existing,
                        placement.teacher_id
                    );
                } else {
                    pair_to_teacher.insert(pair, placement.teacher_id);
                }
            }
        }

    }
}
