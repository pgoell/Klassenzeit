# Mix `preferred_block_size` in `lahc_small_problem` spec (active sprint, item 40)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Correctness phase: item 40.
**Goal.** Widen the proptest generator at `solver/solver-core/tests/lahc_property.rs:91-174` so `preferred_block_size` is drawn from `[1u8, 2u8]` per problem and `hours_per_week` is constrained to a multiple of it. The Kempe chain code that walks a multi-position window (`for k in 0..n_seed { ... start_pos + k }` at `solver/solver-core/src/lahc.rs:1672-1677` and downstream apply / rollback) is currently only exercised by the bake-off Doppelstunde fixtures, where the soft-score dedup hides the documented `start_pos` reuse pattern. With item 39's `validate_no_double_booking` now wired through `solve_with_config` under `cfg(debug_assertions)`, every property test inherits a hard-violation panic for free; widening the generator turns latent chain-window coverage into actual coverage.

**Non-goal.** No production solver code changes (Path A). No widening of other property-test generators (`score_property.rs`, `properties.rs`, `same_room_property.rs`). No bench refresh. No `SolveConfig` changes. No ADR. No new public API. No targeted regression test under Path A; that ships only if a Kempe bug surfaces (Path B).

## Context

`solver/solver-core/tests/lahc_property.rs` runs 14 property tests over a `prop_compose!`-generated `Problem` (`lahc_small_problem`, line 91). The generator hardcodes `preferred_block_size: 1` at line 155 and varies `hours_per_week` per lesson index as `2 + (i % 3)`. The comment at lines 151-153 explicitly notes the `preferred_block_size=1` choice was tied to the active-sprint item 37 R&R rollback bug, which is now closed (PR #186).

Today's coverage shape: every property-test problem runs the Kempe chain code at `n_seed = 1`, so the chain-window walk degenerates to a single time-block lookup per chain member. The code that fans out to `start_pos + k` for `k > 0` (window verification at `lahc.rs:1672-1677`, apply at `kempe_apply_member`, rollback at `kempe_rollback`) is reached only by the bake-off bench's dreizügige fixture's two-period Doppelstunden, and only via `mise run bench:bakeoff`'s production-weight cells. The bake-off does not run in CI; if a Kempe chain-window bug regressed it would surface only on the next manual bake-off refresh, with no per-iteration diagnostic to localise it.

OPEN_THINGS item 40 anticipates the widened generator may surface a documented Kempe `start_pos` reuse bug ("chain neighbours offset within the seed's window applied at the seed's start position, producing teacher / class collisions"). With item 39's `validate_no_double_booking` panic reachable from any property-test invocation of `solve_with_config`, that bug, if present, fires as a `panic!` rather than a quietly-wrong soft score.

The brainstorm (`/tmp/kz-brainstorm/brainstorm.md` for this run) settled three judgment calls. First, `preferred_block_size` is drawn per problem (one `n` shared across all lessons), not per lesson, so proptest's shrink work stays comparable to today's. Second, `cases: 32` stays; ~16 cases per test will exercise `n=2`, sufficient for the placement-count, determinism, and score-recompute properties to catch a regression. Third, the bug-surfacing path (Path B) ships its fix as commit 1 and the test widening as commit 2 in the same PR, so each commit individually leaves HEAD green per CLAUDE.md's "structural changes preserve behaviour" rule.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 40. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Modify the `prop_compose!` for `lahc_small_problem` in `solver/solver-core/tests/lahc_property.rs`:
  - Add `preferred_block_size in 1u8..=2u8` to the input clause.
  - In the lesson-construction closure, compute `hours_per_week` so it is a multiple of the drawn `preferred_block_size`. Concretely: `let hours = if preferred_block_size == 2 { 2u8 + 2 * ((i as u8) % 2) } else { 2u8 + ((i as u8) % 3) };`. Under `n=1` the values stay `{2, 3, 4}` exactly as today; under `n=2` the values are `{2, 4}`.
  - Set `preferred_block_size: preferred_block_size` (or shadow as `n`) on the `Lesson` literal.
  - Update the comment block at lines 151-154 to note the new generator shape (the OPEN_THINGS item 37 reference can stay as a historical note; add one sentence about item 40's chain-window coverage).
- Run the file 5 times locally with `PROPTEST_CASES=128` and `PROPTEST_SEED ∈ {1..5}` to validate. Then `mise run test:rust` and `mise run lint`.
- Delete OPEN_THINGS item 40. Advance the active-sprint preamble's "next pickup" line from item 40 to item 41 (or item 30 if 41 is judged riskier).
- Update auto-memory `project_roadmap_status.md` to reflect item 40 shipped and what's next.
- Path B only: add commit 1 fixing whichever defect surfaced (default hypothesis: Kempe chain `start_pos` reuse) and a sibling targeted regression test at `solver/solver-core/tests/kempe_n2_chain_property.rs` (or fold the test into the fix commit if small).

**Out of scope.**

- Other property-test generators (`tests/score_property.rs`, `tests/properties.rs`, `tests/same_room_property.rs`). Each tests an orthogonal invariant under its own generator; widening them is its own PR.
- `build_lahc_pinned_problem` (lines 416-512). Its three lessons stay at `preferred_block_size: 1`; the pinned-placement tests assert pin preservation, not chain-window correctness, and changing them would risk masking a different invariant.
- The `slots_per_day in 2..=5` and `n_days in 1..=3` ranges. They already cover the `n=2` case (a 2-block lesson always fits at position 0 of any day with `slots_per_day >= 2`); widening them is unrelated.
- Per-lesson `preferred_block_size` (i.e. mixing `n` within a single problem). Larger state-space, slower shrink, and the documented Kempe bug shape needs heterogeneous `n` to fire reliably; tracked as a follow-up if it becomes load-bearing.
- Bumping `cases` beyond 32. The local 5×128 verification covers the "did we exercise the path?" question; CI's 32 cases are enough density for ongoing regression coverage.
- Bench refresh (`BASELINE.md`, `BENCH_RESULTS.md`). No production-code change in Path A; Path B may need it if the Kempe fix changes per-iteration wall-clock by more than the 3% refresh threshold (re-evaluate after the fix lands).
- Backend, frontend, solver-py, deploy, or any non-`solver-core` change.

## Generator shape

Today (`lahc_property.rs:91-174`, abbreviated):

```rust
prop_compose! {
    fn lahc_small_problem()(
        n_classes in 1usize..=3,
        n_teachers in 1usize..=4,
        n_rooms in 1usize..=3,
        n_days in 1u8..=3,
        slots_per_day in 2u8..=5,
    ) -> Problem {
        // ...
        let lessons: Vec<Lesson> = school_classes
            .iter()
            .enumerate()
            .map(|(i, sc)| Lesson {
                id: LessonId(lahc_id_from(5000 + i as u32)),
                school_class_ids: vec![sc.id],
                subject_id: subject_a,
                teacher_id: teachers[i % teachers.len()].id,
                hours_per_week: 2 + ((i as u8) % 3),
                preferred_block_size: 1,
                lesson_group_id: None,
            })
            .collect();
        // ...
    }
}
```

After the change:

```rust
prop_compose! {
    fn lahc_small_problem()(
        n_classes in 1usize..=3,
        n_teachers in 1usize..=4,
        n_rooms in 1usize..=3,
        n_days in 1u8..=3,
        slots_per_day in 2u8..=5,
        preferred_block_size in 1u8..=2u8,
    ) -> Problem {
        // ...
        let lessons: Vec<Lesson> = school_classes
            .iter()
            .enumerate()
            .map(|(i, sc)| {
                // hours_per_week must be a multiple of preferred_block_size per
                // validate_structural. With n=1, keep today's {2,3,4} formula
                // byte-identically; with n=2, alternate between 2 and 4 per
                // lesson index so the generator covers single-block and
                // two-block-across-days cases without expanding the value range.
                let hours = if preferred_block_size == 2 {
                    2u8 + 2 * ((i as u8) % 2)
                } else {
                    2u8 + ((i as u8) % 3)
                };
                Lesson {
                    id: LessonId(lahc_id_from(5000 + i as u32)),
                    school_class_ids: vec![sc.id],
                    subject_id: subject_a,
                    teacher_id: teachers[i % teachers.len()].id,
                    hours_per_week: hours,
                    preferred_block_size,
                    lesson_group_id: None,
                }
            })
            .collect();
        // ...
    }
}
```

The comment block at lines 151-153 gets one sentence appended noting the per-problem `n` shape and item 40 anchor:

```rust
// Vary hours so FFD spreads multi-block lessons across days; sprint item 37
// rollback bug only fires on multi-block-across-days lessons (preferred_block_size=1
// and hours_per_week>=3), the constant 2 hid it. Sprint item 40 widens the
// generator to draw `preferred_block_size` from {1, 2} per problem so the
// Kempe chain code's multi-position window walk gets coverage in CI.
```

## Validation

Local pre-commit:

```bash
for s in 1 2 3 4 5; do
  PROPTEST_CASES=128 PROPTEST_SEED=$s \
    cargo nextest run -p solver-core --test lahc_property
done
mise run test:rust
mise run lint
```

If all six commands succeed, the PR is single-commit (Path A). If any of the five proptest runs fails with a `validate_no_double_booking` panic (or a placement-count drop, or a score-recompute mismatch), Path B kicks in:

1. Inspect the failing seed in `solver/solver-core/tests/lahc_property.proptest-regressions`. Reproduce locally with `PROPTEST_CASES=1 PROPTEST_SEED=<n> cargo nextest run -p solver-core --test lahc_property -- <test-name>`.
2. Diagnose: is it the documented Kempe `start_pos` reuse, an R&R rollback drift on `n=2`, or a score-delta inconsistency? Cross-reference `solver/CLAUDE.md` "Ruin+apply rollback shape" and "Kempe move semantics" sections.
3. Land commit 1 = `fix(solver-core): <concise diagnosis>` with a targeted regression test at `solver/solver-core/tests/kempe_n2_chain_property.rs` (or wherever fits the fix). Commit 2 = the generator widening above.
4. Re-run the same six commands; both must pass. If commit 1 changes per-iteration wall-clock, run `mise run bench` and refresh `BASELINE.md` if criterion drift exceeds 3%.

CI runs `cases: 32` per the committed config; the 5×128 local sweep is the validation that the change is safe to land but does not run in CI.

## Risks

- **Per-problem `n` may not surface the documented Kempe bug.** The bug shape ("chain neighbours offset within the seed's window applied at the seed's start position") needs the seed and its chain neighbours to have different `preferred_block_size`; with per-problem `n`, every lesson in a problem shares it, so the offset-within-window pattern degenerates. The brainstorm acknowledges this. Outcome: Path A ships even if the bug exists today, with a follow-up filed for per-lesson `n` or a targeted hand-built fixture. The chain-window code path still gets coverage (window verification, apply, rollback all walk `0..n_seed` once `n_seed=2`), just not the heterogeneous-`n` shape.
- **Score-recompute property `lahc_kempe_running_score_matches_recompute_when_feasible` only fires when `violations.is_empty()`.** Wider `n=2` cases have more violations on average (a 4-hour lesson with `n=2` needs at least two days × two contiguous slots), so this property's coverage of `n=2` is lower than placement-count properties' coverage. Acceptable; the placement-count properties are the primary signal for ruin+apply correctness.
- **Proptest shrink time.** Adding one input variable to `prop_compose!` increases the shrink space. With the input range `1u8..=2u8`, the shrink direction is "drop to 1", which is the existing baseline; failures will shrink quickly. No mitigation needed.
- **`hours_per_week` formula correctness.** `validate_structural` panics if `hours_per_week % preferred_block_size != 0`. The proposed formula constructs valid values by construction (no `prop_assume!` needed); if the formula is wrong, the panic surfaces on the first proptest case in CI, not silently.

## Acceptance

1. `lahc_small_problem` in `solver/solver-core/tests/lahc_property.rs` accepts `preferred_block_size` from `[1u8, 2u8]` and constrains `hours_per_week` to a multiple of it. Existing `n=1` coverage is byte-identical (same `2 + (i % 3)` formula).
2. `cargo nextest run -p solver-core --test lahc_property` passes at the committed `cases: 32`.
3. Five local runs of the file at `PROPTEST_CASES=128` with `PROPTEST_SEED ∈ {1..5}` all pass (Path A) or pass after Path B's fix commit lands.
4. `mise run test:rust` and `mise run lint` both green at HEAD.
5. OPEN_THINGS item 40 deleted. Active-sprint preamble's "next pickup" line advanced.
6. Auto-memory `project_roadmap_status.md` refreshed.
