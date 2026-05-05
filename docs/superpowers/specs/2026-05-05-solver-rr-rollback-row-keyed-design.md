# Port Kempe-style row-keyed rollback into `rr_attempt` spec (active sprint, item 37)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Algorithm phase: item 37.
**Goal.** Stop the residual silent placement-drop in R&R / Kempe-on-top-of-R&R that survived items 26 + 27, and lock the same class of bug out at the rollback boundary.

**Non-goal.** No bench refresh of `solver/solver-core/benches/BENCH_RESULTS.md` (item 29). No ADR 0032 (item 29). No Python-side auto-assign solvability tests (item 32). No new `SolveConfig` fields. No production-default move (also item 29). No additional R&R move primitives (rescue / Kempe lesson-group co-swap stay deferred).

## Context

`rr_attempt` (`solver/solver-core/src/lahc.rs:738`) snapshots one `BlockSnapshot` per chosen `(lesson_id, day_of_week)` anchor, ruins each via `rr_ruin_block` (which removes every same-lesson-same-day placement), recreates each by calling `try_place_block` once, and on any failure delegates to `rr_rollback` (`lahc.rs:854`). `rr_rollback` walks the `recreated_in_order: Vec<LessonId>` list and undoes each entry by:

```rust
if let Some(idx) = placements.iter().position(|p| p.lesson_id == *lesson_id) {
    rr_ruin_block(idx, lesson, tb_lookup, placements, state);
}
```

`rr_ruin_block` is then called against the FIRST placement of the lesson in the placements vector. Its day is whatever day that first placement happens to live on. For lessons that have multiple blocks across different days (grundschule's `Deutsch hours_per_week=6 preferred_block_size=1`, `Mathe h=5 N=1`, `Sport h=3 N=1`, etc.), the first found placement is rarely the recreate's destination day — it is much more likely to be one of the lesson's untouched-this-iteration original blocks.

The faulty rollback step then ruins one of the lesson's pristine blocks. The replay loop afterwards puts the snapshotted (originally-ruined-day) rows back, but never replays the rows that the bogus ruin just dropped because those rows were never snapshotted (they were not ruined by this iteration). Net effect per faulty rollback: `preferred_block_size` placements lost, the recreated block kept on its destination day even though the move was supposed to be rejected, and `state.soft_score` reset to `pre_score` so the score does not flag the inconsistency. The bake-off bench's existing per-cell `placements_total < expected` gate (item 28, PR #184) catches the symptom but not the cause: the dev-loop receipt at `--budget 5s --seeds 4 --fixtures grundschule` shows `lahc_rr` and `lahc_rr_kempe` both at median 19/45 with `hard_med=0`, while `lahc` and `cpsat` reach 45/45 on the same problem. `feasibility 0/4` because `placements_total < expected` flips the gate, but `hard_med=0` because the violation list is silent about the missing rows.

The Kempe move already gets this right. `kempe_rollback` (`lahc.rs:1651`) walks `recreated: &[(LessonId, u8, u8)]` (lesson, dest_day, dest_start_pos) and removes EXACT rows by `(lesson_id, time_block_id)` resolved through `tb_by_day_pos`. `solver/CLAUDE.md` documents this pattern and applies it explicitly to "any future LAHC move that ruins on one (lesson, day) and recreates on another", which is exactly R&R's shape. R&R predates the rule and was not retrofitted when Sprint 3 shipped Kempe.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 37. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run). Reproduces on master tip 02fe443.

## Scope

**In scope.**

- Replace `rr_attempt`'s `recreated_in_order: Vec<LessonId>` with a per-recreate row capture: snapshot `placements.len()` immediately before each `try_place_block` call, then `placements[len_before..].to_vec()` after a successful recreate. Store as `recreated_rows: Vec<Vec<Placement>>` parallel to `snapshots`.
- Replace the body of `rr_rollback`'s first loop with a row-keyed remover: for each captured row, find the placement by exact match on `(lesson_id, time_block_id, room_id)` and remove it, decrementing the matching `state` bookkeeping (mirroring the per-row removal in `rr_ruin_block`'s inner loop). Iterate captured row sets in reverse order (later recreates first) and rows within a set in reverse order (so vec indices do not shift while we operate on later rows). The replay loop over `snapshots` is unchanged.
- Add a defensive guard inside `rr_attempt`: if any successful recreate landed on a day where the same lesson already has a placement that was NOT part of this iteration's snapshot, treat the move as a no-op and roll back to the pre-attempt state. Cost is one `HashSet<(LessonId, u8)>` lookup per recreated block. Mirrors the spirit of `rr_collect_anchors`'s "one block per `(lesson, day)`" invariant on the destination side.
- Add `assert!(post_count == pre_count, "rr_attempt left placements imbalanced")` after a successful R&R (count must be invariant; R&R is by construction count-preserving when `failed_recreates == 0`). Add `assert!(post_count == pre_count, "rr_rollback left placements imbalanced")` after rollback completes. Both are O(1) `placements.len()` comparisons; both run in release.
- New regression test `solver/solver-core/tests/rr_rollback.rs`: build a problem where a single lesson with `hours_per_week >= 3, preferred_block_size = 1` is forced to place blocks on multiple days (no other tight constraints), run `lahc_rr` at a 50 ms deadline so the rollback path is reachable, assert `lahc_rr.placements.len() == greedy.placements.len()` over multiple seeds (1..=8). Red without the fix, green with it.
- Widen `solver/solver-core/tests/lahc_property.rs::lahc_small_problem`'s lesson generator so `hours_per_week` covers `2..=4` (today it is fixed at `2`). Bump `lahc_rr_cfg`'s deadline from 20 ms to 50 ms so the rollback path is reachable inside the property cases. The two existing property tests `lahc_rr_never_decreases_placement_count` and `lahc_rr_kempe_never_decreases_placement_count` pin the invariant once the generator surfaces multi-block-across-days lessons.
- Update `solver/CLAUDE.md`'s "Ruin+apply rollback shape: remove exact placements, do not re-ruin by lesson+day" bullet to mention that the pattern now applies to BOTH R&R and Kempe (today's wording phrases it as a Kempe-specific learning that R&R should follow). One-line edit.
- Re-run `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule --out /tmp/...` after the fix and paste the receipt into the PR body.
- Update `docs/superpowers/OPEN_THINGS.md`: delete item 37, promote item 29 to "next pickup" in the active-sprint header.

**Out of scope.**

- Refresh of `solver/solver-core/benches/BENCH_RESULTS.md` at production settings. That is item 29's job; the PR body cites item 29 as the next pickup.
- ADR 0032 (revisit ADR 0031's production-default decision). Item 29.
- Item 30 (peak RAM / time-to-first-feasible / time-to-optimal columns) and item 31 (schedule-quality bake-off output). Stay in their phase order.
- Item 32 (Python-side auto-assign solvability tests). Stays in its phase.
- Item 24 (Kempe lesson-group co-swap), item 22 (`lahc_kempe` standalone bench backend), item 21 (RR_K / period sweep): all stay queued behind the active sprint.
- Refactor of `rr_attempt` to share row-removal code with `rr_ruin_block`. Tempting (the row-decrement bookkeeping is duplicated) but per CLAUDE.md "Don't add features, refactor, or introduce abstractions beyond what the task requires"; revisit only if a third call site materialises.
- Any change to `solver-py` bindings (no Python-visible behaviour change), backend, or frontend code.

## Failure mode and fix

**Trigger.** Any R&R iteration where:

1. The chosen anchor's lesson has at least one OTHER block on a different day, untouched by this iteration's ruins.
2. The recreate's `try_place_block` lands on a different day than the anchor's day.
3. Some OTHER recreate (or the same one through a different move chain) fails, prompting `rr_rollback`.

Grundschule's lesson distribution makes this fire on most R&R iterations: out of 15 lessons, 11 have `hours_per_week >= 2` with `preferred_block_size = 1`, so they each place across multiple days, and any of them can sit at the FRONT of the placements vector.

**Fix shape.** Capture the rows added per recreate, remove by exact id at rollback time. Concretely:

```rust
// before: Vec<LessonId>
let mut recreated_rows: Vec<Vec<Placement>> = Vec::with_capacity(snapshots.len());
for (lesson_id, _snap) in snapshots.iter() {
    let lesson = lesson_lookup.get(lesson_id).expect("ruined lesson must resolve");
    let n = lesson.preferred_block_size;
    let len_before = placements.len();
    let placed = crate::solve::try_place_block(/* ... */);
    if !placed {
        failed_recreates += 1;
    } else {
        recreated_rows.push(placements[len_before..].to_vec());
    }
}
```

```rust
fn rr_rollback(
    recreated_rows: &[Vec<Placement>],
    snapshots: &[(LessonId, BlockSnapshot)],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
) {
    for rows in recreated_rows.iter().rev() {
        // Resolve each captured row to its current placement-vec index.
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
            decrement_row_bookkeeping(lesson, &p, tb_lookup, state);
        }
    }
    for (lesson_id, snapshot) in snapshots.iter().rev() {
        // unchanged from today
    }
}
```

`decrement_row_bookkeeping` extracts the per-row state-mutation from the inner loop of `rr_ruin_block` (one row, one teacher slot, one set of class slots, one room slot, one teacher hour, one locked_room counter). Since `rr_ruin_block` itself loops "all placements of (lesson, day) in reverse" and applies that same per-row decrement, the helper is the exact body of that inner loop. Per CLAUDE.md "bundle a new `pub(crate)` helper with its first caller in the same commit" — the helper plus the new rollback land atomically.

**Defensive guard.** Build the snapshotted lessons' `(lesson, day)` set BEFORE the recreate loop:

```rust
let snapshotted: HashSet<(LessonId, u8)> = snapshots
    .iter()
    .map(|(lesson_id, snap)| {
        let day = snap.rows
            .first()
            .and_then(|r| tb_lookup.get(&r.time_block_id))
            .map(|tb| tb.day_of_week)
            .expect("non-empty snapshot resolves day");
        (*lesson_id, day)
    })
    .collect();
```

After each successful recreate, check whether the lesson now has a placement on a day other than the one snapshotted for it. If yes, treat as if the recreate failed (increment `failed_recreates`, fall through to the rollback branch). The captured-rows pattern ensures rollback removes only the recreate's rows even in that degraded path.

## Determinism and bench impact

Determinism: the fix changes only the rollback's row-removal logic plus a defensive guard; no new RNG draws, no reordering of the existing draws, no change to the LAHC acceptance branch. The R&R determinism property test `lahc_rr_deterministic_under_seed_and_iter_cap` should pass byte-identically post-fix.

Bench: `BASELINE.md` covers FFD greedy plus LAHC change-move performance via the criterion bench (`solver-core/benches/solver_fixtures.rs`); R&R is exercised inside the bake-off bench (`solver-bench`) which is not committed to BASELINE.md. The fix should improve `lahc_rr` and `lahc_rr_kempe` cells dramatically (from `19/45 hard=0 feasibility 0/4` to plausibly `45/45 hard=0 feasibility N/4` at the dev-loop budget). Full `BENCH_RESULTS.md` refresh stays gated behind item 29.

## Tests

1. **`solver-core/tests/rr_rollback.rs` (new)** — targeted regression. Build a Problem with one class, one teacher, three rooms, one subject, one lesson `hours_per_week=4, preferred_block_size=1` plus three filler lessons of `hours_per_week=1, preferred_block_size=1` for that class to provide enough scheduling pressure. 5 days × 5 positions. Run `lahc_rr` at 50 ms deadline for seeds 1..=8. Assert `lahc_rr.placements.len() == greedy.placements.len()` on every seed. Red without the fix, green with it.
2. **`solver-core/tests/lahc_property.rs` widening** — change `lahc_small_problem`'s lesson `hours_per_week` from constant `2` to `prop::sample::select(vec![2u8, 3u8, 4u8])` (or `2..=4u8`). Bump `lahc_rr_cfg`'s deadline from `Duration::from_millis(20)` to `Duration::from_millis(50)`. The two existing property tests automatically discriminate this bug-class. Cost: ~1 second extra wall-clock per property test (32 cases × 30 ms extra), absorbed by the existing test budget.
3. **`solver-core/tests/rr_anchor_filter.rs`** — no change. The filter still applies; this PR adds a separate guard.
4. **`mise run lint` + `mise run test:rust`** — both must stay green.

## Documentation

- `solver/CLAUDE.md`: update the "Ruin+apply rollback shape: remove exact placements, do not re-ruin by lesson+day" bullet so its applicability is "every chain-style or multi-block local-search move (R&R, Kempe, future)" rather than "any future LAHC move that ruins on one (lesson, day) and recreates on another". One-sentence rephrase plus a back-reference to this spec.
- `docs/superpowers/OPEN_THINGS.md`: delete item 37 (closed by this PR), promote item 29 to next pickup, leave the rest of the active sprint and queued sprints alone.

## Acceptance criteria

- New regression test green; both property tests green at the widened generator + bumped deadline.
- `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule` shows `lahc_rr` and `lahc_rr_kempe` at `placements_med=45/45` with feasibility >= `lahc`'s.
- `mise run lint` green.
- PR description carries the dev-loop bake-off receipt before / after.
