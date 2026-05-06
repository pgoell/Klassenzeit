# soft_score full-cost reconciliation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `Solution.soft_score` report the full weighted cost on every solver code path so LAHC and cpsat bake-off backends compare on the same number (active-sprint item 41).

**Architecture:** Single-line change at `solver/solver-core/src/solve.rs:249` swaps `state.soft_score` (LAHC running slice) for `score_solution(problem, &solution.placements, &config.weights)` (full recompute). The LAHC inner loop continues to optimise the slice via the existing partition-delta machinery. A targeted regression test pins the new contract under `PRODUCTION_ACTIVE_WEIGHTS`. The bake-off bench's now-redundant cpsat-side recompute is dropped.

**Tech Stack:** Rust 2021 (`solver-core`, `solver-bench`); `cargo nextest` for tests; `cargo bench` for criterion; `mise run lint` for the workspace lint suite (clippy `-D warnings`, rustfmt, machete).

---

## File map

- Modify: `solver/solver-core/src/solve.rs:249` (the assignment) plus its 3-line preceding doc comment.
- Modify: `solver/solver-core/src/types.rs:344-348` (rustdoc on `Solution::soft_score`).
- Modify: `solver/solver-core/tests/score_property.rs` (add fixture builder + regression test; widen import line for `PRODUCTION_ACTIVE_WEIGHTS`).
- Modify: `solver/solver-bench/src/main.rs:349-354` (drop the duplicate `score_solution` recompute).
- Modify: `docs/superpowers/OPEN_THINGS.md` (delete item 41 block; rewrite the next-pickup paragraph).
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md` (description field + body).
- Conditional modify: `solver/solver-core/benches/BASELINE.md` (only if `mise run bench` shows >3% drift on at least one fixture).

---

## Task 1: Add failing regression test

**Files:**
- Modify: `solver/solver-core/tests/score_property.rs:1-9` (import line) and append at end of file.

This task lands the red half of TDD. The test asserts the new contract; on master it fails because `solution.soft_score` carries the slice (`class_day_balance`-free) while `score_solution` returns the full cost (which includes the `class_day_balance: 5` contribution).

- [ ] **Step 1.1: Read current import line at `tests/score_property.rs:1-9`**

Confirm the imports look like:

```rust
use solver_core::{
    score_solution, solve_with_config, ConstraintWeights, Lesson, LessonId, Placement, Problem,
    Room, RoomId, SchoolClass, SchoolClassId, SolveConfig, Subject, SubjectId, Teacher, TeacherId,
    TeacherQualification, TimeBlock, TimeBlockId,
};
```

- [ ] **Step 1.2: Add `PRODUCTION_ACTIVE_WEIGHTS` to the import line**

Replace the import block above with:

```rust
use solver_core::{
    score_solution, solve_with_config, ConstraintWeights, Lesson, LessonId, Placement, Problem,
    Room, RoomId, SchoolClass, SchoolClassId, SolveConfig, Subject, SubjectId, Teacher, TeacherId,
    TeacherQualification, TimeBlock, TimeBlockId, PRODUCTION_ACTIVE_WEIGHTS,
};
```

- [ ] **Step 1.3: Append the fixture builder and regression test at end of file**

Append the following block to `tests/score_property.rs` (after the last existing `proptest! { ... }` block; before EOF):

```rust
/// Hand-built problem that exercises the `class_day_balance` axis under
/// `PRODUCTION_ACTIVE_WEIGHTS`. FFD-greedy packs the lesson's two hours
/// onto a single day (best slice score: zero class_gap), leaving the
/// second day empty. The slice score is therefore zero; the full
/// `score_solution` adds a non-zero `class_day_balance` cost. Pin: this
/// fixture is the regression for item 41.
fn build_class_day_balance_problem() -> Problem {
    let class_id = SchoolClassId(id_from(5000));
    let teacher_id = TeacherId(id_from(2000));
    let room_id = RoomId(id_from(3000));
    let subject_id = SubjectId(id_from(4000));
    let lesson_id = LessonId(id_from(6000));

    let time_blocks: Vec<TimeBlock> = (0u8..2)
        .flat_map(|d| {
            (0u8..2).map(move |p| TimeBlock {
                id: TimeBlockId(id_from(u32::from(d) * 100 + u32::from(p) + 1000)),
                day_of_week: d,
                position: p,
            })
        })
        .collect();

    Problem {
        time_blocks,
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
        lessons: vec![Lesson {
            id: lesson_id,
            school_class_ids: vec![class_id],
            subject_id,
            teacher_id,
            hours_per_week: 2,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        teacher_qualifications: vec![TeacherQualification {
            teacher_id,
            subject_id,
        }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    }
}

/// Item 41 contract: `solve_with_config` must report `solution.soft_score`
/// as the full weighted cost (`score_solution(problem, placements,
/// weights)`), not the LAHC running slice. Under `PRODUCTION_ACTIVE_WEIGHTS`
/// the `class_day_balance` axis is non-zero on a one-day-packed plan; on
/// master before the fix this assertion fails because the slice misses it.
#[test]
fn solve_soft_score_under_production_weights_equals_score_solution() {
    let problem = build_class_day_balance_problem();
    let cfg = SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        deadline: None,
        ..SolveConfig::default()
    };
    let sol = solve_with_config(&problem, &cfg).expect("solve must succeed on the tiny fixture");
    let recomputed = score_solution(&problem, &sol.placements, &PRODUCTION_ACTIVE_WEIGHTS);
    assert_eq!(
        sol.soft_score, recomputed,
        "Solution.soft_score must equal score_solution(...) under PRODUCTION_ACTIVE_WEIGHTS; \
         got slice={}, full={}",
        sol.soft_score, recomputed,
    );
}
```

- [ ] **Step 1.4: Run the new test and confirm it FAILS on master**

Run: `cargo nextest run -p solver-core --test score_property solve_soft_score_under_production_weights_equals_score_solution`

Expected: FAIL with an `assertion failed: sol.soft_score == recomputed` message; the slice value will be the smaller number, the full recompute the larger.

If the test passes here, the fixture does not exercise a divergent axis under PRODUCTION_ACTIVE_WEIGHTS. Stop and inspect: print `sol.soft_score` and `recomputed` via `dbg!()` to find which axis is or isn't firing.

- [ ] **Step 1.5: Run the rest of the score_property suite to confirm no collateral damage**

Run: `cargo nextest run -p solver-core --test score_property`

Expected: only the new test fails; the existing proptest cases still pass.

- [ ] **Step 1.6: Commit the failing test**

```bash
git add solver/solver-core/tests/score_property.rs
git commit -m "test(solver-core): pin production-weight soft_score reporting (item 41)"
```

The commit message body is empty; the title carries the intent.

---

## Task 2: Flip the assignment to the full recompute

**Files:**
- Modify: `solver/solver-core/src/solve.rs` around line 249 (the assignment plus its surrounding 3 lines of context).

This task lands the green half of TDD. The slice-vs-full divergence introduced in Task 1 is closed by routing through `score_solution` at the boundary.

- [ ] **Step 2.1: Inspect the current site at `solver/solver-core/src/solve.rs:240-251`**

Confirm it reads:

```rust
    #[cfg(debug_assertions)]
    if let Err(e) = validate_no_double_booking(problem, &solution.placements) {
        panic!("no-double-booking post-condition violated: {e}");
    }

    solution.soft_score = state.soft_score;
    Ok(solution)
}
```

- [ ] **Step 2.2: Replace the assignment with the `score_solution` call plus its explanatory comment**

Apply this edit to `solver/solver-core/src/solve.rs:249`:

```rust
    // state.soft_score is the LAHC running slice (class_gap + teacher_gap
    // + subject_pref). Solution.soft_score is the full weighted cost on
    // the final placements, including prefer_home_room and
    // class_day_balance, so consumers compare every backend on the same
    // number.
    solution.soft_score = crate::score::score_solution(problem, &solution.placements, &config.weights);
    Ok(solution)
}
```

`crate::score::score_solution` works because `solver-core/src/lib.rs` already declares `pub mod score;`. (Cross-check: `crate::score` is reachable from `solve.rs` because both live under the crate root.)

- [ ] **Step 2.3: Run the regression test, confirm it now passes**

Run: `cargo nextest run -p solver-core --test score_property solve_soft_score_under_production_weights_equals_score_solution`

Expected: PASS.

- [ ] **Step 2.4: Run the full solver-core test suite to catch collateral failures**

Run: `mise run test:rust`

Expected: every test passes. The four `lahc.soft_score == score_solution(...)` round-trip assertions in `tests/lahc_property.rs` continue to pass trivially (both sides become the recompute). Three `s.soft_score == 0` assertions (`tests/early_exit.rs:95`, `solve.rs:1654`, `solve.rs:1736`) continue to pass because their fixtures use weights that zero out `prefer_home_room` and `class_day_balance` (slice == full == 0).

If a test breaks here, do NOT weaken the assertion. Inspect the fixture's weights; if the test deliberately exercises non-trivial slice-vs-full divergence, the assertion was wrong (slice-coupled) and should be updated to compare against `score_solution(...)`. If unclear, stop and surface; do not paper over.

- [ ] **Step 2.5: Run lint to catch fmt and clippy noise**

Run: `mise run lint`

Expected: green.

- [ ] **Step 2.6: Commit the fix**

```bash
git add solver/solver-core/src/solve.rs
git commit -m "fix(solver-core): report full weighted cost as solution.soft_score (item 41)"
```

---

## Task 3: Update the `Solution::soft_score` rustdoc

**Files:**
- Modify: `solver/solver-core/src/types.rs:344-348` (the doc comment on `Solution::soft_score`).

The doc comment today says "Sum of weighted soft-constraint penalties across `placements`" plus "Zero when both weights are zero or when the schedule is fully compact." Post-fix the field is the full weighted cost; the "fully compact" framing is misleading because compactness only zeros the slice axes.

- [ ] **Step 3.1: Inspect the current rustdoc at `solver/solver-core/src/types.rs:344-348`**

Confirm it reads:

```rust
    /// Sum of weighted soft-constraint penalties across `placements`.
    /// Populated by `solve_with_config` against the caller's
    /// `ConstraintWeights`. Zero when both weights are zero or when the
    /// schedule is fully compact.
    pub soft_score: u32,
```

- [ ] **Step 3.2: Replace with the post-fix contract**

Apply this edit:

```rust
    /// Full weighted soft-constraint cost on the final placements,
    /// computed by `score::score_solution(problem, placements, weights)`
    /// at the end of every `solve_with_config`. The LAHC inner loop
    /// optimises a faster slice (`class_gap + teacher_gap +
    /// subject_pref`) for delta efficiency; this reported field is the
    /// canonical objective so cross-backend bench cells (LAHC, cpsat)
    /// compare on the same number. Zero when every active weight axis
    /// contributes zero (e.g. zero weights, or a fully optimal plan
    /// against the active weights).
    pub soft_score: u32,
```

- [ ] **Step 3.3: Confirm lint still green**

Run: `mise run lint`

Expected: green. (The crate-level `#![deny(missing_docs)]` is already satisfied; this is a rewrite, not a removal.)

- [ ] **Step 3.4: Fold into Task 2's commit via `--amend`, or keep separate**

The change is a doc-only refresh that lives logically with the fix. Two acceptable options:
1. Amend Task 2's commit: `git add solver/solver-core/src/types.rs && git commit --amend --no-edit`. Cleaner history.
2. New commit: `docs(solver-core): clarify Solution::soft_score is the full weighted cost`. Easier to review independently.

Pick option 1: the rustdoc is a part of the contract change, not a standalone documentation pass. Run:

```bash
git add solver/solver-core/src/types.rs
git commit --amend --no-edit
```

(`--amend` is allowed here because the commit has not been pushed; we are not amending shared history.)

---

## Task 4: Drop the duplicate `score_solution` recompute from the bench's cpsat arm

**Files:**
- Modify: `solver/solver-bench/src/main.rs:349-354`.

Once `solution.soft_score` carries the full cost on every backend, the bench's defence-in-depth recompute is duplicate coverage. The cross-backend agreement is already pinned by `solver-py/tests/test_score_solution_json.py`'s round-trip test.

- [ ] **Step 4.1: Inspect current site at `solver/solver-bench/src/main.rs:340-356`**

Confirm it reads:

```rust
        let feasible = hard == 0 && placements_total == expected;
        if feasible {
            feasibility_count += 1;
            let soft = solver_core::score_solution(
                problem,
                &solution.placements,
                &solver_core::PRODUCTION_ACTIVE_WEIGHTS,
            );
            soft_score_feasible.push(soft);
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(placements_total);
    }
```

- [ ] **Step 4.2: Replace the recompute with the field read**

Apply this edit:

```rust
        let feasible = hard == 0 && placements_total == expected;
        if feasible {
            feasibility_count += 1;
            soft_score_feasible.push(solution.soft_score);
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(placements_total);
    }
```

- [ ] **Step 4.3: Audit unused imports**

The `solver_core::score_solution` and `solver_core::PRODUCTION_ACTIVE_WEIGHTS` imports may still be used in the LAHC arm of the same file. Run:

```bash
grep -n "score_solution\|PRODUCTION_ACTIVE_WEIGHTS" solver/solver-bench/src/main.rs
```

Expected: no remaining references after the edit (the LAHC arm uses `solution.soft_score` directly via `solve_with_config`'s output). If either symbol is gone from the file body, drop it from the `use` line.

- [ ] **Step 4.4: Confirm the bench compiles**

Run: `cargo build -p solver-bench`

Expected: green compile, no warnings.

- [ ] **Step 4.5: Smoke-run the bench at minimum settings to confirm it still produces output**

Run: `cargo run -p solver-bench --release -- --budget 1s --seeds 1 --fixtures grundschule --out /tmp/bench-smoke.md`

Expected: completes without error; `/tmp/bench-smoke.md` shows the four backends with feasibility and soft-score numbers. The actual numbers don't matter here; this is a "does it run end-to-end" check.

If `cpsat` row shows `-` because the python module isn't on the path, that is acceptable for this smoke test. The verification target is the LAHC cells executing through the new code path.

- [ ] **Step 4.6: Lint and commit**

Run: `mise run lint`

Expected: green.

```bash
git add solver/solver-bench/src/main.rs
git commit -m "chore(solver-bench): drop duplicate score_solution recompute from cpsat arm"
```

---

## Task 5: Conditional criterion baseline refresh

**Files:**
- Conditional modify: `solver/solver-core/benches/BASELINE.md`.

The fix adds one `score_solution` call per solve. Per the spec, expected drift is sub-1% on every fixture. The 3% refresh threshold is the gate.

- [ ] **Step 5.1: Run the criterion bench against the current branch**

Run: `mise run bench`

Expected: criterion output for `grundschule`, `zweizuegig`, `dreizuegig`. Each fixture prints a `change: [-N% +N%]` line vs. the committed `BASELINE.md`.

- [ ] **Step 5.2: Decide whether to refresh `BASELINE.md`**

Branches:
- (a) All three fixtures show drift <3% (point estimate). Skip refresh; no commit. Surface the observed drift in the PR body.
- (b) At least one fixture shows drift >=3% AND <20%. Refresh: `mise run bench:record`, then `git add solver/solver-core/benches/BASELINE.md && git commit -m "chore(solver-core): refresh criterion baseline post-soft_score-fix"`. Surface the new numbers and the deltas in the PR body.
- (c) Any fixture shows drift >20%. Stop. The single `score_solution` call should not produce 20% regression on a 50-300 placement fixture; investigate (e.g., is `score_solution` being called inside the LAHC loop by accident?) before continuing.

- [ ] **Step 5.3: Surface the bench result in the eventual PR body**

Note in the PR body the observed grundschule / zweizuegig / dreizuegig drift numbers, regardless of whether the baseline was refreshed. This is for reviewer evidence; the file edit is conditional, the surfacing is unconditional.

---

## Task 6: Remove item 41 from OPEN_THINGS and refresh auto-memory

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md` (delete item 41 block; rewrite the next-pickup paragraph to advance to item 30).
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md` (description field + body).

- [ ] **Step 6.1: Read the relevant block of `docs/superpowers/OPEN_THINGS.md`**

The active-sprint preamble lives at lines 7-15 (approximately); item 41 is the only entry under "### Correctness phase". Confirm before editing.

- [ ] **Step 6.2: Delete the item 41 block in OPEN_THINGS**

The block is one bullet:

```markdown
41. **Reconcile `solution.soft_score` with the full weighted cost.** `[P0]` ... Pick one and align `solver-bench/src/main.rs:245` so both backends compare on the same number.
```

Delete it entirely. The "### Correctness phase" header may or may not need to stay. Two options:
- Keep the header empty: leaves a placeholder for any future correctness item that surfaces. Mildly anti-DRY but signals the phase ordering remains.
- Remove the header: tighter file. Reorder if the next phase ("### Observability phase") becomes the new top of the sprint body.

Pick: remove the header along with item 41. The phase-ordering doc is in the preamble paragraph, not the section structure; the headers are lightweight scaffolding.

- [ ] **Step 6.3: Rewrite the active-sprint preamble to advance the next-pickup line**

Today's preamble (line 9):

```markdown
Next pickup: P0 item 41 (reconcile `solution.soft_score` with the full weighted cost; bake-off cells will compare on the same objective once this lands). Item 40 (mix `preferred_block_size` in `lahc_small_problem`) shipped this PR; ...
```

Rewrite to:

```markdown
Next pickup: P0 item 30 (add `peak_memory_kb`, `time_to_first_feasible_ms`, `time_to_optimal_ms` columns to `BENCH_RESULTS.md`; observability phase opens once item 41 ships). Item 41 (reconcile `solution.soft_score` with the full weighted cost) shipped this PR; LAHC and cpsat bake-off cells now route through `score_solution(problem, placements, &PRODUCTION_ACTIVE_WEIGHTS)` end-to-end. Item 40 (mix `preferred_block_size` in `lahc_small_problem`) shipped in PR #190; ...
```

(Verbatim wording can shift; the load-bearing facts are: item 41 shipped, item 30 is next, the slice-vs-full reporting gap is closed.)

- [ ] **Step 6.4: Add a sprint-tidy follow-up entry for the BENCH_RESULTS.md refresh**

Append under the active sprint program (before the "### Observability phase" header) a new tidy block:

```markdown
### Sprint-tidy phase

42. **Refresh `BENCH_RESULTS.md` post-item-41.** `[P1]` Item 41 closed the slice-vs-full reporting gap; the existing committed numbers in `BENCH_RESULTS.md` show LAHC cells against the slice and cpsat cells against the full cost (apples-to-oranges). Run `mise run bench:bakeoff` at production cell shape (`--budget 60s --seeds 20`, ~80 min) and commit the refresh. The new numbers inform the ADR 0032 production-default revisit; if the LAHC vs. cpsat ordering changes, surface in a follow-up plan rather than amending ADR 0032 in the same PR.
```

- [ ] **Step 6.5: Read the auto-memory project-roadmap file**

Read: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`

Note the current `description:` frontmatter and body; both reference item 41 as next-pickup.

- [ ] **Step 6.6: Update both the description field and the body of the auto-memory entry**

Replace the `description:` frontmatter to reference item 30 as next-pickup. Replace the body to record item 41 as shipped (this PR) and item 30 as the new pickup. The autopilot doc is explicit that the description must update too, not just the body.

- [ ] **Step 6.7: Commit the docs + memory changes**

```bash
git add docs/superpowers/OPEN_THINGS.md
# Auto-memory file lives outside the repo; do not git-add it. The Write tool persists it directly.
git commit -m "docs: remove shipped item 41 from OPEN_THINGS, queue bench refresh"
```

---

## Task 7: Final verification, push, PR

This task is the gate before push.

- [ ] **Step 7.1: Run the full lint suite**

Run: `mise run lint`

Expected: green.

- [ ] **Step 7.2: Run the full test suite (Rust + Python + frontend)**

Run: `mise run test`

Expected: green. The Rust suite covers the new regression test; the Python and frontend suites are unaffected by this change but verify nothing else broke.

- [ ] **Step 7.3: Skill audit before push**

Walk the `/autopilot` skill table:
- step 0: `superpowers:using-superpowers` ✓ (called at session start)
- step 2: `superpowers:brainstorming` ✓ (called pre-spec)
- step 4: `superpowers:writing-plans` ✓ (called pre-plan)
- step 5: `superpowers:test-driven-development` and `superpowers:subagent-driven-development` (called when implementation begins)
- step 6: `claude-md-management:revise-claude-md`, `claude-md-management:claude-md-improver`, `fewer-permission-prompts` (called pre-PR)

If any row is missing, invoke now and reshape the artefact it governs before push.

- [ ] **Step 7.4: Push the branch**

Run: `mise exec -- git push -u origin fix/solver-soft-score-full-cost`

Expected: pre-push hook runs (`cargo nextest run --workspace`, `uv run pytest`, frontend Vitest) and the push succeeds.

- [ ] **Step 7.5: Open the PR**

Run:

```bash
gh pr create --base master --head fix/solver-soft-score-full-cost \
  --title "fix(solver-core): report full weighted cost as solution.soft_score (item 41)" \
  --body "$(cat <<'EOF'
## Summary

- `solver-core/src/solve.rs:249` now sets `solution.soft_score` via `score::score_solution(problem, &solution.placements, &config.weights)` instead of `state.soft_score`. The LAHC inner-loop slice (`class_gap + teacher_gap + subject_pref`) stays internal; the reported field is the full weighted cost on every backend.
- New regression test `solve_soft_score_under_production_weights_equals_score_solution` in `tests/score_property.rs` exercises the `class_day_balance` axis under `PRODUCTION_ACTIVE_WEIGHTS`; fails on master, passes after the fix.
- Bench's cpsat arm at `solver-bench/src/main.rs:349-354` drops its now-duplicate `score_solution` recompute. Both backends route through the same scorer.

## Non-goals

- LAHC inner-loop optimisation objective alignment with the full cost. Out of scope; queued as a follow-up.
- `BENCH_RESULTS.md` refresh. Queued as sprint-tidy item 42 (~80 min wall-clock).

## Test plan

- [x] `mise run test:rust` green
- [x] `mise run lint` green
- [x] New regression test fails on master, passes after the fix
- [x] `mise run bench` shows <3% drift on grundschule / zweizuegig / dreizuegig (or BASELINE.md refreshed)

Spec: `docs/superpowers/specs/2026-05-06-soft-score-full-cost-design.md`
Plan: `docs/superpowers/plans/2026-05-06-soft-score-full-cost.md`
EOF
)"
```

- [ ] **Step 7.6: Post brainstorm comments**

Run: `python3 .claude/commands/post_brainstorm_comments.py <pr-number>`

The script reads `/tmp/kz-brainstorm/brainstorm.md` and posts the preamble + per-section comments.

- [ ] **Step 7.7: Set automerge**

Run: `gh pr merge <pr-number> --auto --squash`

Expected: queues the merge for when CI passes.

---

## Self-review notes

- **Spec coverage:** every "In scope" bullet in the spec is implemented by a task here (replace assignment → Task 2; rustdoc updates → Task 2 + Task 3; regression test → Task 1; bench cleanup → Task 4; OPEN_THINGS + auto-memory → Task 6; conditional baseline refresh → Task 5).
- **Placeholder scan:** every step has its actual code or command. The conditional baseline refresh in Task 5 documents both branches (refresh vs. skip) explicitly.
- **Type consistency:** `score_solution` signature `(problem, placements, weights)` is used identically in Task 1's test, Task 2's fix, and Task 4's bench cleanup. `PRODUCTION_ACTIVE_WEIGHTS` is imported in Task 1 and reused. `SolveConfig::default()` matches today's struct shape (no fields renamed in this PR).
- **Test fixture sanity:** the `build_class_day_balance_problem` builder mirrors the existing `small_problem` proptest layout (same Id offsets at 1000/2000/3000/4000/5000/6000) and uses `Subject` / `SchoolClass` / `Teacher` / `Lesson` field shapes that match the latest types.
