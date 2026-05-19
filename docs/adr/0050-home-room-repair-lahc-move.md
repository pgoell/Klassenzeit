# 0050: Home-Room Repair LAHC Move

- **Status:** Accepted
- **Date:** 2026-05-19

## Context

On dreizuegig (3-track) Grundschule fixtures at production budget (60000 ms `lahc_rr`, ~294 placements), all four LAHC backends plateau at `home_room_miss ≈ 139` (≈47% of placements not in the class's home_room). The 2026-05-15 pinned medians were `(139, 138, 139, 139)` across `(lahc, lahc_rr, lahc_kempe, lahc_rr_kempe)`. FFD greedy picks `home_room` only as a slice-tied tiebreaker; the existing LAHC moves (`try_change_block_move`, `try_swap_move`, R&R, Kempe) reshape time-block geometry, not rooms, so the neighbour explore lifts < 1 home_room miss per second of search at the plateau.

The 2026-05-16 attempt on option (a) of OPEN_THINGS item 86 (promote `home_pairs` to FFD secondary sort key) was reverted: the 20-iter `test_grundschule_schedule_meets_quality_bar` flake-loop denied at 18/20 with both failures on a different axis (`interior_gap` on einzuegig classes 1a/2a). Profile rule 12 forecloses an FFD-side fix at this granularity. The remaining option (b) attacks the room axis with the time block held fixed, orthogonal to Change and to the FFD seed.

## Decision

Add a new LAHC move `try_home_room_repair_move` to `solver/solver-core/src/lahc.rs`. At a fixed time block, the move attempts two paths in order:

- **(A) Room-free path:** home_room is free at the placement's time block. Lock the destination room to home_room via `pick_room(lock: Some(home_room))`; accept on canonical-delta with the existing LAHC list criterion.
- **(B) Collision-swap path:** home_room is occupied by exactly one other placement Q at the same time block. Verify Q's subject is suitable in P's old room; swap rooms (P → home_room, Q → P's old room); accept on canonical-delta. Q's own multi-class / home_room status does not restrict the swap; the canonical delta captures Q's `prefer_home_room` change so an LAHC accept that worsens Q's home_room placement rejects on score.

The move is gated by a new `SolveConfig.lahc_home_room_period: Option<u32>` field (default `None`). Production backends (`lahc`, `lahc_rr`, `lahc_rr_kempe`) opt in at `Some(7)` via `backend/src/klassenzeit_backend/scheduling/solver_io.py`; the bench mirrors this default and exposes a `--home-room-period <u32>` override. The R&R / Kempe / Home-Room / Change precedence ladder fires R&R first (rescue mechanism for feasibility), Kempe second (conflict-chain mechanism for tight constraints), Home-Room third (soft-quality smooth axis), Change/Swap last. A dedicated `home_room_rng = SmallRng::seed_from_u64(seed.wrapping_add(3))` channel keeps the existing `change_rng` / `rr_rng` / `kempe_rng` streams byte-identical when the new period is `None` or fires at `u32::MAX` (covered by a determinism property test in `lahc_property.rs`).

The move scope is n=1 cells only. Doppelstunden (`preferred_block_size > 1`), grouped lessons (`lesson_group_id.is_some()`), pinned placements, classes without a `home_room`, and multi-class lessons with mismatched per-member home_rooms all reject early. Per-iter RNG cost on a home-room iter is exactly one `random_range` draw (placement index).

## Alternatives considered

- **Add a `move_selector` slot inside the Change branch (rebalance 5:1:1).** Rejected: reshuffles the RNG draw layout in the Change/Swap branch and perturbs every existing test seed. Blast radius too high for a quality-axis fix.
- **Replace the Swap selector branch with home-room moves.** Rejected: Swap is the only existing same-time-block move; deleting it regresses other axes the bench has been tuned against.
- **Greedy accept on home_room delta only.** Rejected: option-a's failure shows that home_room "fixes" can tip a more important axis (interior_gap, class_day_balance). Canonical-accept is the only safe ladder.
- **FFD-side reseeding (option a).** Tried 2026-05-16; reverted per profile rule 12 on a different-axis denial.
- **Three-way room swap (P-Q-R cycle).** Out of scope for this PR. Reserved as a future move if the cell-level (A)+(B) hybrid plateaus.
- **Block-aware variant for Doppelstunden.** Out of scope; the residual block-level miss is not the dominant share of `home_room_miss`. File a follow-up if a future bench refresh shows the cell-level move closed the cell axis but block-level miss remains material.

## Consequences

- New PyO3 kwarg `lahc_home_room_period` on `klassenzeit_solver.solve_json_with_config` / `solve_json_with_progress`; default `None` so external callers see no behavioural change.
- All three LAHC backends in `solver_io.py` pass `lahc_home_room_period=7` to the binding; CP-SAT is untouched (CP-SAT objective mirror already weights `prefer_home_room`).
- Bench's per-backend `SolveConfig` literal defaults to `Some(7)` so bake-off cells reflect production. `--home-room-period <u32>` overrides for A/B sweeps.
- OPEN_THINGS item 86 closes in this PR. The 20-iter `test_grundschule_schedule_meets_quality_bar` flake-loop at production budget is the ship gate (profile rule 9).
- BENCH_RESULTS.md is NOT refreshed in this PR; the production refresh ships in a separate post-merge PR per profile rule 8 (smoke validates the fix; production refresh refreshes data).
- Future quality-axis levers can mirror this shape: a dedicated periodic move with its own RNG channel, gated by a `SolveConfig.lahc_<axis>_period: Option<u32>` field, threaded through `solve_json_with_config` / PyO3 / `solver_io.py`. Reuse the precedence ladder when the axis is soft-quality (place after R&R/Kempe). Re-evaluate placement only if a new move competes with R&R/Kempe for the same conflict shape.

## Anchors

- ADR 0023 (home-room weight semantics).
- ADR 0030 (CP-SAT objective mirror).
- ADR 0037 (production-default `lahc_rr`).
- ADR 0038 (per-backend deadline configuration).
- ADR 0043 (precedent for canonical-delta soft-axis weight bumps).
- ADR 0049 (precedent for the production-budget flake-loop as a ship gate).
- OPEN_THINGS item 86 closed by this PR.
