# Item 54: class-day-balance as FFD greedy search-time objective Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `class_day_balance` to the FFD greedy window picker (`try_place_block` and `try_place_group`) so the construction phase steers toward balanced per-class day distributions, alongside the existing `slice + home_room` ranking.

**Architecture:** Introduce a new `pub(crate)` allocation-free helper `class_day_balance_cost_for_class_after_add` in `solver-core/src/score.rs` that mirrors the existing `class_day_balance_cost_for_class_with_swap`. Thread `days: u8` into both pickers (computed once per `solve_with_config` invocation). Inside each picker's window loop, sum the helper across the lesson's member classes, weight by `weights.class_day_balance`, and add the result to the candidate's ranking score. Pruning and early-exit semantics are preserved (the lower-bound argument still holds because the new term is non-negative).

**Tech Stack:** Rust 2021 edition, solver-core crate, proptest 1.x for property tests, criterion for the perf bench, `cargo nextest` for the test runner.

---

### Task 1: Add the `class_day_balance_cost_for_class_after_add` helper to `score.rs`

**Files:**
- Modify: `solver/solver-core/src/score.rs`
- Test: `solver/solver-core/src/score.rs` (inline `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` block at the end of `solver/solver-core/src/score.rs`, immediately after `class_day_balance_cost_for_class_with_swap_matches_post_apply_recompute`:

```rust
#[test]
fn class_day_balance_cost_for_class_after_add_matches_post_apply_recompute() {
    let class = SchoolClassId(score_uuid(97));
    let mut pre: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    pre.insert((class, 0), vec![0, 1, 2]);
    pre.insert((class, 2), vec![0]);
    // Predict the cost as if we appended 2 placements on day 1 (currently empty).
    let predicted = class_day_balance_cost_for_class_after_add(class, 5, &pre, 1, 2);
    let mut post = pre.clone();
    let day1 = post.entry((class, 1)).or_default();
    day1.push(0);
    day1.push(1);
    let actual = class_day_balance_cost_for_class(class, 5, &post);
    assert_eq!(predicted, actual);
}

#[test]
fn class_day_balance_cost_for_class_after_add_returns_zero_for_zero_days() {
    let class = SchoolClassId(score_uuid(98));
    let positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    assert_eq!(
        class_day_balance_cost_for_class_after_add(class, 0, &positions, 0, 1),
        0
    );
}

#[test]
fn class_day_balance_cost_for_class_after_add_grows_lopsided_total() {
    // Existing partition: 3 placements all on day 0; days = 4. Adding one more
    // on day 0 should raise the per-class cost; adding one on day 1 should
    // raise it less (pulls toward balance). Both bounded by the unweighted
    // helper's formula.
    let class = SchoolClassId(score_uuid(99));
    let mut positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    positions.insert((class, 0), vec![0, 1, 2]);
    let cost_add_to_packed = class_day_balance_cost_for_class_after_add(class, 4, &positions, 0, 1);
    let cost_add_to_empty = class_day_balance_cost_for_class_after_add(class, 4, &positions, 1, 1);
    assert!(
        cost_add_to_empty < cost_add_to_packed,
        "adding to an empty day should not increase imbalance more than adding to the packed day; \
         empty={cost_add_to_empty} packed={cost_add_to_packed}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p solver-core --lib score::tests::class_day_balance_cost_for_class_after_add_matches_post_apply_recompute score::tests::class_day_balance_cost_for_class_after_add_returns_zero_for_zero_days score::tests::class_day_balance_cost_for_class_after_add_grows_lopsided_total`
Expected: build error "cannot find function `class_day_balance_cost_for_class_after_add` in this scope".

- [ ] **Step 3: Add the helper**

Insert this function in `solver/solver-core/src/score.rs` immediately after `class_day_balance_cost_for_class_with_swap` (the function that ends around line 257) and before `class_day_balance_cost_for_class_from_counts`:

```rust
/// Variant of `class_day_balance_cost_for_class` that overlays a virtual
/// addition of `add_n` placements on `add_day` for `class_id`, without
/// mutating `class_positions`. Returns the per-class scaled L1 cost as if
/// the addition had been applied. Used by FFD greedy's `try_place_block`
/// and `try_place_group` window pickers to rank candidates by post-place
/// class-day-balance contribution alongside the existing slice and
/// home-room terms (item 54). Allocation-free; walks `0..days` twice.
pub(crate) fn class_day_balance_cost_for_class_after_add(
    class_id: SchoolClassId,
    days: u8,
    class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>,
    add_day: u8,
    add_n: u8,
) -> u32 {
    if days == 0 {
        return 0;
    }
    let d = u32::from(days);
    let added = u32::from(add_n);
    let mut sum: u32 = 0;
    for day in 0..days {
        sum = sum.saturating_add(
            class_positions
                .get(&(class_id, day))
                .map(|v| v.len() as u32)
                .unwrap_or(0),
        );
    }
    sum = sum.saturating_add(added);
    if sum == 0 {
        return 0;
    }
    let mut scaled: u32 = 0;
    for day in 0..days {
        let raw = class_positions
            .get(&(class_id, day))
            .map(|v| v.len() as u32)
            .unwrap_or(0);
        let c = if day == add_day { raw.saturating_add(added) } else { raw };
        scaled = scaled.saturating_add(c.saturating_mul(d).abs_diff(sum));
    }
    scaled / d
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p solver-core --lib score::tests::class_day_balance_cost_for_class_after_add_matches_post_apply_recompute score::tests::class_day_balance_cost_for_class_after_add_returns_zero_for_zero_days score::tests::class_day_balance_cost_for_class_after_add_grows_lopsided_total`
Expected: 3 tests pass.

- [ ] **Step 5: Commit (atomic with Task 2's call sites; do NOT commit yet if running tasks individually)**

Skip the commit at this point. The workspace rule "Bundle a new `pub(crate)` helper with its first caller in the same commit" (`solver/CLAUDE.md`) requires the helper and its first caller to land atomically; otherwise `cargo clippy --workspace --all-targets -- -D warnings` (which `mise run lint` runs) trips on `dead_code` for the unused `pub(crate)` item. Continue to Task 2.

---

### Task 2: Wire `days` and `class_day_balance` into `try_place_block`

**Files:**
- Modify: `solver/solver-core/src/solve.rs`

- [ ] **Step 1: Compute `days` once in `solve_with_config_stats` and thread it through to `try_place_block`**

Locate the block at `solver/solver-core/src/solve.rs` between `let max_position_per_day: HashMap<u8, u8> = ...` (around line 132-141) and `let order = crate::ordering::ffd_order(problem, &idx);` (around line 143). Insert the `days` computation between them:

```rust
let days: u8 = problem
    .time_blocks
    .iter()
    .map(|tb| tb.day_of_week)
    .max()
    .map(|m| m.saturating_add(1))
    .unwrap_or(0);
```

- [ ] **Step 2: Add `days` to `try_place_block`'s call site**

Find the existing `try_place_block(...)` call inside the FFD greedy lesson loop (around lines 211-225) and append `days,` as the new last argument:

```rust
let placed = try_place_block(
    problem,
    lesson,
    n,
    &idx,
    &teacher_max,
    &class_max_lessons_per_day,
    &config.weights,
    &home_room_lookup,
    &mut state,
    &mut solution.placements,
    &tb_order,
    &room_order,
    &max_position_per_day,
    days,
);
```

- [ ] **Step 3: Add `days: u8` to `try_place_block`'s signature**

Find `pub(crate) fn try_place_block(...)` (around line 420). Add `days: u8` as the new last parameter:

```rust
#[allow(clippy::too_many_arguments)] // Reason: internal helper; refactoring to a struct hurts clarity more than it helps
pub(crate) fn try_place_block(
    problem: &Problem,
    lesson: &Lesson,
    n: u8,
    idx: &Indexed,
    teacher_max: &HashMap<TeacherId, u8>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    weights: &ConstraintWeights,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    state: &mut GreedyState,
    placements: &mut Vec<Placement>,
    tb_order: &[usize],
    room_order: &[usize],
    max_position_per_day: &HashMap<u8, u8>,
    days: u8,
) -> bool {
```

- [ ] **Step 4: Compute `balance_post` in the picker after the room scan and fold into `total_score`**

Find the section around line 734-737 (immediately after `let Some((room_id, room_penalty)) = best_room else { continue; };`):

```rust
        let Some((room_id, room_penalty)) = best_room else {
            continue;
        };
        let total_score = slice_score.saturating_add(room_penalty);
```

Replace those four lines with:

```rust
        let Some((room_id, room_penalty)) = best_room else {
            continue;
        };

        // Class-day-balance contribution to candidate ranking (item 54). Sum
        // the per-class post-place L1 cost across every member class of this
        // lesson, then weight. Zero-weight short-circuit avoids the partition
        // walk on tests that disable the axis. The pruning check above
        // (`slice_score >= b.total_score`) stays sound: `room_penalty` and
        // `balance_post` are both non-negative, so `slice_score` remains a
        // lower bound on the candidate's eventual `total_score`.
        let balance_post: u32 = if weights.class_day_balance == 0 {
            0
        } else {
            let mut acc: u32 = 0;
            for class in class_ids {
                acc = acc.saturating_add(crate::score::class_day_balance_cost_for_class_after_add(
                    *class,
                    days,
                    &state.class_positions,
                    first_tb.day_of_week,
                    n,
                ));
            }
            weights.class_day_balance.saturating_mul(acc)
        };
        let total_score = slice_score
            .saturating_add(room_penalty)
            .saturating_add(balance_post);
```

- [ ] **Step 5: Verify the workspace builds and the existing tests still pass**

Run: `cargo build -p solver-core`
Expected: clean build, no new warnings.

Run: `cargo nextest run -p solver-core --lib`
Expected: all existing solver-core unit tests pass; behavior is byte-identical when `class_day_balance == 0` (default in most unit tests) since the new term short-circuits.

- [ ] **Step 6: Do not commit yet**

Continue to Task 3 to wire up `try_place_group` so the commit covers both pickers atomically.

---

### Task 3: Wire `days` and `class_day_balance` into `try_place_group`

**Files:**
- Modify: `solver/solver-core/src/solve.rs`

- [ ] **Step 1: Add `days` to `try_place_group`'s call site**

Find the existing `try_place_group(...)` call inside the lesson-group branch of the FFD greedy lesson loop (around lines 174-187) and append `days,` as the new last argument:

```rust
try_place_group(
    problem,
    &member_indices,
    n,
    &idx,
    &teacher_max,
    &class_max_lessons_per_day,
    &config.weights,
    &mut state,
    &mut solution.placements,
    &tb_order,
    &room_order,
    &max_position_per_day,
    days,
)
```

- [ ] **Step 2: Add `days: u8` to `try_place_group`'s signature**

Find `fn try_place_group(...)` (around line 876). Add `days: u8` as the new last parameter:

```rust
fn try_place_group(
    problem: &Problem,
    member_indices: &[usize],
    n: u8,
    idx: &Indexed,
    teacher_max: &HashMap<TeacherId, u8>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    weights: &ConstraintWeights,
    state: &mut GreedyState,
    placements: &mut Vec<Placement>,
    tb_order: &[usize],
    room_order: &[usize],
    max_position_per_day: &HashMap<u8, u8>,
    days: u8,
) -> bool {
```

- [ ] **Step 3: Compute `balance_post` and fold into `score` after `subject_pref`**

Find the block around lines 1121-1132 in `try_place_group`:

```rust
        let class_delta_w = class_delta_sum.saturating_mul(i64::from(weights.class_gap));
        let teacher_delta_w = teacher_delta_sum.saturating_mul(i64::from(weights.teacher_gap));
        let new_signed = i64::from(state.search_score_slice)
            .saturating_add(class_delta_w)
            .saturating_add(teacher_delta_w)
            .saturating_add(i64::from(subject_pref));
        let score = u32::try_from(new_signed.max(0)).unwrap_or(u32::MAX);

        if let Some(b) = &best {
            if score >= b.score {
                continue;
            }
        }
```

Replace with:

```rust
        let class_delta_w = class_delta_sum.saturating_mul(i64::from(weights.class_gap));
        let teacher_delta_w = teacher_delta_sum.saturating_mul(i64::from(weights.teacher_gap));
        let new_signed = i64::from(state.search_score_slice)
            .saturating_add(class_delta_w)
            .saturating_add(teacher_delta_w)
            .saturating_add(i64::from(subject_pref));
        let slice_score = u32::try_from(new_signed.max(0)).unwrap_or(u32::MAX);

        // Class-day-balance contribution for this group window (item 54). One
        // group placement adds `n` lessons to every class in `class_set`
        // simultaneously, so the per-class cost stacks across the shared set.
        let balance_post: u32 = if weights.class_day_balance == 0 {
            0
        } else {
            let mut acc: u32 = 0;
            for class in &class_set {
                acc = acc.saturating_add(crate::score::class_day_balance_cost_for_class_after_add(
                    *class,
                    days,
                    &state.class_positions,
                    first_tb.day_of_week,
                    n,
                ));
            }
            weights.class_day_balance.saturating_mul(acc)
        };
        let score = slice_score.saturating_add(balance_post);

        if let Some(b) = &best {
            if score >= b.score {
                continue;
            }
        }
```

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo build -p solver-core`
Expected: clean build, no new warnings.

- [ ] **Step 5: Run the full solver-core test suite**

Run: `cargo nextest run -p solver-core`
Expected: every existing test passes (the new term short-circuits at `weights.class_day_balance == 0`, which is the default in most fixtures).

- [ ] **Step 6: Do not commit yet**

Continue to Task 4 to add the targeted unit + property tests before the atomic commit.

---

### Task 4: Add the targeted picker unit test in `solve.rs`

**Files:**
- Modify: `solver/solver-core/src/solve.rs` (`tests` module at the bottom)

- [ ] **Step 1: Locate the existing `try_place_block_room_picker_*` tests as the template**

The two existing tests at `solver/solver-core/src/solve.rs:2650` and `solver/solver-core/src/solve.rs:2764` (`try_place_block_room_picker_minimises_home_room_penalty` and `try_place_block_room_picker_falls_back_to_id_order_when_no_home_room_advantage`) construct a small `Problem`, call `solve_with_config` with hand-tuned weights, and assert on `solution.placements`. Mirror that shape.

Read around `try_place_block_room_picker_minimises_home_room_penalty` to learn the helper functions in scope (`solve_uuid`, etc.) and the shape of the constructed `Problem`.

- [ ] **Step 2: Write the failing test**

Append this `#[test]` to the same `mod tests` block, immediately after `try_place_block_room_picker_falls_back_to_id_order_when_no_home_room_advantage` (search for `fn try_place_block_room_picker_falls_back_to_id_order_when_no_home_room_advantage`, then append after its closing brace):

```rust
/// FFD greedy's window picker must respond to `weights.class_day_balance`
/// (item 54). Build a 1-class, 4-day, 1-period-per-day fixture with two
/// pre-placed lessons (forced via pins) on day 0, then place a third
/// lesson via FFD. With `class_day_balance == 0` the picker chooses day 0
/// (lowest tb id, slice tiebreak; pinning consumes day 0's slot already so
/// the next eligible is day 1, but the picker has no preference between
/// day 1 and day 2). With `class_day_balance > 0` the picker prefers the
/// day that minimises post-place L1 spread.
///
/// This test pins TWO lessons on day 0, leaving days 1, 2, 3 empty, and
/// gives FFD ONE more lesson to place. The least-imbalanced choice is to
/// place on the most-loaded ALTERNATE day (currently zero on all of 1, 2,
/// 3). Tiebreak among 1, 2, 3 falls to lowest-id, which is day 1.
/// With `class_day_balance == 0`, the picker has no spread preference and
/// the tiebreak is pure tb-id which still lands on day 1. To exercise
/// the new term, we pin two lessons on day 0 AND one on day 1, making
/// day 1 second-most-loaded. The remaining lesson should now land on
/// day 2 (the most under-loaded day among feasibles); without the new
/// term, day 1 would still win on tb-id tiebreak and the post-state would
/// be 2/2/0/0 (cost = 4) instead of 2/1/1/0 (cost = 2).
#[test]
fn try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight() {
    let class_id = SchoolClassId(solve_uuid(1));
    let teacher_id = TeacherId(solve_uuid(2));
    let room_id = RoomId(solve_uuid(3));
    let subject_id = SubjectId(solve_uuid(4));
    let lesson_a = LessonId(solve_uuid(10)); // pinned on day 0
    let lesson_b = LessonId(solve_uuid(11)); // pinned on day 0
    let lesson_c = LessonId(solve_uuid(12)); // pinned on day 1
    let lesson_d = LessonId(solve_uuid(13)); // FFD-placed
    let tb_d0 = TimeBlockId(solve_uuid(20));
    let tb_d1 = TimeBlockId(solve_uuid(21));
    let tb_d2 = TimeBlockId(solve_uuid(22));
    let tb_d3 = TimeBlockId(solve_uuid(23));
    let tb_d0_b = TimeBlockId(solve_uuid(24)); // second tb on day 0 for the second pin
    let problem = Problem {
        time_blocks: vec![
            TimeBlock { id: tb_d0, day_of_week: 0, position: 0 },
            TimeBlock { id: tb_d0_b, day_of_week: 0, position: 1 },
            TimeBlock { id: tb_d1, day_of_week: 1, position: 0 },
            TimeBlock { id: tb_d2, day_of_week: 2, position: 0 },
            TimeBlock { id: tb_d3, day_of_week: 3, position: 0 },
        ],
        teachers: vec![Teacher {
            id: teacher_id,
            max_hours_per_week: 30,
        }],
        rooms: vec![Room { id: room_id }],
        subjects: vec![Subject {
            id: subject_id,
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        }],
        school_classes: vec![SchoolClass {
            id: class_id,
            home_room_id: None,
            max_lessons_per_day: None,
        }],
        lessons: vec![
            Lesson {
                id: lesson_a,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_id,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: lesson_b,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_id,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: lesson_c,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_id,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
            Lesson {
                id: lesson_d,
                school_class_ids: vec![class_id],
                subject_id,
                teacher_id,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: None,
            },
        ],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id,
            subject_id,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![
            PinnedPlacement {
                lesson_id: lesson_a,
                time_block_id: tb_d0,
                room_id,
            },
            PinnedPlacement {
                lesson_id: lesson_b,
                time_block_id: tb_d0_b,
                room_id,
            },
            PinnedPlacement {
                lesson_id: lesson_c,
                time_block_id: tb_d1,
                room_id,
            },
        ],
    };
    // class_day_balance == 0 baseline: with all weights zero the picker
    // tiebreaks on tb id, landing lesson_d on day 1 (lowest-id unpinned tb).
    let cfg_balance_off = SolveConfig {
        weights: ConstraintWeights::default(),
        deadline: None,
        ..SolveConfig::default()
    };
    let sol_off = solve_with_config(&problem, &cfg_balance_off)
        .expect("baseline solve must succeed on the tiny fixture");
    let placement_off_d = sol_off
        .placements
        .iter()
        .find(|p| p.lesson_id == lesson_d)
        .expect("FFD must place lesson_d on the baseline solve");
    assert_eq!(
        placement_off_d.time_block_id, tb_d1,
        "baseline (class_day_balance=0): lesson_d expected on day 1 by tb-id tiebreak"
    );

    // class_day_balance > 0: the picker should prefer the day that minimises
    // post-place L1 spread. Pre-FFD counts are 2/1/0/0 across days 0..3, so
    // day 2 is the least-loaded eligible day; lesson_d should land there.
    let cfg_balance_on = SolveConfig {
        weights: ConstraintWeights {
            class_day_balance: 5,
            ..ConstraintWeights::default()
        },
        deadline: None,
        ..SolveConfig::default()
    };
    let sol_on = solve_with_config(&problem, &cfg_balance_on)
        .expect("balance-on solve must succeed on the tiny fixture");
    let placement_on_d = sol_on
        .placements
        .iter()
        .find(|p| p.lesson_id == lesson_d)
        .expect("FFD must place lesson_d on the balance-on solve");
    assert_ne!(
        placement_on_d.time_block_id, tb_d1,
        "balance-on (class_day_balance=5): picker must NOT pile lesson_d onto day 1; \
         expected day 2 (the least-loaded day under L1-spread minimisation)"
    );
    assert_eq!(
        placement_on_d.time_block_id, tb_d2,
        "balance-on: lesson_d expected on day 2 (L1-spread-minimising candidate)"
    );
}
```

- [ ] **Step 3: Run the new test to verify it passes**

Run: `cargo nextest run -p solver-core --lib solve::tests::try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight`
Expected: PASS.

- [ ] **Step 4: If the test fails, diagnose**

Common failure modes and fixes:
- **`PinnedPlacement` import missing**: it already exists in the `mod tests` `use crate::types::{... PinnedPlacement ...};` block at the top of the module (around line 1421). If a different test module is in scope, add the import explicitly.
- **Pin's `time_block_id` rejected**: `validate_structural` checks pin shape; ensure the day-0 pins use distinct tb ids and the lesson's `hours_per_week=1` matches one pin per lesson.
- **Baseline assertion fails because the picker chooses something other than day 1**: this means the baseline FFD's tb-id tiebreak picks elsewhere; print `sol_off.placements` and adjust the asserted day if the actual baseline differs from the anticipated day 1. The acceptance criterion is "balance-on lands on a different day than balance-off"; both `assert_ne!` and the strict `assert_eq!(..., tb_d2)` may need updating to whatever the actual baseline + balance-on picks become.

- [ ] **Step 5: Do not commit yet**

Continue to Task 5 (regression test in `score_property.rs`) before the atomic commit.

---

### Task 5: Add the post-solve regression assertion in `score_property.rs`

**Files:**
- Modify: `solver/solver-core/tests/score_property.rs`

- [ ] **Step 1: Append the regression test**

Append this `#[test]` to the END of `solver/solver-core/tests/score_property.rs`:

```rust
/// Item 54: FFD greedy's `try_place_block` picker must respond to
/// `weights.class_day_balance`. Solve the existing two-day class-day-balance
/// fixture once with the axis disabled and once with it enabled at the
/// production weight (5). Re-evaluate both placement sets under a balance-
/// only scorer (`class_day_balance = 1`, every other weight `0`) and assert
/// the balance-on placement set produces a strictly lower contribution.
#[test]
fn ffd_greedy_class_day_balance_weight_lowers_post_solve_class_day_balance_cost() {
    let problem = build_class_day_balance_problem();
    let cfg_off = SolveConfig {
        weights: ConstraintWeights::default(),
        deadline: None,
        ..SolveConfig::default()
    };
    let cfg_on = SolveConfig {
        weights: ConstraintWeights {
            class_day_balance: 5,
            ..ConstraintWeights::default()
        },
        deadline: None,
        ..SolveConfig::default()
    };
    let sol_off = solve_with_config(&problem, &cfg_off).expect("baseline solve");
    let sol_on = solve_with_config(&problem, &cfg_on).expect("balance-on solve");

    // Re-score both placement sets under a balance-only scorer so the
    // comparison isolates the class_day_balance contribution.
    let scorer = ConstraintWeights {
        class_day_balance: 1,
        ..ConstraintWeights::default()
    };
    let balance_off = score_solution(&problem, &sol_off.placements, &scorer);
    let balance_on = score_solution(&problem, &sol_on.placements, &scorer);
    assert!(
        balance_on < balance_off,
        "balance-on solve must produce a strictly lower class_day_balance \
         contribution; got off={balance_off} on={balance_on}"
    );
}
```

- [ ] **Step 2: Run the new test**

Run: `cargo nextest run -p solver-core --test score_property ffd_greedy_class_day_balance_weight_lowers_post_solve_class_day_balance_cost`
Expected: PASS.

- [ ] **Step 3: Sweep the test under varied seeds (proptest discipline)**

This test is `#[test]`, not `proptest!`, so the seed-sweep doc in `solver/CLAUDE.md` does not apply. Skip.

- [ ] **Step 4: Do not commit yet**

Continue to Task 6 (the `BackendObjective` doc-string update).

---

### Task 6: Update the `BackendObjective` notes in `quality.rs`

**Files:**
- Modify: `solver/solver-core/src/quality.rs`

- [ ] **Step 1: Update `lahc_notes`**

Find this string literal in `solver/solver-core/src/quality.rs::build_backend_objectives` (around line 379-380):

```rust
    let lahc_notes = "LAHC accepts and exits on the full canonical (see lahc::run); \
                      item 54 reserved for FFD greedy-time class-day-balance tiebreak.";
```

Replace with:

```rust
    let lahc_notes = "LAHC accepts and exits on the full canonical (see lahc::run); \
                      FFD greedy ranks windows by slice + home_room + class_day_balance (item 54).";
```

- [ ] **Step 2: Verify the existing `BackendObjective` tests still pass**

Run: `cargo nextest run -p solver-core --lib quality::tests`
Expected: every test passes (the notes string is not asserted on; only the `optimised` / `declared_skipped` partitions and component coverage are tested).

- [ ] **Step 3: Run `mise run lint`**

Run: `mise run lint`
Expected: all linters pass (including `cargo machete`, `clippy`, `unique-fns`, `actionlint`).

- [ ] **Step 4: Run the full Rust test suite**

Run: `cargo nextest run --workspace`
Expected: every test passes.

If any test fails, diagnose before committing. Common pitfalls:
- A `Problem { ... }` literal site somewhere in `tests/` constructs the type without using `..Default::default()`; if `Problem` had a new field, that breaks. Item 54 does NOT add a `Problem` field, so this should not surface here.
- A property test's seed has unexpectedly become deterministic-flaky. Re-run under the documented sweep: `for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property; done`.

- [ ] **Step 5: Run the LAHC property test sweep**

Run: `for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property; done`
Expected: all 5 sweeps pass; the change does not introduce new RNG draws and `lahc_deterministic_under_seed_and_iter_cap` should hold byte-identical to master.

---

### Task 7: Atomic commit

**Files:**
- Already-modified: `solver/solver-core/src/score.rs`, `solver/solver-core/src/solve.rs`, `solver/solver-core/src/quality.rs`, `solver/solver-core/tests/score_property.rs`

- [ ] **Step 1: Stage and review the diff**

Run: `git status && git diff --stat`
Expected: changes only in `solver/solver-core/src/score.rs`, `solver/solver-core/src/solve.rs`, `solver/solver-core/src/quality.rs`, `solver/solver-core/tests/score_property.rs`.

- [ ] **Step 2: Commit with the Conventional Commits message**

```bash
git add solver/solver-core/src/score.rs solver/solver-core/src/solve.rs solver/solver-core/src/quality.rs solver/solver-core/tests/score_property.rs
git commit -m "$(cat <<'EOF'
feat(solver-core): ffd greedy ranks windows on class_day_balance (item 54)

Adds class_day_balance to try_place_block and try_place_group's
window pickers alongside the existing slice + home_room ranking.
After items 50/51/52 LAHC accepts on the canonical objective, but
the FFD construction phase ignored class_day_balance entirely and
seeded LAHC with unbalanced per-class day distributions.

Changes:
- New pub(crate) helper class_day_balance_cost_for_class_after_add
  in score.rs (allocation-free; mirrors *_with_swap shape).
- try_place_block and try_place_group take a new days: u8 parameter
  and fold the per-class post-place balance contribution into their
  candidate ranking score.
- Pruning and early-exit semantics preserved: the new term is
  non-negative so slice_score remains a valid lower bound.
- BackendObjective notes for the LAHC family updated to drop the
  "item 54 reserved" placeholder.
- Unit test in solve.rs and regression test in score_property.rs
  pin the picker behavior; helper-level unit tests cover the new
  score.rs function.
EOF
)"
```

- [ ] **Step 3: Verify the commit succeeded and pre-commit hooks passed**

Run: `git log -1 --format=%s%n%b`
Expected: the message above (one line subject, blank line, body), and lefthook's pre-commit summary above showed every linter passed.

If the pre-commit hook fails, do NOT use `--no-verify`. Investigate:
- Lint failure: read the linter output, fix the offense, `git add`, `git commit --amend` is OK at this point because the commit is still local-only. Per workspace policy a fresh commit is preferred over amending; either is fine here since nothing has been pushed.
- Test failure inside lefthook: lefthook's pre-push runs the full suite, but pre-commit only runs lint. A test failure at this stage would surface from the explicit `mise run lint` above; re-run that and fix.

---

### Task 8: Verify perf and feasibility before push

**Files:** none (read-only verification).

- [ ] **Step 1: Smoke-bench feasibility on grundschule + zweizuegig**

Run: `cargo run --release -p solver-bench -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig`
Expected: every cell reports `feasibility = 4/4`. If any cell drops to less than 4/4, halt and investigate (the picker change should not affect feasibility, only ranking among feasibles).

- [ ] **Step 2: Criterion delta on grundschule (the only fixture mise run bench can run end-to-end today)**

Run twice from the master tip and the feature branch tip respectively:

```bash
cargo bench -p solver-core --bench solver_fixtures -- 'grundschule'
```

The first run establishes the baseline; the second prints the delta. Record the percentage delta on `solver_greedy/grundschule` and `solver_lahc/grundschule` for the PR body. A regression beyond 20% halts the merge for triage; a smaller regression is acceptable but cited in the PR body.

If `mise run bench` panics on `solver_greedy/zweizuegig` at `solver-core/benches/solver_fixtures.rs:133` (item 15 still open), that's expected; partial grundschule signal is the gate.

- [ ] **Step 3: Cite the BENCH_RESULTS.md plan in the PR body**

A full `mise run bench:bakeoff` refresh is a separate post-merge step (5 hours wall-clock at production cell shape per `solver/CLAUDE.md`). The PR body notes that the refresh will follow in a separate run; it does NOT block the merge.

- [ ] **Step 4: Push**

Run: `mise exec -- git push -u origin feat/solver-class-day-balance-search-objective`
Expected: lefthook's pre-push runs the full suite (`cargo nextest run --workspace`, `uv run pytest`, frontend Vitest). Push succeeds when every step passes.

If pre-push fails:
- Lint: as before, fix the underlying issue and amend or follow up.
- Pytest: solver-py tests pull in the maturin-built wheel; if signatures changed (they did NOT in this PR), `mise run solver:rebuild` first. For this PR no Python-facing signatures changed, so failure here is unexpected; investigate before retrying.
- Vitest: this PR does not touch frontend code. A vitest failure here is unexpected; investigate before retrying.

- [ ] **Step 5: Open the PR**

This is handled by the autopilot workflow; not a step inside this plan.
