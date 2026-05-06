# R&R recreate phase scores by soft-delta, not just hard-feasibility (item 49)

**Sprint program.** Solver feasibility correctness + observability (active program), follow-ups bucket (`## Open solver follow-ups`).
**Phase.** Open follow-up: item 49 (P1).
**Goal.** Stop the R&R recreate phase from collapsing `worst_home_med` by making `solve::try_place_block`'s room scan score each `(time-block window, room)` candidate by `slice_score + home_room_penalty(room)` and pick the lowest-total candidate, rather than the first id-order hard-feasible room. The 2026-05-06 production refresh shows `lahc_rr` regressing the per-class home-room ratio from 0.50 to 0.05 on zweizuegig and 0.04 to 0.00 on dreizuegig, isolated to this picker behaviour.

**Non-goal.** No `class_day_balance` extension (item 47 / item 21 / item 22 territory). No CP-SAT objective parity (item 48). No K-best capping (Q7 in the brainstorm). No new `SolveConfig` fields. No `BENCH_RESULTS.md` refresh in this PR (queues with items 21 + 22 + 48 per item 47). No ADR.

## Context

`solve::try_place_block` is the per-block placement primitive used by both the FFD greedy bootstrap and the R&R recreate phase (`lahc::rr_attempt`). Today it scores each candidate window by an analytical slice delta (`class_gap + teacher_gap + subject_pref`) and picks rooms by id-order via the inner `'rooms` loop's `break` after the first hard-feasible match (`solver/solver-core/src/solve.rs:617-673`). The slice contract on `state.soft_score` (no `home_room`, no `class_day_balance`) is enforced in `lahc.rs:962-974` via `running_slice_from_placements` after every R&R recreate.

The room scan never samples `score::home_room_penalty`; consequently, when a class's home room is busy and the next-id-order room is feasible, the picker happily lands a placement in the non-home room even when waiting for the home room (or scanning to a different window) would yield a strictly lower total soft score. R&R fires every `lahc_rr_period = 25` iterations and ruins K-or-fewer block-anchors; each recreate goes through the same blind picker, so home placements LAHC accumulated between R&R fires get undone.

Concrete bench evidence (PR #199, `solver/solver-core/benches/BENCH_RESULTS.md` at production cell shape `--budget 60s --seeds 20`):

- zweizuegig: plain `lahc` `worst_home_med = 0.50`, `lahc_rr` collapses to `0.05`. Soft score 775 → 1119 (+44 percent).
- dreizuegig: plain `lahc` `worst_home_med = 0.04`, `lahc_rr` collapses to `0.00`. Soft score 2235 → 2434 (+9 percent).

The collapse is mechanical: every R&R fire rolls a fraction of placements out of home rooms, with no opposing pressure to roll them back. `lahc::run`'s Change move (the per-iteration neighbour generator) is a TB-only swap (does not touch `room_id`), so the room dimension is locked in by FFD or by R&R recreate; once the recreate has placed a lesson in a non-home room, no later move recovers it.

`score::home_room_penalty(lesson, home_room_lookup, placement_room_id, weights)` (`solver/solver-core/src/score.rs:300-318`) is non-negative by construction (zero when `weights.prefer_home_room == 0`, the home room matches, or no class home room is set; otherwise `prefer_home_room * (count of class members whose home room differs from the placement room)`). It is allocation-free.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 49. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Extend `solve::try_place_block` (`solver/solver-core/src/solve.rs:377-775`) so the room scan scores `(window, room)` candidates by `slice_score + home_room_penalty(room)`, picks the room with the lowest total, and uses room.id as the final tiebreak. The window-level pruning bound and early-exit are updated to compare on total. Persists `state.soft_score = slice_score` (slice contract preserved) so downstream Change moves continue to operate on the same axis.
- Thread a `home_room_lookup: &HashMap<SchoolClassId, Option<RoomId>>` argument through `try_place_block`'s call sites: the FFD greedy site in `solve_with_config` (`solver/solver-core/src/solve.rs`), the R&R recreate site in `lahc::rr_attempt` (`solver/solver-core/src/lahc.rs:892-905`), and the lesson-group fallback `try_place_group` if it shares the same room scan shape (audit during implementation; if so, fix in the same commit).
- Build `home_room_lookup` once per `solve_with_config` from `problem.school_classes` (each entry's `home_room_id`). One pass, `O(school_classes)`, zero-allocation HashMap reuse pattern matches the existing lookups in `lahc::run`.
- Add a unit test `try_place_block_room_picker_minimises_home_room_penalty` co-located in `solve.rs::tests`. Drives `try_place_block` directly with a hand-built `GreedyState`, two feasible rooms (one home, one non-home), and asserts the placed `Placement.room_id` is the home room.
- Add an integration test `lahc_rr_recreate_picks_lowest_soft_delta` in a new `solver/solver-core/tests/rr_recreate_soft_aware.rs`. 3-lesson fixture: lesson L1 starts in a non-home room (id-ordered FFD pick under the OLD picker; explicitly constructed by tightening room ids so id-order picks the wrong one, then asserting the new picker's behaviour). After R&R ruins L1 and recreates, L1's room is the home room. Naming follows OPEN_THINGS verbatim.
- Run `mise run bench` (criterion) before commit; cite the per-fixture absolute-µs delta in the PR body. If `solver_greedy/grundschule|zweizuegig|dreizuegig` regresses by >20 percent of the lower-bound absolute, ship a follow-up commit on the branch (`bench(solver-core): refresh BASELINE.md after recreate soft-aware picker`) per the 20-percent budget triage rule in `solver/CLAUDE.md`.
- Run a 5×128 PROPTEST_CASES sweep on the new integration test seeds plus the existing `lahc_property::lahc_rr_never_decreases_placement_count` and `lahc_rr_kempe_never_decreases_placement_count` (per `solver/CLAUDE.md`).
- Delete OPEN_THINGS item 49. Update item 21, item 22, item 47, item 11, item 14 cross-references to remove "Land after item 49" / "wait on item 49" notes.
- Update `solver/CLAUDE.md` if the implementation surfaces a non-obvious invariant worth durable capture (e.g. the slice-vs-total split on the picker's score formula, the pruning soundness argument).
- Update auto-memory `project_roadmap_status.md` after the PR merges (next-pickup pointer rolls forward to item 21 since item 49 unblocks it).

**Out of scope.**

- `class_day_balance` axis in the picker. The slice already captures the gap shape via `class_gap`; doubling up risks a different convergence regime. Item 47's revisit is the place to evaluate that axis.
- CP-SAT objective parity. Item 48, separate PR.
- Kempe snapshot/delta migration of R&R's post-recreate `running_slice_from_placements` recompute. Item 38, deferred behind a bench-evidence trigger (`mise run bench` showing the recompute as ≥10 percent of the LAHC loop with `--rr-period 5`).
- K-best capping. The window-level pruning already terminates the search aggressively; capping introduces a tunable knob with no defensible default. Promoted to mitigation only if `mise run bench` shows >20 percent FFD regression.
- `lahc_rr_period` / `RR_K` tuning. Item 21, lands after this fix per the OPEN_THINGS sequencing.
- `BENCH_RESULTS.md` refresh. Item 47 batches the production-shape rerun behind items 49 + 48 + 21 + 22 so the post-fix table is internally consistent.
- ADR. The decision is mechanical (a missing soft-axis gets sampled), not architectural; the existing ADR 0029 + ADR 0032 cover the bake-off methodology and production-default lineage.

## Implementation shape

### Picker change in `try_place_block`

Today's room scan (`solver/solver-core/src/solve.rs:617-673`):

```rust
let mut chosen_room: Option<RoomId> = None;
'rooms: for &room_idx in room_order {
    let room = &problem.rooms[room_idx];
    // ... shared_lock / suitability / room_busy / room_blocked checks ...
    chosen_room = Some(room.id);
    break;
}
```

Replace with a "best feasible" scan that samples `home_room_penalty(lesson, home_room_lookup, room.id, weights)` per feasible candidate room and tracks the minimum. Tiebreak on room.id (the existing room_order is sorted by id, so a stable iteration plus `<` comparison achieves this).

```rust
let mut best_room: Option<(RoomId, u32)> = None; // (room.id, home_room_penalty)
'rooms: for &room_idx in room_order {
    let room = &problem.rooms[room_idx];
    // ... shared_lock / suitability / room_busy / room_blocked checks unchanged ...
    let penalty = score::home_room_penalty(lesson, home_room_lookup, room.id, weights);
    match best_room {
        None => best_room = Some((room.id, penalty)),
        Some((_, best_penalty)) if penalty < best_penalty => best_room = Some((room.id, penalty)),
        _ => {} // strict `<` keeps id-order tiebreak
    }
    if penalty == 0 {
        break; // unbeatable; next rooms are id-greater and same or worse penalty
    }
}
let Some((room_id, room_penalty)) = best_room else {
    continue;
};
let total_score = slice_score.saturating_add(room_penalty);
```

`BlockCandidate` (currently `solve.rs:~360`) gains a `slice_score: u32` field; the existing `score: u32` becomes `total_score`. Window pruning (`if score >= b.score continue`) compares on total; the inner-loop `continue` after `score_pruned` triggers on `slice_score >= best.total_score` since `slice_score` is the lower bound on this window's achievable total (`home_room_penalty >= 0`). Early-exit at the bottom (`if score == state.soft_score break`) becomes `if total_score == state.soft_score break`, which fires only when both slice delta and home-room delta are zero (a window that adds no gaps and lands in a home room for every member class).

The persist site (`solver/solver-core/src/solve.rs:771`, `state.soft_score = c.score`) becomes `state.soft_score = c.slice_score`, holding the slice contract.

### Building the home-room lookup

`lahc::run`'s prelude (`solver/solver-core/src/lahc.rs:108-160` ish) already builds several lookups; add the home-room one:

```rust
let home_room_lookup: HashMap<SchoolClassId, Option<RoomId>> = problem
    .school_classes
    .iter()
    .map(|c| (c.id, c.home_room_id))
    .collect();
```

Pass through to `rr_attempt`'s `try_place_block` call. The greedy bootstrap site (`solve_with_config` in `solver/solver-core/src/solve.rs`) builds the same lookup at the same place; thread it through.

### `try_place_group` audit

`try_place_group` (`solver/solver-core/src/solve.rs:810`) handles lesson-group co-placement. Audit during implementation: if it shares the same id-order room scan, apply the same fix in the same commit. If it picks rooms differently (e.g. constrained by the per-member union of feasibility), keep the fix scoped to `try_place_block` and add a TODO comment with a follow-up bullet to OPEN_THINGS.

### Tests

**Unit test (in `solve.rs::tests`):** `try_place_block_room_picker_minimises_home_room_penalty`.

- 1 class with `home_room_id = Some(R0)`.
- 2 feasible rooms `R0`, `R1` (both pass suitability and same-room-lock checks).
- 1 free time-block window.
- `weights.prefer_home_room = 100`, every other weight 0.
- Call `try_place_block` directly with a hand-built `GreedyState`. Assert the placed `Placement.room_id == R0`.
- Sibling test `try_place_block_room_picker_falls_back_to_id_order_when_no_home_room_advantage`: same shape but `home_room_id = None`. Assert the lowest-id feasible room wins (no behavioural regression on the FFD greedy default).

**Integration test (in new `solver/solver-core/tests/rr_recreate_soft_aware.rs`):** `lahc_rr_recreate_picks_lowest_soft_delta`.

- Hand-built 3-lesson `Problem`. Class `c1` with `home_room_id = Some(R0)`. Rooms `R0` (home), `R1`. Construct so FFD greedy under the OLD picker would land lesson L1 in R1 (busy R0 at the FFD-chosen window, then R&R ruins L1, the OLD picker would re-land in R1 because R1 is id-feasible first; the NEW picker re-evaluates and prefers R0 once it's free).
- Run `solve_with_config` with the LAHC config that enables R&R (`lahc_rr_period = 1` so R&R fires on iteration 1; `max_iterations = Some(small)` so the test is wall-clock cheap). Assert L1's final placement room is R0.
- Sibling assertion: `placements.len() == sum(lesson.hours_per_week)` (validates the placement-count contract is not regressed).

**Property tests:** the existing `lahc_property::lahc_rr_never_decreases_placement_count` and `lahc_rr_kempe_never_decreases_placement_count` already cover the placement-count contract over a wider input space; they re-run automatically.

### Bench gate

Run `mise run bench` after the change. Compare deltas against current `BASELINE.md`:

- `solver_greedy/grundschule`: ~99 µs baseline. Acceptable absolute-µs delta: ≤20 µs.
- `solver_greedy/zweizuegig`: ~600 µs baseline. Acceptable: ≤120 µs.
- `solver_greedy/dreizuegig`: ~1100 µs baseline. Acceptable: ≤220 µs.
- `solver_lahc/grundschule`: bounded by deadline; the FFD bootstrap cost is dominated by the LAHC loop. Expect noise.

Cite the deltas in the PR body. If any regresses beyond the absolute-µs gate, the mitigation lever is the in-room-loop early-break on `penalty == 0` (already in the proposed shape above): if a home-room match is found early in the id-order, no later room can be strictly better (next rooms are id-greater and at-best-tied on penalty). If that's already on and still over budget, the K-best capping path becomes the next mitigation.

If the bench numbers improve (FFD picks home rooms up front, less downstream R&R work), refresh `BASELINE.md` in the same PR via a separate `bench(solver-core): refresh BASELINE.md after recreate soft-aware picker` commit so the next algorithm-phase PR has an accurate floor.

### Determinism

The picker's tiebreak after the change is `(total_score, day, start_pos, room.id)`. `room.id` is last; `room_order` iterates by id; strict `<` updates only on strictly-lower penalty. Determinism property tests (`lahc_property::lahc_deterministic_under_seed_and_iter_cap`) re-run unchanged.

The 5×128 PROPTEST_CASES sweep on the integration test confirms determinism per the new code path.

## Acceptance criteria

- `cargo nextest run -p solver-core` is green, including the two new tests.
- `mise run lint` is green.
- `mise run bench` shows no fixture regressing beyond the absolute-µs gates above.
- 5×128 PROPTEST_CASES sweep on `solver/solver-core/tests/lahc_property.rs` (`lahc_rr_never_decreases_placement_count`, `lahc_rr_kempe_never_decreases_placement_count`, `lahc_deterministic_under_seed_and_iter_cap`) is green; any failing seeds get pinned in `lahc_property.proptest-regressions`.
- OPEN_THINGS item 49 deleted; cross-references in items 11, 14, 21, 47 updated to drop the "after item 49" gating.
- `solver/CLAUDE.md` carries the slice-vs-total split rule on `try_place_block`'s picker (one paragraph, in the existing solver-core rules section).
- Auto-memory `project_roadmap_status.md` next-pickup pointer rolls forward to item 21 (or the next still-open follow-up).

## Risks accepted

- FFD inner-loop cost increase. Bounded at `room_count` per window. Mitigated by `penalty == 0` early-break (typical case once home rooms exist).
- The home-room weight in `PRODUCTION_ACTIVE_WEIGHTS` is dominated by something else and `worst_home_med` does not recover. Surface in the PR body via the partial bake-off smoke at downscaled budget; do not flip this PR's gate.
- A property test under the widened `lahc_small_problem` generator surfaces a determinism flake on the new picker. Mitigated by the deterministic tiebreak and the 5×128 sweep gate; pin any failing seed in `lahc_property.proptest-regressions` before commit.

## Anchors

- Brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run, sequential self-answered Q&A).
- OPEN_THINGS: `docs/superpowers/OPEN_THINGS.md` item 49.
- Source: `solver/solver-core/src/solve.rs:377-775` (try_place_block), `solver/solver-core/src/lahc.rs:816-1004` (rr_attempt), `solver/solver-core/src/score.rs:300-318` (home_room_penalty).
- Bench: `solver/solver-core/benches/BENCH_RESULTS.md` (production refresh PR #199), `solver/solver-core/benches/BASELINE.md` (criterion floor).
