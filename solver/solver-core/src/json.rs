//! JSON string adapter over `solve`. Consumed by `solver-py` in step 2 of the
//! sprint. Input errors are wrapped in a tagged envelope; success emits the
//! `Solution` JSON directly.

use std::time::Duration;

use serde::Serialize;

use crate::error::Error;
use crate::solve::solve_with_config;
use crate::types::{ConstraintWeights, Problem, SolveConfig};

/// Solve a timetable problem supplied as a JSON string and return the resulting
/// `Solution` serialised as JSON. Uses the production-default 200 ms LAHC
/// deadline; equivalent to [`solve_json_with_config`] with `Some(200)`.
/// Malformed input JSON and serialisation failures are mapped to
/// [`Error::Input`] so callers can distinguish client mistakes from
/// solver-internal issues.
pub fn solve_json(json: &str) -> Result<String, Error> {
    solve_json_with_config(json, Some(200))
}

/// Solve a timetable problem supplied as a JSON string with an explicit LAHC
/// wall-clock deadline (milliseconds). `None` skips the LAHC pass entirely
/// and returns the greedy result; `Some(n)` runs LAHC for `n` milliseconds.
/// Soft-constraint weights are the production active defaults
/// (`class_gap = teacher_gap = prefer_early_period = avoid_first_period
///   = avoid_last_period = 1`) so that the only knob exposed via the JSON
/// adapter is the deadline.
pub fn solve_json_with_config(json: &str, deadline_ms: Option<u64>) -> Result<String, Error> {
    let problem: Problem =
        serde_json::from_str(json).map_err(|e| Error::Input(format!("json: {e}")))?;
    let config = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 1,
            teacher_gap: 1,
            prefer_early_period: 1,
            avoid_first_period: 1,
            prefer_home_room: 0,
            avoid_last_period: 1,
            prefer_late_period: 0,
            class_day_balance: 0,
        },
        deadline: deadline_ms.map(Duration::from_millis),
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config)?;
    serde_json::to_string(&solution).map_err(|e| Error::Input(format!("serialize: {e}")))
}

/// Tagged JSON envelope that step 2's `solver-py` wrapper emits to Python so the
/// FastAPI layer can branch on a single field instead of parsing error strings.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ErrorEnvelope<'a> {
    /// Wraps an [`Error::Input`] with its stringified reason.
    Input {
        /// Human-readable description of the input failure.
        reason: &'a str,
    },
}

impl<'a> From<&'a Error> for ErrorEnvelope<'a> {
    fn from(e: &'a Error) -> Self {
        match e {
            Error::Input(msg) => ErrorEnvelope::Input {
                reason: msg.as_str(),
            },
        }
    }
}

/// Serialise a [`Error`] into the tagged JSON envelope consumed by `solver-py`.
/// Falls back to a hand-rolled literal if `serde_json` cannot serialise the
/// envelope (should not happen with the current variants).
pub fn error_envelope_json(e: &Error) -> String {
    serde_json::to_string(&ErrorEnvelope::from(e))
        .unwrap_or_else(|_| "{\"kind\":\"input\",\"reason\":\"serialize failed\"}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
    use crate::types::{
        Lesson, PinnedPlacement, Problem, Room, SchoolClass, Subject, Teacher,
        TeacherQualification, TimeBlock,
    };
    use uuid::Uuid;

    fn json_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn minimal_json() -> String {
        let p = Problem {
            time_blocks: vec![TimeBlock {
                id: TimeBlockId(json_uuid(10)),
                day_of_week: 0,
                position: 0,
            }],
            teachers: vec![Teacher {
                id: TeacherId(json_uuid(20)),
                max_hours_per_week: 5,
            }],
            rooms: vec![Room {
                id: RoomId(json_uuid(30)),
            }],
            subjects: vec![Subject {
                id: SubjectId(json_uuid(40)),
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
            }],
            school_classes: vec![SchoolClass {
                id: SchoolClassId(json_uuid(50)),
                home_room_id: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(json_uuid(60)),
                school_class_ids: vec![SchoolClassId(json_uuid(50))],
                subject_id: SubjectId(json_uuid(40)),
                teacher_id: TeacherId(json_uuid(20)),
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: TeacherId(json_uuid(20)),
                subject_id: SubjectId(json_uuid(40)),
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        serde_json::to_string(&p).unwrap()
    }

    #[test]
    fn solve_json_round_trips_minimal_problem() {
        let out = solve_json(&minimal_json()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["placements"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["violations"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn solve_json_returns_input_error_for_malformed_json() {
        let err = solve_json("not json").unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("json:")));
    }

    #[test]
    fn error_envelope_tags_input_variant() {
        let env = error_envelope_json(&Error::Input("no time_blocks".into()));
        let parsed: serde_json::Value = serde_json::from_str(&env).unwrap();
        assert_eq!(parsed["kind"], "input");
        assert_eq!(parsed["reason"], "no time_blocks");
    }

    fn trivially_empty_json() -> String {
        let p = Problem {
            time_blocks: vec![TimeBlock {
                id: TimeBlockId(json_uuid(10)),
                day_of_week: 0,
                position: 0,
            }],
            teachers: vec![],
            rooms: vec![Room {
                id: RoomId(json_uuid(30)),
            }],
            subjects: vec![],
            school_classes: vec![],
            lessons: vec![],
            teacher_qualifications: vec![],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        serde_json::to_string(&p).unwrap()
    }

    #[test]
    fn solve_json_with_config_none_skips_lahc_and_returns_greedy() {
        let out = solve_json_with_config(&trivially_empty_json(), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["placements"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["violations"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn solve_json_with_config_some_matches_solve_json_for_default_deadline() {
        let problem = trivially_empty_json();
        let with_config = solve_json_with_config(&problem, Some(200)).unwrap();
        let default = solve_json(&problem).unwrap();
        let with_config_parsed: serde_json::Value = serde_json::from_str(&with_config).unwrap();
        let default_parsed: serde_json::Value = serde_json::from_str(&default).unwrap();
        assert_eq!(with_config_parsed, default_parsed);
    }

    #[test]
    fn problem_round_trip_preserves_pinned_placements() {
        let lesson_uuid = json_uuid(60);
        let time_block_uuid = json_uuid(10);
        let room_uuid = json_uuid(30);
        let original = Problem {
            time_blocks: vec![TimeBlock {
                id: TimeBlockId(time_block_uuid),
                day_of_week: 0,
                position: 0,
            }],
            teachers: vec![Teacher {
                id: TeacherId(json_uuid(20)),
                max_hours_per_week: 5,
            }],
            rooms: vec![Room {
                id: RoomId(room_uuid),
            }],
            subjects: vec![Subject {
                id: SubjectId(json_uuid(40)),
                prefer_early_period: 0,
                avoid_first_period: 0,
                avoid_last_period: 0,
                prefer_late_period: 0,
            }],
            school_classes: vec![SchoolClass {
                id: SchoolClassId(json_uuid(50)),
                home_room_id: None,
            }],
            lessons: vec![Lesson {
                id: LessonId(lesson_uuid),
                school_class_ids: vec![SchoolClassId(json_uuid(50))],
                subject_id: SubjectId(json_uuid(40)),
                teacher_id: TeacherId(json_uuid(20)),
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            }],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: TeacherId(json_uuid(20)),
                subject_id: SubjectId(json_uuid(40)),
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![PinnedPlacement {
                lesson_id: LessonId(lesson_uuid),
                time_block_id: TimeBlockId(time_block_uuid),
                room_id: RoomId(room_uuid),
            }],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: Problem = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn problem_defaults_pinned_placements_to_empty_when_field_omitted() {
        let parsed: Problem = serde_json::from_str(&trivially_empty_json()).unwrap();
        assert!(parsed.pinned_placements.is_empty());
    }
}
