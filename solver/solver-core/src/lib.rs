//! solver-core — pure Rust solver logic. No Python, no PyO3.

#![deny(missing_docs)]

pub mod error;
pub mod ids;
pub(crate) mod index;
pub mod json;
mod lahc;
mod ordering;
pub mod score;
pub mod solve;
#[cfg(feature = "fixtures")]
pub mod test_fixtures;
#[cfg(feature = "solver-trace")]
mod trace;
pub mod types;
pub mod validate;

pub use error::Error;
pub use ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
pub use json::{error_envelope_json, solve_json, solve_json_with_config};
pub use score::score_solution;
pub use solve::{solve, solve_with_config};
pub use types::{
    ConstraintWeights, Lesson, Placement, Problem, Room, RoomBlockedTime, RoomSubjectSuitability,
    SchoolClass, Solution, SolveConfig, Subject, Teacher, TeacherBlockedTime, TeacherQualification,
    TimeBlock, Violation, ViolationKind,
};
