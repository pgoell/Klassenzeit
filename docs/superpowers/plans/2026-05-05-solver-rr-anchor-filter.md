# Solver R&R anchor-filter implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `lahc_rr` and `lahc_rr_kempe` from silently dropping placements when FFD packs multiple `N=1` blocks of the same lesson onto one day. Single behavioural commit + property tests + targeted integration test, atomic per OPEN_THINGS item 27.

**Architecture:** Port the existing Kempe inline filter at `solver/solver-core/src/lahc.rs:1367-1386` into the producer `rr_collect_anchors` (`lahc.rs:689`). Every consumer of the anchor list inherits the invariant; the Kempe re-filter becomes redundant and is deleted. Test surface: two new property tests in `tests/lahc_property.rs`, one targeted integration test in `tests/rr_anchor_filter.rs` pinning the deterministic minimal repro.

**Tech Stack:** Rust 2021, `solver-core` (pure library, `#![deny(missing_docs)]`), `proptest = 1` for property tests, `cargo nextest` for the runner. No new dependencies, no PyO3 binding change, no Python.

---

## File structure

- **Modify:** `solver/solver-core/src/lahc.rs`
  - `rr_collect_anchors` (line 689): add the filter step.
  - `kempe_attempt` (line 1367): drop the now-redundant inline `.filter`; keep the comment block (rewritten to point at `rr_collect_anchors`).
- **Modify:** `solver/solver-core/tests/lahc_property.rs`
  - Add `lahc_rr_kempe_cfg(seed)` helper (mirror of existing `lahc_rr_cfg`, `lahc_kempe_cfg`).
  - Add property `lahc_rr_never_decreases_placement_count` and `lahc_rr_kempe_never_decreases_placement_count` inside the existing `proptest!{}` block.
- **Create:** `solver/solver-core/tests/rr_anchor_filter.rs`
  - One module containing the minimal-repro fixture builder, two integration tests (`rr_does_not_drop_packed_block`, `kempe_does_not_drop_packed_block`).
- **Modify:** `docs/superpowers/OPEN_THINGS.md`
  - Delete items 26 and 27 from the active sprint's algorithm phase. Update the sprint preamble's "Next pickup" line to point at item 28.

The fix and the tests ride in one commit per spec section "Acceptance criteria"; OPEN_THINGS update is a second commit (`docs:`) so the algorithm change is reviewable in isolation.

---

## Task 1: Targeted integration test (red)

**Files:**
- Create: `solver/solver-core/tests/rr_anchor_filter.rs`

- [ ] **Step 1: Survey the existing test scaffolding**

Read `solver/solver-core/tests/lahc_property.rs` lines 1-100 to confirm the `Problem`-builder shape (what fields, what id-from-int helper). The new file uses raw `Problem { ... }` construction; it does NOT reuse `prop_compose!`-style generators.

Read `solver/solver-core/src/types.rs` to enumerate the current `Problem` field list (the spec's "fixture cascade" note in `solver/CLAUDE.md` applies to any new `Problem` literal: every field must be set explicitly because Rust struct literals are exhaustive).

- [ ] **Step 2: Write the failing test file**

Create `solver/solver-core/tests/rr_anchor_filter.rs`:

```rust
//! Regression test for the FFD-packs-two-N=1-blocks-on-one-day pattern.
//!
//! Pre-fix `rr_collect_anchors` emitted one anchor per (lesson, day) without
//! checking that the day held only one block of the lesson. When FFD packed
//! both `hours_per_week=2, preferred_block_size=1` rows of a lesson on the
//! same day (because room or teacher availability forced it), an R&R move
//! ruined both rows but only recreated one, silently dropping the other.
//! The drop was invisible because LAHC's acceptance gate only rejects on
//! `failed_recreates > 0`, and the soft score actually improves when
//! placements vanish.
//!
//! This test pins the minimal repro: one class, one teacher, one room, one
//! subject, one lesson with `hours_per_week=2, preferred_block_size=1`,
//! and a room blocked-time-blocks set that forces FFD to put both hours on
//! day 0. The post-fix invariant is that `lahc_rr` and `lahc_rr_kempe`
//! return as many placements as greedy.

use std::time::Duration;

use solver_core::ids::{
    LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId,
};
use solver_core::solve_with_config;
use solver_core::types::{
    ConstraintWeights, Lesson, Problem, Room, SchoolClass, SolveConfig, Subject, Teacher,
    TeacherQualification, TimeBlock,
};
use uuid::Uuid;

fn anchor_filter_id(n: u32) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[12..16].copy_from_slice(&n.to_be_bytes());
    Uuid::from_bytes(bytes)
}

/// Minimal `Problem` where FFD must pack both hours of one lesson onto day 0.
///
/// 5 days, 2 positions per day, 10 time blocks total. The room blocks 8 of
/// the 10; only day-0 positions 0 and 1 are free. The lesson has
/// `hours_per_week=2, preferred_block_size=1`, so FFD places one row at day-0
/// position 0 and one at day-0 position 1. Both anchors share `(lesson, day)
/// = (lesson_a, 0)`, which the pre-fix `rr_collect_anchors` deduped to a
/// single anchor whose ruin removed both placements but whose recreate only
/// restored one.
fn build_anchor_filter_fixture() -> Problem {
    let class_a = SchoolClassId(anchor_filter_id(1));
    let teacher_a = TeacherId(anchor_filter_id(2));
    let room_a = RoomId(anchor_filter_id(3));
    let subject_a = SubjectId(anchor_filter_id(4));
    let lesson_a = LessonId(anchor_filter_id(5));

    let mut time_blocks: Vec<TimeBlock> = Vec::with_capacity(10);
    let mut tb_idx = 0u32;
    for d in 0u8..5 {
        for p in 0u8..2 {
            time_blocks.push(TimeBlock {
                id: TimeBlockId(anchor_filter_id(100 + tb_idx)),
                day_of_week: d,
                position: p,
            });
            tb_idx += 1;
        }
    }

    // Block all rooms on every day except day 0. tb indexes 0,1 are day 0;
    // 2..10 are days 1..4 and must be blocked for the room.
    let blocked_for_room: Vec<TimeBlockId> = time_blocks
        .iter()
        .filter(|tb| tb.day_of_week != 0)
        .map(|tb| tb.id)
        .collect();

    Problem {
        school_classes: vec![SchoolClass {
            id: class_a,
            home_room_id: None,
        }],
        teachers: vec![Teacher {
            id: teacher_a,
            max_hours_per_week: 40,
        }],
        rooms: vec![Room { id: room_a }],
        subjects: vec![Subject {
            id: subject_a,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
        }],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: teacher_a,
            subject_id: subject_a,
        }],
        lessons: vec![Lesson {
            id: lesson_a,
            subject_id: subject_a,
            teacher_id: teacher_a,
            school_class_ids: vec![class_a],
            hours_per_week: 2,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        time_blocks,
        room_subject_suitabilities: vec![],
        teacher_blocked_time_blocks: vec![],
        room_blocked_time_blocks: blocked_for_room
            .into_iter()
            .map(|tb_id| (room_a, tb_id))
            .collect(),
        class_blocked_time_blocks: vec![],
        pinned_placements: vec![],
        lesson_groups: vec![],
    }
}

fn anchor_filter_weights() -> ConstraintWeights {
    ConstraintWeights {
        class_gap: 1,
        teacher_gap: 1,
        ..ConstraintWeights::default()
    }
}

#[test]
fn rr_does_not_drop_packed_block() {
    let problem = build_anchor_filter_fixture();

    let greedy = solve_with_config(
        &problem,
        &SolveConfig {
            weights: anchor_filter_weights(),
            ..SolveConfig::default()
        },
    )
    .unwrap();
    assert_eq!(
        greedy.placements.len(),
        2,
        "greedy must place both hours on day 0",
    );

    let lahc_rr = solve_with_config(
        &problem,
        &SolveConfig {
            weights: anchor_filter_weights(),
            seed: 7,
            deadline: Some(Duration::from_millis(50)),
            max_iterations: Some(50),
            lahc_rr_period: Some(1),
            lahc_kempe_period: None,
        },
    )
    .unwrap();
    assert_eq!(
        lahc_rr.placements.len(),
        greedy.placements.len(),
        "lahc_rr must not drop placements; got {} placements vs greedy {}",
        lahc_rr.placements.len(),
        greedy.placements.len(),
    );
}

#[test]
fn kempe_does_not_drop_packed_block() {
    let problem = build_anchor_filter_fixture();

    let greedy = solve_with_config(
        &problem,
        &SolveConfig {
            weights: anchor_filter_weights(),
            ..SolveConfig::default()
        },
    )
    .unwrap();

    let lahc_kempe = solve_with_config(
        &problem,
        &SolveConfig {
            weights: anchor_filter_weights(),
            seed: 7,
            deadline: Some(Duration::from_millis(50)),
            max_iterations: Some(50),
            lahc_rr_period: None,
            lahc_kempe_period: Some(1),
        },
    )
    .unwrap();
    assert_eq!(
        lahc_kempe.placements.len(),
        greedy.placements.len(),
        "lahc_kempe must not drop placements; got {} placements vs greedy {}",
        lahc_kempe.placements.len(),
        greedy.placements.len(),
    );
}
```

Notes for the engineer:

- The `Problem { ... }` literal is exhaustive. `cargo build` will list any field you missed; add `<field>: vec![]` (or whatever empty/default makes sense) until it compiles. The list above matches the field set as of master `5d4183a..`; if `Problem` gained a field since, add it.
- `SolveConfig::default()` already sets `lahc_rr_period: None` and `lahc_kempe_period: None`; the test sets them explicitly for legibility and so the test is robust against a future default change.
- `max_iterations: Some(50)` keeps the test deterministic on slow hosts (CLAUDE.md: "`SolveConfig.max_iterations` is a test-only field").
- Both ids are 16-byte UUIDs; `anchor_filter_id(n)` mirrors `lahc_id_from(n)` from `lahc_property.rs` but uses a unique name to satisfy the global-unique-fn-name rule (CLAUDE.md root: `scripts/check_unique_fns.py` walks all Rust integration tests under `tests/`).

- [ ] **Step 3: Run the test on master to confirm red**

```bash
cargo nextest run -p solver-core --test rr_anchor_filter
```

Expected: `rr_does_not_drop_packed_block` FAILS with `lahc_rr must not drop placements; got 1 placements vs greedy 2`. `kempe_does_not_drop_packed_block` may PASS already (Kempe's existing inline filter at `lahc.rs:1367-1386` already covers this on master). If it passes, that confirms the inline filter is correct; the test serves as a regression guard once the filter moves to `rr_collect_anchors`.

If `rr_does_not_drop_packed_block` PASSES on master, the fixture does not exercise the buggy path. Likely cause: FFD did not actually pack both hours on day 0 (e.g., the room block list was wrong). Inspect `greedy.placements` and adjust the fixture until FFD packs both hours on day 0. Do NOT proceed to step 4 until the test fails on master with the expected message.

- [ ] **Step 4: Do not commit yet**

The targeted integration test must land in the same commit as the filter port (per spec acceptance criteria; spec quote: "Single commit `fix(solver-core): port Kempe anchor filter to rr_collect_anchors`"). Hold the file uncommitted; later tasks add the property tests and the filter, and the final task commits everything together.

---

## Task 2: Property tests (red)

**Files:**
- Modify: `solver/solver-core/tests/lahc_property.rs`

- [ ] **Step 1: Add `lahc_rr_kempe_cfg` helper**

Modify `solver/solver-core/tests/lahc_property.rs` between the existing `lahc_kempe_cfg` (line 33-41) and `lahc_id_from` (line 43). Insert:

```rust
fn lahc_rr_kempe_cfg(seed: u64) -> SolveConfig {
    SolveConfig {
        weights: lahc_weights(),
        seed,
        deadline: Some(Duration::from_millis(20)),
        lahc_rr_period: Some(5),
        lahc_kempe_period: Some(5),
        ..SolveConfig::default()
    }
}
```

- [ ] **Step 2: Add the two property tests**

Inside the existing `proptest!{}` block in `solver/solver-core/tests/lahc_property.rs`, after the existing `lahc_rr_never_increases_hard_violations` test (line 218-225), insert:

```rust
#[test]
fn lahc_rr_never_decreases_placement_count(p in lahc_small_problem()) {
    let greedy = solve_with_config(&p, &SolveConfig {
        weights: lahc_weights(),
        ..SolveConfig::default()
    }).unwrap();
    let lahc_rr = solve_with_config(&p, &lahc_rr_cfg(7)).unwrap();
    prop_assert!(
        lahc_rr.placements.len() >= greedy.placements.len(),
        "lahc_rr dropped placements: {} < greedy {}",
        lahc_rr.placements.len(),
        greedy.placements.len(),
    );
}

#[test]
fn lahc_rr_kempe_never_decreases_placement_count(p in lahc_small_problem()) {
    let greedy = solve_with_config(&p, &SolveConfig {
        weights: lahc_weights(),
        ..SolveConfig::default()
    }).unwrap();
    let lahc_rr_kempe = solve_with_config(&p, &lahc_rr_kempe_cfg(7)).unwrap();
    prop_assert!(
        lahc_rr_kempe.placements.len() >= greedy.placements.len(),
        "lahc_rr_kempe dropped placements: {} < greedy {}",
        lahc_rr_kempe.placements.len(),
        greedy.placements.len(),
    );
}
```

- [ ] **Step 3: Run the property tests on master to confirm red (or document why they pass)**

```bash
cargo nextest run -p solver-core --test lahc_property -- never_decreases_placement_count
```

Expected outcome: at least one of the two properties FAILS with proptest's "found a minimal failing case" output. If both pass on master, the `lahc_small_problem()` generator does not produce problems where FFD packs multiple N=1 blocks on one day. Two paths:

1. Look at the generator (`tests/lahc_property.rs:50-145`). It produces 1-3 classes, 1-4 teachers, 1-3 rooms, 1-3 days, 2-5 slots/day, with each class/subject pair becoming one lesson with `hours_per_week=4, preferred_block_size=1` (the body builds 4 hours per lesson). With as few as 2 days × 2 slots and 4 hours per lesson, FFD MUST pack at least two hours on one day. Property should fail on at least one case under `cargo test --release` if the wall-clock deadline lets even one R&R move land.
2. If, despite the above, the property still passes, it's because the 20 ms deadline doesn't let R&R execute on the small fixture sizes. In that case, run with a longer deadline locally to confirm the buggy code is breakable: temporarily change `lahc_rr_cfg` to `Duration::from_millis(200)` and re-run. If it then fails, the property is correct; the original 20 ms deadline is the right CI-friendly trade-off and the longer deadline is for local verification only. Revert the deadline before committing.

If the properties both pass even at 200 ms on master, STOP and ask the user. The fixture is not exercising the buggy path; the spec assumes it does.

- [ ] **Step 4: Do not commit yet**

Same as Task 1 step 4: hold the changes; the filter port commits them all together.

---

## Task 3: Filter port (green)

**Files:**
- Modify: `solver/solver-core/src/lahc.rs:678-718` (`rr_collect_anchors` doc comment + body)
- Modify: `solver/solver-core/src/lahc.rs:1361-1386` (`kempe_attempt` inline filter removal)

- [ ] **Step 1: Re-read the current `rr_collect_anchors` body**

```bash
sed -n '678,718p' solver/solver-core/src/lahc.rs
```

Confirm the function shape matches the spec sketch. If `solver-core` has rebased onto a newer master that changed the function, adjust the diff accordingly; the invariant ("filter `(lesson, day)` where the day's placement count for that lesson exceeds `preferred_block_size`") is the contract, the line numbers are guidance.

- [ ] **Step 2: Apply the filter to `rr_collect_anchors`**

Replace the body of `rr_collect_anchors` with a single-pass shape that counts and filters. Update the doc comment to spell out the invariant.

```rust
/// Collect the set of `(lesson, day)` blocks eligible to be ruined by an R&R
/// or Kempe attempt. Returns one tuple per block for lessons that are
/// neither pinned nor part of a lesson group, and only when the day holds
/// exactly one block of the lesson (`count(placements_on_day) <=
/// preferred_block_size`). The single-anchor-per-block contract lets the
/// recreate step call `try_place_block` once per chosen anchor without
/// silently dropping placements when FFD packed multiple `N=1` rows of the
/// same lesson on one day for compactness. Returned in a deterministic
/// order so the R&R / Kempe RNG shuffle reproduces under a fixed seed.
///
/// Tuples (not placement indices) because a single ruin removes every
/// placement of a lesson on its day, which can shift indices both above and
/// below other anchors when a lesson has multiple non-contiguous block
/// placements on the same day. Callers look up the current placement index
/// at ruin time from this tuple.
fn rr_collect_anchors(
    placements: &[Placement],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    pinned: &HashSet<LessonId>,
) -> Vec<(LessonId, u8)> {
    // First pass: count placements per (lesson, day) for non-pinned,
    // non-group lessons. The count drives the per-anchor filter below.
    let mut counts: HashMap<(LessonId, u8), u32> = HashMap::new();
    for p in placements.iter() {
        let Some(lesson) = lesson_lookup.get(&p.lesson_id) else {
            continue;
        };
        if pinned.contains(&p.lesson_id) {
            continue;
        }
        if lesson.lesson_group_id.is_some() {
            continue;
        }
        let Some(tb) = tb_lookup.get(&p.time_block_id) else {
            continue;
        };
        *counts.entry((p.lesson_id, tb.day_of_week)).or_insert(0) += 1;
    }

    // Second pass: keep anchors whose day-count fits in one block. FFD can
    // pack multiple N=1 blocks of the same lesson on one day for
    // compactness; those anchors are excluded so a ruin can't silently
    // drop the second block.
    let mut anchors: Vec<(LessonId, u8)> = counts
        .into_iter()
        .filter_map(|((lesson_id, day), count)| {
            let lesson = lesson_lookup.get(&lesson_id)?;
            if count <= u32::from(lesson.preferred_block_size) {
                Some((lesson_id, day))
            } else {
                None
            }
        })
        .collect();
    // Deterministic order before the R&R / Kempe RNG shuffles.
    anchors.sort_unstable_by(|a, b| a.0 .0.cmp(&b.0 .0).then(a.1.cmp(&b.1)));
    anchors
}
```

Notes:

- `u32` for the count keeps the `lesson.preferred_block_size` (`u8` per `solver-core/src/types.rs`) comparison via `u32::from(...)`. Per-day placement counts are bounded by the number of time blocks per day (small `u8` in practice); `u32` is overkill but cheap and avoids the saturating-add dance.
- The doc comment now mentions "or Kempe attempt" to reflect that Kempe also calls this function.
- The two-pass shape preserves determinism: the final `sort_unstable_by` sorts by `LessonId.0` then `day`, exactly the same order the previous one-pass shape produced.

- [ ] **Step 3: Remove the redundant filter in `kempe_attempt`**

Modify `solver/solver-core/src/lahc.rs:1361-1389`. Replace:

```rust
    // Seed pick: collect block-anchors (R&R eligibility rules; identical for
    // Kempe). Filter out anchors where the lesson has more than one block
    // on the chosen day; FFD can pack two N=1 blocks of the same lesson
    // onto one day for compactness, which would make the swap drop hours.
    // Empty means there is nothing eligible to swap.
    let raw_anchors = rr_collect_anchors(placements, lesson_lookup, tb_lookup, pinned);
    let anchors: Vec<(LessonId, u8)> = raw_anchors
        .into_iter()
        .filter(|(lesson_id, day)| {
            let lesson = match lesson_lookup.get(lesson_id) {
                Some(l) => *l,
                None => return false,
            };
            let hours_on_day = placements
                .iter()
                .filter(|p| {
                    p.lesson_id == *lesson_id
                        && tb_lookup
                            .get(&p.time_block_id)
                            .is_some_and(|tb| tb.day_of_week == *day)
                })
                .count();
            hours_on_day == usize::from(lesson.preferred_block_size)
        })
        .collect();
    if anchors.is_empty() {
        return false;
    }
```

With:

```rust
    // Seed pick: rr_collect_anchors filters (lesson, day) where FFD packed
    // multiple N=1 blocks of the same lesson on one day. See its doc
    // comment for the single-anchor-per-block invariant.
    let anchors = rr_collect_anchors(placements, lesson_lookup, tb_lookup, pinned);
    if anchors.is_empty() {
        return false;
    }
```

- [ ] **Step 4: Run the targeted integration test**

```bash
cargo nextest run -p solver-core --test rr_anchor_filter
```

Expected: both tests PASS.

- [ ] **Step 5: Run the property tests**

```bash
cargo nextest run -p solver-core --test lahc_property
```

Expected: all 8+ properties PASS (the 6 existing + the 2 new). If any of the existing properties fail, the filter changed determinism or RNG draw count; STOP and re-read the spec section "Determinism and existing properties".

- [ ] **Step 6: Run the full solver-core suite**

```bash
mise run test:rust
```

Expected: all tests pass workspace-wide. Watch in particular for `tests/grundschule_smoke.rs` (the integration test that exercises the production path) and `tests/lahc_rr_property.rs` if any other R&R-specific properties exist.

- [ ] **Step 7: Lint**

```bash
mise run lint
```

Expected: pass. Watch for `clippy::doc_lazy_continuation` on the rewritten doc comment (the `+` inside `/// ...` was a known footgun per `solver/CLAUDE.md`); the doc comment above uses dashes / parentheses only.

- [ ] **Step 8: Bench delta check (optional, fast loop)**

```bash
mise run bench
```

Expected: criterion shows ±5 % delta on each fixture. The new `rr_collect_anchors` adds one HashMap pass per `rr_attempt` / `kempe_attempt`; on `zweizuegig` (196 placements) the cost is sub-microsecond per attempt, well below the 20 % regression budget. If a fixture breaches 20 %, switch to a single-pass shape that builds both `counts` and `anchors` in one walk, deferring the filter decision until the second-pass projection. The two-pass shape is the spec's preferred default for legibility; the single-pass fallback is implementation-only.

This step is informational, not blocking: criterion is host-sensitive and the regression budget is for intentional algorithm changes; this PR is a bug fix with no expected performance characteristic change. Note any delta in the PR body for the reviewer.

- [ ] **Step 9: Single atomic commit**

```bash
git add solver/solver-core/src/lahc.rs solver/solver-core/tests/lahc_property.rs solver/solver-core/tests/rr_anchor_filter.rs
git commit -m "fix(solver-core): port Kempe anchor filter to rr_collect_anchors"
```

Commit body (use a HEREDOC):

```bash
git commit -m "$(cat <<'EOF'
fix(solver-core): port Kempe anchor filter to rr_collect_anchors

R&R silently dropped placements when FFD packed multiple N=1 blocks
of the same lesson on one day. rr_collect_anchors emitted one tuple
per (lesson, day); rr_ruin_block removed every same-lesson-same-day
placement; rr_attempt's recreate called try_place_block once per
anchor, restoring one block and dropping the rest. The drop was
invisible because LAHC's acceptance gate only rejects on
failed_recreates > 0 and the soft score actually improves when
placements vanish.

The Kempe move already filtered this case at lahc.rs:1367-1386. Port
the filter into the producer (rr_collect_anchors) so every consumer
inherits the invariant; remove Kempe's now-redundant re-filter.

Property tests in tests/lahc_property.rs guard against future
regressions; tests/rr_anchor_filter.rs pins the deterministic
minimal repro. OPEN_THINGS items 26 and 27.
EOF
)"
```

---

## Task 4: OPEN_THINGS update

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Delete items 26 and 27**

`docs/superpowers/OPEN_THINGS.md` carries the active sprint preamble plus the algorithm-phase items 26 and 27. Both ship in the commit above, so they leave OPEN_THINGS entirely (per the autopilot rule: "When an item ships, DELETE it from OPEN_THINGS entirely. Do not leave a `✅ Shipped <date>` line behind").

After deletion, the algorithm-phase section is empty. The active sprint preamble's "Next pickup" line still reads "P0 item 26"; update it to "P0 item 28 (placement-count validation in the bake-off)" so the next session starts from the correct truth.

If the algorithm-phase heading would be empty after deletion, delete the heading too. The next phase ("Bench prevention phase") becomes the new top of the sprint's task list.

- [ ] **Step 2: Lint and commit**

```bash
mise run lint
git add docs/superpowers/OPEN_THINGS.md
git commit -m "docs: close OPEN_THINGS items 26 and 27"
```

The OPEN_THINGS edit also includes whatever uncommitted edits already live in the working tree from the prior session (the rewrite that opened the active sprint). The squash-merge is one commit either way; including the prior rewrite in this commit keeps the docs clean per the autopilot rule "All edits land on the feature branch in this run".

---

## Task 5: Manual verification narrative for the PR body

**Files:** none (this is PR-body content, not a code task)

- [ ] **Step 1: Capture the manual verification numbers**

The dev-DB zweizügig delta is the user-data receipt the spec relies on. Record in the PR body:

> Manual verification: with the fix landed, run the dev-DB zweizügig schedule end-to-end through the production solver path (`POST /api/schedule/generate`). Expected: `lahc_rr_kempe` returns 196/196 placements, matching the greedy 191/196 baseline floor (item 26 acceptance). Pre-fix, the same run returned 68/196.

This is a manual repro step for the user to run after the PR merges; the targeted integration test from Task 1 is the in-repo guard. Item 32 promotes this manual verification into an automated Python test.

- [ ] **Step 2: Note the next pickup**

The PR body queues item 28 as the next pickup so the bench-harness fix is visible without OPEN_THINGS-archeology. Quote: "Next pickup: item 28 (`placements_total >= placements_expected` validation in the bake-off harness). The bench refresh that produces ADR 0032 lands once item 28's harness change is in."

---

## Self-review

Spec coverage:
- "Port the Kempe anchor filter into `rr_collect_anchors`" — Task 3 step 2.
- "Remove the now-redundant re-filter in `kempe_attempt`" — Task 3 step 3.
- "Update the doc comment on `rr_collect_anchors`" — Task 3 step 2 (rewritten doc comment).
- "Add two property tests to `solver/solver-core/tests/lahc_property.rs`" — Task 2.
- "Add one targeted integration test in `solver/solver-core/tests/`" — Task 1.
- "Update `docs/superpowers/OPEN_THINGS.md`: delete items 26 and 27" — Task 4.
- Acceptance: existing tests, two new property tests, targeted integration test, lint, pre-push — all covered in Task 3 steps 4-7.

Placeholder scan: no TBDs, no "implement later"; every code block is complete, every command is exact.

Type consistency: `lahc_rr_kempe_cfg` defined in Task 2 step 1 is consumed in Task 2 step 2; `build_anchor_filter_fixture` defined in Task 1 step 2 is consumed by both tests in the same file; `anchor_filter_id` and `anchor_filter_weights` likewise.

Plan complete and saved to `docs/superpowers/plans/2026-05-05-solver-rr-anchor-filter.md`.
