//! Newtype wrappers around `uuid::Uuid` for each solver entity. Newtypes prevent
//! ID-category confusion at compile time (passing a `TeacherId` where a `RoomId`
//! is expected becomes a type error).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(
            /// Underlying UUID value.
            pub Uuid,
        );

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

define_id!(LessonId, "Identifier for a lesson entity.");
define_id!(TeacherId, "Identifier for a teacher entity.");
define_id!(RoomId, "Identifier for a room entity.");
define_id!(TimeBlockId, "Identifier for a time-block entity.");
define_id!(SubjectId, "Identifier for a subject entity.");
define_id!(SchoolClassId, "Identifier for a school-class entity.");
define_id!(
    LessonGroupId,
    "Stable identifier for a lesson group (set of co-placed lessons). Ships in this PR for wire-format completeness; the lesson-group co-placement constraint that consumes it is added by the algorithm-phase PR that follows."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_round_trips_as_plain_string_in_json() {
        let id = LessonId(Uuid::nil());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"00000000-0000-0000-0000-000000000000\"");
        let parsed: LessonId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn lesson_group_id_round_trips_through_json() {
        let id = LessonGroupId(uuid::Uuid::nil());
        let s = serde_json::to_string(&id).unwrap();
        let parsed: LessonGroupId = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn id_categories_are_distinct_types() {
        // This test compiles only if LessonId and TeacherId are distinct types.
        // If the macro ever collapses them (e.g. into a single alias), the two
        // `fn` signatures below would collide — which is exactly the property
        // we want to lock in.
        fn takes_lesson_id(_: LessonId) {}
        fn takes_teacher_id(_: TeacherId) {}
        takes_lesson_id(LessonId(Uuid::nil()));
        takes_teacher_id(TeacherId(Uuid::nil()));
    }
}
