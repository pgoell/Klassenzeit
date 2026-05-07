# Strict `<` on `try_place_block`'s window-level best-candidate selection (item 60)

**Sprint program.** Solver feasibility correctness + observability (active program), follow-ups bucket (`## Open solver follow-ups`).
**Phase.** Open follow-up: item 60 (P2 tidy).
**Goal.** Make the FFD greedy picker's cross-window comparison deterministic on `total_score` ties so the FIRST-walked feasible window wins, mirroring the strict-`<` rule already in force inside the room scan.

**Non-goals.** No change to `score::score_solution`, `quality_report`, or any `ConstraintWeights` field. No change to LAHC's accept criterion, R&R, or Kempe. No new picker tiebreak axis (item 54 already added `class_day_balance` into `total_score`). No fixture changes; no new property tests. No production-default ADR revisit (item 47).

## Context

`solver-core/src/solve.rs::try_place_block` ranks `BlockCandidate`s with the lowest-`total_score` rule:

```
total_score = slice_score + home_room_penalty + class_day_balance_post
```

Two distinct strictness rules are at play in the picker:

1. **Room scan within a window.** `solve.rs:730-742` keeps the room with the lowest `home_room_penalty`, with strict `<` plus a `penalty == 0` early-break. Pinned by `try_place_block_room_picker_falls_back_to_id_order_when_no_home_room_advantage`: with `room_order = [1, 0]` and no home-room advantage, the picker keeps R1 because `0 < 0` is false. The determinism contract is "callers wanting lowest-id-wins-on-tie must pass `room_order` already sorted by id."
2. **Cross-window comparison.** `solve.rs:782-790` overwrites `best = Some(BlockCandidate {...})` unconditionally on every non-pruned candidate. The pruning bound `slice_score >= b.total_score` filters which windows enter the comparison, but among the survivors the LAST-walked feasible candidate wins on tied `total_score`.

The asymmetry is documented in `solver/CLAUDE.md`:

> **`try_place_block`'s window-level picker overwrites `best` unconditionally on every non-pruned candidate.** The strict `<` documented elsewhere is for the room scan within a window (lowest `home_room_penalty` wins), not for the cross-window comparison. ... Picker tests that assert an exact tb-id outcome are brittle to fixture ordering and to the early-exit fire pattern; assert behavioral outcomes ... instead. Item 54 surfaced this; preserve, revisit only if a bench cell shows it as load-bearing.

Item 54's targeted balance-on test illustrates the brittleness: the post-place class_day_balance cost on day 2 and day 3 are both 3 (total_score 15 each), so the test had to be reshaped to `chosen_day == 2 || chosen_day == 3` because the LAST-walked overwrite picks day 3 deterministically.

`tb_order` is sorted by `(day_of_week, position, time_block_id)` once per solve (per `solver/CLAUDE.md` "Lowest-delta greedy iterates pre-sorted indices"). With strict-`<` on the cross-window comparison, "lowest (day, position, tb_id) wins on tied `total_score`" becomes the deterministic rule — symmetric to "lowest room.id wins on tied `home_room_penalty`" inside the room scan.

The OPEN_THINGS gate ("Land only if a future BENCH_RESULTS.md refresh shows a cell where the unconditional overwrite is load-bearing") was written defensively. The production-shape refresh remains blocked behind item 15 (zweizuegig criterion panic), so the gate cannot be cleared today via the literal procedure. The change still has independent merit as a determinism / symmetry tightening, and the existing tests already prove the new rule is safe (see Test Plan below).

Anchor items: `docs/superpowers/OPEN_THINGS.md` item 60 (open follow-up), item 54 (the test that surfaced the LAST-walked behavior), item 15 (BENCH_RESULTS production-refresh blocker), item 47 (production-default ADR revisit).
Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Replace the unconditional `best = Some(BlockCandidate {...})` assignment at `solver/solver-core/src/solve.rs:782` with a strict-`<` guard:

    ```rust
    if best.as_ref().is_none_or(|b| total_score < b.total_score) {
        best = Some(BlockCandidate { /* ... */ });
    }
    ```

- Tighten `try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight` (`solve.rs:2964`) from `chosen_day == 2 || chosen_day == 3` to `chosen_day == 2`, plus rewrite its docstring (currently: "The last-walked feasible window wins, which is day 3").
- Re-anchor the comment cluster at `solve.rs:622-625` so its phrasing "Tiebreak (day, start_pos, room.id) preserved via strict `<`" describes the now-end-to-end strict-`<` rule honestly.
- Rewrite the `solver/CLAUDE.md` paragraph that documents the LAST-walked behavior to describe the strict-`<` rule and the symmetry argument with the room scan; remove the "preserve, revisit only if a bench cell..." escape hatch (the symmetry is the standing rule now).
- Verify with `cargo nextest run -p solver-core`, `mise run lint`, and a smoke `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig` (~30 s wall-clock) so the PR body cites the no-regression result.

**Out of scope.**

- `try_place_group`'s window-level picker (different signature, no item 60 reference; revisit if a future bench cell flags it).
- LAHC's Change-move room picker (`solve.rs::pick_room`) — already strict-`<`, unchanged.
- Production-shape `mise run bench:bakeoff` refresh (blocked behind item 15).
- New property tests; existing `lahc_property` and `score_property` suites stay unchanged.

## Components touched

- `solver/solver-core/src/solve.rs` — the assignment site (one block), the test docstring + assertion (one block), the pruning-comment cluster (one comment).
- `solver/CLAUDE.md` — one paragraph rewritten.

No backend, frontend, ADR, ORM, or migration changes. No `Problem` or `Solution` field changes. No PyO3 binding changes; no `mise run solver:rebuild` cascade.

## Behavioral contract after the change

For `try_place_block`'s lesson loop:

1. Walk windows in `tb_order` (sorted by `(day_of_week, position, tb_id)`).
2. For each window, run the existing pruning, lock-conflict, room-scan, and `class_day_balance` post-place computation. Compute `total_score = slice_score + home_room_penalty + class_day_balance_post`.
3. Update `best` iff `total_score < best.total_score` (or `best` is `None`). This is the new behavior.
4. Early-exit unchanged: `total_score == state.search_score_slice` still breaks the loop.

The result: among feasible windows that tie on `total_score`, the FIRST-walked window (lowest `(day_of_week, position, tb_id)`) wins. Symmetric to the room-scan rule.

## Test plan

**Existing tests that must still pass without modification:**

- `try_place_block_room_picker_minimises_home_room_penalty`: single-window fixture (`tb_order = [0]`), so cross-window comparison never engages. Unaffected.
- `try_place_block_room_picker_falls_back_to_id_order_when_no_home_room_advantage`: same single-window shape. Unaffected.
- Every property test in `solver-core/tests/lahc_property.rs` (placement-count never decreases, R&R never decreases, Kempe never decreases, determinism under seed + iter cap, canonical-non-increasing). All operate end-to-end through `solve_with_config`; deterministic FFD seeds are still deterministic, and the change moves a tied tiebreak from "day 3" to "day 2" without altering placement count or feasibility.
- `solver-core/tests/score_property.rs`, `tests/grundschule_smoke.rs`, `tests/ffd_solver_outcome.rs`, `tests/rr_anchor_filter.rs`. None depend on the LAST-walked-wins rule.

**One existing test gets a tightened assertion:**

- `try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight` currently asserts `chosen_day == 2 || chosen_day == 3` because the LAST-walked overwrite picked day 3 on the cost-3-vs-cost-3 tie. After the change, the picker walks day 1 first (best total = 25), then day 2 (15 < 25, best = 15), then day 3 (15 < 15 is false, day 2 keeps the lead). The new assertion is `chosen_day == 2`, and the docstring explains the strict-`<` outcome.

**TDD red-green:**

- Red: tighten the existing assertion + docstring first; `cargo nextest run -p solver-core --test ... try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight` (or just unit-test name) FAILS on the LAST-walked-wins behavior (lands on day 3, which is no longer accepted).
- Green: apply the strict-`<` guard at `solve.rs:782`. The same test now lands on day 2 and passes.
- Refactor: re-anchor the pruning comment cluster and rewrite the CLAUDE.md paragraph; nothing in the test suite changes again.

**Smoke bench (manual, in PR body, not CI-gated):**

`mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig --out /tmp/strict-lt-smoke.md` (~30 s).
Acceptance: `lahc`, `lahc_rr`, and `lahc_rr_kempe` rows on grundschule and zweizuegig do not regress feasibility, `worst_spread`, or `home_room_miss` against current master at the same downscale. Master has no committed `BENCH_RESULTS.md` smoke row at this shape, so the comparison is "branch smoke vs master smoke run side-by-side" rather than "branch vs committed table." CP-SAT cells will fail with `ModuleNotFoundError` since this branch does not run `mise run solver:rebuild`; the noise is acceptable per `solver/CLAUDE.md` because the change is Rust-only and the LAHC cells are independent of the wheel.

## Performance considerations

The new guard is one branch per non-pruned candidate. Pruning already filters most windows. On the hottest fixtures (`zweizuegig` 196 placements × candidate scan) the marginal cost is one `Option::is_none_or` plus an integer compare per surviving candidate — well below the noise floor of any `BASELINE.md` cell. No allocation. No new `score::*` calls.

The change does not alter the lower-bound pruning predicate (`slice_score >= b.total_score`) and does not weaken the early-exit (`total_score == state.search_score_slice` still breaks).

## Risks

- **Day-2-or-day-3 test brittleness check.** The single existing test that tolerated either day under the LAST-walked behavior is the one we tighten. No other test currently asserts a window outcome on a tied `total_score`. Mitigation: read every `try_place_block_*` test before the change; if any assert a tb_id outcome that is not the lowest-tb_id-wins shape, file a follow-up. Reviewed during brainstorm; none found.
- **Smoke bench surfacing a regression.** If grundschule or zweizuegig regresses on `worst_spread`, `home_room_miss`, or feasibility under the smoke shape, the change is reverted and the OPEN_THINGS item escalates to "blocked on item 15 + bench refresh." Mitigation: smoke runs on the feature branch, not master; PR reviewers see the side-by-side numbers.
- **Stale doc drift.** Three places document the LAST-walked rule explicitly; missing one leaves the comment lying about the code. Mitigation: spec lists all three sites (`solve.rs:782` neighbours, `solve.rs:2959` docstring, `solver/CLAUDE.md` paragraph).

## Acceptance

- `cargo nextest run -p solver-core` is green; `try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight` asserts day 2 and passes.
- `mise run lint` is green.
- `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig` shows no feasibility / `worst_spread` / `home_room_miss` regression on `lahc*` rows vs a same-shape smoke on master.
- `solver/CLAUDE.md`'s LAST-walked paragraph reads as the strict-`<` rule with the symmetry argument; no "preserve, revisit only if a bench cell..." text remains.
- OPEN_THINGS item 60 deleted (the autopilot's "delete on ship" rule).
