# FFD lock-in diagnostic implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land a deterministic Rust reproducer of the FFD `no_suitable_room` lock-in on the demo Grundschule, plus a feature-gated trace of FFD's inner-loop decisions, so the Decision phase (item 3 of the active sprint) can pick Path A / B / C with cited evidence.

**Architecture:** Two narrow additions to `solver/solver-core` and one mise task tweak. No behaviour change. New `#[test]` in `tests/same_room_property.rs` builds a Grundschule-shaped `Problem` with hand-pinned teacher allocation and asserts FFD currently returns at least one `ViolationKind::NoSuitableRoom`. New `solver-trace` Cargo feature wraps a new `solver/solver-core/src/trace.rs` module; every `continue` / acceptance / failure branch in `solve.rs::try_place_block` gets a `#[cfg(feature = "solver-trace")]` call site. `mise run lint:rust` is widened to also clippy with `--all-features` so the gated branch can't rot.

**Tech Stack:** Rust 1.85, solver-core (pure), Cargo features, no new deps.

**Spec:** `docs/superpowers/specs/2026-05-04-ffd-lock-in-diagnostic-design.md`. Commits land on branch `feat/solver-ffd-diagnostic`.

---

## File structure

| File | Action | Responsibility |
|---|---|---|
| `solver/solver-core/Cargo.toml` | Modify | Add `[features] solver-trace = []`. |
| `solver/solver-core/src/lib.rs` | Modify | Declare `#[cfg(feature = "solver-trace")] mod trace;`. |
| `solver/solver-core/src/trace.rs` | Create | Holds `pub(crate) fn ffd_trace(...)` and the per-process atomic sequence counter. Whole file is gated by `#[cfg(feature = "solver-trace")]` via the lib.rs declaration. |
| `solver/solver-core/src/solve.rs` | Modify | One `#[cfg(feature = "solver-trace")] trace::ffd_trace(...)` call before each existing `continue` / acceptance / failure branch in `try_place_block`. Mapping per spec L82-95. |
| `solver/solver-core/tests/same_room_property.rs` | Modify | Add `ffd_lock_in_grundschule()` builder + `#[test] fn ffd_locks_in_on_demo_grundschule_and_returns_no_suitable_room()`. |
| `mise.toml` | Modify | Widen `lint:rust` clippy invocation with `--all-features`. |
| `docs/superpowers/OPEN_THINGS.md` | Modify | Mark sprint diagnostic items 1 + 2 as shipped after PR opens (final commit on the branch). |

---

## Task 1: Build the deterministic FFD lock-in reproducer

**Files:**
- Modify: `solver/solver-core/tests/same_room_property.rs`

This is the red-green-refactor for item 1. The test asserts a *current* failure mode (FFD returns `NoSuitableRoom`); when item 4 of the sprint lands, that PR flips this assertion. We TDD it: write the test, watch it fail (because no helper exists), build the helper to compile, then iterate the teacher allocation until the assertion goes from `assertion_failed: violations is empty` to `assertion succeeded: NoSuitableRoom present`.

- [ ] **Step 1.1: Add the new test fn skeleton (compiles, fails).**

Append to `solver/solver-core/tests/same_room_property.rs`:

```rust
/// Demo-Grundschule-shaped fixture sized to reproduce the FFD lock-in flake
/// described in `docs/superpowers/OPEN_THINGS.md` (active sprint, diagnostic
/// phase, item 1) and in `solver/CLAUDE.md` L44. Mirrors
/// `backend/src/klassenzeit_backend/seed/demo_grundschule.py` at fixed
/// deterministic UUIDs: 4 classes (1a..4a), 5 days × 7 periods, 7 rooms (4
/// Klassenraeume + Turnhalle + Musikraum + Kunstraum), 9 subjects (D, M, SU,
/// E, ETH, KU, MU, SP, FOe), 6 teachers (MUE, SCH, WEB, FIS, BEC, HOF), and
/// the Hessen Stundentafel hours (grades 1-2 = 8 lessons per class, grades
/// 3-4 = 9). Hand-pinned teacher allocation is chosen so FFD reliably locks
/// `(class 1a, day 0, D)` into the wrong Klassenraum and the matching second
/// hour fails to place because every academically-suitable room is already
/// held by a sibling class's lock.
fn ffd_lock_in_grundschule() -> Problem {
    todo!("filled in by step 1.3");
}

/// Asserts the FFD lock-in failure mode the active sprint's diagnostic phase
/// is built around: a Grundschule-shaped Problem produces at least one
/// `ViolationKind::NoSuitableRoom` under greedy-only solving with the
/// production active-default weights. When the active sprint's item 4 lands
/// (Path A / B / C from item 3), that PR renames this test to
/// `ffd_does_not_lock_in_on_demo_grundschule` and flips the assertion to
/// `assert!(solution.violations.is_empty())`. The rename is the visible
/// signal that the regression became a guarantee.
#[test]
fn ffd_locks_in_on_demo_grundschule_and_returns_no_suitable_room() {
    let problem = ffd_lock_in_grundschule();
    let config = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 10,
            teacher_gap: 10,
            prefer_early_period: 1,
            avoid_first_period: 1,
            prefer_home_room: 5,
            avoid_last_period: 1,
            prefer_late_period: 1,
            class_day_balance: 5,
        },
        deadline: None, // greedy only; LAHC cannot escape the lock-in
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&problem, &config).expect("solve");
    let no_suitable: Vec<_> = solution
        .violations
        .iter()
        .filter(|v| matches!(v.kind, ViolationKind::NoSuitableRoom))
        .collect();
    assert!(
        !no_suitable.is_empty(),
        "expected at least one NoSuitableRoom violation; got {:?}",
        solution.violations
    );
}
```

- [ ] **Step 1.2: Confirm it fails to compile (TDD red).**

Run: `cargo nextest run -p solver-core --test same_room_property -E 'test(/^ffd_locks_in/)' 2>&1 | head -40`
Expected: compile error pointing at `todo!("filled in by step 1.3")` from inside `ffd_lock_in_grundschule`.

- [ ] **Step 1.3: Build the fixture.**

Replace the `todo!()` body with the full Grundschule-shaped builder. Mirror the seed's hour matrix exactly. Sketch (the executor MUST type out every field explicitly because `Problem` is an exhaustive struct literal per `solver/CLAUDE.md` L42):

```rust
fn ffd_lock_in_grundschule() -> Problem {
    // Time blocks: 5 days, 7 periods each. id base 100.
    let time_blocks: Vec<TimeBlock> = (0..35u8)
        .map(|i| TimeBlock {
            id: TimeBlockId(same_room_uuid(100 + i)),
            day_of_week: i / 7,
            position: i % 7,
        })
        .collect();

    // Rooms 50..56. Klassenraeume 50..53 are academic-suitable; 54 = TH (Sport),
    // 55 = MU-Raum, 56 = KU-Raum.
    let rooms: Vec<Room> = (0..7u8)
        .map(|i| Room {
            id: RoomId(same_room_uuid(50 + i)),
        })
        .collect();
    let klassenraum_ids = [rooms[0].id, rooms[1].id, rooms[2].id, rooms[3].id];
    let turnhalle = rooms[4].id;
    let musikraum = rooms[5].id;
    let kunstraum = rooms[6].id;

    // Classes 70..73 = 1a..4a. home_room_id = own Klassenraum.
    let classes: Vec<SchoolClass> = (0..4u8)
        .map(|i| SchoolClass {
            id: SchoolClassId(same_room_uuid(70 + i)),
            home_room_id: Some(klassenraum_ids[i as usize]),
        })
        .collect();

    // Subjects 80..88: D M SU E ETH KU MU SP FOe.
    let d = SubjectId(same_room_uuid(80));
    let m = SubjectId(same_room_uuid(81));
    let su = SubjectId(same_room_uuid(82));
    let e_subj = SubjectId(same_room_uuid(83));
    let eth = SubjectId(same_room_uuid(84));
    let ku = SubjectId(same_room_uuid(85));
    let mu = SubjectId(same_room_uuid(86));
    let sp = SubjectId(same_room_uuid(87));
    let foe = SubjectId(same_room_uuid(88));
    let subjects: Vec<Subject> = vec![
        Subject { id: d, prefer_early_period: 1, avoid_first_period: 0, avoid_last_period: 1, prefer_late_period: 0 },
        Subject { id: m, prefer_early_period: 1, avoid_first_period: 0, avoid_last_period: 1, prefer_late_period: 0 },
        Subject { id: su, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0, prefer_late_period: 0 },
        Subject { id: e_subj, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0, prefer_late_period: 0 },
        Subject { id: eth, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0, prefer_late_period: 0 },
        Subject { id: ku, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0, prefer_late_period: 0 },
        Subject { id: mu, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0, prefer_late_period: 0 },
        Subject { id: sp, prefer_early_period: 0, avoid_first_period: 1, avoid_last_period: 0, prefer_late_period: 0 },
        Subject { id: foe, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0, prefer_late_period: 0 },
    ];

    // Teachers 30..35: MUE SCH WEB FIS BEC HOF.
    let mue = TeacherId(same_room_uuid(30));
    let sch = TeacherId(same_room_uuid(31));
    let web = TeacherId(same_room_uuid(32));
    let fis = TeacherId(same_room_uuid(33));
    let bec = TeacherId(same_room_uuid(34));
    let hof = TeacherId(same_room_uuid(35));
    let teachers = vec![
        Teacher { id: mue, max_hours_per_week: 28 },
        Teacher { id: sch, max_hours_per_week: 28 },
        Teacher { id: web, max_hours_per_week: 28 },
        Teacher { id: fis, max_hours_per_week: 28 },
        Teacher { id: bec, max_hours_per_week: 18 },
        Teacher { id: hof, max_hours_per_week: 21 },
    ];

    // Qualifications matrix mirrors the seed.
    let teacher_qualifications = vec![
        // MUE: D, M, SU, KU
        TeacherQualification { teacher_id: mue, subject_id: d },
        TeacherQualification { teacher_id: mue, subject_id: m },
        TeacherQualification { teacher_id: mue, subject_id: su },
        TeacherQualification { teacher_id: mue, subject_id: ku },
        // SCH: D, M, SU, KU
        TeacherQualification { teacher_id: sch, subject_id: d },
        TeacherQualification { teacher_id: sch, subject_id: m },
        TeacherQualification { teacher_id: sch, subject_id: su },
        TeacherQualification { teacher_id: sch, subject_id: ku },
        // WEB: D, M, SU, E
        TeacherQualification { teacher_id: web, subject_id: d },
        TeacherQualification { teacher_id: web, subject_id: m },
        TeacherQualification { teacher_id: web, subject_id: su },
        TeacherQualification { teacher_id: web, subject_id: e_subj },
        // FIS: D, M, SU, E
        TeacherQualification { teacher_id: fis, subject_id: d },
        TeacherQualification { teacher_id: fis, subject_id: m },
        TeacherQualification { teacher_id: fis, subject_id: su },
        TeacherQualification { teacher_id: fis, subject_id: e_subj },
        // BEC: ETH, MU, FOe (RK/RE skipped because no class teaches them in this fixture)
        TeacherQualification { teacher_id: bec, subject_id: eth },
        TeacherQualification { teacher_id: bec, subject_id: mu },
        TeacherQualification { teacher_id: bec, subject_id: foe },
        // HOF: SP, KU, FOe
        TeacherQualification { teacher_id: hof, subject_id: sp },
        TeacherQualification { teacher_id: hof, subject_id: ku },
        TeacherQualification { teacher_id: hof, subject_id: foe },
    ];

    // Stundentafel: grades 1-2 (8 subjects), grades 3-4 (9 subjects, adds E).
    // Hand-pinned teacher per (class, subject). Initial allocation; iterate
    // in step 1.5 if it doesn't trigger lock-in.
    struct LessonRow { class_idx: usize, subject: SubjectId, hours: u8, block_size: u8, teacher: TeacherId }
    let class_1a = classes[0].id;
    let class_2a = classes[1].id;
    let class_3a = classes[2].id;
    let class_4a = classes[3].id;
    let rows: Vec<LessonRow> = vec![
        // 1a (grades 1-2): D=6/MUE, M=5/MUE, SU=2 doppel/MUE, ETH=2/BEC, KU=2/HOF, MU=1/BEC, SP=3/HOF, FOe=2/BEC
        LessonRow { class_idx: 0, subject: d, hours: 6, block_size: 1, teacher: mue },
        LessonRow { class_idx: 0, subject: m, hours: 5, block_size: 1, teacher: mue },
        LessonRow { class_idx: 0, subject: su, hours: 2, block_size: 2, teacher: mue },
        LessonRow { class_idx: 0, subject: eth, hours: 2, block_size: 1, teacher: bec },
        LessonRow { class_idx: 0, subject: ku, hours: 2, block_size: 1, teacher: hof },
        LessonRow { class_idx: 0, subject: mu, hours: 1, block_size: 1, teacher: bec },
        LessonRow { class_idx: 0, subject: sp, hours: 3, block_size: 1, teacher: hof },
        LessonRow { class_idx: 0, subject: foe, hours: 2, block_size: 1, teacher: bec },
        // 2a (grades 1-2): same shape, SCH on D/M/SU
        LessonRow { class_idx: 1, subject: d, hours: 6, block_size: 1, teacher: sch },
        LessonRow { class_idx: 1, subject: m, hours: 5, block_size: 1, teacher: sch },
        LessonRow { class_idx: 1, subject: su, hours: 2, block_size: 2, teacher: sch },
        LessonRow { class_idx: 1, subject: eth, hours: 2, block_size: 1, teacher: bec },
        LessonRow { class_idx: 1, subject: ku, hours: 2, block_size: 1, teacher: sch },
        LessonRow { class_idx: 1, subject: mu, hours: 1, block_size: 1, teacher: bec },
        LessonRow { class_idx: 1, subject: sp, hours: 3, block_size: 1, teacher: hof },
        LessonRow { class_idx: 1, subject: foe, hours: 2, block_size: 1, teacher: bec },
        // 3a (grades 3-4): D=5/WEB, M=5/WEB, SU=4 doppel/WEB, E=2/WEB, ETH=2/BEC, KU=2/HOF, MU=1/BEC, SP=3/HOF, FOe=2/HOF
        LessonRow { class_idx: 2, subject: d, hours: 5, block_size: 1, teacher: web },
        LessonRow { class_idx: 2, subject: m, hours: 5, block_size: 1, teacher: web },
        LessonRow { class_idx: 2, subject: su, hours: 4, block_size: 2, teacher: web },
        LessonRow { class_idx: 2, subject: e_subj, hours: 2, block_size: 1, teacher: web },
        LessonRow { class_idx: 2, subject: eth, hours: 2, block_size: 1, teacher: bec },
        LessonRow { class_idx: 2, subject: ku, hours: 2, block_size: 1, teacher: hof },
        LessonRow { class_idx: 2, subject: mu, hours: 1, block_size: 1, teacher: bec },
        LessonRow { class_idx: 2, subject: sp, hours: 3, block_size: 1, teacher: hof },
        LessonRow { class_idx: 2, subject: foe, hours: 2, block_size: 1, teacher: hof },
        // 4a (grades 3-4): same shape, FIS on academics
        LessonRow { class_idx: 3, subject: d, hours: 5, block_size: 1, teacher: fis },
        LessonRow { class_idx: 3, subject: m, hours: 5, block_size: 1, teacher: fis },
        LessonRow { class_idx: 3, subject: su, hours: 4, block_size: 2, teacher: fis },
        LessonRow { class_idx: 3, subject: e_subj, hours: 2, block_size: 1, teacher: fis },
        LessonRow { class_idx: 3, subject: eth, hours: 2, block_size: 1, teacher: bec },
        LessonRow { class_idx: 3, subject: ku, hours: 2, block_size: 1, teacher: hof },
        LessonRow { class_idx: 3, subject: mu, hours: 1, block_size: 1, teacher: bec },
        LessonRow { class_idx: 3, subject: sp, hours: 3, block_size: 1, teacher: hof },
        LessonRow { class_idx: 3, subject: foe, hours: 2, block_size: 1, teacher: hof },
    ];
    let class_ids = [class_1a, class_2a, class_3a, class_4a];
    let lessons: Vec<Lesson> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| Lesson {
            id: LessonId(same_room_uuid(200 + (i as u8))),
            school_class_ids: vec![class_ids[r.class_idx]],
            subject_id: r.subject,
            teacher_id: r.teacher,
            hours_per_week: r.hours,
            preferred_block_size: r.block_size,
            lesson_group_id: None,
        })
        .collect();

    // Room-subject suitabilities mirror the seed: Klassenraeume suit
    // {D, M, SU, E, ETH, FOe} (academic subjects taught in classroom);
    // Turnhalle suits SP only; Musikraum suits MU only; Kunstraum suits KU only.
    let mut room_subject_suitabilities = Vec::new();
    let academic = [d, m, su, e_subj, eth, foe];
    for room in &klassenraum_ids {
        for subj in academic {
            room_subject_suitabilities.push(RoomSubjectSuitability { room_id: *room, subject_id: subj });
        }
    }
    room_subject_suitabilities.push(RoomSubjectSuitability { room_id: turnhalle, subject_id: sp });
    room_subject_suitabilities.push(RoomSubjectSuitability { room_id: musikraum, subject_id: mu });
    room_subject_suitabilities.push(RoomSubjectSuitability { room_id: kunstraum, subject_id: ku });

    Problem {
        time_blocks,
        teachers,
        rooms,
        subjects,
        school_classes: classes,
        lessons,
        teacher_qualifications,
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities,
        pinned_placements: vec![],
    }
}
```

Note that the seed's RK and RE subjects + the seed's `_KLASSENRAUM_SUITABLE_SUBJECTS` entries for them are intentionally absent: no class in this fixture teaches them, so their inclusion would be dead state.

- [ ] **Step 1.4: Run the new test, verify it now compiles and check the outcome.**

Run: `cargo nextest run -p solver-core --test same_room_property -E 'test(/^ffd_locks_in/)' 2>&1 | tail -40`

Two possible outcomes:
- **A. Test PASSES** (i.e. the fixture produces a `NoSuitableRoom` violation as desired) → assertion succeeds → record the locked triple in step 1.6 and move to step 1.7.
- **B. Test FAILS** with "expected at least one NoSuitableRoom violation; got []" → the fixture solves cleanly. Iterate the teacher allocation in step 1.5 until outcome A.

- [ ] **Step 1.5 (conditional, only if step 1.4 was outcome B): iterate the teacher allocation until the test passes.**

Two cheap knobs in increasing-cost order:
1. Swap one classroom-suitable teacher inside grades 1-2 or 3-4 (e.g. flip 2a's KU teacher from SCH to HOF) to load HOF / BEC differently and shift FFD's lowest-id-room walk. Keep the pinning explicit; do not introduce randomness.
2. If teacher swaps don't reproduce, add ONE `RoomBlockedTime` simulating a competing class's persisted block (e.g. block Klasse 1a room at (day=0, position=0)), document the choice in the test docstring.

Re-run the test after each iteration. Stop when outcome A holds for 10/10 consecutive runs:

```bash
for i in $(seq 1 10); do
  cargo nextest run -p solver-core --test same_room_property \
    -E 'test(/^ffd_locks_in/)' --no-fail-fast 2>&1 | tail -3
done
```

Expected: every run reports `1 test passed`.

- [ ] **Step 1.6: Strengthen the assertion to pin the locked triple.**

Once the test reliably triggers `NoSuitableRoom`, replace the `assert!(!no_suitable.is_empty(), ...)` with a more specific assertion that records *which* lesson failed. This protects against a future change that produces a *different* lock-in (which would also be a regression worth surfacing). Sketch:

```rust
    let first = no_suitable.first().expect("non-empty by previous assert");
    let lesson = problem.lessons.iter().find(|l| l.id == first.lesson_id).expect("lesson exists");
    assert_eq!(
        lesson.subject_id,
        d,
        "expected first NoSuitableRoom to be on subject D; got {:?}; lock-in pattern may have shifted, update the test docstring",
        lesson.subject_id,
    );
```

(Replace `d` with whichever subject empirically fails after step 1.5; the executor records the actual triple in the test docstring at the end of step 1.6.)

- [ ] **Step 1.7: Run lint + verify the existing tests still pass.**

Run: `mise run lint:rust`
Expected: pass.

Run: `cargo nextest run -p solver-core --test same_room_property`
Expected: 3 tests, 3 passed (the new one plus the existing two).

- [ ] **Step 1.8: Commit.**

```bash
git add solver/solver-core/tests/same_room_property.rs
git commit -m "test(solver-core): deterministic FFD lock-in reproducer for demo Grundschule"
```

---

## Task 2: Add `solver-trace` Cargo feature + module skeleton

**Files:**
- Modify: `solver/solver-core/Cargo.toml`
- Modify: `solver/solver-core/src/lib.rs`
- Create: `solver/solver-core/src/trace.rs`

This task lands the feature flag and the module skeleton. The module compiles only under the feature; outside it, the `mod trace;` line is itself gated, so default builds produce the same binary they do today.

- [ ] **Step 2.1: Add the feature flag to `Cargo.toml`.**

Edit `solver/solver-core/Cargo.toml` after the `[lib]` block:

```toml
[features]
# Enable diagnostic stderr trace lines from `try_place_block`. Off by default;
# the trace is a one-shot diagnostic for the FFD lock-in described in the
# active sprint of `docs/superpowers/OPEN_THINGS.md`. Production callers
# (solver-py, the bench) never enable it.
solver-trace = []
```

- [ ] **Step 2.2: Wire the gated module in `lib.rs`.**

Edit `solver/solver-core/src/lib.rs`. Insert immediately after `mod ordering;`:

```rust
#[cfg(feature = "solver-trace")]
mod trace;
```

- [ ] **Step 2.3: Create `trace.rs` with the formatter and the per-process counter.**

Create `solver/solver-core/src/trace.rs`:

```rust
//! Diagnostic stderr trace for `solve::try_place_block`. Compiles only under
//! `--features solver-trace`; off by default. See
//! `docs/superpowers/specs/2026-05-04-ffd-lock-in-diagnostic-design.md`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::ids::{LessonId, RoomId};

/// Per-process ascending sequence number. Concurrent tests interleave their
/// trace output; readers filter by lesson id and reconstruct order by `seq`.
/// The counter is unconditionally `static` (cheap and safe under concurrent
/// `cargo test`), only the call sites that increment it are feature-gated.
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

fn short_uuid(u: uuid::Uuid) -> String {
    let s = u.simple().to_string();
    s[..8].to_string()
}
```

- [ ] **Step 2.4: Verify both feature configurations compile clean.**

Run (without the feature, the module must NOT compile):
```bash
cargo build -p solver-core
```
Expected: pass; no mention of `trace.rs`.

Run (with the feature, the module must compile clean against `#![deny(missing_docs)]`):
```bash
cargo build -p solver-core --features solver-trace
cargo clippy -p solver-core --all-targets --features solver-trace -- -D warnings
```
Expected: pass; no warnings.

- [ ] **Step 2.5: Commit.**

```bash
git add solver/solver-core/Cargo.toml solver/solver-core/src/lib.rs solver/solver-core/src/trace.rs
git commit -m "feat(solver-core): solver-trace feature + ffd_trace formatter"
```

---

## Task 3: Wire trace call sites into `try_place_block`

**Files:**
- Modify: `solver/solver-core/src/solve.rs`

Inside `try_place_block`, immediately before each `continue` / acceptance / final-failure branch, add a `#[cfg(feature = "solver-trace")] trace::ffd_trace(...)` call. The mapping is the call-site map from the spec (L82-95). Each call passes the current lesson id, the current window's `(day, position)`, the relevant `room` (or `None`), and the canonical reason string. This task does NOT change any existing logic; the diff is additive insertions only.

- [ ] **Step 3.1: Bring the trace module into `solve.rs` scope.**

Edit `solver/solver-core/src/solve.rs`. Add after the existing `use` block at the top:

```rust
#[cfg(feature = "solver-trace")]
use crate::trace;
```

- [ ] **Step 3.2: Insert the trace calls one by one, matching the spec's mapping.**

For each row in the call-site map below, insert a `#[cfg(feature = "solver-trace")] trace::ffd_trace(lesson.id, <day>, <position>, <room>, "<reason>");` immediately before the existing branch. Variable names follow the existing function:
- `lesson.id` is in scope.
- `first_tb.day_of_week` and `first_tb.position` give the window's (day, position).
- For window-level rejections that fire BEFORE `first_tb` is bound (none in `try_place_block` — `first_tb` is bound at the top of the `'outer` loop), the day/pos are always available.
- Inside `'rooms`, use `&problem.rooms[room_idx]` for the room id.

Mapping (from the spec, L82-95):

| Existing line (current `solve.rs`) | Insert before | Args |
|---|---|---|
| `continue 'outer;` at L342 (non-contiguous) | yes | `(lesson.id, first_tb.day_of_week, first_tb.position, None, "non_contiguous_window")` |
| `continue 'outer;` at L351-353 (teacher busy or blocked) | yes, two separate calls in the two `if` branches | `"teacher_busy"` if `state.used_teacher.contains((teacher, tb.id))`; else `"teacher_blocked"`. Note: refactor the existing combined `if … || …` into two distinct `if` branches so the trace can disambiguate. The existing logic is unchanged: both branches still `continue 'outer`. |
| `continue 'outer;` at L355-358 (class busy) | yes | `(lesson.id, first_tb.day_of_week, first_tb.position, None, "class_busy")` |
| `continue;` at L362-364 (teacher over capacity) | yes | `(lesson.id, first_tb.day_of_week, first_tb.position, None, "teacher_over_capacity")` |
| `continue;` at L413-417 (score pruning) | yes | `(lesson.id, first_tb.day_of_week, first_tb.position, None, "score_pruned")` |
| `continue;` at L438-440 (locked-room conflict) | yes | `(lesson.id, first_tb.day_of_week, first_tb.position, None, "locked_room_conflict")` |
| `continue;` at L447-450 (locked-room mismatch in `'rooms`) | yes | `(lesson.id, first_tb.day_of_week, first_tb.position, Some(room.id), "locked_room_mismatch")` |
| `continue;` at L452-454 (room not suited to subject) | yes | `(lesson.id, first_tb.day_of_week, first_tb.position, Some(room.id), "room_unsuitable")` |
| `continue 'rooms;` at L457-459 (room busy / blocked) | yes, two separate calls split by `state.used_room.contains` vs `idx.room_blocked` | `"room_busy"` / `"room_blocked"`. Same refactor as the teacher case: split the combined `if … || …` into two `if` branches; both branches still `continue 'rooms`. |
| `best = Some(...)` at L468 (window+room candidate) | yes | `(lesson.id, c.day, c.start_pos, Some(room_id), "window_candidate")` (use the temporaries available at that point — `first_tb.day_of_week`, `first_tb.position`, `room_id`). |
| Terminal `Err: no candidate window` at L484-486 | yes | call `unplaced_kind` then map to one of `"unplaced_no_suitable_room"` / `"unplaced_no_free_time_block"` / `"unplaced_teacher_over_capacity"`. Pass `(lesson.id, 0, 0, None, …)` because there is no candidate window. |
| After commit at L488-524 (placed) | yes | `(lesson.id, c.day, c.start_pos, Some(c.room_id), "placed")`. |

Concretely, the insertion next to the current room-busy / room-blocked check looks like:

```rust
            for k in 0..n_usize {
                let tb = &problem.time_blocks[tb_order[outer_pos + k]];
                if state.used_room.contains(&(room.id, tb.id)) {
                    #[cfg(feature = "solver-trace")]
                    trace::ffd_trace(lesson.id, first_tb.day_of_week, first_tb.position, Some(room.id), "room_busy");
                    continue 'rooms;
                }
                if idx.room_blocked(room.id, tb.id) {
                    #[cfg(feature = "solver-trace")]
                    trace::ffd_trace(lesson.id, first_tb.day_of_week, first_tb.position, Some(room.id), "room_blocked");
                    continue 'rooms;
                }
            }
```

The same shape applies at every other call site; each insertion is two lines (one `#[cfg]` attribute + one trace call) immediately before the existing `continue` / `best = …` / terminal branch.

- [ ] **Step 3.3: Verify both feature configurations compile clean.**

Run:
```bash
cargo build -p solver-core
cargo build -p solver-core --features solver-trace
cargo clippy -p solver-core --all-targets --features solver-trace -- -D warnings
cargo clippy -p solver-core --all-targets -- -D warnings
```
All four expected: pass; zero warnings.

- [ ] **Step 3.4: Verify no existing test broke.**

Run: `cargo nextest run -p solver-core`
Expected: every existing test passes; the lock-in reproducer from Task 1 still passes.

- [ ] **Step 3.5: Spot-check the trace under the lock-in reproducer.**

Run:
```bash
cargo nextest run -p solver-core --test same_room_property -E 'test(/^ffd_locks_in/)' --features solver-trace --no-capture 2>&1 | head -100
```

(Note: nextest's flag is `--no-capture`, not `--nocapture`. Verify locally; if the executor's nextest version differs, fall back to `cargo test --features solver-trace --test same_room_property ffd_locks_in -- --nocapture`.)

Expected: many `ffd_trace seq=… lesson=… day=… pos=… room=… reason=…` lines on stderr. At least one terminal `reason=unplaced_no_suitable_room` line. Save the trace to `/tmp/ffd-trace.log` for use in Task 5:

```bash
cargo test -p solver-core --features solver-trace --test same_room_property ffd_locks_in -- --nocapture 2>/tmp/ffd-trace.log
```

- [ ] **Step 3.6: Commit.**

```bash
git add solver/solver-core/src/solve.rs
git commit -m "feat(solver-core): instrument try_place_block with solver-trace decision lines"
```

---

## Task 4: Widen `lint:rust` to also clippy `--all-features`

**Files:**
- Modify: `mise.toml`

Without this, the `solver-trace`-gated branch can rot silently between the time this PR lands and the next time someone explicitly compiles it. Cheapest fix: a second `cargo clippy` line in `lint:rust` that enables every workspace feature.

- [ ] **Step 4.1: Add the second clippy line.**

Edit `mise.toml`. Locate `[tasks."lint:rust"]` (around line 188-193) and replace the `run` block:

```toml
[tasks."lint:rust"]
run = [
  "cargo fmt --check",
  "cargo clippy --workspace --all-targets -- -D warnings",
  "cargo clippy --workspace --all-targets --all-features -- -D warnings",
  "cargo machete",
]
```

- [ ] **Step 4.2: Verify the new line passes.**

Run: `mise run lint:rust`
Expected: both clippy lines pass; machete passes.

- [ ] **Step 4.3: Commit.**

```bash
git add mise.toml
git commit -m "build(mise): clippy --all-features in lint:rust to keep solver-trace honest"
```

---

## Task 5: Capture the diagnostic note (PR-body draft)

**Files:**
- Create or update: `/tmp/ffd-trace.log` (transient, not committed)
- The PR body itself (drafted as part of step 7 of `/autopilot`)

This task does not commit code. It captures the trace, summarises the lock-in pattern, and stages four bullets for the PR body.

- [ ] **Step 5.1: Capture the trace.**

```bash
cargo test -p solver-core --features solver-trace --test same_room_property ffd_locks_in -- --nocapture 2>/tmp/ffd-trace.log
```

Expected: a few hundred `ffd_trace …` lines plus libtest's normal output.

- [ ] **Step 5.2: Find the first `placed` line and the first `unplaced_no_suitable_room` line.**

```bash
grep -n "reason=placed\|reason=unplaced_no_suitable_room" /tmp/ffd-trace.log | head -10
```

- [ ] **Step 5.3: Find the room-rejection lines that immediately precede the unplaced terminal line.**

```bash
LOSER_LINE=$(grep -n "reason=unplaced_no_suitable_room" /tmp/ffd-trace.log | head -1 | cut -d: -f1)
sed -n "$((LOSER_LINE - 30)),$LOSER_LINE p" /tmp/ffd-trace.log
```

These are the per-room rejections for the failing window. Each line tells you which room rejected with which reason (`room_busy`, `locked_room_mismatch`, `room_unsuitable`).

- [ ] **Step 5.4: Cross-reference the lesson short-id back to the test fixture.**

```bash
grep "lesson=" /tmp/ffd-trace.log | head -5
```

The first 8 hex chars of the lesson UUID identify the lesson; cross-reference against `same_room_uuid(200 + i)` to get back to the (class, subject) pair. (Helper: `python3 -c "import uuid; [print(i, uuid.UUID(bytes=bytes([200+i])*16).hex[:8]) for i in range(34)]"` lists all candidate lesson short-ids.)

- [ ] **Step 5.5: Draft the PR-body diagnostic note.**

Stage four bullets (these go into the PR body in step 7 of `/autopilot`):

1. **Locked triple.** "FFD locks `(class <X>, day <Y>, subject <S>) → room <R>` after seq=<N> (line <L> of the trace)."
2. **Why FFD chose that room.** "`BlockCandidate.score = <V>` from `solve.rs::try_place_block` L399-406 with `class_delta_w = <CD>`, `teacher_delta_w = <TD>`, `subject_pref = <SP>`. Lower-id room <R'> was rejected at seq=<N'> with reason=`<R>` (line <L'>)."
3. **Why the failing lesson can't place.** "Lesson <FAIL> at every window on day <Y> sees: <count> `locked_room_mismatch` (room <R> already locked to a sibling class), <count> `room_unsuitable` (Turnhalle / Musikraum / Kunstraum reject academic subjects), <count> `room_busy`."
4. **Smallest unblock.** "Path A (same-room-aware FFD ordering): place all hours of (class, day, subject) consecutively in `ordering::ffd_order` so the lock fires only after all hours of one class's day-subject group are placed. Estimated change: ~30 lines in `ordering.rs::ffd_order` plus a property test."

These are EVIDENCE-DRIVEN; the executor MUST replace `<X>`, `<Y>`, `<R>`, `<V>`, etc. with the actual values from the trace before posting the PR body.

---

## Task 6: Mark OPEN_THINGS items 1 + 2 as shipped + capture session learnings

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`
- Possibly modify: `solver/CLAUDE.md`, `.claude/CLAUDE.md`, `.claude/settings.json`

This is the docs commit that lands at the end of the run, after the diagnostic note exists. The `claude-md-management:revise-claude-md`, `claude-md-management:claude-md-improver`, and `fewer-permission-prompts` skills (per `/autopilot` step 6) own this task; the per-CLAUDE.md edits they propose land here.

- [ ] **Step 6.1: Update OPEN_THINGS to reflect shipped items 1 + 2.**

Edit `docs/superpowers/OPEN_THINGS.md`. Inside the active sprint's "Diagnostic phase" section, mark items 1 and 2 as ✅ shipped with the PR number filled in once it's open.

- [ ] **Step 6.2: Run the three skills (per /autopilot step 6).**

Skill order (each via the `Skill` tool, sequentially):
1. `claude-md-management:revise-claude-md`
2. `claude-md-management:claude-md-improver`
3. `fewer-permission-prompts`

Apply every proposed edit on this branch in autonomous mode (no user-pause).

- [ ] **Step 6.3: Commit the docs/settings/CLAUDE.md edits.**

```bash
git add -A docs/superpowers/OPEN_THINGS.md .claude/ solver/CLAUDE.md backend/CLAUDE.md frontend/CLAUDE.md
git commit -m "docs: mark FFD lock-in diagnostic items 1+2 shipped + session learnings"
```

If the diff is purely settings, use `chore(settings): tighten Claude Code allowlist`. Match the type to what landed.

---

## Self-Review

**Spec coverage check.** Walked the spec's "Scope" section. Items: new test (Task 1), `solver-trace` feature (Task 2), `solver/solver-core/src/trace.rs` (Task 2), trace call sites in `solve.rs` (Task 3), `mise run lint` covers `--all-features` (Task 4), diagnostic note (Task 5), OPEN_THINGS update (Task 6). All present.

**Placeholder scan.** No "TBD" / "TODO" / "implement later". Step 1.5 is conditional and explicit about the cheap knobs to try; not a placeholder, an iteration loop with stop criterion.

**Type consistency check.** The trace fn signature `(LessonId, u8, u8, Option<RoomId>, &'static str)` is consistent across Task 2 (definition) and Task 3 (every call site). Reason strings are consistent across the spec's L82-95 mapping and Task 3's call-site map.

**Spec departure tracker (none expected for this PR).** The spec said "the existing `mise run lint:rust` already runs `--all-features`"; verification (this plan-writing step) confirmed it does NOT. Plan compensates with Task 4 (added during plan-writing). Spec stays as-is; the plan's Task 4 is the source of truth.
