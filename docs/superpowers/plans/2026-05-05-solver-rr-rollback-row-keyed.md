# Solver R&R row-keyed rollback implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the silent placement-drop in `lahc_rr` and `lahc_rr_kempe` on grundschule by porting Kempe's row-keyed rollback pattern into `rr_attempt`, plus widen the property-test generator and re-run the dev-loop bake-off.

**Architecture:** R&R captures the exact `Vec<Placement>` rows added per successful recreate. Rollback removes those exact rows by `(lesson_id, time_block_id, room_id)` match (the Kempe pattern documented in `solver/CLAUDE.md`). A defensive guard treats "recreate landed on a day where the lesson already had a different block" as a soft failure that triggers full rollback. Property tests widen `hours_per_week` so multi-block-across-days lessons are exercised.

**Tech Stack:** Rust 2021 (`solver-core`), proptest, `cargo nextest`, `mise run` task runner.

---

## File map

- Modify: `solver/solver-core/src/lahc.rs` — `rr_attempt` (line 738), `rr_rollback` (line 854); add a private helper `rr_remove_row_bookkeeping` mirroring `rr_ruin_block`'s inner per-row decrement.
- Create: `solver/solver-core/tests/rr_rollback.rs` — targeted regression test asserting placement count is preserved on a multi-block-across-days problem.
- Modify: `solver/solver-core/tests/lahc_property.rs` — widen `lahc_small_problem`'s `hours_per_week` range; bump `lahc_rr_cfg`'s deadline from 20 ms to 50 ms.
- Modify: `solver/CLAUDE.md` — generalise the "Ruin+apply rollback shape" bullet so it names both R&R and Kempe.
- Modify: `docs/superpowers/OPEN_THINGS.md` — delete item 37, promote item 29 to next pickup.

## Task 1: Pin the bug with a failing targeted regression test

**Files:**
- Create: `solver/solver-core/tests/rr_rollback.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Regression test for the R&R rollback row-keyed fix (active sprint item 37).
//!
//! Pre-fix, `rr_rollback` used `placements.iter().position(|p| p.lesson_id == lesson_id)`
//! to locate a recreated lesson's row, then ruined the WHOLE day at that
//! placement. For a lesson with multiple blocks across different days, the
//! "first" placement was usually one of the lesson's untouched original blocks,
//! and the rollback dropped that block instead of undoing the recreate. The
//! grundschule fixture surfaces this bug visibly (`lahc_rr` drops to 19/45 at
//! 5 s budget). This test pins the rule that R&R can never reduce placement
//! count below FFD greedy on a hand-built multi-block-across-days problem.

use std::time::Duration;

use solver_core::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use solver_core::types::{
    ConstraintWeights, Lesson, Problem, Room, SchoolClass, SolveConfig, Subject, Teacher,
    TeacherQualification, TimeBlock,
};
use solver_core::solve_with_config;
use uuid::Uuid;

fn rr_rollback_uuid(n: u32) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[12..16].copy_from_slice(&n.to_be_bytes());
    Uuid::from_bytes(bytes)
}

fn rr_rollback_problem() -> Problem {
    // 5 days × 5 positions = 25 time blocks. One class, one teacher, three
    // rooms (so the lesson is room-feasible across days). Lesson L has
    // hours_per_week=5, preferred_block_size=1, so FFD places one row per
    // weekday. Three filler lessons with hours_per_week=1 keep R&R's
    // shuffle non-trivial without crowding the schedule.
    let subject = SubjectId(rr_rollback_uuid(1));
    let teacher = TeacherId(rr_rollback_uuid(1000));
    let class = SchoolClassId(rr_rollback_uuid(2000));
    let room_a = RoomId(rr_rollback_uuid(3000));
    let room_b = RoomId(rr_rollback_uuid(3001));
    let room_c = RoomId(rr_rollback_uuid(3002));
    let mut time_blocks: Vec<TimeBlock> = Vec::with_capacity(25);
    let mut tb_idx: u32 = 0;
    for d in 0..5u8 {
        for p in 0..5u8 {
            time_blocks.push(TimeBlock {
                id: TimeBlockId(rr_rollback_uuid(4000 + tb_idx)),
                day_of_week: d,
                position: p,
            });
            tb_idx += 1;
        }
    }
    let lesson_multi = LessonId(rr_rollback_uuid(5000));
    let filler_a = LessonId(rr_rollback_uuid(5001));
    let filler_b = LessonId(rr_rollback_uuid(5002));
    let filler_c = LessonId(rr_rollback_uuid(5003));
    Problem {
        time_blocks,
        teachers: vec![Teacher {
            id: teacher,
            max_hours_per_week: 28,
        }],
        rooms: vec![
            Room { id: room_a },
            Room { id: room_b },
            Room { id: room_c },
        ],
        subjects: vec![Subject {
            id: subject,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
        }],
        school_classes: vec![SchoolClass {
            id: class,
            home_room_id: None,
        }],
        lessons: vec![
            Lesson {
                id: lesson_multi,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 5,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: filler_a,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: filler_b,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: filler_c,
                school_class_ids: vec![class],
                subject_id: subject,
                teacher_id: teacher,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
        ],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id: teacher,
            subject_id: subject,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

fn rr_rollback_weights() -> ConstraintWeights {
    ConstraintWeights {
        class_gap: 1,
        teacher_gap: 1,
        ..ConstraintWeights::default()
    }
}

#[test]
fn lahc_rr_preserves_placements_on_multi_block_across_days() {
    let problem = rr_rollback_problem();
    let greedy = solve_with_config(
        &problem,
        &SolveConfig {
            weights: rr_rollback_weights(),
            ..SolveConfig::default()
        },
    )
    .expect("greedy solve");
    for seed in 1u64..=8 {
        let lahc_rr = solve_with_config(
            &problem,
            &SolveConfig {
                weights: rr_rollback_weights(),
                seed,
                deadline: Some(Duration::from_millis(50)),
                lahc_rr_period: Some(5),
                ..SolveConfig::default()
            },
        )
        .expect("lahc_rr solve");
        assert_eq!(
            lahc_rr.placements.len(),
            greedy.placements.len(),
            "seed {seed}: lahc_rr dropped placements ({} < greedy {})",
            lahc_rr.placements.len(),
            greedy.placements.len(),
        );
    }
}
```

- [ ] **Step 2: Run the test against the unfixed solver**

Run: `cargo nextest run -p solver-core --test rr_rollback lahc_rr_preserves_placements_on_multi_block_across_days`
Expected: FAIL on at least one seed (`lahc_rr dropped placements: <N> < greedy 8`).

Capture the failure output for the PR body's "before" half.

- [ ] **Step 3: Commit the failing test**

```bash
git add solver/solver-core/tests/rr_rollback.rs
git commit -m "test(solver-core): pin r&r row-keyed rollback regression"
```

## Task 2: Implement the row-keyed rollback fix

**Files:**
- Modify: `solver/solver-core/src/lahc.rs:738-848` (`rr_attempt`)
- Modify: `solver/solver-core/src/lahc.rs:854-878` (`rr_rollback`)

- [ ] **Step 1: Read the current `rr_attempt` and `rr_rollback`**

Run: `sed -n '738,878p' solver/solver-core/src/lahc.rs`

Confirm the function signatures and current bodies match the spec context.

- [ ] **Step 2: Replace `rr_attempt`'s recreate-tracking and rollback call sites**

Edit `rr_attempt` so the recreate loop tracks `Vec<Vec<Placement>>` and the rollback calls pass the captured rows. Replace the body from `let mut failed_recreates: usize = 0;` (line 791) through `return false;` (line 845) with:

```rust
    // Capture every placement row added per successful recreate. Rolling
    // back by exact row id avoids the multi-block-across-days bug where
    // `placements.iter().position(|p| p.lesson_id == ...)` would otherwise
    // return one of the lesson's untouched original blocks instead of the
    // recreated one and `rr_ruin_block` would drop pristine rows.
    let snapshotted_lesson_days: HashSet<(LessonId, u8)> = snapshots
        .iter()
        .filter_map(|(lesson_id, snap)| {
            let row = snap.rows.first()?;
            let day = tb_lookup.get(&row.time_block_id)?.day_of_week;
            Some((*lesson_id, day))
        })
        .collect();

    let mut failed_recreates: usize = 0;
    let mut recreated_rows: Vec<Vec<Placement>> = Vec::with_capacity(snapshots.len());
    for (lesson_id, _snap) in snapshots.iter() {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("ruined lesson must resolve");
        let n = lesson.preferred_block_size;
        let len_before = placements.len();
        let placed = crate::solve::try_place_block(
            problem,
            lesson,
            n,
            idx,
            teacher_max,
            weights,
            state,
            placements,
            tb_order,
            room_order,
            max_position_per_day,
        );
        if !placed {
            failed_recreates += 1;
            continue;
        }
        let added: Vec<Placement> = placements[len_before..].to_vec();
        // Defensive guard: if the recreate landed on a day where the same
        // lesson already had a placement that wasn't part of this iteration's
        // snapshot, the post-accept state would have two windows of the same
        // lesson on one day, which `rr_collect_anchors` would then filter out
        // forever. Treat as a recreate failure and roll back.
        let dest_day = added
            .first()
            .and_then(|p| tb_lookup.get(&p.time_block_id))
            .map(|tb| tb.day_of_week);
        let collides = match dest_day {
            Some(day) => {
                !snapshotted_lesson_days.contains(&(*lesson_id, day))
                    && placements
                        .iter()
                        .filter(|p| p.lesson_id == *lesson_id)
                        .any(|p| {
                            tb_lookup
                                .get(&p.time_block_id)
                                .is_some_and(|tb| tb.day_of_week == day)
                                && !added.iter().any(|a| a.time_block_id == p.time_block_id)
                        })
            }
            None => false,
        };
        if collides {
            failed_recreates += 1;
            recreated_rows.push(added);
            continue;
        }
        recreated_rows.push(added);
    }

    let pre_count = pre_score; // placeholder removed below; see Step 3.
    if failed_recreates > 0 {
        rr_rollback(
            &recreated_rows,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
        );
        state.soft_score = pre_score;
        return false;
    }

    let new_score = state.soft_score;
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let lahc_ok = new_score <= pre_score || new_score <= prior;
    if !lahc_ok {
        rr_rollback(
            &recreated_rows,
            &snapshots,
            lesson_lookup,
            tb_lookup,
            placements,
            state,
        );
        state.soft_score = pre_score;
        return false;
    }

    true
```

(The `pre_count` line is staged for Step 3's assert; remove it as part of the same edit if you prefer to land Step 3 atomically.)

- [ ] **Step 3: Add the placement-count asserts and snapshot the pre-attempt count**

Add `let pre_count = placements.len();` immediately after `let pre_score = state.soft_score;` near the top of `rr_attempt`. After the LAHC-accept branch (just before `true` on the success path), add:

```rust
    debug_assert_eq!(
        placements.len(),
        pre_count,
        "rr_attempt accepted but placement count drifted (pre={pre_count} post={})",
        placements.len(),
    );
```

After the `state.soft_score = pre_score;` lines on each rollback branch (failed_recreates > 0 and !lahc_ok), add:

```rust
    debug_assert_eq!(
        placements.len(),
        pre_count,
        "rr_rollback left placement count drifted (pre={pre_count} post={})",
        placements.len(),
    );
```

Remove the `let pre_count = pre_score;` placeholder line introduced in Step 2.

- [ ] **Step 4: Replace `rr_rollback`'s body**

Replace `rr_rollback` (line 854) entirely with:

```rust
/// Roll back a partial or complete R&R recreate. For each captured set of
/// recreated rows, remove only those exact `(lesson_id, time_block_id,
/// room_id)` rows (the Kempe pattern). Then for each snapshot, replay the
/// original placement rows back into `placements` + `state`. The captured-rows
/// approach avoids the multi-block-across-days hazard the older
/// `placements.iter().position(|p| p.lesson_id == ...)` lookup had.
fn rr_rollback(
    recreated_rows: &[Vec<Placement>],
    snapshots: &[(LessonId, BlockSnapshot)],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
) {
    for rows in recreated_rows.iter().rev() {
        let mut rows_to_remove: Vec<usize> = Vec::with_capacity(rows.len());
        for row in rows.iter() {
            if let Some(idx) = placements.iter().position(|p| {
                p.lesson_id == row.lesson_id
                    && p.time_block_id == row.time_block_id
                    && p.room_id == row.room_id
            }) {
                rows_to_remove.push(idx);
            }
        }
        rows_to_remove.sort_unstable();
        for &idx in rows_to_remove.iter().rev() {
            let p = placements.remove(idx);
            let lesson = lesson_lookup
                .get(&p.lesson_id)
                .expect("rolled-back placement's lesson resolves");
            rr_remove_row_bookkeeping(lesson, &p, tb_lookup, state);
        }
    }
    for (lesson_id, snapshot) in snapshots.iter().rev() {
        let lesson = lesson_lookup
            .get(lesson_id)
            .expect("snapshot lesson resolves");
        for row in snapshot.rows.iter().rev() {
            replay_placement(lesson, row, tb_lookup, placements, state);
        }
    }
}

/// Decrement the per-row bookkeeping for one removed placement: matches the
/// inner loop of `rr_ruin_block` row-by-row. Lifted into its own helper so
/// `rr_rollback` and `rr_ruin_block` share the same source of truth.
fn rr_remove_row_bookkeeping(
    lesson: &Lesson,
    row: &Placement,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    state: &mut crate::solve::GreedyState,
) {
    let tb = tb_lookup
        .get(&row.time_block_id)
        .expect("removed row's tb resolves");
    let day = tb.day_of_week;
    let position = tb.position;
    state
        .used_teacher
        .remove(&(lesson.teacher_id, row.time_block_id));
    for class in &lesson.school_class_ids {
        state.used_class.remove(&(*class, row.time_block_id));
        if let Some(part) = state.class_positions.get_mut(&(*class, day)) {
            if let Ok(j) = part.binary_search(&position) {
                part.remove(j);
            }
            if part.is_empty() {
                state.class_positions.remove(&(*class, day));
            }
        }
    }
    state.used_room.remove(&(row.room_id, row.time_block_id));
    if let Some(part) = state.teacher_positions.get_mut(&(lesson.teacher_id, day)) {
        if let Ok(j) = part.binary_search(&position) {
            part.remove(j);
        }
        if part.is_empty() {
            state.teacher_positions.remove(&(lesson.teacher_id, day));
        }
    }
    if let Some(h) = state.hours_by_teacher.get_mut(&lesson.teacher_id) {
        *h = h.saturating_sub(1);
    }
    for class in &lesson.school_class_ids {
        let key = (*class, day, lesson.subject_id);
        if let Some(entry) = state.locked_room.get_mut(&key) {
            entry.1 = entry.1.saturating_sub(1);
            if entry.1 == 0 {
                state.locked_room.remove(&key);
            }
        }
    }
}
```

- [ ] **Step 5: Replace the inner per-row block in `rr_ruin_block` to call the helper**

Inside `rr_ruin_block`'s `for &i in indices.iter().rev() { ... }` body, replace the per-row state mutation (the block from `state.used_teacher.remove(...)` through `entry.1 = entry.1.saturating_sub(1); ... }` for `locked_room`) with a single call:

```rust
        let p = placements.remove(i);
        rr_remove_row_bookkeeping(lesson, &p, tb_lookup, state);
        rows.push(p);
```

Keep the `rows.push(p);` to preserve the snapshot's contents.

- [ ] **Step 6: Run the targeted regression test**

Run: `cargo nextest run -p solver-core --test rr_rollback lahc_rr_preserves_placements_on_multi_block_across_days`
Expected: PASS on all 8 seeds.

- [ ] **Step 7: Run the full solver-core test suite**

Run: `cargo nextest run -p solver-core`
Expected: all green. The existing `lahc_rr_*` and `lahc_kempe_*` property tests stay green; `rr_anchor_filter` integration test stays green; unit tests in `lahc.rs::tests` (including `rr_attempt_does_not_panic_when_lesson_has_multiple_blocks_on_same_day` at line 2682) stay green.

- [ ] **Step 8: Run lint**

Run: `mise run lint:rust`
Expected: green. No `dead_code` warnings on `rr_remove_row_bookkeeping` (it has two callers in the same commit per the solver-CLAUDE.md "bundle helper plus first caller" rule).

- [ ] **Step 9: Commit**

```bash
git add solver/solver-core/src/lahc.rs
git commit -m "fix(solver-core): port kempe-style row-keyed rollback to rr_attempt"
```

## Task 3: Widen the property-test generator + bump deadline

**Files:**
- Modify: `solver/solver-core/tests/lahc_property.rs:60-138` (`lahc_small_problem`)
- Modify: `solver/solver-core/tests/lahc_property.rs:23-31` (`lahc_rr_cfg`)

- [ ] **Step 1: Edit `lahc_small_problem` to vary `hours_per_week`**

Replace the lessons block (lines 111-123) with:

```rust
        let lessons: Vec<Lesson> = school_classes
            .iter()
            .enumerate()
            .map(|(i, sc)| Lesson {
                id: LessonId(lahc_id_from(5000 + i as u32)),
                school_class_ids: vec![sc.id],
                subject_id: subject_a,
                teacher_id: teachers[i % teachers.len()].id,
                // Vary across 2..=4 so the property cases include lessons that
                // place across multiple days (preferred_block_size = 1). The
                // fixed value 2 hid the multi-block-across-days rollback bug
                // tracked as active sprint item 37.
                hours_per_week: 2 + ((i as u8) % 3),
                preferred_block_size: 1,
                lesson_group_id: None,
            })
            .collect();
```

- [ ] **Step 2: Bump `lahc_rr_cfg`'s deadline**

In `lahc_rr_cfg` (line 23), change `Duration::from_millis(20)` to `Duration::from_millis(50)`. Apply the same bump to `lahc_kempe_cfg` and `lahc_rr_kempe_cfg` so all three R&R-style configs reach the rollback path inside the property cases.

- [ ] **Step 3: Run the property tests**

Run: `cargo nextest run -p solver-core --test lahc_property`
Expected: all green. `lahc_rr_never_decreases_placement_count` and `lahc_rr_kempe_never_decreases_placement_count` should pass on the wider generator now that Task 2 fixed the rollback.

- [ ] **Step 4: Sanity check that the wider generator can hit the rollback path**

Run: `cargo nextest run -p solver-core --test lahc_property -- --test-threads=1` (one thread so test output is interleaved cleanly).
Confirm runtime per property test stays under ~3 seconds (32 cases × 50 ms deadline = 1.6 s upper bound; actual is lower because most cases finish before deadline).

- [ ] **Step 5: Commit**

```bash
git add solver/solver-core/tests/lahc_property.rs
git commit -m "test(solver-core): widen lahc property generator to multi-block-across-days"
```

## Task 4: Re-run the dev-loop bake-off, capture the receipt

**Files:**
- None modified (receipt goes into the PR body in Task 6).

- [ ] **Step 1: Run the bake-off at dev-loop budget on grundschule**

Run: `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule --out /tmp/bakeoff-grundschule-postfix.md`
Expected: `lahc_rr` and `lahc_rr_kempe` cells at `placements_med=45/45` with `feasibility >= 1/4` (ideally 4/4 like `lahc`).

Save the `cell done:` lines for the PR body.

- [ ] **Step 2: Sanity-check zweizügig as well**

Run: `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures zweizuegig --out /tmp/bakeoff-zweizuegig-postfix.md`
Expected: no regression vs current behaviour. `lahc_rr` and `lahc_rr_kempe` placements_med should not drop below `lahc` placements_med.

(If `zweizuegig` shows a regression, surface it as a follow-up under "Open solver follow-ups" in OPEN_THINGS rather than blocking this PR. The fixture has tighter teacher/room constraints than grundschule and may need a separate investigation.)

## Task 5: Update CLAUDE.md and OPEN_THINGS.md

**Files:**
- Modify: `solver/CLAUDE.md` (the "Ruin+apply rollback shape" bullet)
- Modify: `docs/superpowers/OPEN_THINGS.md` (delete item 37, promote item 29)

- [ ] **Step 1: Read the current "Ruin+apply rollback shape" bullet**

Run: `grep -n "Ruin+apply rollback shape" solver/CLAUDE.md`

Open the bullet (it currently phrases the pattern as a Kempe-specific learning that R&R should follow).

- [ ] **Step 2: Generalise the bullet's wording**

Edit so the bullet reads as the canonical pattern for any chain-style or multi-block local-search move. Replace the existing bullet with:

```markdown
- **Ruin+apply rollback shape: remove exact placements, do not re-ruin by lesson+day.** Every chain-style or multi-block local-search move (R&R `rr_attempt`, Kempe `kempe_attempt`, any future move that ruins on one (lesson, day) and recreates on another) must capture the exact `(LessonId, time_block_id, room_id)` rows added at apply time and remove only those rows on rollback. The older "find the first placement of the lesson and ruin its day" shape is unsafe whenever the lesson has another untouched block on a different day: the rollback drops a pristine block instead of undoing the recreate. R&R surfaced this as the active-sprint item 37 silent placement-drop on grundschule (`lahc_rr` 19/45 vs greedy 45/45) before the row-keyed pattern was ported in. The shared per-row decrement helper is `rr_remove_row_bookkeeping` in `lahc.rs`; both `rr_ruin_block`'s inner loop and `rr_rollback` call it, so a future caller does not need to re-implement the bookkeeping math.
```

- [ ] **Step 3: Edit OPEN_THINGS.md**

```bash
grep -n "^Next pickup\|item 37\|item 29\|^### Algorithm phase\|^### Bench prevention phase\|^37\.\|^29\." docs/superpowers/OPEN_THINGS.md | head -20
```

Promote item 29 to next pickup, delete item 37 entirely. Concretely, edit the "Next pickup" line (line 9 today) to:

```markdown
Next pickup: P0 item 29 (refresh `BENCH_RESULTS.md` at production settings + ADR 0032). Items 26 + 27 (R&R anchor filter + property tests), item 28 (bench placement-count gate), and item 37 (R&R row-keyed rollback) shipped in PRs #183, #184, and this PR. The dev-loop bake-off receipt at 5s/4-seeds on grundschule shows `lahc_rr` and `lahc_rr_kempe` at 45/45 with feasibility now matching `lahc`, so the production-default verdict in ADR 0032 reads off corrected numbers.
```

Then delete the `### Algorithm phase` heading and item 37's body block (lines 13-15 today; the heading goes too because it would be empty). The next section becomes `### Bench prevention phase` directly under the active-sprint header.

- [ ] **Step 4: Commit**

```bash
git add solver/CLAUDE.md docs/superpowers/OPEN_THINGS.md
git commit -m "docs(solver): generalise rollback-shape rule, close item 37"
```

## Task 6: Skill audit + open PR + automerge

**Files:**
- None directly; this task captures the audit, the push, and the PR body.

- [ ] **Step 1: Skill audit**

Walk the table in `.claude/commands/autopilot.md` and confirm each row's skill was invoked via the `Skill` tool this session. Required for this run:

- step 0: `superpowers:using-superpowers` ✓ (first action)
- step 2: `superpowers:brainstorming` ✓
- step 4: `superpowers:writing-plans` ✓
- step 5: `superpowers:test-driven-development` (invoke before Task 1)
- step 5: `superpowers:subagent-driven-development` (invoke before dispatching plan tasks)
- step 6: `claude-md-management:revise-claude-md`, `claude-md-management:claude-md-improver`, `fewer-permission-prompts` (invoke before pushing)

If any step-5 or step-6 skill was not invoked, invoke it now and let it reshape its artifact.

- [ ] **Step 2: Push the branch**

```bash
mise exec -- git push -u origin fix/solver-rr-placement-drop
```

- [ ] **Step 3: Create the PR**

```bash
gh pr create --base master --head fix/solver-rr-placement-drop \
  --title "fix(solver-core): port kempe row-keyed rollback to r&r (sprint 5 item 37)" \
  --body "$(cat <<'BODY'
## Summary

Closes the silent placement-drop bug in `lahc_rr` and `lahc_rr_kempe` on grundschule that survived items 26 + 27. The trigger is `rr_rollback`'s `placements.iter().position(|p| p.lesson_id == ...)` lookup: for lessons with multiple blocks across different days, the "first" placement of the lesson is rarely the recreated block, so `rr_ruin_block` was dropping pristine rows from another day instead of undoing the recreate. Replace the position-by-lesson-id lookup with the Kempe pattern of capturing exact `(LessonId, time_block_id, room_id)` rows at apply time and removing only those rows on rollback. Add a defensive guard for "recreate landed on a day where the lesson already had a different block" plus pre/post placement-count `debug_assert!`s so the same class of bug cannot ship again.

## Scope

In:
- `rr_attempt` captures `Vec<Vec<Placement>>` of recreated rows; `rr_rollback` removes by exact id; new private helper `rr_remove_row_bookkeeping` shared with `rr_ruin_block`
- new targeted regression test `solver-core/tests/rr_rollback.rs`
- widened `lahc_property` generator (`hours_per_week` covers 2..=4) plus 50 ms deadline for R&R configs
- `solver/CLAUDE.md` rollback-shape bullet generalised to R&R + Kempe + future moves
- `docs/superpowers/OPEN_THINGS.md` item 37 removed, item 29 promoted to next pickup

Non-goals:
- BENCH_RESULTS.md refresh + ADR 0032 (item 29, next sprint pickup)
- peak RAM / time-to-first-feasible columns (item 30)
- schedule-quality bake-off output (item 31)
- Python-side auto-assign solvability tests (item 32)

## Dev-loop bake-off receipt

Before (master tip 02fe443):
```
cell done: grundschule / lahc       feasibility 4/4 hard_med=0 placements_med=45/45 soft_med=16  total_ms_med=5000
cell done: grundschule / lahc_rr    feasibility 0/4 hard_med=0 placements_med=19/45 soft_med=-   total_ms_med=5000
cell done: grundschule / lahc_rr_kempe feasibility 0/4 hard_med=0 placements_med=19/45 soft_med=- total_ms_med=5000
cell done: grundschule / cpsat      feasibility 4/4 hard_med=0 placements_med=45/45 soft_med=330 total_ms_med=418
```

After (this branch): paste the postfix `cell done:` lines from `/tmp/bakeoff-grundschule-postfix.md`.

## Test plan

- [ ] `cargo nextest run -p solver-core --test rr_rollback`
- [ ] `cargo nextest run -p solver-core --test lahc_property`
- [ ] `cargo nextest run -p solver-core` (full suite)
- [ ] `mise run lint`
- [ ] `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule` shows lahc_rr / lahc_rr_kempe at 45/45
- [ ] dev-loop bake-off zweizügig sanity (no regression vs current)

## Links

- Spec: `docs/superpowers/specs/2026-05-05-solver-rr-rollback-row-keyed-design.md`
- Plan: `docs/superpowers/plans/2026-05-05-solver-rr-rollback-row-keyed.md`
- Active sprint program: `docs/superpowers/OPEN_THINGS.md`
BODY
)"
```

- [ ] **Step 4: Post brainstorm Q&A as PR comments**

```bash
python3 .claude/commands/post_brainstorm_comments.py <pr-number>
```

- [ ] **Step 5: Set automerge**

```bash
gh pr merge <pr-number> --auto --squash
```

- [ ] **Step 6: Wait for merge**

Poll: `gh pr view <pr-number> --json state -q .state` until it returns `MERGED`.

- [ ] **Step 7: Refresh local master**

```bash
git checkout master
git pull origin master
git branch -d fix/solver-rr-placement-drop
```

## Self-review checklist

- Spec section "In scope" maps to Tasks 1-5 (regression test, fix, property widening, docs).
- Spec section "Failure mode and fix" code blocks match Task 2's diff plan (captured rows + row-keyed rollback + defensive guard + asserts).
- Spec section "Tests" maps to Tasks 1 and 3.
- Spec section "Documentation" maps to Task 5.
- Spec section "Acceptance criteria" maps to Task 4 (bake-off receipt) plus the test plan in Task 6.
- No placeholders ("TBD", "TODO", etc.).
- All function and helper names consistent: `rr_remove_row_bookkeeping`, `rr_rollback`, `rr_attempt`, `rr_ruin_block`.
