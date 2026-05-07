# Item 60 Strict-`<` Window Tiebreak Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tighten the FFD greedy `try_place_block` window-level best-candidate selection from "LAST-walked feasible candidate wins on `total_score` ties" to "FIRST-walked feasible candidate wins (strict `<`)", mirroring the existing room-scan rule and `tb_order`'s `(day, position, tb_id)` sort.

**Architecture:** One Rust assignment in `solver/solver-core/src/solve.rs` is wrapped in an `is_none_or(|b| total_score < b.total_score)` guard. The single existing test that depended on the LAST-walked behavior tightens its assertion plus docstring; the surrounding pruning-comment cluster and the `solver/CLAUDE.md` paragraph that documented the LAST-walked behavior get rewritten to describe the strict-`<` rule.

**Tech Stack:** Rust 2021 (workspace MSRV 1.85), `cargo nextest`, `mise run lint`, `mise run bench:bakeoff`. No PyO3 / Python / TypeScript / SQL touched.

---

## File Structure

Two files modified, no files created.

- **Modify** `solver/solver-core/src/solve.rs`:
  - The picker assignment block at lines 782-790 (one block, four lines added).
  - The pruning-comment cluster at lines 622-625 (one comment paragraph rewritten).
  - The test docstring at lines 2940-2964 plus the assertions at lines 3135-3175 (one docstring paragraph + one `assert!` + one `assert_ne!`).
- **Modify** `solver/CLAUDE.md`:
  - The paragraph starting **"`try_place_block`'s window-level picker overwrites `best` unconditionally on every non-pruned candidate."** Rewrite to describe the strict-`<` rule and the symmetry with the room scan; remove the "preserve, revisit only if a bench cell..." escape hatch.

The change ships as a single Conventional Commit on branch `refactor/solve-strict-window-tiebreak`. The smoke bench is post-commit verification, not a tracked artifact.

---

## Task 1: Tighten the existing balance-on test (red phase)

**Files:**
- Modify: `solver/solver-core/src/solve.rs:2940-2964` (docstring), `solver/solver-core/src/solve.rs:3135-3175` (assertions inside `try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight`)
- Test: same file, same test (`cargo nextest run -p solver-core --lib solve::tests::try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight`)

- [ ] **Step 1: Rewrite the test docstring**

The existing docstring at `solve.rs:2940-2963` (a single `///`-prefixed block ending immediately before `#[test]`) currently explains the LAST-walked-wins reasoning. Replace it with the strict-`<` reasoning. Find this block:

```rust
    /// FFD greedy's window picker must respond to `weights.class_day_balance`
    /// (item 54). Build a 1-class, 4-day fixture with three lessons pinned
    /// onto day 0 (positions 0, 1, 2) and one lesson pinned onto day 1
    /// (position 0). Pre-FFD per-class day counts are 3/1/0/0. FFD places one
    /// remaining lesson; eligible windows are day 1 pos 1, day 2 pos 0, and
    /// day 3 pos 0.
    ///
    /// Baseline (`class_day_balance == 0`): every weight is zero, so every
    /// candidate scores 0; the picker hits the early-exit at the first
    /// feasible window (`total_score == state.search_score_slice == 0`),
    /// landing lesson_e on day 1 (tb_d1_p1, the earliest non-busy tb in
    /// `tb_order`). Balance-on (`class_day_balance == 5`): no early-exit
    /// fires (totals are non-zero), so the picker walks every feasible
    /// window. Day 1 yields 3/2/0/0 with per-class L1 cost 5 (total = 25);
    /// day 2 yields 3/1/1/0 with cost 3 (total = 15); day 3 yields 3/1/0/1
    /// with cost 3 (total = 15). The picker's pruning rule fires only when
    /// the slice lower bound is at least the current best total; with
    /// `slice_score = 0` for every window the rule never fires, and the
    /// BlockCandidate assignment overwrites best on each non-pruned window.
    /// The last-walked feasible window wins, which is day 3. The contract
    /// the test pins is that balance-on lands on a strictly different day
    /// than balance-off, and that day's post-place class_day_balance cost
    /// is strictly lower than balance-off's day (day 3 cost 3 vs day 1
    /// cost 5).
```

Replace with:

```rust
    /// FFD greedy's window picker must respond to `weights.class_day_balance`
    /// (item 54). Build a 1-class, 4-day fixture with three lessons pinned
    /// onto day 0 (positions 0, 1, 2) and one lesson pinned onto day 1
    /// (position 0). Pre-FFD per-class day counts are 3/1/0/0. FFD places one
    /// remaining lesson; eligible windows are day 1 pos 1, day 2 pos 0, and
    /// day 3 pos 0.
    ///
    /// Baseline (`class_day_balance == 0`): every weight is zero, so every
    /// candidate scores 0; the picker hits the early-exit at the first
    /// feasible window (`total_score == state.search_score_slice == 0`),
    /// landing lesson_e on day 1 (tb_d1_p1, the earliest non-busy tb in
    /// `tb_order`). Balance-on (`class_day_balance == 5`): no early-exit
    /// fires (totals are non-zero), so the picker walks every feasible
    /// window. Day 1 yields 3/2/0/0 with per-class L1 cost 5 (total = 25);
    /// day 2 yields 3/1/1/0 with cost 3 (total = 15); day 3 yields 3/1/0/1
    /// with cost 3 (total = 15). The picker's pruning rule fires only when
    /// the slice lower bound is at least the current best total; with
    /// `slice_score = 0` for every window the rule never fires. Under the
    /// strict-`<` cross-window comparison (item 60), the picker walks day 1
    /// first (best total = 25), then day 2 (15 < 25, becomes best), then
    /// day 3 (15 < 15 is false, day 2 keeps the lead). The contract the
    /// test pins is that balance-on lands deterministically on day 2: the
    /// FIRST-walked window of the cost-3 tier, mirroring the room-scan's
    /// "lowest-id wins on tie" rule via `tb_order`'s
    /// `(day_of_week, position, tb_id)` sort.
```

- [ ] **Step 2: Tighten the assertions**

At `solve.rs:3156-3175` the test currently asserts:

```rust
        assert_ne!(
            placement_on_e.time_block_id, tb_d1_p1,
            "balance-on (class_day_balance=5): picker must NOT pile lesson_e onto day 1; \
             expected an L1-spread-minimising candidate (day 2 or day 3)"
        );
        // Verify the post-place class_day_balance cost on the chosen day is
        // strictly lower than the baseline's day-1 cost. Day 1 baseline
        // yields 3/2/0/0 (cost 5); day 2 or day 3 yields cost 3.
        let chosen_tb = placement_on_e.time_block_id;
        let chosen_day = problem
            .time_blocks
            .iter()
            .find(|tb| tb.id == chosen_tb)
            .expect("chosen tb must resolve")
            .day_of_week;
        assert!(
            chosen_day == 2 || chosen_day == 3,
            "balance-on: picker must land lesson_e on day 2 or day 3 (post-place L1 cost 3 < day-1 cost 5); \
             actual day = {chosen_day}"
        );
```

Replace with:

```rust
        assert_ne!(
            placement_on_e.time_block_id, tb_d1_p1,
            "balance-on (class_day_balance=5): picker must NOT pile lesson_e onto day 1; \
             expected the FIRST-walked L1-spread-minimising candidate (day 2 under strict `<`)"
        );
        // Verify the chosen day is exactly day 2 (the FIRST-walked window of
        // the cost-3 tier). Day 1 baseline yields 3/2/0/0 (cost 5); day 2
        // and day 3 both yield cost 3, but strict `<` resolves the tie to
        // day 2 because tb_order is sorted by (day, position, tb_id).
        let chosen_tb = placement_on_e.time_block_id;
        let chosen_day = problem
            .time_blocks
            .iter()
            .find(|tb| tb.id == chosen_tb)
            .expect("chosen tb must resolve")
            .day_of_week;
        assert_eq!(
            chosen_day, 2,
            "balance-on: picker must land lesson_e on day 2 (FIRST-walked of the tied cost-3 candidates under strict `<`); \
             actual day = {chosen_day}"
        );
```

- [ ] **Step 3: Run the test to verify it fails on master code**

```
cargo nextest run -p solver-core --lib solve::tests::try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight
```

Expected: FAIL on the new `assert_eq!(chosen_day, 2, ...)` because the LAST-walked overwrite still picks day 3. Failure message will read along the lines of `assertion `left == right` failed: ... actual day = 3`.

If the test passes here, the picker is already strict-`<` somehow (it should not be; verify via `git diff` that no other change leaked into the test) and the plan must be re-examined.

- [ ] **Step 4: Do not commit yet.**

The red test is staged in the working tree but not committed. Task 2 will commit it together with the production code change.

---

## Task 2: Apply the strict-`<` guard (green phase) and rewrite the surrounding doc comments

**Files:**
- Modify: `solver/solver-core/src/solve.rs:782-790` (the `BlockCandidate` assignment block)
- Modify: `solver/solver-core/src/solve.rs:622-625` (the pruning-comment cluster)

- [ ] **Step 1: Replace the unconditional assignment with the strict-`<` guard**

At `solve.rs:782-790` the existing block is:

```rust
        best = Some(BlockCandidate {
            outer_pos,
            day: first_tb.day_of_week,
            start_pos,
            end_pos,
            room_id,
            slice_score,
            total_score,
        });
```

Replace with:

```rust
        // Strict `<` cross-window comparison (item 60). FIRST-walked feasible
        // window wins on tied `total_score`; combined with `tb_order`'s sort
        // by `(day_of_week, position, tb_id)` this resolves to "lowest
        // `(day, position)` wins on tie", symmetric to the room scan's
        // "lowest `room.id` wins on tie" rule above.
        if best.as_ref().is_none_or(|b| total_score < b.total_score) {
            best = Some(BlockCandidate {
                outer_pos,
                day: first_tb.day_of_week,
                start_pos,
                end_pos,
                room_id,
                slice_score,
                total_score,
            });
        }
```

- [ ] **Step 2: Re-anchor the pruning-comment cluster**

At `solve.rs:620-625` the existing comment ends with the line "Tiebreak (day, start_pos, room.id) preserved via strict `<` and tb_order's sort." That phrasing was written when the cross-window comparison was *not* strict-`<`. After this change it is honest top-to-bottom, but worth being explicit. Find this block:

```rust
        // Pruning: skip the room scan if this window's slice-score lower bound
        // cannot beat the current best total. `home_room_penalty >= 0` for every
        // (window, room), so slice is a sound lower bound on total; a window
        // whose slice already exceeds the best total cannot produce a strictly
        // better candidate. Tiebreak (day, start_pos, room.id) preserved via
        // strict `<` and tb_order's sort.
```

Replace with:

```rust
        // Pruning: skip the room scan if this window's slice-score lower bound
        // cannot beat the current best total. `home_room_penalty >= 0` and
        // `class_day_balance_post >= 0` for every (window, room), so slice is
        // a sound lower bound on total; a window whose slice already
        // exceeds-or-ties the best total cannot produce a strictly better
        // candidate. The cross-window comparison at the BlockCandidate
        // assignment site (item 60) is strict `<` end-to-end: FIRST-walked
        // wins on tied `total_score` via `tb_order`'s sort by
        // `(day_of_week, position, tb_id)`, symmetric to the room scan's
        // "lowest `room.id` wins on tie" rule.
```

- [ ] **Step 3: Run the targeted test to verify it now passes**

```
cargo nextest run -p solver-core --lib solve::tests::try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight
```

Expected: PASS. The picker walks day 1 first (best total = 25), day 2 (15 < 25, best), day 3 (15 < 15 is false, day 2 stays). `chosen_day == 2`.

- [ ] **Step 4: Run the full solver-core suite**

```
cargo nextest run -p solver-core
```

Expected: every test passes. In particular:

- `try_place_block_room_picker_minimises_home_room_penalty` and `try_place_block_room_picker_falls_back_to_id_order_when_no_home_room_advantage` pass (single-window fixtures, cross-window comparison never engages).
- `lahc_property` suite passes (determinism under seed + iter cap, placement-count never decreases for R&R / Kempe, canonical-non-increasing).
- `score_property`, `grundschule_smoke`, `ffd_solver_outcome`, `rr_anchor_filter` pass.

If any other test relies on the LAST-walked-wins outcome, fix it in this commit only after confirming it pinned a tb_id outcome (which the spec explicitly checked and found none).

---

## Task 3: Rewrite the `solver/CLAUDE.md` paragraph that documented the LAST-walked behavior

**Files:**
- Modify: `solver/CLAUDE.md` (one paragraph, the bullet starting **"`try_place_block`'s window-level picker overwrites `best` unconditionally on every non-pruned candidate."**)

- [ ] **Step 1: Replace the LAST-walked paragraph with the strict-`<` paragraph**

Find this bullet in `solver/CLAUDE.md`:

```markdown
- **`try_place_block`'s window-level picker overwrites `best` unconditionally on every non-pruned candidate.** The strict `<` documented elsewhere is for the room scan within a window (lowest `home_room_penalty` wins), not for the cross-window comparison. The pruning `if slice_score >= b.total_score { continue; }` filters which windows are considered, but among the survivors the LAST-walked feasible candidate wins on tied total_score (because tb_order is sorted by `(day, position, id)` and the iteration runs to completion when early-exit doesn't fire). Picker tests that assert an exact tb-id outcome are brittle to fixture ordering and to the early-exit fire pattern; assert behavioral outcomes ("balance-on lands on a strictly different day", "post-place class_day_balance strictly lower") instead. Item 54 surfaced this; preserve, revisit only if a bench cell shows it as load-bearing.
```

Replace with:

```markdown
- **`try_place_block`'s window-level picker uses strict `<` end-to-end.** Both the room scan within a window (lowest `home_room_penalty` wins, lowest `room.id` on tie via `room_order`'s id sort) and the cross-window comparison (lowest `total_score` wins, lowest `(day, position, tb_id)` on tie via `tb_order`'s sort) resolve ties to the FIRST-walked candidate. The pruning bound `if slice_score >= b.total_score { continue; }` filters which windows are considered before the comparison runs; among the survivors strict `<` keeps the FIRST-walked candidate. Item 60 brought the cross-window site in line with the room scan; before item 60 the cross-window assignment was unconditional and the LAST-walked feasible candidate won on tied `total_score`. Targeted picker tests can now assert exact `(day, position)` outcomes on tied candidates as long as `tb_order` is the canonical sort.
```

- [ ] **Step 2: Stage everything and verify the diff**

```
git status
git diff --stat
git diff solver/solver-core/src/solve.rs solver/CLAUDE.md
```

Expected: two files changed (`solver/solver-core/src/solve.rs` and `solver/CLAUDE.md`); roughly +20 / -10 lines on `solve.rs` and +1 / -1 paragraph on `CLAUDE.md`.

- [ ] **Step 3: Run `mise run lint`**

```
mise run lint
```

Expected: green. The change preserves clippy `-D warnings`, fmt, machete, the unique-fns check, and ty / vulture / biome / actionlint (none of which touch solver-core).

- [ ] **Step 4: Commit**

```
git add solver/solver-core/src/solve.rs solver/CLAUDE.md
mise exec -- git commit -m "refactor(solver-core): strict < on window-level picker tiebreak (item 60)"
```

Expected: cog approves the conventional message; pre-commit hook re-runs `mise run lint` and passes.

---

## Task 4: Smoke bench (verification, not a commit)

**Files:**
- None modified.

- [ ] **Step 1: Run the smoke bench on the feature branch**

```
mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig --out /tmp/strict-lt-branch.md
```

Expected wall-clock ~30 seconds. The cpsat column will report `ModuleNotFoundError` because `mise run solver:rebuild` did not run; that noise is acceptable per `solver/CLAUDE.md` (the change is Rust-only and the LAHC cells are independent of the wheel).

- [ ] **Step 2: Run the same smoke on master for side-by-side comparison**

```
git stash --include-untracked
git checkout master
mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig --out /tmp/strict-lt-master.md
git checkout refactor/solve-strict-window-tiebreak
git stash pop || true
```

Expected wall-clock ~30 seconds again. (The stash-pop branch handles the case where the earlier commit cleared the working tree; harmless if there's nothing to pop.)

- [ ] **Step 3: Diff the two smoke outputs and capture the result for the PR body**

```
diff -u /tmp/strict-lt-master.md /tmp/strict-lt-branch.md > /tmp/strict-lt-smoke.diff || true
cat /tmp/strict-lt-smoke.diff
```

Expected: feasibility (20/20 → 20/20), `worst_spread`, and `home_room_miss` columns on the `lahc`, `lahc_rr`, and `lahc_rr_kempe` rows do not regress on either fixture. Soft scores can shift slightly because day 2 and day 3 produce identical canonical scores on the targeted fixture, but real fixtures may break ties differently and the FIRST-walked outcome is not strictly score-preserving on every cell. Acceptance for the smoke is "feasibility is preserved and `worst_spread` / `home_room_miss` do not regress"; soft-score deltas in either direction are tolerated within the 5s × 4-seeds noise floor.

- [ ] **Step 4: Save the diff for the PR body**

```
cp /tmp/strict-lt-smoke.diff /tmp/strict-lt-smoke-pr.diff
```

The PR body cites the diff verbatim under a "Smoke bench" section.

---

## Self-review

**Spec coverage.** Each spec section maps to a task:

- "Replace the unconditional assignment" — Task 2 step 1.
- "Tighten existing balance-on test plus docstring" — Task 1 steps 1-2.
- "Re-anchor the pruning-comment cluster" — Task 2 step 2.
- "Rewrite the solver/CLAUDE.md paragraph" — Task 3 step 1.
- "Smoke bench at the end" — Task 4.
- "OPEN_THINGS item 60 deletion" — handled in step 6 of /autopilot (finalize docs); not a code task.

**Placeholder scan.** No "TBD", "TODO", "implement later", or "fill in details" in the task bodies. Every step has either an exact command, a code block, or both.

**Type consistency.** No new types introduced. The single `Option::is_none_or` call exists at workspace MSRV 1.85 (stable since Rust 1.82). The `BlockCandidate` literal is unchanged in structure.

**Risk re-check.** Task 2 step 4 runs the full `solver-core` suite as a backstop in case any test other than the documented one pinned the LAST-walked-wins outcome. The brainstorm pre-checked all picker tests and found none, but the suite gate confirms.

**Skill choice for execution.** The plan has four sequential tasks that all touch `solver/solver-core/src/solve.rs` (shared state). Per the autopilot rule "Tasks that share state ... dispatch one agent at a time", these run as four sequential subagents. Task 4's smoke bench can run in the main session because it is purely verification with no edits.
