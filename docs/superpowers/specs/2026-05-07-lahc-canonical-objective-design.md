# LAHC accepts and exits on the canonical objective (item 52)

**Sprint program.** Solver feasibility correctness + observability (active program), follow-ups bucket (`## Open solver follow-ups`).
**Phase.** Sprint-tidy phase: item 52 (P0).
**Goal.** Make LAHC's accept criterion, `time_to_optimal_ms` probe, and early-exit predicate operate on the canonical objective (`score_solution(problem, placements, weights)`, includes `prefer_home_room` and `class_day_balance`) instead of the running slice. Guarantee that the LAHC incumbent returned to the caller is non-increasing versus the post-greedy canonical.

**Non-goals.** No CP-SAT objective port (item 48). No greedy-time class_day_balance tiebreak (item 54). No bench refresh (item 47 owns the next ADR 0035 rerun). No Timefold spike (item 55). No change to `Solution.soft_score` API surface, `SolveStats`, JSON wire format, or Python binding signatures. No change to greedy's `try_place_block` picker scoring or persist contract.

## Context

`solver-core/src/lahc.rs::run` is the LAHC outer loop. Today it accepts and exits on `state.soft_score`, the running slice (`class_gap + teacher_gap + subject_pref`). The canonical objective `score_solution` adds two axes the slice does not measure: `prefer_home_room` (per-placement penalty for non-home-room placements) and `class_day_balance` (per-class L1 distance from a per-day mean count). After item 41 / item 51, `solve_with_config_stats` overwrites `solution.soft_score = score_solution(problem, placements, weights)` post-LAHC, so the user sees the canonical value but LAHC was steering on the slice.

Symptoms of the divergence:
1. **LAHC can accept moves that worsen canonical.** A Change move that drops a placement from a home-room into a feasible non-home-room while reducing slice by 1 has positive home_room delta of `weights.prefer_home_room`. Net canonical worsens, slice improves, LAHC accepts.
2. **`time_to_optimal_ms` probes the wrong incumbent.** The probe fires when the slice falls; it can fire while canonical is rising.
3. **Early-exit at `state.soft_score == 0` short-circuits before canonical is at the floor.** The LAHC loop exits at slice floor regardless of remaining home_room or class_day_balance cost.
4. **Final placements may be canonical-worse than greedy.** LAHC's late-acceptance allows accepting moves that worsen current cost; with no running-best snapshot, LAHC can return placements whose canonical exceeds the post-greedy canonical.

`solver/CLAUDE.md` documents the slice contract: greedy picker scores by total but persists slice; the Change-move debug_assert relies on slice non-negativity; R&R recomputes slice via `running_slice_from_placements` because `try_place_block` accumulates against slice; Kempe snapshots per-partition gap counts for delta. All four shapes assume `state.soft_score` is the slice. Item 52 must extend LAHC to accept on canonical without disturbing the slice contract.

The contract was already partially rehearsed: item 49 ranks R&R recreate candidates by `soft_score` delta including home_room (per the recent commit `dbcd6d2`); items 50 + 51 introduced `quality_report` and the static `BackendObjective` table that today declares `declared_skipped = {HomeRoom, ClassDayBalance}` for the LAHC variants and references item 52 in the notes.

Anchor items: `docs/superpowers/OPEN_THINGS.md` items 52, 51 (lineage), 50 (lineage), 47, 48, 54.
Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Rename `GreedyState.soft_score: u32` to `GreedyState.search_score_slice: u32` in `solver-core/src/solve.rs`. Update every reader/writer in `solver-core/src` and `solver-core/tests`. The `Solution.soft_score` API surface stays.
- Add `GreedyState.canonical_score: u32` field, initialised by `GreedyState::new()` to `0`. Set to `score::score_solution(problem, placements, weights)` once at the end of greedy in `solve_with_config_stats`, immediately before the LAHC dispatch.
- Maintain `state.canonical_score` in lockstep with `state.search_score_slice` across all three LAHC moves:
    - **Change move (`try_change_move` in `solver-core/src/lahc.rs`)**: extend the existing slice delta with two additional terms.
        - `home_room_delta = Σ_{class ∈ lesson.school_class_ids} (home_room_penalty(lesson, lookup, new_room_id, weights) - home_room_penalty(lesson, lookup, old_room_id, weights))`.
        - `class_day_balance_delta`: for each class in `lesson.school_class_ids`, compute the per-class scaled cost twice (pre and post). Use `state.class_positions[(class, day)].len()` as the per-day count. Per-class total `sum` is invariant under the move (the lesson stays in the class set; only the day changes), so the same `sum` feeds pre and post. Helper signature: `class_day_balance_delta_for_class(class_id, days, &state.class_positions, old_day, new_day, sum)` returning `i64`. Same-day moves return zero immediately.
        - `canonical_delta = slice_delta + home_room_delta + (i64::from(weights.class_day_balance) * class_day_balance_delta)`.
        - On accept: `state.canonical_score = u32::try_from(i64::from(state.canonical_score) + canonical_delta)`.
    - **R&R (`rr_attempt` in `solver-core/src/lahc.rs`)**: after a successful recreate, `state.search_score_slice` is recomputed via `running_slice_from_placements` (today). Add a sibling line `state.canonical_score = score::score_solution(problem, placements, weights);`. R&R amortises the full recompute by `lahc_rr_period` (default 25 iterations).
    - **Kempe (`kempe_attempt` in `solver-core/src/lahc.rs`)**: extend the existing snapshot+delta pattern.
        - **home_room delta**: for each chain row that is removed, subtract `home_room_penalty(lesson, lookup, old_row.room_id, weights)`; for each chain row that is added, add `home_room_penalty(lesson, lookup, new_row.room_id, weights)`. Same shape as the existing `removed_subject_pref` / `added_subject_pref` accumulators in `kempe_attempt`.
        - **class_day_balance delta**: snapshot pre-attempt per-class day-count vectors `class_counts_pre[class] = Vec<u32>(days)` for every class in the union of chain-member lessons' `school_class_ids`. After apply, walk the same classes and sum the per-class scaled cost from the post counts. Delta = post_balance_cost - pre_balance_cost over affected classes only. The unaffected-class day counts contribute zero by construction. Reuse the per-class scaled-cost helper (`score::class_day_balance_cost_for_class` or inline equivalent), keeping the integer-division semantics of `class_day_balance_cost` exact.
        - `canonical_delta = gap_delta + subject_pref_delta + (weights.prefer_home_room as i64 * home_room_delta) + (weights.class_day_balance as i64 * class_day_balance_delta)`.
        - On accept: `state.canonical_score = u32::try_from(i64::from(state.canonical_score) + canonical_delta)`.
- LAHC outer loop (`solver-core/src/lahc.rs::run`):
    - `lahc_list` initialisation: `vec![state.canonical_score; LAHC_LIST_LEN]` (was `state.soft_score`).
    - `running_best = state.canonical_score` at loop entry.
    - `let mut best_placements: Vec<Placement> = placements.clone();` at loop entry.
    - Acceptance threshold (Change move): `let accept = new_canonical <= state.canonical_score || new_canonical <= prior;` where `prior = lahc_list[(iter as usize) % LAHC_LIST_LEN]`.
    - Per-iteration tail: `lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.canonical_score;`.
    - First-feasible probe: unchanged (depends on placement count + hard feasibility, not on the soft objective name).
    - Time-to-optimal probe: fires when `state.canonical_score < running_best`. Update `running_best`, `best_placements = placements.clone()`, `stats.time_to_optimal_ms`.
    - Early-exit predicate: `state.canonical_score == 0 && placements.len() == placements_expected`.
    - On loop exit: `*placements = best_placements;`. The post-LAHC `solve_with_config_stats` recomputes `solution.soft_score = score_solution(problem, &solution.placements, weights)` against the restored placements.
- `solver-core/src/quality.rs::build_backend_objectives` updates:
    - `lahc_optimised = QualityComponent::ALL.iter().copied().collect()`.
    - `lahc_skipped = BTreeSet::new()`.
    - `lahc_notes = "LAHC accepts and exits on the full canonical (see lahc::run); item 54 reserved for FFD greedy-time class-day-balance tiebreak."`.
    - `lahc_rr` and `lahc_rr_kempe` rows inherit the same sets; their notes refresh to point at "Inherits LAHC's canonical objective".
- Tests in `solver-core/tests/lahc_property.rs`:
    - `canonical_score_matches_score_solution_at_lahc_exit` (proptest, ~32 cases): generates a small problem, runs `solve_with_config_stats` with a tight `max_iterations` and `deadline`, asserts `state.canonical_score == score_solution(problem, placements, weights)` at LAHC exit. Pinned via the new `debug_assert_eq!` at the LAHC iteration tail in test builds.
    - `lahc_canonical_score_is_non_increasing_versus_greedy_under_production_weights` (proptest, ~32 cases): runs `solve_with_config_stats` twice on the same problem (deadline `None` for greedy-only, deadline `Some(200ms)` plus `max_iterations: Some(2_000)` for greedy + LAHC), asserts `lahc_solution.soft_score <= greedy_solution.soft_score`. Fixture: an extension of `score_property::build_class_day_balance_problem` with a non-default `home_room_id` plus a feasible non-home alternative room, configured under `PRODUCTION_ACTIVE_WEIGHTS` so both axes carry non-zero weight.
    - `lahc_returns_running_best_canonical_when_search_drifts` (targeted, hand-built fixture): drives a Change-move sequence where LAHC's accept criterion accepts a slice-improving but canonical-worsening move, then deadline expires before LAHC finds the way back. Asserts that the returned placement set is the running-best snapshot, not the wandering current.
- Debug-assert (`debug_assert_eq!`) at the end of every LAHC iteration, gated behind `cfg(debug_assertions)`:

    ```rust
    #[cfg(debug_assertions)]
    debug_assert_eq!(
        state.canonical_score,
        score::score_solution(problem, placements, &config.weights),
        "state.canonical_score must equal score_solution(...) at every LAHC iteration tail",
    );
    ```

  Cost is acceptable in test builds; release builds compile this away.

**Out of scope.**

- Bench refresh (`mise run bench:bakeoff`): production cell shape costs ~5 hours wall-clock; deferred to item 47's ADR 0035 PR.
- Greedy-time class_day_balance tiebreak (item 54): independent.
- CP-SAT objective port (item 48): independent.
- Public API changes: `Solution.soft_score`, JSON wire format, Python binding signatures all unchanged.
- Renaming `Solution.soft_score` to `Solution.canonical_score` at the public surface: cascades into Pydantic, frontend types, JSON fixtures, the bench. Out of scope; the public surface already represents the canonical (post item 41).

## Architecture

### Field shape on `GreedyState`

```rust
pub(crate) struct GreedyState {
    // ... existing fields ...
    /// Running LAHC search slice: `class_gap + teacher_gap + subject_pref`.
    /// Maintained by greedy's `try_place_block` persist site, by Change-move
    /// delta, by Kempe snapshot+delta, and by R&R via
    /// `running_slice_from_placements`. The slice contract is preserved
    /// exactly as documented in `solver/CLAUDE.md`.
    pub(crate) search_score_slice: u32,

    /// Running canonical objective: `score_solution(problem, placements,
    /// weights)`. Initialised at the end of greedy. Maintained in lockstep
    /// with `search_score_slice` across all three LAHC moves. Drives LAHC's
    /// accept criterion, `time_to_optimal_ms` probe, early-exit predicate,
    /// and the running-best snapshot.
    pub(crate) canonical_score: u32,
}
```

### LAHC outer-loop sketch

```rust
pub(crate) fn run(...) {
    let Some(deadline) = config.deadline else { return; };
    if placements.is_empty() { return; }
    // ... existing RNG / lookup setup ...

    let mut lahc_list = vec![state.canonical_score; LAHC_LIST_LEN];
    let mut running_best = state.canonical_score;
    let mut best_placements: Vec<Placement> = placements.clone();
    // ... existing iter setup ...

    while iter < max_iter && solve_start.elapsed() < deadline {
        // dispatch on rr / kempe / change as today
        // each branch updates state.search_score_slice AND state.canonical_score
        iter += 1;
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
    }

    *placements = best_placements;
}
```

The slice and canonical fields stay aligned with placements (identity at LAHC entry; lockstep across deltas). After the snapshot restore, `state.search_score_slice` and `state.canonical_score` may not reflect the restored placements, which is fine: `solve_with_config_stats` does not read either field after `lahc::run` returns; it derives `solution.soft_score` from `score_solution(problem, &solution.placements, weights)` directly.

### Class-day-balance delta helper

The existing `score::class_day_balance_cost` walks every class in `problem.school_classes`, allocating one `Vec<u32>(days)` per class. The Change-move hot path needs a per-class delta without allocation. New helper:

```rust
pub(crate) fn class_day_balance_cost_for_class(
    class_id: SchoolClassId,
    days: u8,
    class_positions: &HashMap<(SchoolClassId, u8), Vec<u8>>,
) -> u32 {
    if days == 0 { return 0; }
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
    if sum == 0 { return 0; }
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

Two `O(days)` walks (sum then scaled) on stack-only u32 accumulators. Allocation-free. Returns `u32` so callers compute `delta = i64::from(post) - i64::from(pre)`.

`score::class_day_balance_cost` keeps its signature for the `score_solution` cold path; the new helper is used by Change move and Kempe delta computations only. The two stay consistent because the cold-path implementation is the sum of the per-class helper across `problem.school_classes`.

### Acceptance bar (mechanical mapping to OPEN_THINGS)

> "Acceptance: property tests assert the canonical score is non-increasing versus greedy under production weights on fixtures with non-zero home-room and day-balance costs."

- "Property tests": `lahc_canonical_score_is_non_increasing_versus_greedy_under_production_weights` (proptest, ~32 cases).
- "Canonical score is non-increasing versus greedy": the test asserts `lahc_solution.soft_score <= greedy_solution.soft_score`. Holds by construction: `best_placements` is initialised to the post-greedy placements and only swapped on canonical-strict-improvement events; the LAHC exit restore preserves `lahc_solution.soft_score == best_canonical_at_exit <= initial_canonical == greedy_solution.soft_score`.
- "Production weights": the test uses `PRODUCTION_ACTIVE_WEIGHTS`.
- "Non-zero home-room and day-balance costs": the test fixture extends `score_property::build_class_day_balance_problem` with a non-default `home_room_id` placement target and a feasible alternative room; both axes evaluate to non-zero on the post-greedy placements.

## Commit split

Four commits on `feat/lahc-canonical-objective`:

1. `refactor(solver-core): rename state.soft_score to state.search_score_slice (item 52 prep)` — pure structural rename inside `solver-core/src` and `solver-core/tests`. No behaviour change. Tests still green.
2. `feat(solver-core): track canonical objective on GreedyState (item 52 prep)` — add `state.canonical_score`, initialise post-greedy, maintain in Change move (incremental delta), R&R (full recompute via `score_solution`), Kempe (snapshot+delta extension). Add `debug_assert_eq!` at LAHC iteration tail. Add `canonical_score_matches_score_solution_at_lahc_exit` property test. LAHC accept criterion still on slice; no observable behaviour change.
3. `feat(solver-core): LAHC accepts and exits on canonical objective (item 52)` — switch `lahc_list`, accept threshold, `time_to_optimal_ms` probe, and early-exit predicate to `state.canonical_score`. Snapshot `best_placements` on every running-best event; restore at LAHC exit. Update `build_backend_objectives` so LAHC variants declare every QualityComponent in `optimised`. Add `lahc_canonical_score_is_non_increasing_versus_greedy_under_production_weights` (proptest) and `lahc_returns_running_best_canonical_when_search_drifts` (targeted) tests.
4. `docs: refresh slice/canonical notes (item 52)` — update `solver/CLAUDE.md` to document the slice/canonical split, the canonical maintenance shapes per move, and the running-best snapshot mechanism. Delete item 52 from `OPEN_THINGS.md` and update next-pickup rotor. Refresh auto-memory roadmap entry frontmatter `description` and body.

## Verification

1. `mise run test:rust` green (existing tests + three new tests).
2. `mise run test:py` green (no binding changes; the parity assertion on Python's CP-SAT side is item 48, not 52).
3. `mise run lint` green (clippy `-D warnings`, `cargo machete`, `cargo fmt`).
4. `mise run bench` (`cargo bench -p solver-core --bench solver_fixtures`) compared against `BASELINE.md`; the 20% criterion regression budget must hold on every fixture (`grundschule`, `zweizuegig`, `dreizuegig`). Expected delta: under 2% on Change-move-dominated runs (small home_room and class_day_balance delta arithmetic). If a fixture breaches, halt and triage.
5. Five-seed property-test sweep: `for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property; done`, all green. The new tests must not flake.
6. `mise run e2e` is unchanged structurally (LAHC consumed via the binding; no contract change). Skip in this PR; Lefthook pre-push runs the unit suites.

## Risks

- **Hot-path perf regression in Change move**: home_room delta is two function calls + arithmetic; class_day_balance delta is two `O(days)` walks per affected class (D ≤ 7). Net Change-move cost increase ~50 ns. Criterion budget verifies; halt if breached.
- **Drift between `state.canonical_score` and `score_solution`**: caught by the `debug_assert_eq!` at every LAHC iteration tail in test builds. Surfaces immediately in any property test if a delta term is wrong.
- **Snapshot memory**: at most a few hundred `Vec<Placement>` clones per LAHC run, ~7 KB each on dreizuegig. Sub-millisecond total, negligible against the 5 s wall-clock budget.
- **R&R recompute cost**: amortised by `lahc_rr_period` (25 iterations). The post-recreate `score_solution` walk doubles the existing slice-side recompute cost, ~4% of wall-clock budget on R&R-heavy runs. Acceptable.
- **Determinism**: no new RNG calls, no new HashSet iteration. The existing `lahc_deterministic_under_seed_and_iter_cap` property test still pins.
- **Item 54 collision**: item 54 widens greedy's FFD tiebreak to consider class_day_balance. Item 52 widens LAHC's accept criterion. Disjoint scopes; both items can ship independently. The `build_backend_objectives` notes for `lahc` reference item 54 explicitly so the next picker sees the relationship.

## Pointers

- `solver-core/src/lahc.rs::run` (line 32) — LAHC outer loop.
- `solver-core/src/lahc.rs::try_change_move` (line 211) — Change-move accept site.
- `solver-core/src/lahc.rs::rr_attempt` (line ~700) and `kempe_attempt` (line ~1500) — R&R / Kempe move shapes.
- `solver-core/src/score.rs::score_solution` (line 15) — canonical scorer.
- `solver-core/src/score.rs::class_day_balance_cost` (line 140) — per-class L1 cost; new helper sits next to this.
- `solver-core/src/quality.rs::build_backend_objectives` (line 375) — backend objective declarations.
- `solver-core/src/solve.rs::solve_with_config_stats` (around line 290) — post-LAHC `solution.soft_score = score_solution(...)` site.
- `docs/superpowers/OPEN_THINGS.md` items 52 (this), 51 (lineage), 50 (lineage), 47 (downstream bench), 48 (sibling), 54 (sibling).
- `docs/superpowers/specs/2026-05-07-item-51-backend-objective-parity-design.md` — closest precedent in shape.
