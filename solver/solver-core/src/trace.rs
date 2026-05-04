//! Diagnostic stderr trace for `solve::try_place_block`. Compiles only under
//! `--features solver-trace`; off by default. See
//! `docs/superpowers/specs/2026-05-04-ffd-lock-in-diagnostic-design.md`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::ids::{LessonId, RoomId};

// Per-process ascending sequence number. Concurrent tests interleave their
// trace output; readers filter by lesson id and reconstruct order by `seq`.
// The counter is unconditionally `static` (cheap and safe under concurrent
// `cargo test`); only the call sites that increment it are feature-gated.
static FFD_TRACE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Emit one stderr line describing one FFD inner-loop decision. Called from
/// every `continue` / acceptance / failure branch of `solve::try_place_block`.
/// `room` is `None` for window-level rejections (teacher / class / capacity /
/// contiguity / score-pruning / locked-room conflict) and `Some(_)` for room
/// rejections inside the room loop and for the terminal `placed` branch.
pub(crate) fn ffd_trace(
    lesson_id: LessonId,
    day: u8,
    position: u8,
    room: Option<RoomId>,
    reason: &'static str,
) {
    let seq = FFD_TRACE_SEQ.fetch_add(1, Ordering::Relaxed);
    let lesson_short = short_uuid(lesson_id.0);
    let room_short = match room {
        Some(r) => short_uuid(r.0),
        None => "-".to_string(),
    };
    eprintln!(
        "ffd_trace seq={seq} lesson={lesson_short} day={day} pos={position} room={room_short} reason={reason}"
    );
}

// `simple()` renders the UUID without dashes; first 8 hex chars are enough
// to disambiguate lessons / rooms in trace output.
fn short_uuid(u: uuid::Uuid) -> String {
    let s = u.simple().to_string();
    s[..8].to_string()
}
