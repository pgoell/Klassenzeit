# Item 53: validate_daily_caps as production post-condition — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote `validate_daily_caps` from a `#[cfg(debug_assertions)]` panic to a release-mode `?` post-condition next to `validate_no_room_hopping` and `validate_no_double_booking`, drop the unreachable debug-only `validate_no_double_booking` panic block, and pin the new contract with a property test.

**Architecture:** Three-commit PR on `feat/solver-validate-daily-caps-production` (current branch). Each commit is independent and `git bisect`-friendly: a structural cleanup, then a forward-guard test, then the behavioral fix bundled with its `solver/CLAUDE.md` doc sync. Spec at `docs/superpowers/specs/2026-05-07-item-53-validate-daily-caps-production-design.md`.

**Tech Stack:** Rust (`solver-core` crate), `cargo nextest`, `proptest 1.x` (`PROPTEST_CASES` / `PROPTEST_SEED` env vars for the local widening sweep), `mise` task runner.

---

## File Structure

- **Modify:** `solver/solver-core/src/solve.rs` (lines 291-305 region; the post-condition validator block at the tail of `solve_with_config_stats`)
- **Modify:** `solver/solver-core/tests/lahc_property.rs` (widen one import; add one `proptest!` body next to `lahc_rr_kempe_does_not_double_book_class`)
- **Modify:** `solver/CLAUDE.md` (drop one bullet, edit one bullet)

No new files.

---

## Task 1: Drop unreachable debug-only `validate_no_double_booking` panic block

**Files:**
- Modify: `solver/solver-core/src/solve.rs:291-305`

**Why:** The release-mode `validate_no_double_booking(problem, &solution.placements)?;` at line 293 returns `Err(Error::Input)` on any failure, propagating up via `?`. The `#[cfg(debug_assertions)]` panic block at lines 302-305 sits below it, so in debug builds the `?` runs first and the panic block is unreachable; in release builds the panic block is compiled out. Pure dead code.

- [ ] **Step 1: Read the current block**

Read `solver/solver-core/src/solve.rs` lines 291-305 to confirm the layout matches what the spec describes:

```rust
    // Post-solve hard-constraint sanity check. A failure here is a solver bug.
    validate_no_room_hopping(problem, &solution.placements)?;
    validate_no_double_booking(problem, &solution.placements)?;

    // Debug-only post-condition: daily caps (ADR 0033) are enforced as
    // legality pruning, so a violation here means the pruning has a hole.
    // Loud in dev/tests, free in release.
    #[cfg(debug_assertions)]
    if let Err(e) = validate_daily_caps(problem, &solution.placements) {
        panic!("daily-cap post-condition violated: {e}");
    }
    #[cfg(debug_assertions)]
    if let Err(e) = validate_no_double_booking(problem, &solution.placements) {
        panic!("no-double-booking post-condition violated: {e}");
    }
```

- [ ] **Step 2: Apply the edit**

Use the Edit tool to delete only the redundant `validate_no_double_booking` debug block (lines 302-305 in the snippet above). Leave the `validate_daily_caps` debug block intact for now (Task 3 handles it).

After the edit, the file should read:

```rust
    // Post-solve hard-constraint sanity check. A failure here is a solver bug.
    validate_no_room_hopping(problem, &solution.placements)?;
    validate_no_double_booking(problem, &solution.placements)?;

    // Debug-only post-condition: daily caps (ADR 0033) are enforced as
    // legality pruning, so a violation here means the pruning has a hole.
    // Loud in dev/tests, free in release.
    #[cfg(debug_assertions)]
    if let Err(e) = validate_daily_caps(problem, &solution.placements) {
        panic!("daily-cap post-condition violated: {e}");
    }
```

- [ ] **Step 3: Verify build + tests**

Run: `cargo nextest run -p solver-core`
Expected: PASS (no behavior changed; the deleted block was unreachable).

Also run: `mise run lint`
Expected: PASS (clippy + cargo fmt + cargo machete all clean).

- [ ] **Step 4: Commit**

```bash
git add solver/solver-core/src/solve.rs
git commit -m "refactor(solver-core): drop unreachable debug-only validate_no_double_booking call

The release-mode validate_no_double_booking(...)? two lines above returns
Err on any failure and propagates via ?, so control never reaches the
debug-only panic block below it. Dead code in both build profiles."
```

---

## Task 2: Add `lahc_rr_kempe_respects_daily_caps` property test

**Files:**
- Modify: `solver/solver-core/tests/lahc_property.rs`

**Why:** Forward guard for the active sprint's correctness phase. `lahc_rr_kempe` exercises Change + R&R + Kempe moves; daily-caps pruning lives in `try_place_block` (FFD greedy + R&R recreate) and `try_change_move` (Change move). One property test on the most aggressive backend covers every move-path cap-pruning hole. Mirrors the existing `lahc_rr_kempe_does_not_double_book_class` shape exactly.

- [ ] **Step 1: Locate the import line and the existing property test**

Read `solver/solver-core/tests/lahc_property.rs` around line 14 (the `use solver_core::validate::...` line) and around line 425 (the `lahc_rr_kempe_does_not_double_book_class` property test). Confirm the file structure matches what the spec describes.

- [ ] **Step 2: Widen the import**

Use Edit to change the import line near the top of the file from:

```rust
use solver_core::validate::validate_no_double_booking;
```

to:

```rust
use solver_core::validate::{validate_daily_caps, validate_no_double_booking};
```

- [ ] **Step 3: Add the new property test next to its sibling**

Find the existing test in the `proptest!` block:

```rust
    #[test]
    fn lahc_rr_kempe_does_not_double_book_class(p in lahc_small_problem()) {
        let cfg = lahc_rr_kempe_cfg(0);
        let solution = solve_with_config(&p, &cfg).expect("lahc_rr_kempe must succeed");
        validate_no_double_booking(&p, &solution.placements)
            .expect("validate_no_double_booking must pass on lahc_rr_kempe output");
    }
```

Use the Edit tool to add a sibling test directly after it (still inside the same `proptest!` block):

```rust
    #[test]
    fn lahc_rr_kempe_respects_daily_caps(p in lahc_small_problem()) {
        let cfg = lahc_rr_kempe_cfg(0);
        let solution = solve_with_config(&p, &cfg).expect("lahc_rr_kempe must succeed");
        validate_daily_caps(&p, &solution.placements)
            .expect("validate_daily_caps must pass on lahc_rr_kempe output");
    }
```

- [ ] **Step 4: Run the new test alone**

Run: `cargo nextest run -p solver-core --test lahc_property lahc_rr_kempe_respects_daily_caps`
Expected: PASS (the validator already runs in debug builds via Task 1's leftover panic block, so any cap-violation regression would have surfaced as a panic; the property test now codifies it as a contract).

- [ ] **Step 5: Run the property-test widening sweep**

Per `solver/CLAUDE.md`'s "Property-test generator widenings need a 5x128 local sweep before commit" rule. The `lahc_small_problem` generator did not change in this task, but the sweep doubles as a confidence gate that the new property holds across seed space.

Run from the repo root:

```bash
for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property lahc_rr_kempe_respects_daily_caps; done
```

Expected: every iteration PASS. If a seed surfaces a real cap violation today (bug uncovered, not regression introduced), stop and report — that's a separate fix that must land before this PR.

- [ ] **Step 6: Run the full lahc_property file once**

Run: `cargo nextest run -p solver-core --test lahc_property`
Expected: PASS (no other test was touched).

- [ ] **Step 7: Run lint**

Run: `mise run lint`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add solver/solver-core/tests/lahc_property.rs
git commit -m "test(solver-core): pin lahc_rr_kempe daily-caps contract

Property test mirroring lahc_rr_kempe_does_not_double_book_class:
random small problem, lahc_rr_kempe solve, validate_daily_caps must
pass on the output. Forward guard for the active sprint's correctness
phase: a future move-path cap-pruning hole surfaces here under any
proptest seed instead of slipping through release-mode solves."
```

---

## Task 3: Promote `validate_daily_caps` to release-mode `?` and sync `solver/CLAUDE.md`

**Files:**
- Modify: `solver/solver-core/src/solve.rs:291-301` (replace the debug-only block)
- Modify: `solver/CLAUDE.md` (drop one bullet, edit one bullet)

**Why:** The behavioral commit. Release builds will now propagate `Err(Error::Input("input: subject ... exceeds max_hours_per_day ..."))` instead of silently returning a cap-violating placements vector. The `solver/CLAUDE.md` edits are necessitated by the code change: the "validate_daily_caps is debug-only" bullet documents a state that no longer exists, and the trio bullet's "plus a `#[cfg(debug_assertions)]` panic block" clause is no longer accurate for any of the three validators.

- [ ] **Step 1: Replace the debug-only block with a release-mode `?` call**

Use Edit on `solver/solver-core/src/solve.rs` to transform the post-validator region. After Task 1, the block reads:

```rust
    // Post-solve hard-constraint sanity check. A failure here is a solver bug.
    validate_no_room_hopping(problem, &solution.placements)?;
    validate_no_double_booking(problem, &solution.placements)?;

    // Debug-only post-condition: daily caps (ADR 0033) are enforced as
    // legality pruning, so a violation here means the pruning has a hole.
    // Loud in dev/tests, free in release.
    #[cfg(debug_assertions)]
    if let Err(e) = validate_daily_caps(problem, &solution.placements) {
        panic!("daily-cap post-condition violated: {e}");
    }
```

Replace the entire region (the four-line release block above plus the eight-line debug block below it) with:

```rust
    // Post-solve hard-constraint sanity check. A failure here is a solver bug.
    validate_no_room_hopping(problem, &solution.placements)?;
    validate_no_double_booking(problem, &solution.placements)?;
    validate_daily_caps(problem, &solution.placements)?;
```

The three validators now share one comment and one wiring shape.

- [ ] **Step 2: Build to confirm no unused-import warning**

Run: `cargo build --release -p solver-core`
Expected: PASS, no `unused import: validate_daily_caps` warning. (The `solver/CLAUDE.md` bullet documenting that warning becomes stale after this step; Task 3 step 4 deletes it.)

- [ ] **Step 3: Run the workspace tests**

Run: `mise run test:rust`
Expected: PASS. The full suite includes the kempe smoke at `tests/daily_caps.rs::caps_kempe_solve_under_production_caps_smoke`, which forces `Subject.max_hours_per_day = 2` on the dreizuegig fixture and runs lahc_rr_kempe at 5000 max iterations across 10 seeds. If the production cap-pruning has a hole, this is where it would surface as a release-mode `Err` propagating up to `.unwrap()`.

If a seed surfaces a real cap violation today, stop and report. Per the spec, no live cap violation is known on master; the kempe smoke has been green throughout the active sprint while running in debug mode (where the old panic block fired).

- [ ] **Step 4: Drop the stale `solver/CLAUDE.md` bullet**

Read `solver/CLAUDE.md` and find the bullet that begins:

```markdown
- **`validate_daily_caps` is `#[cfg(debug_assertions)]`-only at the `solve.rs` call site.**
```

Use the Edit tool to delete the entire bullet (the whole paragraph from the leading `- **` to the end of the bullet text). The unused-import warning the bullet documents is gone after Step 2.

- [ ] **Step 5: Tighten the "Post-condition validators trio" bullet**

Find the bullet that begins:

```markdown
- **Post-condition validators trio.** `solver-core/src/validate.rs` hosts three checks that `solve_with_config` runs on the final placements vector: `validate_no_room_hopping` (same-room invariant), `validate_daily_caps` (Subject and SchoolClass per-day caps), `validate_no_double_booking` (class / teacher / room non-overlap + lesson cardinality + block shape). Wiring pattern is identical: a release-mode `Result`-form call propagates `Err(Error::Input)` to the caller, plus a `#[cfg(debug_assertions)]` panic block that fails property and integration tests loudly. Validator failures indicate a solver bug, not malformed input. New post-condition validators should follow the same shape (one fn, one walk over `placements`, sibling tests inline in `validate.rs::tests`).
```

Use Edit to replace the wiring sentence ("Wiring pattern is identical: ...") so the bullet reads:

```markdown
- **Post-condition validators trio.** `solver-core/src/validate.rs` hosts three checks that `solve_with_config` runs on the final placements vector: `validate_no_room_hopping` (same-room invariant), `validate_daily_caps` (Subject and SchoolClass per-day caps), `validate_no_double_booking` (class / teacher / room non-overlap + lesson cardinality + block shape). Wiring pattern is identical: one release-mode `Result`-form call per validator at the tail of `solve_with_config_stats`; `Err(Error::Input)` propagates via `?` and integration tests `.expect()` it. Validator failures indicate a solver bug, not malformed input. New post-condition validators should follow the same shape (one fn, one walk over `placements`, sibling tests inline in `validate.rs::tests`).
```

(Only the "Wiring pattern is identical: ..." sentence changes; everything else stays.)

- [ ] **Step 6: Re-run the workspace tests after the doc edits**

Run: `mise run test:rust`
Expected: PASS (docs are not in the test path, but this confirms nothing else slipped).

Also run: `mise run lint`
Expected: PASS.

- [ ] **Step 7: Pre-merge smoke bench**

Per `solver/CLAUDE.md`'s "Pre-merge smoke bench" rule. The change is wiring-only (no algorithm change), so a 5s/4-seed downscale is enough to catch any unexpected behavior shift.

```bash
mise run solver:rebuild
mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig --out /tmp/item-53-smoke.md
```

Expected: feasibility 4/4 across every LAHC variant on grundschule and zweizuegig; soft-score columns within smoke noise of master. CP-SAT cells may fail with `ModuleNotFoundError` on a fresh wheel; that is independent of this PR. If feasibility drops on any LAHC cell, stop and report — a regression was introduced.

- [ ] **Step 8: Commit**

```bash
git add solver/solver-core/src/solve.rs solver/CLAUDE.md
git commit -m "fix(solver-core): promote validate_daily_caps to production post-condition

Release builds now propagate Err(Error::Input) on a daily-cap violation
instead of silently returning a cap-violating placements vector. The
validator trio in solve.rs now share one wiring shape: one release-mode
? call per validator. Drops the corresponding solver/CLAUDE.md bullet
documenting the debug-only state and tightens the trio bullet.

Acceptance per OPEN_THINGS item 53: a release build cannot return a
schedule that exceeds Subject.max_hours_per_day or
SchoolClass.max_lessons_per_day."
```

---

## Self-review

**Spec coverage.** Every spec section maps to a task:
- Spec C1 (drop redundant debug-only `validate_no_double_booking`) → Task 1.
- Spec C2 (property test) → Task 2 plus the 5x128 sweep.
- Spec C3 (promote `validate_daily_caps` + CLAUDE.md edits) → Task 3.
- Spec "Tests" section → existing tests stay green (covered by `mise run test:rust` in Tasks 1 + 3); new property test added in Task 2.
- Spec "Acceptance" → Task 3 step 8 commit message names every acceptance line.

**Placeholder scan.** Every step has the actual edit, command, or expected outcome. No "TBD" / "fill in details".

**Type consistency.** Function names match: `validate_daily_caps`, `validate_no_room_hopping`, `validate_no_double_booking`, `lahc_rr_kempe_cfg`, `lahc_small_problem`, `solve_with_config`. Test names match: `lahc_rr_kempe_does_not_double_book_class` (existing), `lahc_rr_kempe_respects_daily_caps` (new). File paths match across tasks.

No gaps. Plan is ready to execute.
