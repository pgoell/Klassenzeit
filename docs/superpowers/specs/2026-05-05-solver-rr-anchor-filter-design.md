# Port Kempe anchor filter to `rr_collect_anchors` spec (active sprint, items 26 + 27)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Algorithm phase: items 26 and 27.
**Goal.** Stop the silent placement-drop bug in R&R and back it with property tests so the same regression cannot reach production again.

**Non-goal.** No bench refresh (deferred to items 28 + 29). No Python-side auto-assign solvability tests (item 32). No anchor-shape refactor or richer return type for `rr_collect_anchors`. No new `SolveConfig` fields. No ADR; this is a bug fix that ports an existing invariant from one call site to another.

## Context

`rr_collect_anchors` (`solver/solver-core/src/lahc.rs:689`) emits one tuple per `(lesson_id, day_of_week)` block. `rr_ruin_block` (`lahc.rs:614-625`) then removes every same-lesson-same-day placement when an anchor is ruined, but `rr_attempt`'s recreate (`lahc.rs:786`) calls `try_place_block` exactly once per anchor. The mismatch surfaces only when FFD packed multiple `N=1` blocks of the same lesson onto one day for compactness: ruin removes both, recreate restores one, the other vanishes.

Two layers hid the bug:

1. FFD-time `solution.violations` does not record blocks lost during local search; it is the violation list as of FFD completion. Once LAHC starts moving placements, dropped rows are invisible to the violation list.
2. The LAHC asymmetric acceptance gate only rejects a move on `failed_recreates > 0`. A move that ruins two placements and successfully recreates one (the recreate did not "fail") passes the gate, and the soft score actually improves because absent placements pay no constraint cost.

Concrete numbers from the user's dev-DB zweizügig seed: greedy alone returns 191/196 placements; `lahc_rr` and `lahc_rr_kempe` return 68/196; `cpsat` at a 5 s budget returns 196/196. The bake-off bench (`mise run bench:bakeoff`) calls a cell feasible iff `solution.violations.len() == 0` and never checks `solution.placements.len()`, so the broken cells reported `feasibility 20/20 soft=0` and ADR 0031 picked `lahc_rr_kempe` as production default off that signal.

The Kempe move already filters this case at `lahc.rs:1367-1386`: after calling `rr_collect_anchors`, it keeps only anchors where `hours_on_day == lesson.preferred_block_size`. The fix is to port that filter into the producer (`rr_collect_anchors`) so every consumer (R&R and Kempe today, any future ruin-style move) inherits the invariant. Item 27 (which item 26's text requires landing in the same commit) backs the fix with two property tests so a future regression cannot pass CI silently.

`solver/CLAUDE.md` already documents the latent shape ("FFD can pack multiple N=1 blocks of the same lesson onto one day. ... Anchor-based LAHC moves (R&R, Kempe) that assume one block per `(lesson, source_day)` need to handle this case"). The bug fix turns the latent prediction into a guarded one.

Anchor item: `docs/superpowers/OPEN_THINGS.md` items 26 and 27. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Port the Kempe anchor filter into `rr_collect_anchors` in `solver/solver-core/src/lahc.rs`. Skip any `(lesson, day)` where the day already holds more placements of that lesson than `lesson.preferred_block_size`.
- Remove the now-redundant re-filter in `kempe_attempt` (`lahc.rs:1367-1386`); update its comment to point at `rr_collect_anchors` for the invariant.
- Update the doc comment on `rr_collect_anchors` to spell out the invariant (`anchors are guaranteed to ruin exactly one block of length preferred_block_size`).
- Add two property tests to `solver/solver-core/tests/lahc_property.rs` (siblings of `lahc_rr_never_increases_hard_violations` at line 218):
  - `lahc_rr_never_decreases_placement_count`: `prop_assert!(lahc_rr.placements.len() >= greedy.placements.len())`.
  - `lahc_rr_kempe_never_decreases_placement_count`: same shape, `lahc_rr_kempe` config.
- Add one targeted integration test in `solver/solver-core/tests/` pinning the FFD-packs-two-N=1-blocks-on-one-day pattern. Hand-built minimal `Problem` (one class, one teacher, one room, one subject, one lesson with `hours_per_week=2, preferred_block_size=1`, 5 days, room `blocked_time_blocks` covering all but one day so FFD must pack both hours on the same day). Asserts `lahc_rr.placements.len() == greedy.placements.len()` deterministically.
- Update `docs/superpowers/OPEN_THINGS.md`: delete items 26 and 27 (they ship with this commit). Leave the rest of the active sprint alone. The "Algorithm phase" heading stays; item 28 (the bench harness fix) becomes the next pickup.

**Out of scope.**

- Refresh of `solver/solver-core/benches/BENCH_RESULTS.md`. Item 28 (`placements_total` validation in the bench harness) must land first; refreshing without the harness fix carries the same blind spot the bug originally hid behind. The PR body queues item 28 as the next pickup.
- ADR 0032 (revisit ADR 0031). Deferred to item 29 once item 28 has refreshed `BENCH_RESULTS.md` honestly.
- Python-side `test_seeded_grundschule_*_solves_with_auto_assigned_teachers` tests. These are item 32 and require seeding the demo fixtures, running the production solver backend, and tuning per-fixture deadlines; out of scope for this fix.
- Any change to `SolveConfig` (no new fields, no plumbing changes), `solver-py` bindings (no Python-visible behaviour change), or backend / frontend code.
- Refactor of `rr_collect_anchors` to return a richer struct or to fold the count into the anchor tuple. Bug fix only; per CLAUDE.md "Don't add features, refactor, or introduce abstractions beyond what the task requires".

## Filter semantics

`rr_collect_anchors` today:

```rust
fn rr_collect_anchors(
    placements: &[Placement],
    lesson_lookup: &HashMap<LessonId, &Lesson>,
    tb_lookup: &HashMap<TimeBlockId, &TimeBlock>,
    pinned: &HashSet<LessonId>,
) -> Vec<(LessonId, u8)> {
    let mut seen: HashSet<(LessonId, u8)> = HashSet::new();
    let mut anchors: Vec<(LessonId, u8)> = Vec::new();
    for p in placements.iter() {
        let Some(lesson) = lesson_lookup.get(&p.lesson_id) else { continue };
        if pinned.contains(&p.lesson_id) { continue; }
        if lesson.lesson_group_id.is_some() { continue; }
        let Some(tb) = tb_lookup.get(&p.time_block_id) else { continue };
        let key = (p.lesson_id, tb.day_of_week);
        if seen.insert(key) {
            anchors.push(key);
        }
    }
    anchors.sort_unstable_by(|a, b| a.0.0.cmp(&b.0.0).then(a.1.cmp(&b.1)));
    anchors
}
```

After the fix, the same loop additionally counts placements per `(lesson_id, day)` and drops any anchor where the count exceeds `preferred_block_size`. The walk over `placements` is `O(P)` for collection plus a second pass to count; we can combine into a single pass by building a `HashMap<(LessonId, u8), u8>` of counts and then materialising the anchor list from it. The shape:

```rust
fn rr_collect_anchors(...) -> Vec<(LessonId, u8)> {
    let mut counts: HashMap<(LessonId, u8), u8> = HashMap::new();
    for p in placements.iter() {
        let Some(lesson) = lesson_lookup.get(&p.lesson_id) else { continue };
        if pinned.contains(&p.lesson_id) { continue; }
        if lesson.lesson_group_id.is_some() { continue; }
        let Some(tb) = tb_lookup.get(&p.time_block_id) else { continue };
        let key = (p.lesson_id, tb.day_of_week);
        *counts.entry(key).or_insert(0) = counts.get(&key).copied().unwrap_or(0).saturating_add(1);
    }
    let mut anchors: Vec<(LessonId, u8)> = counts
        .into_iter()
        .filter_map(|((lesson_id, day), count)| {
            let lesson = lesson_lookup.get(&lesson_id)?;
            if count <= lesson.preferred_block_size { Some((lesson_id, day)) } else { None }
        })
        .collect();
    anchors.sort_unstable_by(|a, b| a.0.0.cmp(&b.0.0).then(a.1.cmp(&b.1)));
    anchors
}
```

Sketch only; the implementation may pick a different shape as long as (a) determinism is preserved (final `sort_unstable_by` clause unchanged) and (b) the counting pass is single-walk-`O(P)` to keep the hot-path cost flat. Behaviour contract: any `(lesson, day)` whose placement count exceeds `preferred_block_size` is excluded. `count == preferred_block_size` is the canonical "exactly one block" case and is included; `count > preferred_block_size` is the FFD-pack pathology and is excluded.

The Kempe site at `lahc.rs:1367-1386` becomes:

```rust
let anchors = rr_collect_anchors(placements, lesson_lookup, tb_lookup, pinned);
if anchors.is_empty() { return false; }
```

The whole `.filter` block on `raw_anchors` is deleted; the comment is rewritten to point at `rr_collect_anchors`'s doc comment for the invariant.

## Property tests

Two new tests in `solver/solver-core/tests/lahc_property.rs`, sibling shape to `lahc_rr_never_increases_hard_violations` (line 218) and `lahc_kempe_never_increases_hard_violations` (line 276):

```rust
#[test]
fn lahc_rr_never_decreases_placement_count(p in lahc_small_problem()) {
    let greedy = solve_with_config(&p, &SolveConfig { weights: lahc_weights(), ..SolveConfig::default() }).unwrap();
    let lahc_rr = solve_with_config(&p, &lahc_rr_cfg(7)).unwrap();
    prop_assert!(lahc_rr.placements.len() >= greedy.placements.len());
}

#[test]
fn lahc_rr_kempe_never_decreases_placement_count(p in lahc_small_problem()) {
    let greedy = solve_with_config(&p, &SolveConfig { weights: lahc_weights(), ..SolveConfig::default() }).unwrap();
    let lahc_rr_kempe = solve_with_config(&p, &lahc_rr_kempe_cfg(7)).unwrap();
    prop_assert!(lahc_rr_kempe.placements.len() >= greedy.placements.len());
}
```

`lahc_rr_kempe_cfg` is a new helper at the top of the file mirroring `lahc_rr_cfg` and `lahc_kempe_cfg`, with both `lahc_rr_period: Some(5)` and `lahc_kempe_period: Some(5)`. The 20 ms deadline matches the existing R&R and Kempe properties; CI cost adds about a second per property at 256 cases.

## Targeted integration test

A new `solver/solver-core/tests/rr_anchor_filter.rs` (file name reflects the bug class so `git log -- tests/rr_anchor_filter.rs` is the regression history). Hand-built minimal `Problem` exercising the FFD-pack pathology:

- 1 class, 1 teacher, 1 room, 1 subject, 1 teacher qualification, 1 lesson with `hours_per_week=2, preferred_block_size=1`.
- 5 days, 2 positions per day (10 time blocks).
- Room `blocked_time_blocks` covering 9 of the 10 time blocks except day 0 positions 0 and 1; the lesson must place both hours on day 0.
- After greedy: `placements.len() == 2`, both on day 0.
- After `lahc_rr` with a deterministic seed: `placements.len() == 2`. Pre-fix: drops to 1 once R&R picks the day-0 anchor. Post-fix: the filter excludes the day-0 anchor (count 2 > preferred_block_size 1), so R&R has no candidates and returns the greedy solution unchanged.

The test runs with `lahc_rr_period: Some(1)` (every iteration is an R&R attempt) and `max_iterations: Some(50)` (deterministic stop) so the post-fix assertion is byte-stable. Both properties are asserted: (a) `lahc_rr.placements.len() == greedy.placements.len()` (exact match because the only placement-count change R&R can drive on this fixture is a drop; with the filter, R&R has no candidates) and (b) the two placements remain on day 0.

Mirror test for Kempe in the same file (`kempe_does_not_drop_packed_block`); same fixture, `lahc_kempe_period: Some(1)`. Already passing on master because Kempe's own re-filter at `lahc.rs:1367` already covers this; the test pins the shared invariant once both filters live in `rr_collect_anchors`.

## Determinism and existing properties

The new filter happens in `rr_collect_anchors`, called once per `rr_attempt` and once per `kempe_attempt` to build the candidate list. RNG draws happen after the candidate list is built. The R&R RNG draws (anchor shuffle, anchor pick) consume the same number of random bytes whether the candidate list has 5 elements or 5000; determinism per the solver CLAUDE.md ("LAHC RNG draw count must be invariant across loop branches") is preserved. The Kempe RNG draws are unchanged because the deleted `.filter` block produced the same anchor set the new producer-side filter produces.

Verification: the existing `lahc_rr_deterministic_under_seed_and_iter_cap` (line 228) and `lahc_kempe_deterministic_under_seed_and_iter_cap` (line 286) properties pass byte-identically before and after the fix. `lahc_rr_running_score_matches_recompute_when_feasible` (line 243) and `lahc_kempe_running_score_matches_recompute_when_feasible` continue to hold; they assert recompute equals running score on feasible solutions, and the fix only removes silent drops, so feasible solutions become more reliable, not less. The `lahc_rr_pinned_placements_preserved` (line 252) and Kempe equivalent continue to hold; the filter does not interact with pin handling.

## Acceptance criteria

- All existing solver-core tests pass: `cargo nextest run -p solver-core`.
- The two new property tests pass: 256 cases each, both at the 20 ms deadline shape.
- The targeted integration test passes deterministically across 20 consecutive runs: `for i in {1..20}; do cargo test -p solver-core --test rr_anchor_filter -- --test-threads=1; done`.
- `mise run lint` is green (clippy `-D warnings`, ruff, ty, biome, machete, fmt).
- Pre-push runs full test suite cleanly via `mise exec -- git push`.
- Manual verification narrative in PR body: cite the dev-DB zweizügig delta (lahc_rr_kempe 68/196 → 196/196 expected post-fix) as the user-data receipt; item 32 promotes the manual verification into an automated Python test.

## Risks

1. **Hot-path cost.** `rr_collect_anchors` runs once per `rr_attempt`. The new counting pass is `O(P)` (single walk over placements), same complexity as the existing dedup walk; the constant factor goes up by one HashMap insertion per placement. On the dev zweizügig fixture (196 placements) the cost is sub-microsecond. Mitigation: bench `mise run bench` before/after; if the criterion bench shows any cell breaching the 20 % regression budget, the implementation falls back to a two-pass shape (first pass counts, second pass emits) which has identical big-O but better cache behaviour. Acceptance: criterion delta within ±5 % on each fixture.
2. **Property test flakiness.** The two new properties run R&R and Kempe at a 20 ms deadline; on a heavily-loaded host the wall-clock might let some cases run zero LAHC iterations, in which case `lahc.placements == greedy.placements` trivially. This is a strict-monotone outcome (the property holds), not a flake. Mitigation: same deadline shape as the existing `lahc_rr_never_increases_hard_violations`, which has been stable in CI.
3. **Kempe re-filter removal.** Deleting `lahc.rs:1367-1386` is a one-line shape change (`raw_anchors` to `anchors`, drop the `.filter` block). Mitigation: the post-removal Kempe code path is exercised by `lahc_kempe_*` property tests already in the suite, plus the new `lahc_rr_kempe_never_decreases_placement_count` and `kempe_does_not_drop_packed_block` from this PR.

## Plan

A single-task implementation plan covers the work, but it splits cleanly into ordered chunks:

1. Red: write the targeted integration test (`solver/solver-core/tests/rr_anchor_filter.rs`); confirm it fails on master.
2. Red: write the two property tests in `tests/lahc_property.rs`; confirm at least the R&R one fails on master across enough cases to be honest.
3. Green: port the filter into `rr_collect_anchors`; remove the Kempe re-filter; update doc comments. Confirm all five tests (existing + new) pass.
4. Refactor: read the diff and the surrounding doc comments; tighten anything that reads ambiguously after the change. Run `mise run lint` and full `mise run test`.
5. Docs: delete OPEN_THINGS items 26 + 27. Commit.

The plan-document version expands these into checkbox tasks per `superpowers:writing-plans`.
