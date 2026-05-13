//! solver-core — pure Rust solver logic. No Python, no PyO3.

#![deny(missing_docs)]

pub mod error;
pub mod ids;
pub(crate) mod index;
pub mod json;
mod lahc;
mod ordering;
pub mod progress;
pub mod quality;
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
pub use json::{
    error_envelope_json, quality_report_json, score_solution_json, solve_json,
    solve_json_with_config, solve_json_with_progress,
};
pub use progress::ProgressBeacon;
pub use quality::{
    backend_objective, quality_report, BackendObjective, QualityComponent, QualityReport,
};
pub use score::score_solution;
pub use solve::{solve, solve_with_config, solve_with_config_stats, solve_with_progress};
pub use types::{
    ConstraintWeights, Lesson, Placement, Problem, Room, RoomBlockedTime, RoomSubjectSuitability,
    SchoolClass, Solution, SolveConfig, SolveStats, Subject, Teacher, TeacherBlockedTime,
    TeacherQualification, TimeBlock, Violation, ViolationKind, PRODUCTION_ACTIVE_WEIGHTS,
};
