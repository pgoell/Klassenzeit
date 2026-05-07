//! Public data types for `solver-core`. Field names match the backend's SQL
//! join-table columns; wire format is JSON with snake_case fields.

use serde::{Deserialize, Serialize};

use crate::ids::{
    LessonGroupId, LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId,
};
use std::time::Duration;

/// Tunables for one solver invocation. Pass via [`crate::solve_with_config`];
/// the no-config [`crate::solve`] entry point uses [`SolveConfig::default`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolveConfig {
    /// Optional wall-clock budget. `None` means "no LAHC pass; greedy only".
    /// `Some(d)` triggers the LAHC local-search loop after greedy and bounds
    /// it to `d` of wall-clock time.
    pub deadline: Option<Duration>,
    /// Seed for the RNG used by the LAHC local-search loop. The greedy pass
    /// is deterministic without it.
    pub seed: u64,
    /// Weights that govern the soft-constraint scoring function.
    pub weights: ConstraintWeights,
    /// Maximum number of LAHC iterations. `None` means "deadline only".
    /// Primarily exists so property tests can cap iteration count for
    /// determinism without depending on wall-clock; production callers
    /// should leave this `None`.
    pub max_iterations: Option<u64>,
    /// Period for the ruin-and-recreate LAHC move. `None` (default) disables
    /// R&R; the LAHC loop runs Change-only. `Some(n)` triggers an R&R attempt
    /// every nth iteration, with Change attempts on the other (n-1)/n. The
    /// bake-off bench's `lahc_rr` backend sets this to `Some(25)`. Production
    /// callers leave this `None`; the active default in `solve()` is
    /// unchanged.
    pub lahc_rr_period: Option<u32>,
    /// Period for the Kempe-chain LAHC move. `None` (default) disables Kempe;
    /// the LAHC loop runs without chain swaps. `Some(n)` triggers a Kempe
    /// attempt every nth iteration, with R&R taking precedence on iterations
    /// where both periods divide. The bake-off bench's `lahc_rr_kempe` backend
    /// sets this to `Some(23)`. Production callers leave this `None`; the
    /// active default in `solve()` is unchanged.
    pub lahc_kempe_period: Option<u32>,
    /// Number of block-anchors selected per ruin-and-recreate attempt. Active
    /// default is `5`. The bake-off bench's `--rr-k` flag overrides this; item
    /// 21 sweeps the value to find the Pareto frontier on (feasibility,
    /// soft-score median).
    pub lahc_rr_k: u32,
}

impl Default for SolveConfig {
    fn default() -> Self {
        Self {
            deadline: None,
            seed: 0,
            weights: ConstraintWeights::default(),
            max_iterations: None,
            lahc_rr_period: None,
            lahc_kempe_period: None,
            lahc_rr_k: 5,
        }
    }
}

/// Soft-constraint weights consumed by `score_solution` and the lowest-delta
/// greedy in `solve_with_config`. Each field defaults to zero so explicit
/// `ConstraintWeights::default()` callers get unweighted behaviour. The
/// no-config `solve()` entry point applies active defaults of `1` per axis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConstraintWeights {
    /// Penalty per gap-hour in any class's day. A gap-hour is a position p in
    /// a `(school_class_id, day_of_week)` partition where the class has
    /// placements at some position less than p and some position greater than
    /// p on that day, but no placement at position p.
    pub class_gap: u32,
    /// Penalty per gap-hour in any teacher's day. Same definition as
    /// `class_gap`, partitioned by `(teacher_id, day_of_week)` instead.
    pub teacher_gap: u32,
    /// Global multiplier on the early-period axis. Per-placement penalty is
    /// `tb.position * weights.prefer_early_period * subject.prefer_early_period`
    /// (saturating). Zero disables the axis globally; a non-zero global with
    /// `subject.prefer_early_period == 0` still contributes nothing.
    pub prefer_early_period: u32,
    /// Global multiplier on the first-period axis. Per-placement penalty is
    /// `weights.avoid_first_period * subject.avoid_first_period` at
    /// `tb.position == 0` (saturating). Zero disables the axis globally.
    pub avoid_first_period: u32,
    /// Penalty per (class, placement) pair where the class has a non-null
    /// `home_room_id` that does not match the placement's `room_id`.
    /// Multi-class lessons accumulate the penalty per non-matching member
    /// class. Zero means the axis is disabled.
    pub prefer_home_room: u32,
    /// Global multiplier on the last-period axis. Per-placement penalty is
    /// `weights.avoid_last_period * subject.avoid_last_period` at
    /// `tb.position == max_position_for_day` for that placement's
    /// `day_of_week` (saturating). Zero disables the axis globally.
    pub avoid_last_period: u32,
    /// Global multiplier on the late-period axis. Per-placement penalty is
    /// `(max_position_for_day - tb.position) * weights.prefer_late_period * subject.prefer_late_period` (saturating).
    /// Zero disables the axis globally; a non-zero global with
    /// `subject.prefer_late_period == 0` still contributes nothing.
    pub prefer_late_period: u32,
    /// Penalty applied per-class for daily-count imbalance. Cost is the
    /// sum of `|count(day) - mean|` over days for each class with at
    /// least one placement, multiplied by this weight (saturating).
    /// Zero disables the axis. Scoring lands in a follow-up task; this
    /// field is added here so the struct shape is stable for callers.
    pub class_day_balance: u32,
}

/// Optional timing probes produced by [`crate::solve_with_config_stats`].
/// Populated by the LAHC loop and the FFD greedy entry-check; consumers
/// (today: `solver-bench`) median or aggregate across seed runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SolveStats {
    /// Wall-clock from `solve_with_config_stats` entry to first feasible
    /// incumbent. `Some(0.0)` when FFD greedy is already feasible at LAHC
    /// entry. `None` when the run never reaches feasibility.
    pub time_to_first_feasible_ms: Option<f64>,
    /// Wall-clock from `solve_with_config_stats` entry to the last
    /// running-best improvement. `Some(0.0)` when FFD greedy is already at
    /// `state.search_score_slice == 0` and feasible. `None` when no LAHC iteration
    /// improved the running-best (or LAHC was not run because deadline is
    /// `None`). Note that LAHC has no proof of optimality; `time_to_optimal_ms`
    /// is a lower bound on the actual optimisation cost (the time of the LAST
    /// improvement, not the FIRST proof of optimality).
    pub time_to_optimal_ms: Option<f64>,
}

/// Production-active soft-constraint weights. The bake-off bench, the JSON
/// adapter, and the new `score_solution_json` PyO3 binding all use this exact
/// weight set so cross-backend bench cells compare on the same scorer.
pub const PRODUCTION_ACTIVE_WEIGHTS: ConstraintWeights = ConstraintWeights {
    class_gap: 10,
    teacher_gap: 10,
    prefer_early_period: 1,
    avoid_first_period: 1,
    prefer_home_room: 5,
    avoid_last_period: 1,
    prefer_late_period: 1,
    class_day_balance: 5,
};

/// Complete solver input. Flat `Vec`s of relation pairs mirror the backend's SQL
/// join tables so serialisation is a 1:1 shape match with the API payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Problem {
    /// Available time blocks (slots) to place lessons into.
    pub time_blocks: Vec<TimeBlock>,
    /// Teachers eligible to teach lessons.
    pub teachers: Vec<Teacher>,
    /// Rooms available for placements.
    pub rooms: Vec<Room>,
    /// Subjects lessons can belong to.
    pub subjects: Vec<Subject>,
    /// School classes that receive lessons.
    pub school_classes: Vec<SchoolClass>,
    /// Lessons to place.
    pub lessons: Vec<Lesson>,
    /// Teacher / subject qualification pairs.
    pub teacher_qualifications: Vec<TeacherQualification>,
    /// Teacher / time-block pairs that mark a teacher as unavailable in that slot.
    pub teacher_blocked_times: Vec<TeacherBlockedTime>,
    /// Room / time-block pairs that mark a room as unavailable in that slot.
    pub room_blocked_times: Vec<RoomBlockedTime>,
    /// Room / subject pairs that explicitly mark a room as suitable for a subject.
    pub room_subject_suitabilities: Vec<RoomSubjectSuitability>,
    /// Pre-placed lessons the solver must keep verbatim. See
    /// `PinnedPlacement` for the contract; an empty list is the
    /// default for callers that do not need pin enforcement.
    /// Wire format is additive: callers omitting the field
    /// deserialise to an empty Vec.
    #[serde(default)]
    pub pinned_placements: Vec<PinnedPlacement>,
}

/// A pre-placed lesson that the solver must keep at its given
/// (time_block, room) without modification. FFD seeding skips lessons
/// whose ids appear in `Problem.pinned_placements`; LAHC moves never
/// select a placement whose lesson is pinned.
///
/// A multi-hour lesson (`preferred_block_size > 1`) appears as
/// multiple `PinnedPlacement` entries with the same `lesson_id` on
/// consecutive time-blocks of the same day. Malformed pins (unknown
/// lesson, non-contiguous block, duplicate slot) are reported as
/// `ViolationKind::PinnedConflict` and dropped from the active set so
/// the rest of the solve proceeds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedPlacement {
    /// The lesson whose placement is fixed.
    pub lesson_id: LessonId,
    /// The time-block the lesson is pinned to.
    pub time_block_id: TimeBlockId,
    /// The room the lesson is pinned to.
    pub room_id: RoomId,
}

/// A single time slot (e.g., a period on a given weekday).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeBlock {
    /// Stable identifier for this time block.
    pub id: TimeBlockId,
    /// Day of the week (0 = Monday, caller-defined).
    pub day_of_week: u8,
    /// Ordinal position within the day.
    pub position: u8,
}

/// A teacher available to teach lessons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Teacher {
    /// Stable identifier for this teacher.
    pub id: TeacherId,
    /// Maximum teaching hours the teacher can be scheduled for per week.
    pub max_hours_per_week: u8,
}

/// A room available for placements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Room {
    /// Stable identifier for this room.
    pub id: RoomId,
}

/// A subject (the thing being taught in a lesson).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    /// Stable identifier for this subject.
    pub id: SubjectId,
    /// Per-Subject weight applied to the early-period axis. Scoring adds
    /// `tb.position * weights.prefer_early_period * subject.prefer_early_period`
    /// per placement (saturating). Zero disables this axis for the subject.
    /// Wire format is additive: callers omitting the field deserialise to 0.
    #[serde(default)]
    pub prefer_early_period: u32,
    /// Per-Subject weight applied at `tb.position == 0`. Scoring adds
    /// `weights.avoid_first_period * subject.avoid_first_period` per placement
    /// at the first period of any day (saturating). Zero disables this axis.
    /// Wire format is additive: callers omitting the field deserialise to 0.
    #[serde(default)]
    pub avoid_first_period: u32,
    /// Per-Subject weight applied at `tb.position == max_position_for_day`.
    /// Scoring adds `weights.avoid_last_period * subject.avoid_last_period`
    /// per placement at the last period of any day (saturating). Zero
    /// disables this axis. Wire format is additive: callers omitting the
    /// field deserialise to 0.
    #[serde(default)]
    pub avoid_last_period: u32,
    /// Per-Subject weight applied to the late-period axis. Scoring adds
    /// `(max_position_for_day - tb.position) * weights.prefer_late_period * subject.prefer_late_period`
    /// per placement (saturating). Zero disables this axis for the subject.
    /// Wire format is additive: callers omitting the field deserialise to 0.
    #[serde(default)]
    pub prefer_late_period: u32,
    /// Per-day cap on hours of this subject for any single class on any
    /// single day. Counts hours (period span), not lessons; a 2-period
    /// block lesson contributes 2 to the daily count. Hard constraint:
    /// cap-violating candidates are pruned at placement time. Wire format
    /// is additive: callers omitting the field deserialise to 2.
    #[serde(default = "default_max_hours_per_day")]
    pub max_hours_per_day: u8,
}

fn default_max_hours_per_day() -> u8 {
    2
}

/// A school class that receives lessons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchoolClass {
    /// Stable identifier for this school class.
    pub id: SchoolClassId,
    /// Optional home-room identifier; when set, the `prefer_home_room`
    /// soft-constraint axis penalises placements of this class outside the
    /// referenced room. `None` means the class has no preferred room and the
    /// axis no-ops for it. Wire format is additive: existing JSON callers
    /// without the field deserialise to `None`.
    #[serde(default)]
    pub home_room_id: Option<RoomId>,
    /// Optional per-day cap on total lessons for this class on any single
    /// day. Counts lessons (placements), not periods; a 2-period block
    /// lesson contributes 1 to the daily count. `None` means no cap beyond
    /// what the class's `time_blocks` allow. Hard constraint: when set,
    /// cap-violating candidates are pruned at placement time. Wire format
    /// is additive: callers omitting the field deserialise to `None`.
    #[serde(default)]
    pub max_lessons_per_day: Option<u8>,
}

/// A lesson that must be placed `hours_per_week` times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    /// Stable identifier for this lesson.
    pub id: LessonId,
    /// Receiving school classes. A single-class lesson has one entry; a
    /// cross-class lesson (e.g. a parallel Religionsmodell trio) has the full
    /// set of participating classes. Must be non-empty and contain no
    /// duplicates; `validate_structural` rejects violations.
    pub school_class_ids: Vec<SchoolClassId>,
    /// Subject taught in this lesson.
    pub subject_id: SubjectId,
    /// Teacher assigned to this lesson.
    pub teacher_id: TeacherId,
    /// Number of hours of this lesson to place per week.
    pub hours_per_week: u8,
    /// Preferred block size for placement. `1` means single-hour placements;
    /// `n > 1` means each block is `n` consecutive same-day positions in one
    /// room. The solver places `hours_per_week / preferred_block_size` blocks
    /// per lesson. Must be `>= 1` and must divide `hours_per_week`; otherwise
    /// `validate_structural` returns `Err(Error::Input(...))`. Defaults to 1
    /// when the JSON field is omitted, keeping the wire format additive.
    #[serde(default = "default_preferred_block_size")]
    pub preferred_block_size: u8,
    /// Optional group identifier; lessons sharing a non-null `lesson_group_id`
    /// are co-placed by the lesson-group constraint. Read-only in this PR (the
    /// constraint that consumes it ships with the algorithm-phase PR); a
    /// `None` value means the lesson is independent.
    #[serde(default)]
    pub lesson_group_id: Option<LessonGroupId>,
}

fn default_preferred_block_size() -> u8 {
    1
}

/// A single (teacher, subject) qualification pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherQualification {
    /// Qualified teacher.
    pub teacher_id: TeacherId,
    /// Subject the teacher is qualified for.
    pub subject_id: SubjectId,
}

/// Marks a teacher as unavailable in a specific time block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherBlockedTime {
    /// Teacher that is blocked.
    pub teacher_id: TeacherId,
    /// Time block in which the teacher is blocked.
    pub time_block_id: TimeBlockId,
}

/// Marks a room as unavailable in a specific time block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomBlockedTime {
    /// Room that is blocked.
    pub room_id: RoomId,
    /// Time block in which the room is blocked.
    pub time_block_id: TimeBlockId,
}

/// Explicitly marks a room as suitable for a subject. A room with no entries
/// suits every subject; a room with entries suits only the listed subjects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomSubjectSuitability {
    /// Room in question.
    pub room_id: RoomId,
    /// Subject the room is marked suitable for.
    pub subject_id: SubjectId,
}

/// Result of a solver run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Solution {
    /// Successful placements, one per `(lesson, hour)`.
    pub placements: Vec<Placement>,
    /// Violations recorded during solving (e.g., unplaced hours, no qualified teacher).
    pub violations: Vec<Violation>,
    /// Full weighted soft-constraint cost on the final placements,
    /// computed by `score::score_solution(problem, placements, weights)`
    /// at the end of every `solve_with_config`. The LAHC inner loop
    /// optimises a faster slice (`class_gap + teacher_gap +
    /// subject_pref`) for delta efficiency; this reported field is the
    /// canonical objective so cross-backend bench cells (LAHC, cpsat)
    /// compare on the same number. Zero when every active weight axis
    /// contributes zero (e.g. zero weights, or a fully optimal plan
    /// against the active weights).
    pub soft_score: u32,
}

/// A single successful placement of one hour of one lesson.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    /// Lesson whose hour has been placed.
    pub lesson_id: LessonId,
    /// Time block the lesson was placed into.
    pub time_block_id: TimeBlockId,
    /// Room the lesson was placed into.
    pub room_id: RoomId,
}

/// A single hard-constraint violation recorded by the solver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Violation {
    /// Kind of violation.
    pub kind: ViolationKind,
    /// Lesson the violation is about.
    pub lesson_id: LessonId,
    /// Zero-based hour index within the lesson.
    pub hour_index: u8,
    /// Optional reason string accompanying the violation. Today only
    /// `PinnedConflict` populates this; other variants leave it `None`.
    /// Wire format is additive (`#[serde(default)]`).
    #[serde(default)]
    pub reason: Option<String>,
}

/// Discriminator for `Violation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// The lesson's assigned teacher lacks the subject qualification.
    NoQualifiedTeacher,
    /// Placing this hour would push the teacher past `max_hours_per_week`.
    TeacherOverCapacity,
    /// No time block has both the (teacher, class) pair free.
    NoFreeTimeBlock,
    /// No room is suitable for the subject and free in any free time block.
    NoSuitableRoom,
    /// Atomic lesson-group co-placement failed for this block: no `time_block`
    /// admits all group members with pairwise-distinct rooms and free
    /// teachers / classes. One entry per qualified member per failed block.
    LessonGroupSplit,
    /// A pin-on-input entry was malformed and dropped. The accompanying
    /// `Violation.reason` carries the diagnostic code: one of
    /// `"unknown_lesson"`, `"unknown_time_block"`, `"unknown_room"`,
    /// `"duplicate_slot"`, `"block_size_mismatch"`.
    PinnedConflict,
    /// A class accumulated more hours of a single subject on one day than
    /// the subject's `max_hours_per_day` cap allows. Surfaced in solver
    /// telemetry only; the runtime path prunes cap-violating candidates
    /// before they enter the search.
    SubjectDailyHourCapExceeded,
    /// A class accumulated more total lessons on one day than the class's
    /// `max_lessons_per_day` cap allows. Surfaced in solver telemetry only;
    /// the runtime path prunes cap-violating candidates before they enter
    /// the search.
    ClassDailyLessonCapExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn lesson_id() -> LessonId {
        LessonId(Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap())
    }

    #[test]
    fn solve_config_default_disables_rr() {
        let cfg = SolveConfig::default();
        assert_eq!(cfg.lahc_rr_period, None);
    }

    #[test]
    fn production_active_weights_match_legacy_inline_literal() {
        let inline = ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_early_period: 1,
            avoid_first_period: 1,
            prefer_home_room: 5,
            avoid_last_period: 1,
            prefer_late_period: 1,
            class_day_balance: 5,
        };
        assert_eq!(crate::PRODUCTION_ACTIVE_WEIGHTS, inline);
    }

    #[test]
    fn problem_round_trips_through_json() {
        let original = Problem {
            time_blocks: vec![],
            teachers: vec![],
            rooms: vec![],
            subjects: vec![],
            school_classes: vec![],
            lessons: vec![],
            teacher_qualifications: vec![],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: Problem = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn violation_kind_serialises_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&ViolationKind::NoQualifiedTeacher).unwrap(),
            "\"no_qualified_teacher\""
        );
        assert_eq!(
            serde_json::to_string(&ViolationKind::TeacherOverCapacity).unwrap(),
            "\"teacher_over_capacity\""
        );
        assert_eq!(
            serde_json::to_string(&ViolationKind::NoFreeTimeBlock).unwrap(),
            "\"no_free_time_block\""
        );
        assert_eq!(
            serde_json::to_string(&ViolationKind::NoSuitableRoom).unwrap(),
            "\"no_suitable_room\""
        );
    }

    #[test]
    fn lesson_accepts_preferred_block_size_field() {
        let json = format!(
            r#"{{"id":"{}","school_class_ids":["{}"],"subject_id":"{}","teacher_id":"{}","hours_per_week":4,"preferred_block_size":2}}"#,
            Uuid::nil(),
            Uuid::nil(),
            Uuid::nil(),
            Uuid::nil()
        );
        let lesson: Lesson = serde_json::from_str(&json).unwrap();
        assert_eq!(lesson.preferred_block_size, 2);
    }

    #[test]
    fn lesson_defaults_preferred_block_size_to_one_when_field_omitted() {
        let json = format!(
            r#"{{"id":"{}","school_class_ids":["{}"],"subject_id":"{}","teacher_id":"{}","hours_per_week":1}}"#,
            Uuid::nil(),
            Uuid::nil(),
            Uuid::nil(),
            Uuid::nil()
        );
        let lesson: Lesson = serde_json::from_str(&json).unwrap();
        assert_eq!(lesson.preferred_block_size, 1);
    }

    #[test]
    fn lesson_accepts_school_class_ids_with_one_element() {
        let class_id = Uuid::from_bytes([1; 16]);
        let json = format!(
            r#"{{"id":"{}","school_class_ids":["{}"],"subject_id":"{}","teacher_id":"{}","hours_per_week":1}}"#,
            Uuid::nil(),
            class_id,
            Uuid::nil(),
            Uuid::nil()
        );
        let lesson: Lesson = serde_json::from_str(&json).unwrap();
        assert_eq!(lesson.school_class_ids.len(), 1);
        assert_eq!(lesson.school_class_ids[0], SchoolClassId(class_id));
        assert!(lesson.lesson_group_id.is_none());
    }

    #[test]
    fn lesson_accepts_school_class_ids_with_three_elements() {
        let c1 = Uuid::from_bytes([1; 16]);
        let c2 = Uuid::from_bytes([2; 16]);
        let c3 = Uuid::from_bytes([3; 16]);
        let json = format!(
            r#"{{"id":"{}","school_class_ids":["{}","{}","{}"],"subject_id":"{}","teacher_id":"{}","hours_per_week":1}}"#,
            Uuid::nil(),
            c1,
            c2,
            c3,
            Uuid::nil(),
            Uuid::nil()
        );
        let lesson: Lesson = serde_json::from_str(&json).unwrap();
        assert_eq!(lesson.school_class_ids.len(), 3);
        assert_eq!(lesson.school_class_ids[0], SchoolClassId(c1));
        assert_eq!(lesson.school_class_ids[1], SchoolClassId(c2));
        assert_eq!(lesson.school_class_ids[2], SchoolClassId(c3));
    }

    #[test]
    fn lesson_round_trips_lesson_group_id_when_present() {
        let group_id = Uuid::from_bytes([7; 16]);
        let json = format!(
            r#"{{"id":"{}","school_class_ids":["{}"],"subject_id":"{}","teacher_id":"{}","hours_per_week":1,"lesson_group_id":"{}"}}"#,
            Uuid::nil(),
            Uuid::nil(),
            Uuid::nil(),
            Uuid::nil(),
            group_id
        );
        let lesson: Lesson = serde_json::from_str(&json).unwrap();
        assert_eq!(lesson.lesson_group_id, Some(LessonGroupId(group_id)));
        let reserialised = serde_json::to_string(&lesson).unwrap();
        let parsed_again: Lesson = serde_json::from_str(&reserialised).unwrap();
        assert_eq!(parsed_again, lesson);
    }

    #[test]
    fn violation_kind_serialises_lesson_group_split() {
        assert_eq!(
            serde_json::to_string(&ViolationKind::LessonGroupSplit).unwrap(),
            "\"lesson_group_split\""
        );
    }

    #[test]
    fn school_class_round_trips_home_room_id_when_present() {
        let room_id = Uuid::from_bytes([8; 16]);
        let class_id = Uuid::from_bytes([9; 16]);
        let json = format!(r#"{{"id":"{class_id}","home_room_id":"{room_id}"}}"#);
        let sc: SchoolClass = serde_json::from_str(&json).unwrap();
        assert_eq!(sc.home_room_id, Some(RoomId(room_id)));
        let reserialised = serde_json::to_string(&sc).unwrap();
        let parsed_again: SchoolClass = serde_json::from_str(&reserialised).unwrap();
        assert_eq!(parsed_again, sc);
    }

    #[test]
    fn school_class_defaults_home_room_id_to_none_when_field_omitted() {
        let class_id = Uuid::from_bytes([1; 16]);
        let json = format!(r#"{{"id":"{class_id}"}}"#);
        let sc: SchoolClass = serde_json::from_str(&json).unwrap();
        assert!(sc.home_room_id.is_none());
    }

    #[test]
    fn solution_round_trips_with_placements_and_violations() {
        let solution = Solution {
            placements: vec![Placement {
                lesson_id: lesson_id(),
                time_block_id: TimeBlockId(Uuid::nil()),
                room_id: RoomId(Uuid::nil()),
            }],
            violations: vec![Violation {
                kind: ViolationKind::TeacherOverCapacity,
                lesson_id: lesson_id(),
                hour_index: 0,
                reason: None,
            }],
            soft_score: 0,
        };
        let json = serde_json::to_string(&solution).unwrap();
        let parsed: Solution = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, solution);
    }
}
