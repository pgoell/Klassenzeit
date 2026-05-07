# LAHC canonical objective implementation plan (item 52)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make LAHC accept, exit, and probe `time_to_optimal_ms` on the canonical objective (`score_solution(problem, placements, weights)`, includes `prefer_home_room` and `class_day_balance`) instead of the running slice; guarantee the LAHC incumbent is non-increasing versus the post-greedy canonical via a running-best snapshot.

**Architecture:** Split `GreedyState.soft_score` into two fields: `search_score_slice` (existing behaviour, drives greedy picker persist contract and Change-move debug_assert) and `canonical_score` (new, drives LAHC accept / early-exit / time-to-optimal). LAHC maintains both in lockstep across all three moves (Change via incremental delta, R&R via full recompute, Kempe via extended snapshot+delta). At loop entry snapshot `best_placements`, refresh on every running-best canonical event, restore on loop exit.

**Tech Stack:** Rust 1.85, `solver-core` (lib + tests), proptest 1.x, criterion bench (out-of-band perf gate).

---

## File structure

| File | Role | Changes |
| --- | --- | --- |
| `solver/solver-core/src/solve.rs` | Greedy + `GreedyState` definition + `solve_with_config_stats` orchestrator | Rename field, add canonical_score field, initialise canonical post-greedy |
| `solver/solver-core/src/lahc.rs` | LAHC outer loop + Change / R&R / Kempe move implementations | Field rename, canonical maintenance per move, accept on canonical, snapshot best placements, restore on exit |
| `solver/solver-core/src/score.rs` | Canonical scorer + day-balance cost helper | Add `class_day_balance_cost_for_class` per-class O(D) helper |
| `solver/solver-core/src/quality.rs` | Backend objective declarations | Move HomeRoom + ClassDayBalance from `lahc_skipped` to `lahc_optimised`, refresh notes |
| `solver/solver-core/tests/lahc_property.rs` | LAHC property/regression suite | Three new tests (pin canonical maintenance, non-increasing vs greedy, running-best snapshot) |
| `solver/CLAUDE.md` | Solver workspace rules | Slice/canonical distinction, snapshot mechanism, item 54 cross-ref |
| `docs/superpowers/OPEN_THINGS.md` | Open work index | Delete item 52 entry, update next-pickup rotor |
| `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md` | Auto-memory | Refresh frontmatter description and body |

---

## Task 1: Rename `state.soft_score` to `state.search_score_slice` (structural)

**Files:**
- Modify: `solver/solver-core/src/solve.rs` (field definition + every internal reference)
- Modify: `solver/solver-core/src/lahc.rs` (every `state.soft_score` reference)
- Modify: `solver/solver-core/tests/*.rs` (any direct `state.soft_score` reads/writes; expect zero outside `solver-core/src`)

This task contains no behaviour change. It is mechanical and the existing test suite must pass with no test edits.

- [ ] **Step 1: Confirm where the field is referenced**

Run: `rg -n 'soft_score' solver/solver-core/src/ solver/solver-core/tests/ | grep -v 'Solution\.\|solution\.\|solution_\|//\|Solution {\|score_solution'`

Expected: lines that reference `state.soft_score` (Greedy + LAHC). Lines referencing `Solution.soft_score` and `score_solution` (the API surface and the scorer function) must NOT appear. Capture the output to confirm the renamed call sites.

- [ ] **Step 2: Rename the field on `GreedyState`**

In `solver/solver-core/src/solve.rs` find the struct `GreedyState` and change:

```rust
    pub(crate) soft_score: u32,
```

to:

```rust
    pub(crate) search_score_slice: u32,
```

Update the doc comment to read:

```rust
    /// Running LAHC search slice: `class_gap + teacher_gap + subject_pref`.
    /// Maintained by greedy's `try_place_block` persist site, by Change-move
    /// delta, by Kempe snapshot+delta, and by R&R via
    /// `running_slice_from_placements`. Greedy's picker persist contract
    /// stores `slice_score` here; LAHC reads the slice for the non-negative
    /// debug_assert in `try_change_move`.
    pub(crate) search_score_slice: u32,
```

In the same file, also update `GreedyState::new()`'s `soft_score: 0,` to `search_score_slice: 0,`.

- [ ] **Step 3: Rename every reader/writer in `solver-core/src/solve.rs`**

`sed -i 's/state\.soft_score/state.search_score_slice/g' solver/solver-core/src/solve.rs` then audit by `rg 'soft_score' solver/solver-core/src/solve.rs`. Remaining hits must all be on `Solution.soft_score`, `solution.soft_score`, or `score_solution(...)`. None on `state.soft_score`.

Same treatment for `s.soft_score = 0` in the inline `GreedyState { ... }` test literal at line 90 (rename to `s.search_score_slice = 0` if such a literal exists; verify with `rg 'soft_score: 0' solver/solver-core/src/solve.rs`).

- [ ] **Step 4: Rename every reference in `solver-core/src/lahc.rs`**

`sed -i 's/state\.soft_score/state.search_score_slice/g' solver/solver-core/src/lahc.rs` then audit. The matching variable bindings inside `rr_attempt` (`let pre_score = state.soft_score;` becomes `let pre_score = state.search_score_slice;`) and inside `kempe_attempt` (same shape) get rewritten by the sed. The mid-rollback re-assignments (`state.soft_score = pre_score;` becomes `state.search_score_slice = pre_score;`) likewise.

Also rename the local variable name `pre_score` to `pre_slice` in `rr_attempt` and `kempe_attempt` for clarity (it now refers explicitly to the slice, not "the soft score"). Use:

```bash
# Inside rr_attempt and kempe_attempt blocks only — open in editor and rename `pre_score` -> `pre_slice`
```

- [ ] **Step 5: Update doc comments mentioning `state.soft_score` or "running soft score"**

Edit the doc comments inside `solver-core/src/lahc.rs` so each comment that names `state.soft_score` instead names `state.search_score_slice`. Expected hits:
- The `run` function header (line ~25-30): "post-LAHC running total ends up in `state.soft_score`" -> "in `state.search_score_slice`".
- `try_change_move`'s "running score must remain non-negative" assertion message: keep as-is (the assertion is still about non-negativity), but if the message names "soft score" rename to "slice score".
- `running_slice_from_placements` doc: "Matches the slice greedy / Change / Kempe maintain on `state.soft_score`" -> "on `state.search_score_slice`".
- The R&R explanatory comment around line 970-980 ("`try_place_block` accumulates against `state.soft_score`...") -> rename references.

Also rewrite the LAHC tail block in `run` so the variable named `running_best` (still reading `state.soft_score`) becomes `running_best` reading `state.search_score_slice`. The rename is mechanical; do not change semantics yet.

- [ ] **Step 6: Compile and run the workspace tests**

Run: `cargo nextest run -p solver-core`
Expected: all existing tests pass with no edits.

If a test or fixture in `solver-core/tests/*.rs` constructs a `GreedyState { ... }` literal with `soft_score: 0`, rename that field too. Verify with `rg 'soft_score:' solver/solver-core/tests/`.

- [ ] **Step 7: Lint**

Run: `mise run lint:rust`
Expected: clippy + machete + fmt all green. Lint is part of the pre-commit hook so the failure mode is at commit time, not surprise-CI.

- [ ] **Step 8: Commit**

```bash
git add solver/solver-core/src/solve.rs solver/solver-core/src/lahc.rs solver/solver-core/tests/
git commit -m "refactor(solver-core): rename state.soft_score to state.search_score_slice (item 52 prep)"
```

---

## Task 2: Track `state.canonical_score` on `GreedyState` (no behaviour change)

**Files:**
- Modify: `solver/solver-core/src/solve.rs` (add field, initialise canonical post-greedy before LAHC)
- Modify: `solver/solver-core/src/lahc.rs` (Change move maintains canonical, R&R recomputes canonical, Kempe snapshot+delta extends, debug_assert at iteration tail)
- Modify: `solver/solver-core/src/score.rs` (add per-class day-balance helper)
- Create: test `canonical_score_matches_score_solution_at_lahc_exit` in `solver/solver-core/tests/lahc_property.rs`

LAHC's accept criterion still operates on the slice in this task. The new `state.canonical_score` is a passive observer that the property test pins against `score_solution`.

- [ ] **Step 1: Add the per-class day-balance helper**

Open `solver/solver-core/src/score.rs`. After `class_day_balance_cost` (around line 172) add:

```rust
/// Per-class scaled L1 day-balance cost. Walks the class's per-day counts
/// twice (sum, then scaled), no allocation, returns the unweighted cost
/// for the single class. Caller multiplies by `weights.class_day_balance`.
/// Used by LAHC Change-move and Kempe delta paths so the canonical
/// objective stays incrementally maintained without allocating
/// `Vec<u32>(days)` per call. The cold-path `class_day_balance_cost`
/// equals the sum of this helper across `problem.school_classes`.
pub(crate) fn class_day_balance_cost_for_class(
    class_id: SchoolClassId,
    days: u8,
    class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>,
) -> u32 {
    if days == 0 {
        return 0;
    }
    let d = u32::from(days);
    let mut sum: u32 = 0;
    for day in 0..days {
        sum = sum.saturating_add(
            class_positions
                .get(&(class_id, day))
                .map(|v| v.len() as u32)
                .unwrap_or(0),
        );
    }
    if sum == 0 {
        return 0;
    }
    let mut scaled: u32 = 0;
    for day in 0..days {
        let c = class_positions
            .get(&(class_id, day))
            .map(|v| v.len() as u32)
            .unwrap_or(0);
        scaled = scaled.saturating_add(c.saturating_mul(d).abs_diff(sum));
    }
    scaled / d
}
```

- [ ] **Step 2: Add a unit test for the new helper**

In `solver/solver-core/src/score.rs::tests` add:

```rust
#[test]
fn class_day_balance_cost_for_class_matches_full_cost_per_class() {
    use crate::ids::SchoolClassId;
    use uuid::Uuid;
    let class_a = SchoolClassId(Uuid::from_bytes([1; 16]));
    let class_b = SchoolClassId(Uuid::from_bytes([2; 16]));
    let mut by_class_day: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    by_class_day.insert((class_a, 0), vec![0, 1, 2]);
    by_class_day.insert((class_a, 1), vec![0]);
    by_class_day.insert((class_b, 2), vec![0, 1]);
    let classes = vec![
        crate::types::SchoolClass {
            id: class_a,
            home_room_id: None,
            max_lessons_per_day: None,
        },
        crate::types::SchoolClass {
            id: class_b,
            home_room_id: None,
            max_lessons_per_day: None,
        },
    ];
    let days: u8 = 5;
    let total = class_day_balance_cost(&by_class_day, &classes, days);
    let per_class_a = class_day_balance_cost_for_class(class_a, days, &by_class_day);
    let per_class_b = class_day_balance_cost_for_class(class_b, days, &by_class_day);
    assert_eq!(per_class_a + per_class_b, total);
}
```

- [ ] **Step 3: Run the helper test to verify it passes**

Run: `cargo nextest run -p solver-core --lib score::tests::class_day_balance_cost_for_class_matches_full_cost_per_class`
Expected: PASS.

- [ ] **Step 4: Add `canonical_score` field on `GreedyState`**

In `solver/solver-core/src/solve.rs`, add to `GreedyState`:

```rust
    /// Running canonical objective: `score_solution(problem, placements,
    /// weights)`. Initialised at the end of greedy in
    /// `solve_with_config_stats` before LAHC dispatch. Maintained in
    /// lockstep with `search_score_slice` across the LAHC Change move
    /// (incremental delta), R&R (full recompute), and Kempe (snapshot
    /// + delta). Drives LAHC's accept criterion, `time_to_optimal_ms`
    /// probe, early-exit predicate, and running-best snapshot.
    pub(crate) canonical_score: u32,
```

In `GreedyState::new()` add `canonical_score: 0,` next to the existing `search_score_slice: 0,` field.

Any inline `GreedyState { ... }` literals in tests that listed every field must add `canonical_score: 0,`. Run `rg 'GreedyState \{' solver/solver-core/` and check each hit.

- [ ] **Step 5: Initialise `state.canonical_score` post-greedy in `solve_with_config_stats`**

In `solver/solver-core/src/solve.rs`, locate the LAHC dispatch site (just before `lahc::run(...)` is called). Insert:

```rust
    // Initialise the canonical-objective tracker. Greedy persists the slice
    // into state.search_score_slice via try_place_block; the canonical adds
    // home_room + class_day_balance over the slice. Set once here so LAHC
    // can maintain canonical incrementally per move.
    state.canonical_score = crate::score::score_solution(problem, &placements, &config.weights);
```

If LAHC is conditional (deadline-gated), set `state.canonical_score` unconditionally just before the LAHC dispatch — the field is also useful for invariant assertions if anyone reads it post-greedy.

- [ ] **Step 6: Maintain `canonical_score` in the Change move**

In `solver/solver-core/src/lahc.rs::try_change_move`, after the existing `subject_pref_delta` computation and before the slice `delta` calculation, compute `home_room_delta` and `class_day_balance_delta`:

```rust
    // home_room_delta: per-class lookup. Pure, allocation-free.
    let home_room_delta: i64 = if weights.prefer_home_room == 0 {
        0
    } else {
        let mut acc: i64 = 0;
        for class in class_ids {
            // The penalty function takes a single-element lookup; build a
            // local one for the move to keep state borrow-free.
            // The home_room_lookup that lahc::run already builds is reused
            // (passed via the parameter list — see Step 7).
            let old_pen = crate::score::home_room_penalty_one_class(
                *class,
                home_room_lookup,
                p.room_id,
                weights,
            );
            let new_pen = crate::score::home_room_penalty_one_class(
                *class,
                home_room_lookup,
                new_room_id,
                weights,
            );
            acc += i64::from(new_pen) - i64::from(old_pen);
        }
        acc
    };

    // class_day_balance_delta: per-affected-class, two O(D) passes (pre/post).
    // Same-day moves are zero by construction — the per-day count vector for
    // both old_day and new_day is the same partition before and after.
    let class_day_balance_delta: i64 = if weights.class_day_balance == 0
        || old_tb.day_of_week == new_tb.day_of_week
    {
        0
    } else {
        let days = problem
            .time_blocks
            .iter()
            .map(|tb| tb.day_of_week)
            .max()
            .map(|m| m.saturating_add(1))
            .unwrap_or(0);
        let mut acc: i64 = 0;
        for class in class_ids {
            let pre = crate::score::class_day_balance_cost_for_class(
                *class,
                days,
                &state.class_positions,
            );
            // Compute the post-counts in-place: temporarily mutate
            // state.class_positions is forbidden (we have not accepted yet);
            // instead, walk the same class with a one-class virtual override
            // by reading the current map plus the +1/-1 adjustment for
            // (old_day, new_day). Per-class scaled cost helper variant:
            let post = class_day_balance_cost_for_class_with_swap(
                *class,
                days,
                &state.class_positions,
                old_tb.day_of_week,
                new_tb.day_of_week,
            );
            acc += i64::from(post) - i64::from(pre);
        }
        i64::from(weights.class_day_balance) * acc
    };

    let canonical_delta = delta + home_room_delta + class_day_balance_delta;
    let new_canonical_signed = i64::from(state.canonical_score) + canonical_delta;
    let new_canonical = u32::try_from(new_canonical_signed.max(0)).unwrap_or(u32::MAX);
```

The function `home_room_penalty_one_class(class_id, lookup, room_id, weights) -> u32` is added in Step 8 below. The function `class_day_balance_cost_for_class_with_swap(class_id, days, class_positions, old_day, new_day) -> u32` is added in Step 7 below.

After the `apply_change_move(...)` call and the `state.search_score_slice = new_score;` assignment, add:

```rust
    state.canonical_score = new_canonical;
```

- [ ] **Step 7: Add the `class_day_balance_cost_for_class_with_swap` helper**

In `solver/solver-core/src/score.rs`, just below `class_day_balance_cost_for_class`, add:

```rust
/// Variant of `class_day_balance_cost_for_class` that overlays a virtual
/// move of one count from `old_day` to `new_day` (single placement swap)
/// without mutating `class_positions`. Returns the per-class scaled L1
/// cost as if the move had been applied. Used by LAHC Change-move's
/// canonical delta to compute pre/post for one class without allocation.
pub(crate) fn class_day_balance_cost_for_class_with_swap(
    class_id: SchoolClassId,
    days: u8,
    class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>,
    old_day: u8,
    new_day: u8,
) -> u32 {
    if days == 0 || old_day == new_day {
        return class_day_balance_cost_for_class(class_id, days, class_positions);
    }
    let d = u32::from(days);
    let mut sum: u32 = 0;
    for day in 0..days {
        sum = sum.saturating_add(
            class_positions
                .get(&(class_id, day))
                .map(|v| v.len() as u32)
                .unwrap_or(0),
        );
    }
    if sum == 0 {
        return 0;
    }
    let mut scaled: u32 = 0;
    for day in 0..days {
        let raw = class_positions
            .get(&(class_id, day))
            .map(|v| v.len() as u32)
            .unwrap_or(0);
        let c = if day == old_day {
            raw.saturating_sub(1)
        } else if day == new_day {
            raw.saturating_add(1)
        } else {
            raw
        };
        scaled = scaled.saturating_add(c.saturating_mul(d).abs_diff(sum));
    }
    scaled / d
}
```

Add a sibling unit test in `score.rs::tests`:

```rust
#[test]
fn class_day_balance_cost_for_class_with_swap_matches_post_apply_recompute() {
    use crate::ids::SchoolClassId;
    use uuid::Uuid;
    let class = SchoolClassId(Uuid::from_bytes([7; 16]));
    let mut pre: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    pre.insert((class, 0), vec![0, 1, 2]);
    pre.insert((class, 2), vec![0]);
    let predicted = class_day_balance_cost_for_class_with_swap(class, 5, &pre, 0, 3);
    let mut post = pre.clone();
    let day0 = post.get_mut(&(class, 0)).unwrap();
    day0.pop();
    post.entry((class, 3)).or_default().push(0);
    let actual = class_day_balance_cost_for_class(class, 5, &post);
    assert_eq!(predicted, actual);
}
```

- [ ] **Step 8: Add `home_room_penalty_one_class`**

In `solver/solver-core/src/score.rs`, next to `home_room_penalty`, add:

```rust
/// Per-class home_room penalty. Returns `weights.prefer_home_room` if
/// the class has a `home_room_id` and the given `room_id` differs;
/// 0 otherwise. Used by LAHC Change-move and Kempe canonical deltas
/// where the per-row home_room contribution is needed without
/// re-walking `lesson.school_class_ids` inside the existing
/// `home_room_penalty(lesson, ...)` path. Pure, allocation-free.
pub(crate) fn home_room_penalty_one_class(
    class_id: SchoolClassId,
    home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>,
    room_id: RoomId,
    weights: &ConstraintWeights,
) -> u32 {
    if weights.prefer_home_room == 0 {
        return 0;
    }
    if let Some(Some(home_id)) = home_room_lookup.get(&class_id) {
        if *home_id != room_id {
            return weights.prefer_home_room;
        }
    }
    0
}
```

Sibling unit test:

```rust
#[test]
fn home_room_penalty_one_class_matches_lesson_walk_for_single_class_lesson() {
    use crate::ids::SchoolClassId;
    use uuid::Uuid;
    let class = SchoolClassId(Uuid::from_bytes([3; 16]));
    let home = RoomId(Uuid::from_bytes([4; 16]));
    let other = RoomId(Uuid::from_bytes([5; 16]));
    let mut lookup: HashMap<SchoolClassId, Option<RoomId>> = HashMap::new();
    lookup.insert(class, Some(home));
    let weights = ConstraintWeights {
        prefer_home_room: 7,
        ..ConstraintWeights::default()
    };
    assert_eq!(home_room_penalty_one_class(class, &lookup, other, &weights), 7);
    assert_eq!(home_room_penalty_one_class(class, &lookup, home, &weights), 0);
}
```

- [ ] **Step 9: Plumb `home_room_lookup`, `problem`, and `class_max_lessons_per_day` to `try_change_move`**

`try_change_move` already receives `problem` (line 213). Verify by `rg -n 'fn try_change_move' solver/solver-core/src/lahc.rs`. It does NOT receive `home_room_lookup` today; thread the parameter from `lahc::run` through to `try_change_move`. The lookup is built once at the top of `lahc::run` (line 57).

Add `home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>` to the parameter list and pass it from the call site (around line 169-185). Update the `#[allow(clippy::too_many_arguments)]` reason if the count grows.

- [ ] **Step 10: Maintain `canonical_score` in R&R**

In `solver/solver-core/src/lahc.rs::rr_attempt`:

After the existing `let new_score = running_slice_from_placements(problem, placements, weights, max_position_per_day); state.search_score_slice = new_score;` block, add:

```rust
    let new_canonical = crate::score::score_solution(problem, placements, weights);
    state.canonical_score = new_canonical;
```

The accept criterion still uses `pre_slice` / `new_score` (slice). On rollback, restore both `state.search_score_slice = pre_slice;` AND `state.canonical_score = pre_canonical;` where `pre_canonical = state.canonical_score;` is captured at the top of `rr_attempt` next to `pre_slice`.

- [ ] **Step 11: Maintain `canonical_score` in Kempe**

In `solver/solver-core/src/lahc.rs::kempe_attempt`:

Capture `let pre_canonical = state.canonical_score;` next to the existing `let pre_slice = state.search_score_slice;` (renamed in task 1).

Extend the `kempe_snapshot_pre_score` function to additionally snapshot per-affected-class day-counts. Concrete shape:

In `kempe_snapshot_pre_score` (or a sibling function `kempe_snapshot_canonical_pre`), walk the union of every chain member's `lesson.school_class_ids` and the union of the chain's source/dest days. For each affected class, record the per-day count vector before apply. Returns `Vec<(SchoolClassId, Vec<u32>)>` (class id, counts vector indexed by day_of_week).

After the existing `gap_delta` and `subject_pref_delta` computation, compute:

```rust
    // home_room_delta: walk the snapshots' rows (removed) and the apply's
    // recreated rows (added). Mirror the existing removed_subject_pref /
    // added_subject_pref accumulation pattern.
    let home_room_delta: i64 = if weights.prefer_home_room == 0 {
        0
    } else {
        let mut removed: u32 = 0;
        let mut added: u32 = 0;
        for (lesson_id, _src, snap) in snapshots.iter() {
            let lesson = lesson_lookup.get(lesson_id).expect("snap lesson resolves");
            for row in &snap.rows {
                for class in &lesson.school_class_ids {
                    removed = removed.saturating_add(
                        crate::score::home_room_penalty_one_class(
                            *class,
                            home_room_lookup,
                            row.room_id,
                            weights,
                        ),
                    );
                }
            }
        }
        // Walk the recreated rows: chain.iter() gives the new placements
        // applied during this kempe attempt. After kempe_apply_block the
        // rows are appended at the tail of `placements` for each member;
        // capture them in `recreated_in_order` (already tracked) and
        // re-resolve via lesson_lookup + the destination start_pos.
        for (lesson_id, dest_day, dest_start_pos) in recreated_in_order.iter() {
            let lesson = lesson_lookup.get(lesson_id).expect("resolved");
            let n = lesson.preferred_block_size;
            for k in 0..n {
                let row_pos = dest_start_pos + k;
                // Look up the recreated row by (lesson_id, time_block_id).
                let Some(tb_id) = tb_by_day_pos.get(&(*dest_day, row_pos)) else {
                    continue;
                };
                let Some(p) = placements.iter().find(|p| {
                    p.lesson_id == *lesson_id && p.time_block_id == *tb_id
                }) else {
                    continue;
                };
                for class in &lesson.school_class_ids {
                    added = added.saturating_add(
                        crate::score::home_room_penalty_one_class(
                            *class,
                            home_room_lookup,
                            p.room_id,
                            weights,
                        ),
                    );
                }
            }
        }
        i64::from(added) - i64::from(removed)
    };

    // class_day_balance_delta: walk affected classes only (snapshot vs post).
    let class_day_balance_delta: i64 = if weights.class_day_balance == 0 {
        0
    } else {
        let days = problem
            .time_blocks
            .iter()
            .map(|tb| tb.day_of_week)
            .max()
            .map(|m| m.saturating_add(1))
            .unwrap_or(0);
        let mut acc: i64 = 0;
        for (class_id, _pre_counts) in &class_day_counts_pre {
            let pre_cost = crate::score::class_day_balance_cost_for_class_from_counts(
                *class_id,
                days,
                _pre_counts,
            );
            let post_cost = crate::score::class_day_balance_cost_for_class(
                *class_id,
                days,
                &state.class_positions,
            );
            acc += i64::from(post_cost) - i64::from(pre_cost);
        }
        i64::from(weights.class_day_balance) * acc
    };

    let canonical_delta = total_delta
        + home_room_delta
        + class_day_balance_delta;
    let new_canonical_signed = i64::from(pre_canonical) + canonical_delta;
    let new_canonical = u32::try_from(new_canonical_signed.max(0)).unwrap_or(u32::MAX);
```

Add the helper `class_day_balance_cost_for_class_from_counts(class_id, days, counts: &[u32]) -> u32` to `score.rs` (Step 12) so the snapshot can be a `Vec<u32>` per class. On rollback, restore `state.canonical_score = pre_canonical;` next to the existing `state.search_score_slice = pre_slice;` lines.

- [ ] **Step 12: Add `class_day_balance_cost_for_class_from_counts`**

In `solver/solver-core/src/score.rs`, next to `class_day_balance_cost_for_class`:

```rust
/// Per-class scaled L1 day-balance cost computed from a pre-captured
/// counts vector (`counts[day] = placements_for_class_on_day`).
/// Caller supplies the counts; useful when the canonical delta needs
/// the cost against a snapshot that is no longer in `class_positions`
/// (e.g. Kempe's pre-apply snapshot).
pub(crate) fn class_day_balance_cost_for_class_from_counts(
    _class_id: SchoolClassId,
    days: u8,
    counts: &[u32],
) -> u32 {
    if days == 0 || counts.is_empty() {
        return 0;
    }
    let d = u32::from(days);
    let mut sum: u32 = 0;
    for c in counts.iter().take(usize::from(days)) {
        sum = sum.saturating_add(*c);
    }
    if sum == 0 {
        return 0;
    }
    let mut scaled: u32 = 0;
    for c in counts.iter().take(usize::from(days)) {
        scaled = scaled.saturating_add(c.saturating_mul(d).abs_diff(sum));
    }
    scaled / d
}
```

Note: the `_class_id` parameter is unused inside the body but kept in the signature for symmetry with the other helpers and to ease potential future refactoring (e.g. when the helper grows a per-class subject filter).

- [ ] **Step 13: Add `debug_assert_eq!` at the end of every LAHC iteration**

In `solver/solver-core/src/lahc.rs::run`, just before `if state.search_score_slice == 0 && placements.len() == placements_expected { break; }` (around line 200), insert:

```rust
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            state.canonical_score,
            crate::score::score_solution(problem, placements, &config.weights),
            "LAHC must keep state.canonical_score == score_solution(...) at every iteration tail",
        );
```

This catches any per-move delta drift loudly under tests; release builds compile it away.

- [ ] **Step 14: Add the property test pinning canonical maintenance at LAHC exit**

In `solver/solver-core/tests/lahc_property.rs`, add:

```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// LAHC must leave `state.canonical_score` (visible post-solve as
    /// `solution.soft_score`) consistent with `score_solution(problem,
    /// placements, weights)` on the returned placements for every move
    /// type. Pinned by the in-loop debug_assert in lahc::run; this test
    /// names the contract for grep-discoverability and provides cross-
    /// seed coverage that the assert alone would not.
    #[test]
    fn canonical_score_matches_score_solution_at_lahc_exit(
        seed in 0u64..1024,
    ) {
        let problem = lahc_small_problem();
        let weights = ConstraintWeights {
            prefer_home_room: 5,
            class_day_balance: 5,
            ..ConstraintWeights::default()
        };
        let config = SolveConfig {
            seed,
            weights,
            deadline: Some(std::time::Duration::from_millis(50)),
            max_iterations: Some(2_000),
            ..SolveConfig::default()
        };
        let (solution, _stats) = solver_core::solve_with_config_stats(&problem, &config);
        let canonical = score_solution(&problem, &solution.placements, &config.weights);
        prop_assert_eq!(solution.soft_score, canonical);
    }
}
```

If `lahc_small_problem` (referenced from existing tests) does not produce non-zero home_room and day_balance costs, augment the generator's `school_classes` to set `home_room_id = Some(rooms[0].id)` and add at least two days so day_balance can be non-zero. Verify by inserting an assertion on the post-greedy canonical inside the test if necessary; the test must not pass trivially with zero costs.

- [ ] **Step 15: Run the new test and the existing suite**

Run: `cargo nextest run -p solver-core`
Expected: all tests pass, including the new helper unit tests in score.rs and the new lahc_property.rs proptest.

Five-seed sweep for the property test:

Run: `for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property; done`
Expected: every iteration green.

- [ ] **Step 16: Run lint**

Run: `mise run lint:rust`
Expected: green.

- [ ] **Step 17: Commit**

```bash
git add solver/solver-core/src/score.rs solver/solver-core/src/solve.rs solver/solver-core/src/lahc.rs solver/solver-core/tests/lahc_property.rs
git commit -m "feat(solver-core): track canonical objective on GreedyState (item 52 prep)"
```

---

## Task 3: LAHC accepts and exits on the canonical objective + best-snapshot

**Files:**
- Modify: `solver/solver-core/src/lahc.rs` (lahc_list, accept criterion, time_to_optimal, early-exit, snapshot)
- Modify: `solver/solver-core/src/quality.rs` (`build_backend_objectives`)
- Modify: `solver/solver-core/tests/lahc_property.rs` (add two new tests)

This is the only behaviour change. After this task, LAHC accepts on canonical, returns the running-best canonical placements, and the backend declarations reflect the wider optimised set.

- [ ] **Step 1: Write the failing non-increasing test**

In `solver/solver-core/tests/lahc_property.rs` add:

```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// LAHC must never return an incumbent whose canonical score
    /// exceeds the post-greedy canonical. Pinned via the running-best
    /// snapshot in lahc::run: `best_placements` is initialised to the
    /// post-greedy placements and only swapped on canonical-strict-
    /// improvement events.
    #[test]
    fn lahc_canonical_score_is_non_increasing_versus_greedy_under_production_weights(
        seed in 0u64..1024,
    ) {
        let problem = lahc_small_problem();
        let weights = solver_core::PRODUCTION_ACTIVE_WEIGHTS;
        // Greedy-only (deadline None short-circuits LAHC).
        let greedy_config = SolveConfig {
            seed,
            weights,
            deadline: None,
            ..SolveConfig::default()
        };
        let (greedy_solution, _) = solver_core::solve_with_config_stats(&problem, &greedy_config);
        // Greedy + LAHC.
        let lahc_config = SolveConfig {
            seed,
            weights,
            deadline: Some(std::time::Duration::from_millis(200)),
            max_iterations: Some(2_000),
            ..SolveConfig::default()
        };
        let (lahc_solution, _) = solver_core::solve_with_config_stats(&problem, &lahc_config);
        prop_assert!(
            lahc_solution.soft_score <= greedy_solution.soft_score,
            "LAHC canonical {} exceeds greedy canonical {} on seed {}",
            lahc_solution.soft_score,
            greedy_solution.soft_score,
            seed,
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails (or passes by accident)**

Run: `cargo nextest run -p solver-core --test lahc_property lahc_canonical_score_is_non_increasing_versus_greedy_under_production_weights`
Expected: a fraction of seeds FAIL because LAHC accepts on slice and may return canonical-worse placements.

If it passes by accident on the small fixture, augment `lahc_small_problem` to give LAHC enough room to drift canonical (e.g. set `school_classes[0].home_room_id` to a room that conflicts with one of the lessons' suitable rooms so LAHC must compromise on home_room). The point is to prove the snapshot mechanism is necessary.

- [ ] **Step 3: Switch lahc_list, accept, time_to_optimal, early-exit to canonical**

In `solver/solver-core/src/lahc.rs::run`, replace:

```rust
    let mut lahc_list = vec![state.search_score_slice; LAHC_LIST_LEN];
```

with:

```rust
    let mut lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
```

Replace `let mut running_best = state.search_score_slice;` with `let mut running_best = state.canonical_score;`.

Replace the per-iteration tail:

```rust
        lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.search_score_slice;
        if stats.time_to_first_feasible_ms.is_none()
            && state.search_score_slice == 0
            && placements.len() == placements_expected
        {
            stats.time_to_first_feasible_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }
        if state.search_score_slice < running_best {
            running_best = state.search_score_slice;
            stats.time_to_optimal_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }
        if state.search_score_slice == 0 && placements.len() == placements_expected {
            break;
        }
```

with:

```rust
        lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.canonical_score;
        if stats.time_to_first_feasible_ms.is_none()
            && state.canonical_score == 0
            && placements.len() == placements_expected
        {
            stats.time_to_first_feasible_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }
        if state.canonical_score < running_best {
            running_best = state.canonical_score;
            best_placements = placements.clone();
            stats.time_to_optimal_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }
        if state.canonical_score == 0 && placements.len() == placements_expected {
            break;
        }
```

Add `let mut best_placements: Vec<Placement> = placements.clone();` immediately after the `let mut running_best = state.canonical_score;` line.

Add the restore at the natural exit of the `while iter < max_iter && solve_start.elapsed() < deadline { ... }` loop:

```rust
    *placements = best_placements;
```

The placement-set restore is unconditional. If LAHC ran zero accepted moves, `best_placements` equals the entry `placements` and the assignment is a self-copy (cheap).

- [ ] **Step 4: Switch the Change-move acceptance to canonical**

In `try_change_move`, replace:

```rust
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let accept = new_score <= state.search_score_slice || new_score <= prior;
```

with:

```rust
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let accept = new_canonical <= state.canonical_score || new_canonical <= prior;
```

The slice score (`new_score`) and the slice non-negative `debug_assert!` stay untouched. The slice continues to ride along with the move; only the accept gate changes.

- [ ] **Step 5: Switch the R&R acceptance to canonical**

In `rr_attempt`, replace the post-recreate accept block:

```rust
    let new_score =
        running_slice_from_placements(problem, placements, weights, max_position_per_day);
    state.search_score_slice = new_score;
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let lahc_ok = new_score <= pre_slice || new_score <= prior;
```

with:

```rust
    let new_slice =
        running_slice_from_placements(problem, placements, weights, max_position_per_day);
    state.search_score_slice = new_slice;
    let new_canonical = crate::score::score_solution(problem, placements, weights);
    state.canonical_score = new_canonical;
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let lahc_ok = new_canonical <= pre_canonical || new_canonical <= prior;
```

On rollback (the `if !lahc_ok` branch), in addition to `state.search_score_slice = pre_slice;` already present, restore `state.canonical_score = pre_canonical;`.

- [ ] **Step 6: Switch the Kempe acceptance to canonical**

In `kempe_attempt`, replace:

```rust
    let new_score_signed = i64::from(pre_slice) + total_delta;
    let new_score = u32::try_from(new_score_signed.max(0)).unwrap_or(u32::MAX);
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let lahc_ok = new_score <= pre_slice || new_score <= prior;
```

with:

```rust
    let new_slice_signed = i64::from(pre_slice) + total_delta;
    let new_slice = u32::try_from(new_slice_signed.max(0)).unwrap_or(u32::MAX);
    let new_canonical_signed = i64::from(pre_canonical) + canonical_delta;
    let new_canonical = u32::try_from(new_canonical_signed.max(0)).unwrap_or(u32::MAX);
    let prior = lahc_list[(iter as usize) % LAHC_LIST_LEN];
    let lahc_ok = new_canonical <= pre_canonical || new_canonical <= prior;
```

(`canonical_delta` was computed in Task 2 Step 11.)

On accept: `state.search_score_slice = new_slice; state.canonical_score = new_canonical;`. On rollback: restore both.

- [ ] **Step 7: Update `build_backend_objectives` declarations**

In `solver/solver-core/src/quality.rs::build_backend_objectives`:

Replace:

```rust
    let lahc_optimised: BTreeSet<QualityComponent> = [
        ClassGap,
        TeacherGap,
        PreferEarly,
        AvoidFirst,
        AvoidLast,
        PreferLate,
    ]
    .into_iter()
    .collect();
    let lahc_skipped: BTreeSet<QualityComponent> =
        [HomeRoom, ClassDayBalance].into_iter().collect();
    let lahc_notes = "LAHC slice is class_gap + teacher_gap + subject_pref \
                      (see solve.rs:291-292); item 52 widens it; item 54 \
                      adds class-day-balance to the search hot path.";
```

with:

```rust
    let lahc_optimised: BTreeSet<QualityComponent> =
        QualityComponent::ALL.iter().copied().collect();
    let lahc_skipped: BTreeSet<QualityComponent> = BTreeSet::new();
    let lahc_notes = "LAHC accepts and exits on the full canonical (see lahc::run); \
                      item 54 reserved for FFD greedy-time class-day-balance tiebreak.";
```

Update the `lahc_rr` notes string to "Inherits LAHC's canonical objective; R&R recreate ranks rooms by home_room delta after item 49." and the `lahc_rr_kempe` row to keep using `lahc_optimised` / `lahc_skipped`.

- [ ] **Step 8: Add the targeted running-best snapshot test**

In `solver/solver-core/tests/lahc_property.rs` add:

```rust
#[test]
fn lahc_returns_running_best_canonical_when_search_drifts() {
    // Hand-built fixture: a one-class problem where a Change move can
    // strictly decrease slice while strictly increasing canonical
    // (home_room delta dominates). With a tight deadline that fires
    // during the drift, the running-best snapshot must restore the
    // pre-drift placements.
    let problem = build_drift_fixture();
    let weights = solver_core::PRODUCTION_ACTIVE_WEIGHTS;
    let config = SolveConfig {
        seed: 0,
        weights,
        deadline: Some(std::time::Duration::from_millis(20)),
        max_iterations: Some(1_000),
        ..SolveConfig::default()
    };
    let (lahc_solution, _) = solver_core::solve_with_config_stats(&problem, &config);
    // Greedy comparison
    let greedy_config = SolveConfig {
        deadline: None,
        ..config
    };
    let (greedy_solution, _) =
        solver_core::solve_with_config_stats(&problem, &greedy_config);
    assert!(
        lahc_solution.soft_score <= greedy_solution.soft_score,
        "lahc {} > greedy {}",
        lahc_solution.soft_score,
        greedy_solution.soft_score,
    );
}

fn build_drift_fixture() -> Problem {
    // Concrete fixture construction. Use the existing
    // build_class_day_balance_problem skeleton from score_property.rs as
    // a starting point; copy it inline here (test files cannot share a
    // module without a tests/common/mod.rs scaffold). Add a non-default
    // `home_room_id` for the class plus a feasible alternative room so
    // LAHC can pick a non-home room and pay the home_room penalty.
    todo!("inline fixture: copy build_class_day_balance_problem from score_property.rs and set school_classes[0].home_room_id = Some(rooms[0].id) plus rooms[1] as suitable for the same subjects")
}
```

The `todo!()` is intentional in the plan: the implementing subagent expands `build_drift_fixture` inline at write time so the test compiles. The fixture matches `score_property::build_class_day_balance_problem` shape (one class, four days, six lessons) and adds a home_room field for the class. Verify the fixture exhibits drift potential by running the test once with `--show-output` and checking that the move sequence accepted at least one home_room-worsening move; if it does not exhibit drift, augment the fixture (more lessons, more rooms, larger home_room weight).

- [ ] **Step 9: Run all property tests**

Run: `cargo nextest run -p solver-core --test lahc_property`
Expected: all tests green, including the previously failing non-increasing test.

Run the five-seed property sweep to ensure no flake:

Run: `for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property; done`
Expected: every iteration green.

- [ ] **Step 10: Run the full test suite + lint**

Run: `mise run test:rust && mise run lint:rust`
Expected: green.

- [ ] **Step 11: Run the criterion bench against the committed BASELINE**

Run: `mise run bench`
Expected: criterion's reported delta on each fixture (`solver_greedy/grundschule`, `solver_lahc/grundschule`) within the 20% budget. If a fixture exceeds the budget, halt before commit and triage; the most likely culprit is the Change-move home_room_delta or class_day_balance_delta arithmetic. Mitigation: short-circuit the deltas when the corresponding weight is zero (already in the design), or precompute `days` once at the top of `lahc::run` instead of per-move.

- [ ] **Step 12: Commit**

```bash
git add solver/solver-core/src/lahc.rs solver/solver-core/src/quality.rs solver/solver-core/tests/lahc_property.rs
git commit -m "feat(solver-core): LAHC accepts and exits on canonical objective (item 52)"
```

---

## Task 4: Documentation refresh

**Files:**
- Modify: `solver/CLAUDE.md` (slice/canonical split, snapshot mechanism, item 54 cross-ref)
- Modify: `docs/superpowers/OPEN_THINGS.md` (delete item 52)
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md` (frontmatter description and body)
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/MEMORY.md` (one-line index entry refresh)

- [ ] **Step 1: Update `solver/CLAUDE.md`**

Add a bullet under the solver-core rules:

```markdown
- **`GreedyState` carries two scores: `search_score_slice` and `canonical_score`.** The slice (`class_gap + teacher_gap + subject_pref`) is the LAHC search-time hot-path objective: greedy's `try_place_block` picker persists slice into `state.search_score_slice`; the Change-move non-negative-delta `debug_assert!` checks slice; `running_slice_from_placements` recomputes slice for R&R; Kempe's snapshot+delta walks slice. The canonical (full `score_solution`, slice + `prefer_home_room` + `class_day_balance`) is the LAHC accept-time objective: `lahc_list` stores canonical; the accept criterion compares canonical against current and prior; `time_to_optimal_ms` fires on canonical improvement; the early-exit predicate fires at `state.canonical_score == 0`. Both fields are maintained in lockstep across all three moves (Change via incremental delta, R&R via full `score_solution` recompute, Kempe via snapshot + delta extension to per-class day-counts and per-row home_room contributions). Changes that touch one score must keep the other consistent or the per-iteration `debug_assert_eq!(state.canonical_score, score::score_solution(...))` in test builds will fire.
```

Under the LAHC outer-loop bullet, add:

```markdown
- **LAHC returns the running-best canonical incumbent, not the wandering current.** `lahc::run` snapshots `best_placements: Vec<Placement>` at loop entry (initialised to the post-greedy placements) and refreshes the snapshot on every running-best canonical event. On loop exit (deadline / max_iter / early-exit), `*placements = best_placements`. This guarantees `solution.soft_score <= greedy_solution.soft_score` even when the LAHC accept criterion would otherwise allow drift above the entry incumbent. Pinned by `lahc_canonical_score_is_non_increasing_versus_greedy_under_production_weights` in `tests/lahc_property.rs`. Item 52.
```

Update the existing "LAHC outer loop exits at the objective floor" bullet to reference `state.canonical_score` instead of `state.soft_score`.

- [ ] **Step 2: Update `docs/superpowers/OPEN_THINGS.md`**

Delete the item 52 entry. Per the autopilot rules, do not leave a "Shipped" marker; the PR description and `git log` are the canonical record. Update the "Next pickup" header to point at the next P0 (item 48 CP-SAT objective parity, then item 47 ADR 0035) per the sprint-tidy phase rotor. Update the `## Sprint-tidy phase` lead paragraph if it referenced item 52 explicitly.

- [ ] **Step 3: Refresh auto-memory**

Update `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`:
- YAML frontmatter `description` field: replace the current next-pickup name with item 48 / item 47 (whatever the new top of the queue is) and bump the date to 2026-05-07.
- Body: add a bullet "Item 52 shipped 2026-05-07" with the headline of what landed (slice/canonical split, LAHC accept on canonical, running-best snapshot, backend declarations widened).

Update `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/MEMORY.md` index entry for `project_roadmap_status.md` so the one-line hook names the new next-pickup.

- [ ] **Step 4: Lint**

Run: `mise run lint`
Expected: green. The full lint catches markdown formatting drift if any (no markdown linters configured today, but `actionlint` and the commit-types check could flag adjacent edits).

- [ ] **Step 5: Commit**

```bash
git add solver/CLAUDE.md docs/superpowers/OPEN_THINGS.md /home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md /home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/MEMORY.md
git commit -m "docs: refresh slice/canonical notes (item 52)"
```

---

## Self-review checklist

After Task 4 commits:

1. `mise run test` green (all three test runners).
2. `mise run lint` green.
3. `mise run bench` reports every fixture within the 20% criterion budget vs `BASELINE.md`. If a fixture is borderline, refresh `BASELINE.md` only with explicit user approval; the algorithm change of item 52 is *not* a perf change so a refresh would be irregular.
4. Five-seed proptest sweep on `lahc_property` green (`for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property; done`).
5. `git log --oneline master..HEAD` shows exactly four commits with conventional commit prefixes (`refactor`, `feat`, `feat`, `docs`).
6. `rg -n 'state\.soft_score' solver/` finds nothing (rename complete).
7. `rg -n 'state\.canonical_score' solver/` finds the field, the initialisation, and lockstep maintenance in all three moves plus the LAHC tail.
