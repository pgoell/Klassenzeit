# Class-day-balance as an FFD greedy search-time objective (item 54)

**Sprint program.** Solver feasibility correctness + observability (active program), follow-ups bucket (`## Open solver follow-ups`).
**Phase.** Open follow-up: item 54 (P0).
**Goal.** Move `class_day_balance` from "post-hoc scorer term and LAHC accept-time canonical" into the FFD greedy window picker so the construction phase steers toward balanced per-class day distributions instead of relying solely on LAHC's accept criterion to escape an unbalanced FFD seed.

**Non-goals.** No change to `score::score_solution` or to `quality_report`'s axis set (item 50). No change to LAHC's accept criterion or running-best snapshot (item 52). No new LAHC neighbour move. No CP-SAT objective port (item 48). No Timefold spike (item 55). No production-default ADR revisit (item 47). No refactor merging `try_place_block` and `try_place_group` into a shared scoring helper (separate tidy item).

## Context

After items 50 / 51 / 52 landed, the solver-core canonical objective is:

```
score_solution = w_class_gap * class_gaps
              + w_teacher_gap * teacher_gaps
              + w_class_day_balance * class_day_balance_cost
              + w_prefer_home_room * home_room_misses
              + subject_preference_terms
```

LAHC accepts and exits on this canonical (item 52); the `BackendObjective` declaration in `solver-core/src/quality.rs::build_backend_objectives` lists every `QualityComponent` as `optimised` for `lahc` / `lahc_rr` / `lahc_rr_kempe` and notes "item 54 reserved for FFD greedy-time class-day-balance tiebreak."

The remaining gap is the FFD greedy construction phase. `solver-core/src/solve.rs::try_place_block` ranks `BlockCandidate`s by:

```
total_score = slice_score + home_room_penalty(room)
```

`slice_score` is the post-place running cost `class_gap + teacher_gap + subject_pref`, persisted from `state.search_score_slice + class_delta + teacher_delta + subject_pref_delta`. `home_room_penalty` is the room-dependent term. `class_day_balance` is absent from the picker entirely, so any window that matches the slice tiebreak wins regardless of how it loads the per-class day distribution.

The 2026-05-06 `BENCH_RESULTS.md` (refreshed before item 52 landed) shows `worst_spread` failing on `zweizuegig` (5), `dreizuegig` (9), and `lock_in` (5) with hard feasibility 20/20 across every LAHC variant. Item 54's hypothesis: the FFD seed is so unbalanced that LAHC cannot recover within the 60-second deadline, so improving the seed via a balance-aware picker is the cheapest lever.

`try_place_group` (the lesson-group co-placer that handles `dreizuegig`'s per-Jahrgang religion trio: 12 of 102 placements) shares the same gap. Its picker scores by `slice` only (no `home_room_penalty`, no `class_day_balance`), and the group placement amplifies spread asymmetries because each group decision adds one lesson to every member class's day count simultaneously.

Anchor items: `docs/superpowers/OPEN_THINGS.md` item 54 (active follow-up), 50 / 51 / 52 (canonical objective lineage), 14 (the xfail removal that depends on `worst_spread <= 2`), 48 (CP-SAT objective port).
Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- New helper in `solver-core/src/score.rs`:
  - `pub(crate) fn class_day_balance_cost_for_class_after_add(class_id: SchoolClassId, days: u8, class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>, add_day: u8, add_n: u8) -> u32`. Allocation-free, mirrors the shape of `class_day_balance_cost_for_class_with_swap`. Returns the per-class scaled L1 day-balance cost as if `add_n` placements were appended on `add_day`.
- Changes to `solver-core/src/solve.rs`:
  - Compute `days: u8 = problem.time_blocks.iter().map(|tb| tb.day_of_week).max().map(|m| m.saturating_add(1)).unwrap_or(0)` once before the FFD lesson loop in `solve_with_config`.
  - Add `days: u8` to the parameter list of `try_place_block` (already takes 14 args under `#[allow(clippy::too_many_arguments)]`; one more is consistent).
  - Add `days: u8` to the parameter list of `try_place_group`.
  - Inside `try_place_block`'s window loop, the existing pruning check `if let Some(b) = &best { if slice_score >= b.total_score { continue; } }` stays put. The bound is still sound: the current candidate's eventual full score is `slice_score + home_room_penalty + balance_post`, and both summands are non-negative, so `slice_score >= b.total_score` implies the candidate cannot beat `b`. After the room scan picks `room_id` and `room_penalty`, compute `balance_post = if weights.class_day_balance == 0 { 0 } else { weights.class_day_balance.saturating_mul(class_ids.iter().map(|c| class_day_balance_cost_for_class_after_add(*c, days, &state.class_positions, first_tb.day_of_week, n)).sum::<u32>()) }`. Set `total_score = slice_score.saturating_add(room_penalty).saturating_add(balance_post)`. Both `total_score` storage and the early-exit `total_score == state.search_score_slice` continue to operate on the (now-widened) total.
  - Inside `try_place_group`'s window loop, the same shape: compute `balance_post` from `class_set` (already built earlier in the function) and fold it into `score = slice_post.saturating_add(balance_post)`. The pruning check `if score >= b.score` still operates on the now-widened `score`; the early-exit `score == state.search_score_slice` still fires when the candidate is fully neutral.
- Doc string for the new helper plus updated doc strings on `try_place_block` and `try_place_group` reflecting the widened picker contract.
- Update `solver-core/src/quality.rs::build_backend_objectives` `lahc_notes` from `"LAHC accepts and exits on the full canonical (see lahc::run); item 54 reserved for FFD greedy-time class-day-balance tiebreak."` to `"LAHC accepts and exits on the full canonical (see lahc::run); FFD greedy ranks windows by slice + home_room + class_day_balance (item 54)."`. (The `optimised: BTreeSet<QualityComponent>` set is unchanged: it already contains every component.)
- Tests:
  - `solver-core/src/solve.rs` `tests` module: `try_place_block_picker_prefers_balanced_day_under_class_day_balance_weight`. Builds a 2-class, 4-day fixture where two earlier-placed lessons leave class A heavily loaded on day 0 and one class A lesson is now being placed; with `class_day_balance = 0` the picker chooses day 0 (lowest tb id), with `class_day_balance = 5` the picker chooses a less-loaded day. Assert the `placements.last().time_block_id` differs between the two configurations.
  - `solver-core/tests/score_property.rs`: `ffd_greedy_class_day_balance_weight_lowers_post_solve_class_day_balance_cost`. Reuses the existing `build_class_day_balance_problem` fixture; calls `solve_with_config` once with `class_day_balance = 0` and once with `class_day_balance = 5`; asserts the post-solve `class_day_balance_cost` from `score::class_day_balance_cost(...)` is strictly lower in the second run.
  - `solver-core/src/score.rs` `tests` module: `class_day_balance_cost_for_class_after_add_matches_post_apply_recompute`. Builds a small `class_positions` map; computes the helper's output; then mutates the map by appending `add_n` placements on `add_day` and computes `class_day_balance_cost_for_class` against the mutated map; asserts equality.

**Out of scope.**

- LAHC neighbour-move changes. Change/R&R/Kempe already maintain canonical (item 52); no determinism-affecting RNG changes.
- Adding `home_room_penalty` to `try_place_group`. The asymmetry vs `try_place_block` is pre-item-54 and orthogonal; if it surfaces as a regression in BENCH_RESULTS.md, file as a follow-up.
- Refactoring `try_place_block` / `try_place_group` to share scoring helpers. Separate tidy item if drift becomes annoying.
- Refresh of `BASELINE.md`. The criterion bench is end-to-end broken on master (item 15) for `zweizuegig`; partial signal from `solver_greedy/grundschule` is what's available. Include the criterion delta in the PR body; refresh `BASELINE.md` only if the delta exceeds 20% on the running grundschule numbers.
- Refresh of `BENCH_RESULTS.md`. The 5-hour wall-clock cost makes the refresh a deliberate post-PR step; the PR body cites the diff but does not block merge on a specific `worst_spread` value (the original OPEN_THINGS criterion `worst_spread <= 2 on grundschule + zweizuegig` depends on items 48 / 21 / 22 also landing, per the ADR 0032 follow-up notes).

## Acceptance criteria

1. `cargo build -p solver-core` and `cargo build -p solver-bench` compile cleanly with no new warnings.
2. `mise run lint` passes (rustfmt, clippy, machete, the unique-fns checker).
3. `cargo nextest run --workspace` passes in full.
4. The two new unit tests and one new property test exist and pass deterministically (5 × 128 sweep on the property test per the `solver/CLAUDE.md` widening discipline).
5. `solver-core/src/quality.rs::build_backend_objectives`'s `lahc_notes` no longer contains "item 54 reserved".
6. PR body cites the criterion delta on `solver_greedy/grundschule` (post-item-54 vs current master tip, `cargo bench -p solver-core --bench solver_fixtures -- 'grundschule'`); a regression beyond 20% halts the merge for triage.
7. Release-mode `cargo run --release -p solver-bench -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig` runs end-to-end without panic and reports `feasibility = 4/4` per cell. (Smoke; full-cell-shape refresh of BENCH_RESULTS.md is post-merge.)

## Risks

1. **Picker-scan latency regression.** Each window now adds `O(member_classes * days)` partition reads (≈12 × 5 = 60 reads at dreizuegig). Per-FFD-lesson cost grows by a constant factor. Mitigation: cite the criterion delta on grundschule + lahc/grundschule before merge. Refresh `BASELINE.md` only if delta >= 20%.
2. **Picker now prefers balance-improving candidates with higher slice cost, possibly degrading `class_gap_h` or `teacher_gap_h`.** The post-solve canonical score-net should be lower (since balance is in canonical and FFD seeds LAHC), but axis-by-axis the trade is opaque. Mitigation: post-merge `BENCH_RESULTS.md` refresh shows the per-axis diff; if any single column regresses, surface as a follow-up.
3. **Determinism risk in property tests.** No new RNG draws are introduced; the picker is deterministic over `(tb_order, room_order, state)`. The change should not affect `tests/lahc_property.rs::lahc_deterministic_under_seed_and_iter_cap`. Verify with `for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property; done`.
4. **Feasibility regression on lock_in or dreizuegig.** Balance-favoured windows could paint the picker into a corner where future placements are infeasible. The picker still chooses among hard-feasible windows only; the change only affects the *ordering* among feasibles. Mitigation: smoke-bench at `--budget 5s --seeds 4` checks `feasibility = N/N` per cell pre-merge.
5. **Pruning soundness.** The current `if slice_score >= b.total_score { continue; }` is sound iff `home_room + balance >= 0`. Both terms are absolute non-negative costs by construction, so the bound stays valid. Documented inline.

## Test plan

- `cargo nextest run -p solver-core` (workspace).
- `cargo nextest run -p solver-bench` (smoke).
- `for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test score_property; done` (property test sweep).
- `cargo bench -p solver-core --bench solver_fixtures -- 'grundschule'` twice; record the criterion delta in the PR body.
- `cargo run --release -p solver-bench -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig` smoke; assert `feasibility 4/4` per cell.
- `mise run lint`.

## References

- Anchor items: OPEN_THINGS items 50 (canonical objective lineage), 51 (backend objective parity), 52 (LAHC accepts on canonical), 54 (this), 14 (xfail removal that depends on worst_spread `<= 2`), 47 (production-default ADR revisit).
- Source: `solver/solver-core/src/solve.rs::try_place_block`, `solver/solver-core/src/solve.rs::try_place_group`, `solver/solver-core/src/score.rs` (`class_day_balance_cost_for_class`, `class_day_balance_cost_for_class_with_swap`, `class_day_balance_cost_for_class_from_counts`).
- ADRs: 0029 (bake-off methodology), 0030 (cross-backend `score_solution_json`), 0031 / 0032 (production default), 0033 (LAHC deadline raise), 0034 (bench cell subprocess + observability).
