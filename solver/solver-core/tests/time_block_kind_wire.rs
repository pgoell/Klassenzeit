//! Wire-format coverage for the additive `TimeBlock.kind` field.
//! Confirms (a) the field defaults to `TimeBlockKind::Lesson` when callers
//! omit it, and (b) `"lesson"` / `"break"` snake_case roundtrip through
//! serde without information loss.

use serde_json::json;
use solver_core::{
    ids::TimeBlockId,
    types::{TimeBlock, TimeBlockKind},
};
use uuid::Uuid;

#[test]
fn time_block_default_kind_is_lesson() {
    let raw = json!({
        "id": Uuid::nil(),
        "day_of_week": 0,
        "position": 0,
    });
    let tb: TimeBlock = serde_json::from_value(raw).expect("deserialise TimeBlock without kind");
    assert_eq!(tb.kind, TimeBlockKind::Lesson);
}

#[test]
fn time_block_break_roundtrips() {
    let id = Uuid::new_v4();
    let raw = json!({
        "id": id,
        "day_of_week": 1,
        "position": 2,
        "kind": "break",
    });
    let tb: TimeBlock = serde_json::from_value(raw).expect("deserialise break TimeBlock");
    assert_eq!(tb.kind, TimeBlockKind::Break);
    assert_eq!(tb.id, TimeBlockId(id));

    let re = serde_json::to_value(&tb).expect("serialise TimeBlock");
    assert_eq!(re["kind"], "break");
}

#[test]
fn time_block_lesson_kind_roundtrips() {
    let tb = TimeBlock {
        id: TimeBlockId(Uuid::nil()),
        day_of_week: 0,
        position: 0,
        kind: TimeBlockKind::Lesson,
    };
    let re = serde_json::to_value(&tb).expect("serialise lesson TimeBlock");
    assert_eq!(re["kind"], "lesson");

    let back: TimeBlock = serde_json::from_value(re).expect("re-deserialise");
    assert_eq!(back.kind, TimeBlockKind::Lesson);
}
