# Kempe BFS bipartiteness fix spec (active sprint, item 45)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Correctness phase: item 45.
**Goal.** Fix the `lahc_rr_kempe` move that produces a class double-booking caught by `validate_no_double_booking` post-condition validator on grundschule under production budget (`--budget 60s --seeds 20`). The bug is `kempe_build_chain` at `solver/solver-core/src/lahc.rs:1214-1324` not enforcing bipartiteness of the chain conflict graph: when the BFS extends past depth 1, two chain members can be assigned to the same destination day even though they share a class or teacher. The seed and a depth-2 chain member then collide at the time-block window after apply, and the post-condition validator (item 39) panics in `cfg(debug_assertions)` and returns `Err(Error::Input)` in release.

**Non-goal.** No supervisor-resilience changes (item 46, separate P1, separate PR). No bake-off rerun (item 42 stays blocked on item 46). No `kempe_apply_block` collision check (a correctly bipartite chain is collision-free by construction; symptom-side patching would mask future BFS bugs). No new public API. No `SolveConfig` fields. No ADR.

## Context

`solve_with_config` calls `lahc::run` after the FFD greedy bootstrap. `lahc::run` mixes three move types via the `lahc_kempe_period` and `lahc_rr_period` knobs: Change moves, R&R moves, Kempe moves. Each Kempe move attempt picks a block-anchor seed at `(source_day, start_pos, n)`, picks a destination day `dest_day != source_day`, and proposes moving the seed's window to `(dest_day, start_pos..start_pos+n-1)`. `kempe_build_chain` then walks placements at the destination window via BFS, pulling in lessons that conflict (teacher or class) with the popped chain member. Every newly added neighbour is assigned the *opposite* day from the popper: if the popped member is going to `dest_day`, its neighbours go to `source_day`, and vice versa. Chain length is bounded at `KEMPE_MAX_CHAIN = 8`.

The BFS implements a 2-coloring assignment: the seed and even-depth members go to `dest_day`, odd-depth members go to `source_day`. For the resulting Kempe move to be conflict-free, the chain conflict graph must be bipartite: every edge (a "this lesson conflicts with that lesson at the destination window" relation) must run between an even-depth and an odd-depth member. The current BFS does not verify bipartiteness.

Concrete failure shape (the bug):

- Seed `S` at `(D1, P, n)`, classes `[c1]`, teacher `t1`.
- Existing placement `A` at `(D2, P, n)`, classes `[c1]` (conflict with `S` via `c1`).
- Existing placement `B` at `(D1, P, n)`, classes `[c1]`, teacher `t2`.

BFS trace (with `dest_day = D2`, `source_day = D1`):

1. Pop `S`. `popped_dest_day = D2`. Walk `(D2, P..P+n-1)`. Find `A`. Class conflict via `c1`. Add `A` → `chain[A] = D1`.
2. Pop `A`. `popped_dest_day = D1`. Walk `(D1, P..P+n-1)`. Find `B`. Class conflict via `c1` (teacher `t2 != t1`, but same class set). Add `B` → `chain[B] = D2`.
3. Pop `B`. `popped_dest_day = D2`. Walk `(D2, P..P+n-1)`. The current placements vector still has `A` at `D2` (BFS is read-only); `A` is in chain, skipped. **`S` is currently at `D1` in placements** so it is not visited. The BFS terminates with `chain = {S → D2, A → D1, B → D2}`.

After apply: `S` and `B` both placed at `(D2, P..P+n-1)`, both have class `c1` in their class set. `validate_no_double_booking` returns `Err(Error::Input("double-booking: class c1 ..."))` and (in debug builds) panics. Item 32's `xfail(strict=False)` solvability tests already hint at `lahc_rr_kempe` brittleness on grundschule under production deadline; this is the same brittleness surfacing under the stricter post-condition check from item 39.

Item 45 was filed against the production refresh of `BENCH_RESULTS.md` at `--budget 60s --seeds 20` (PR #195). The bug did not surface at the demo budget (`--budget 5s --seeds 4`, the shape committed in PR #192) because the seed loop did not draw a chain shape that triggered the depth-2 collision.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 45. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Modify `kempe_build_chain` at `solver/solver-core/src/lahc.rs:1214-1324`. At the new-neighbour insertion site (currently lines 1314-1320), before assigning `chain[neighbour_id] = neighbour_dest`, walk every entry in `chain` whose value equals `neighbour_dest` and verify the candidate's `(teacher_id, school_class_ids)` does not conflict with the existing same-color member. If any same-color pair conflicts, return `ChainBuild::Aborted`.
- Add a property test `lahc_rr_kempe_does_not_double_book_class` in `solver/solver-core/tests/lahc_property.rs`. Generator: existing `lahc_small_problem` (already widened in item 40 to mix `preferred_block_size`). Body: greedy + LAHC solve with kempe enabled at a small iteration cap, then `validate_no_double_booking(&problem, &placements)` must return `Ok`.
- Add a targeted regression test `lahc_rr_kempe_does_not_double_book_class_at_grundschule` in `solver/solver-core/tests/lahc_property.rs`. Identify the failing grundschule seed via the OPEN_THINGS item 45 repro path: add `eprintln!("seed: {seed}")` to `solver-bench/src/main.rs:411` (the cell-child seed loop), run `target/release/solver-bench --cell grundschule --backend lahc_rr_kempe --budget 60s --seeds 20`, capture the last printed seed before the panic. Pin that seed in the regression test against the `test_fixtures::grundschule()` builder.
- Run a 5x128 PROPTEST_CASES sweep on the new property test before commit, per `solver/CLAUDE.md`. Pin any failing seeds in `solver/solver-core/tests/lahc_property.proptest-regressions`.
- Delete OPEN_THINGS item 45. Advance any "next pickup" pointer downstream of item 45 (item 42 stays parked on item 46).
- Update auto-memory `project_roadmap_status.md` to reflect item 45 shipped (per the autopilot subagent contract).

**Out of scope.**

- Item 46 (supervisor resilience to cell-child panic). Separate P1, separate PR.
- Item 42 (production-shape `BENCH_RESULTS.md` refresh). Stays blocked on item 46; item 45 is one of two prerequisites.
- `kempe_apply_block` collision-check shim. The bipartite chain is collision-free by construction; adding a redundant check at apply time hides future BFS regressions behind a fallback.
- `BASELINE.md` refresh. The bipartiteness check adds an `O(chain_size)` inner loop per added neighbour, bounded at `KEMPE_MAX_CHAIN = 8`; expected absolute-µs delta is single-digit on grundschule. If the criterion bench shows >20% regression, surface in PR body and ship a `bench(solver-core): refresh BASELINE.md after kempe bipartiteness fix` commit on the same branch (per `solver/CLAUDE.md`'s 20% budget triage rule).
- Per-lesson teacher conflicts that shift after the chain applies. The BFS already accounts for teacher conflicts (line 1268: `let teacher_conflict = other.teacher_id == popped_lesson.teacher_id;`); the bipartiteness check covers the same field.
- Widening `lahc_small_problem` further. Item 40 already widened it; item 45's tests reuse the existing generator.

## Implementation shape

The fix sits inside `kempe_build_chain`'s existing per-popped-member loop (lines 1232-1321). The new check goes after the `teacher_conflict` / `class_conflict` predicate (line 1273) and before the in-chain skip (line 1261). Concretely:

```rust
// After the teacher/class conflict check on the placement at popped's dest:
if !teacher_conflict && !class_conflict {
    continue;
}
// existing checks: pinned, lesson_group, hours_on_source ...

// New: same-color conflict check. If this candidate would join the chain at
// `neighbour_dest`, verify no chain member already at `neighbour_dest`
// shares a teacher or any class with the candidate. Same-color edges
// signal a non-bipartite conflict graph; aborting keeps the move atomic.
let candidate_lesson = other; // already resolved above
let same_color_conflict = chain.iter().any(|(existing_id, existing_dest)| {
    if *existing_dest != neighbour_dest {
        return false;
    }
    let Some(existing_lesson) = lesson_lookup.get(existing_id).copied() else {
        return false;
    };
    let teacher_conflict = existing_lesson.teacher_id == candidate_lesson.teacher_id;
    let class_conflict = existing_lesson
        .school_class_ids
        .iter()
        .any(|c| candidate_lesson.school_class_ids.contains(c));
    teacher_conflict || class_conflict
});
if same_color_conflict {
    return ChainBuild::Aborted;
}
```

The check happens *before* `frontier_seen.insert(...)` so a same-color reject never adds the candidate to either the frontier or `chain`.

`neighbour_dest` is computed once per popped member at line 1309 and is constant for every candidate added during that pop. The chain-iteration cost per candidate is `O(chain_size) <= O(KEMPE_MAX_CHAIN) = O(8)`; total per BFS is `O(KEMPE_MAX_CHAIN^2) = O(64)`. Negligible against the LAHC 5s deadline and the FFD bootstrap cost.

The CLAUDE.md note about `Vec<LessonId>` ordering vs `HashSet` iteration applies only to determinism of the *frontier extension*. The new check uses `chain.iter().any(...)`, which iterates a `HashMap`; the result is a boolean (does any entry match?) so iteration order does not affect correctness or determinism. No sort needed on the new path.

## Test plan

**Property test:**

```rust
// solver/solver-core/tests/lahc_property.rs
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        ..ProptestConfig::default()
    })]

    #[test]
    fn lahc_rr_kempe_does_not_double_book_class(p in lahc_small_problem()) {
        // Greedy + LAHC with kempe enabled at a small iteration cap.
        let config = SolveConfig {
            seed: 0,
            weights: PRODUCTION_ACTIVE_WEIGHTS,
            deadline: None,
            max_iterations: Some(2000),
            lahc_kempe_period: Some(7),
            lahc_rr_period: Some(25),
            // ... other defaults
        };
        let solution = solver_core::solve_with_config(&p, &config)
            .expect("solve_with_config should not error on generated problems");
        validate_no_double_booking(&p, &solution.placements)
            .expect("validate_no_double_booking must pass on Kempe output");
    }
}
```

The exact `SolveConfig` field surface follows the existing property tests in the same file. The 2000-iteration cap is enough for LAHC to issue Kempe moves on most generated problems while keeping per-case wall-clock tractable.

**Targeted regression:**

```rust
#[test]
fn lahc_rr_kempe_does_not_double_book_class_at_grundschule() {
    let p = solver_core::test_fixtures::grundschule();
    let config = SolveConfig {
        seed: <FAILING_SEED>,
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        // production-shape config except deadline + max_iterations
        deadline: None,
        max_iterations: Some(<ITERATIONS_TO_TRIGGER>),
        lahc_kempe_period: Some(7),
        lahc_rr_period: Some(25),
        // ...
    };
    let solution = solver_core::solve_with_config(&p, &config).expect("solve");
    validate_no_double_booking(&p, &solution.placements).expect("no double booking");
}
```

`<FAILING_SEED>` and `<ITERATIONS_TO_TRIGGER>` come from the `eprintln`-and-rerun probe described above. If the bug fires within the first ~5000 iterations, cap there; otherwise pin to whatever the probe reveals.

**Local sweep before commit:**

```bash
for s in 1 2 3 4 5; do
  PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run \
    -p solver-core --test lahc_property lahc_rr_kempe_does_not_double_book_class
done
```

If any seed pins, the proptest-regressions file captures the seed; rerun until clean.

**Acceptance:**

- Property test passes on the fix branch (red on master).
- Targeted regression passes on the fix branch (red on master).
- 5x128 sweep passes clean on the fix branch.
- `mise run test:rust` passes.
- `mise run lint` passes.
- No criterion bench regression beyond 20% on `solver_lahc/grundschule`; if breached, refresh BASELINE.md in the same branch.

## Risks

- **Acceptance-rate drop on Kempe.** Some chains that previously applied (and produced invalid schedules) now abort. Net effect on solver feasibility is positive; the rejected chains were producing post-condition violations. The LAHC outer loop continues with an unchanged chain RNG draw count (the per-iteration draw invariant from `solver/CLAUDE.md` is preserved: the per-Kempe-attempt RNG draws happen *before* `kempe_build_chain` is called).
- **False-positive aborts on legal chains.** The bipartiteness check is necessary and sufficient: a chain is conflict-free iff its conflict graph (restricted to chain members, with edges defined by teacher/class) is bipartite with the BFS's 2-coloring as a valid bipartition. The fix never aborts a chain that would have applied cleanly.
- **Determinism.** `chain.iter()` over a `HashMap` is unordered. The check returns a boolean; no order-dependent result is exposed. The frontier extension still uses the sorted `Vec<LessonId>` (line 1307); determinism property tests in `lahc_property.rs` continue to pass.
- **CLAUDE.md `cargo machete` interaction.** No new dependencies. No new `pub(crate)` helpers (the check is inlined). No commit-split risk.

## Commit shape

Single `fix(solver-core): kempe_build_chain bipartiteness, prevents class double-booking (item 45)` commit containing:

- The bipartiteness check inside `kempe_build_chain`.
- The new property test in `lahc_property.rs`.
- The targeted regression in `lahc_property.rs`.
- Any pinned seeds in `lahc_property.proptest-regressions` produced by the 5x128 sweep.
- OPEN_THINGS item 45 deletion.
- Auto-memory `project_roadmap_status.md` advance.
