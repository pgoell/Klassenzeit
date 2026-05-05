# `validate_no_double_booking` post-condition validator spec (active sprint, item 39)

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Correctness phase: item 39.
**Goal.** Add a post-condition validator that walks the final placements vector and refuses any output containing a class / teacher / room double-booking, an hours-per-week mismatch, or a malformed block (non-contiguous, cross-day, or cross-room positions inside one `preferred_block_size`-window). Wire the validator after `validate_no_room_hopping` in `solve_with_config` so production paths surface failures as `Error::Input`, and wire a `#[cfg(debug_assertions)]` panic alongside `validate_daily_caps` so property and integration tests fail loudly. The validator is the foundation for trusting the bake-off numbers: any future move (R&R, Kempe, third-generation) that produces a silent hard violation is caught by this validator regardless of whether the soft scorer's per-`(class, day)` deduplication masks it.

**Non-goal.** No fix for the existing Kempe `kempe_apply_block` `state.used_teacher` / `used_class` / `used_room` insert-without-contains-check bug. Item 39 is the diagnostic, not the fix; the fix lands as a follow-up if and only if the validator's debug-panic fires on an existing test. No item 40 (mixing `preferred_block_size` in `lahc_small_problem`). No item 41 (reconciling `solution.soft_score` with the full weighted cost). No bench refresh (item 30 is the next pickup once correctness phase closes). No new `Error` variant (the Result return uses the existing `Error::Input` for parity with `validate_no_room_hopping` and `validate_daily_caps`).

## Context

`solve_with_config` (`solver/solver-core/src/solve.rs:108`) is the single entry point that produces a `Solution`. After the LAHC inner loop returns (`crate::lahc::run`), it runs two post-condition checks today:

```rust
// Post-solve hard-constraint sanity check. A failure here is a solver bug.
validate_no_room_hopping(problem, &solution.placements)?;

// Debug-only post-condition: daily caps (ADR 0033) are enforced as
// legality pruning, so a violation here means the pruning has a hole.
// Loud in dev/tests, free in release.
#[cfg(debug_assertions)]
if let Err(e) = validate_daily_caps(problem, &solution.placements) {
    panic!("daily-cap post-condition violated: {e}");
}
```

Both validators live in `solver/solver-core/src/validate.rs` and follow the same shape: walk `placements`, build `(key) -> value` maps, return `Err(Error::Input(...))` on the first violation. They cover two of the four hard-constraint families the solver enforces during the placement loop: same-room invariant, daily caps. The two other families, **non-overlap** (no two placements may share a teacher / class / room at the same time-block) and **lesson cardinality + block shape** (each lesson must be placed exactly `hours_per_week` times in `hours_per_week / preferred_block_size` blocks of `preferred_block_size` contiguous positions), are enforced as pruning inside `try_place_block` and the LAHC delta evaluators but never re-asserted on the final placements vector. A move that bypasses pruning (today: `kempe_apply_block` inserts into `state.used_*` HashSets without contains-checks) produces a silent hard violation that the soft scorer's deduplication step (`score::score_solution` at `solver/solver-core/src/score.rs:79-94`) collapses to a single counted position before computing gap costs. The soft score therefore *improves* on a doubly-booked schedule while the schedule is illegal.

Concrete evidence from `solver/solver-core/benches/BENCH_RESULTS.md`: every `lahc_rr_kempe` cell reports `soft_score = 0` on Doppelstunde-bearing fixtures, which is theoretically achievable on grundschule but not on whole-school dreizügig zweizügig at production weights. The dedup hides the cost.

The bug item 39 names: `kempe_apply_block` (`solver/solver-core/src/lahc.rs:1517`) computes `start_pos + k` for `k in 0..n` (where `n = lesson.preferred_block_size`) and inserts every `(teacher, time_block_id)`, `(class, time_block_id)`, `(room, time_block_id)` triple into `state.used_*`. If a chain neighbour (BFS-collected at chain construction time) happens to land at the same `(dest_day, start_pos)` window as the seed, the second `kempe_apply_block` call inserts duplicate `time_block_id` keys into `state.used_*` (HashSet ignores duplicates without erroring) and pushes a duplicate row into `placements`. The chain construction in `kempe_attempt` does not pre-check for window self-overlap. This is the bug item 39 is the diagnostic for; item 40 is the property-test generator change that surfaces the bug deterministically. Both items 39 and 40 land in the active sprint's correctness phase; item 39 ships first because the diagnostic is also useful for any future move, not just Kempe.

`validate_daily_caps` already coexists with `validate_no_room_hopping` in the working tree (see "Bundled in-flight work" below). The new validator follows the same one-fn-one-walk shape, lives next to the two existing post-conditions, and shares the same `Error::Input` failure type.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 39. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run). Branched from master tip `4559138` (PR #188).

## Scope

**In scope.**

- `validate_no_double_booking(problem: &Problem, placements: &[Placement]) -> Result<(), Error>` in `solver/solver-core/src/validate.rs`, placed immediately after `validate_daily_caps` so file-order traces sprint phase order. One walk over `placements` builds five sets of state and returns `Err(Error::Input(...))` on the first violation found:
  - `class_used: HashMap<(SchoolClassId, TimeBlockId), LessonId>` populated for every `class_id` in `lesson.school_class_ids`. Duplicate insert is the class double-booking failure.
  - `teacher_used: HashMap<(TeacherId, TimeBlockId), LessonId>` populated once per placement (one teacher per lesson). Duplicate insert is the teacher double-booking failure.
  - `room_used: HashMap<(RoomId, TimeBlockId), LessonId>` populated once per placement. Duplicate insert is the room double-booking failure.
  - `rows_by_lesson: HashMap<LessonId, Vec<(u8, u8, RoomId)>>` keyed by `lesson_id`, value carries `(day_of_week, position, room_id)` per placement. After the walk, for each lesson:
    - Assert `rows.len() == lesson.hours_per_week`. Less is a missing-rows failure; more is a duplicate-rows failure.
    - Group `rows` by `day_of_week`; assert each day has `>= preferred_block_size` rows; partition each day's rows into runs of contiguous positions; assert every run has length `preferred_block_size`; assert every run shares one `room_id`. The assertion catches non-contiguous positions, blocks split across rooms, blocks shorter than `preferred_block_size`.
    - Assert `rows.len() / preferred_block_size == sum_of_blocks_across_days` (no orphan single rows of an N>1 block).
- `Error::Input` failure messages prefixed with `double-booking:` / `lesson cardinality:` / `block shape:` so the panic message in `solve.rs:240` discriminates which check fired without a `match`.
- Wire the call at `solver/solver-core/src/solve.rs:233` (right after `validate_no_room_hopping`):
  ```rust
  validate_no_room_hopping(problem, &solution.placements)?;
  validate_no_double_booking(problem, &solution.placements)?;
  ```
- Wire the cfg-panic at `solve.rs:238` alongside `validate_daily_caps`:
  ```rust
  #[cfg(debug_assertions)]
  if let Err(e) = validate_daily_caps(problem, &solution.placements) {
      panic!("daily-cap post-condition violated: {e}");
  }
  #[cfg(debug_assertions)]
  if let Err(e) = validate_no_double_booking(problem, &solution.placements) {
      panic!("no-double-booking post-condition violated: {e}");
  }
  ```
- Ten unit tests inline at the bottom of `validate.rs` (one `#[test]` per failure shape plus a happy path). Reuses the existing `minimal_problem()` helper. Each test constructs a hand-crafted `Vec<Placement>` and asserts the matching `Error::Input` substring fires. See "Tests" below for the full list.
- `mise run test:rust` after the validator commit must report green. If `caps_kempe_solve_under_production_caps_smoke` (the working-tree smoke test in `tests/daily_caps.rs`) panics with `no-double-booking post-condition violated:`, that confirms the Kempe `used_*` insert bug fires on Doppelstunden under production weights, and the same PR adds a seventh commit `fix(solver-core): kempe contains-check guards in apply_block` that aborts the chain at construction time when a self-overlap is detected. That fix is conditional and only lands if the panic fires.
- `docs/superpowers/OPEN_THINGS.md` strips item 39 entirely (no "shipped" marker per autopilot rules; the PR description and `git log` are the canonical record). Items 40 and 41 stay in the correctness phase. The next-pickup line in the active-sprint header points at item 40 if Q4 branch 1 fired (no Kempe fix landed) or at item 30 if Q4 branch 2 fired (Kempe fix landed and item 40's risk is reduced).

**Out of scope.**

- Any change to `score::score_solution`'s deduplication (`score.rs:79-94`). The dedup is correct for the soft score's gap-counting semantics; it is NOT the validator's job to alter scoring. The validator catches the schedule-level violation independently of how the soft scorer treats it.
- Item 40 (mixing `preferred_block_size` in `lahc_small_problem`). Land separately so the property-test generator's diff is reviewable on its own.
- Item 41 (reconciling `solution.soft_score` with the full weighted cost). Different concern: the slice contamination is a scoring bug, not a hard-constraint bug.
- Bench refresh (`mise run bench:bakeoff`). The validator changes runtime-mode hard-error semantics, not optimisation behaviour; baseline numbers are unaffected. The `#[cfg(debug_assertions)]` panic is dev/test only and the bake-off bench runs in release mode.
- Adding `validate_no_double_booking` calls to standalone unit tests of `kempe_apply_block` or `try_place_block`. The wiring through `solve_with_config` covers every existing caller transitively; explicit per-call assertions are YAGNI.
- New `Error::SolverInvariant` enum variant. Cascades to `solver-py` error mapping and backend response models; deferred to a future audit if a typed solver-bug error becomes useful across all three sibling validators at once.
- Backend / frontend changes. The validator's Result is consumed in `solver-core`; failures propagate as the existing `Error::Input` shape and reach the backend through `solver-py` as `PyValueError`, which the backend already handles as a `500 Internal Server Error` with the message included.

## Bundled in-flight work

Per user direction, the same PR carries the working-tree changes that landed before this run started. Each chunk gets its own typed commit on the same feature branch:

1. `feat(solver-core): validate_daily_caps post-condition + cfg-panic wiring (sprint 5 item 39 prep)`. Picks up the already-present `validate.rs` (`validate_daily_caps` plus four unit tests), `solve.rs` (cfg-panic at line 238), `tests/daily_caps.rs` (`caps_kempe_solve_under_production_caps_smoke`), and `docs/superpowers/OPEN_THINGS.md` (additions of items 39, 40, 41 into the correctness phase header). The OPEN_THINGS additions ride here because they describe the correctness sprint as a whole; the strip-item-39 edit lands in step 6 of the autopilot run.
2. `fix(solver-core): drop class_day_balance and home_room from rr_attempt slice score`. The `lahc.rs` `rr_attempt` post-recreate score-recompute change. Restores the slice/full-score asymmetry: greedy / Change / Kempe maintain the slice (`class_gap + teacher_gap + subj_pref`), so R&R must too; including `class_day_balance` or `home_room` contaminates `state.soft_score` and downstream Change-move deltas drive it negative over time.
3. `refactor(solver-core): caller computes kempe removed_subject_pref from ruined rows`. The `lahc.rs` `kempe_snapshot_pre_score` signature change. Caller-side computation reads only the actually-ruined rows so multi-block-on-other-day placements aren't wrongly double-counted (a chain member with another untouched block on a different day must contribute zero to the delta).
4. `feat(solver-core): validate_no_double_booking post-condition (sprint 5 item 39)`. The headline change for this PR (this spec).
5. `fix(backend): collect_pinned_placements pins cross-class lessons whenever any sibling unaffected (sprint 5 item 33)`. The `solver_io.py` rewrite. Cross-class lessons get pinned whenever ANY of their classes lies outside `exclude_class_ids`; only lessons whose membership is entirely inside the exclusion set are dropped. Closes item 33; the design contract for "schedule one class without disturbing others" is now: sibling schedules are immutable on per-class re-solve, including cross-class lessons that touch the focus class.
6. `style(frontend): order Google Fonts @import before tailwindcss`. Trivial CSS reorder so the Quicksand / Lora / Fira Code font preload precedes Tailwind's reset.

If Q4 branch 2 fires (Kempe contains-check panic on the smoke test), commit 7 is `fix(solver-core): kempe contains-check guards in apply_block`, landed between commit 4 and the documentation pass. Commit 7 is conditional and not pre-written here.

The order is deliberate. Commit 1 lands the daily-caps validator first so its cfg-panic pattern is visible when commit 4 mirrors it. Commits 2 and 3 land the lahc.rs fixes before commit 4 so any subtle interaction between the slice fix and the validator surfaces in commit 4's TDD step. Commits 5 and 6 are independent of the solver-core stack and could ride in any order; landing them last keeps the solver-core sequence reviewable in isolation.

## Failure mode and fix

**Trigger.** `validate_no_double_booking` returns `Err(Error::Input(msg))` when:

1. **Class double-booking.** Two placements `p1`, `p2` exist where `p1.lesson_id != p2.lesson_id` and `p1.time_block_id == p2.time_block_id` and the two lessons share at least one `school_class_id`. Message: `"double-booking: class {:?} at time-block {:?}: lessons {:?} and {:?}"`.
2. **Teacher double-booking.** Two placements with `p1.lesson_id != p2.lesson_id`, `p1.time_block_id == p2.time_block_id`, and the two lessons share a `teacher_id`. Message: `"double-booking: teacher {:?} at time-block {:?}: lessons {:?} and {:?}"`.
3. **Room double-booking.** Two placements with `p1.lesson_id != p2.lesson_id`, `p1.time_block_id == p2.time_block_id`, `p1.room_id == p2.room_id`. Message: `"double-booking: room {:?} at time-block {:?}: lessons {:?} and {:?}"`.
4. **Lesson cardinality (too few).** A lesson appears in `placements` fewer than `hours_per_week` times. Message: `"lesson cardinality: lesson {:?} has {} placements, expected {}"`. The validator does NOT check for "lesson appears zero times" as a special case; the existing `validate_structural` and the `Violation::NoFreeTimeBlock` flow handle that. The validator runs after both, so any lesson with a zero count here was unplaceable and is already surfaced as a violation, which means the solver returned `solution.placements.len() < expected` and the bake-off bench's per-cell placement-count gate (item 28) catches it. The validator is a defence in depth, not the primary detector.
5. **Lesson cardinality (too many).** A lesson appears more times than `hours_per_week`. Same message shape. This is the shape that catches `kempe_apply_block`'s duplicate-row insert: the second `kempe_apply_block` call on a chain self-overlap pushes a row that takes the lesson's count from `H` to `H + N`.
6. **Block shape (non-contiguous positions on one day).** A lesson with `preferred_block_size > 1` has `>= preferred_block_size` rows on one day, but the positions don't form a contiguous run of length `preferred_block_size`. Message: `"block shape: lesson {:?} on day {} has positions {:?}, expected contiguous run of length {}"`.
7. **Block shape (block split across rooms).** A lesson's contiguous-position window on one day uses two different `room_id`s. Message: `"block shape: lesson {:?} on day {} has rooms {:?}, expected one room per block"`.
8. **Block shape (orphan row of N>1 block).** A lesson with `preferred_block_size = N >= 2` has a day with exactly 1 row (or any non-multiple of N). Message: `"block shape: lesson {:?} on day {} has {} rows, expected multiple of {}"`. (Caught by the per-day-rows < N check before the contiguous-run partition.)

**Detection algorithm.** One pass over `placements`, then one pass over the per-lesson row map:

```rust
pub fn validate_no_double_booking(problem: &Problem, placements: &[Placement]) -> Result<(), Error> {
    use std::collections::hash_map::Entry;
    use std::collections::HashMap;

    let lesson_by_id: HashMap<LessonId, &Lesson> = problem.lessons.iter().map(|l| (l.id, l)).collect();
    let tb_by_id: HashMap<TimeBlockId, &TimeBlock> = problem.time_blocks.iter().map(|t| (t.id, t)).collect();

    let mut class_used: HashMap<(SchoolClassId, TimeBlockId), LessonId> = HashMap::new();
    let mut teacher_used: HashMap<(TeacherId, TimeBlockId), LessonId> = HashMap::new();
    let mut room_used: HashMap<(RoomId, TimeBlockId), LessonId> = HashMap::new();
    let mut rows_by_lesson: HashMap<LessonId, Vec<(u8, u8, RoomId)>> = HashMap::new();

    for p in placements {
        let lesson = lesson_by_id.get(&p.lesson_id)
            .ok_or_else(|| Error::Input(format!("unknown lesson {:?}", p.lesson_id)))?;
        let tb = tb_by_id.get(&p.time_block_id)
            .ok_or_else(|| Error::Input(format!("unknown time block {:?}", p.time_block_id)))?;

        for class_id in &lesson.school_class_ids {
            match class_used.entry((*class_id, p.time_block_id)) {
                Entry::Vacant(v) => { v.insert(p.lesson_id); }
                Entry::Occupied(o) if *o.get() == p.lesson_id => { /* same lesson, allowed */ }
                Entry::Occupied(o) => {
                    return Err(Error::Input(format!(
                        "double-booking: class {:?} at time-block {:?}: lessons {:?} and {:?}",
                        class_id, p.time_block_id, o.get(), p.lesson_id
                    )));
                }
            }
        }
        match teacher_used.entry((lesson.teacher_id, p.time_block_id)) {
            Entry::Vacant(v) => { v.insert(p.lesson_id); }
            Entry::Occupied(o) if *o.get() == p.lesson_id => { /* same lesson, allowed */ }
            Entry::Occupied(o) => {
                return Err(Error::Input(format!(
                    "double-booking: teacher {:?} at time-block {:?}: lessons {:?} and {:?}",
                    lesson.teacher_id, p.time_block_id, o.get(), p.lesson_id
                )));
            }
        }
        match room_used.entry((p.room_id, p.time_block_id)) {
            Entry::Vacant(v) => { v.insert(p.lesson_id); }
            Entry::Occupied(o) if *o.get() == p.lesson_id => { /* same lesson, same row, allowed (caught by cardinality below) */ }
            Entry::Occupied(o) => {
                return Err(Error::Input(format!(
                    "double-booking: room {:?} at time-block {:?}: lessons {:?} and {:?}",
                    p.room_id, p.time_block_id, o.get(), p.lesson_id
                )));
            }
        }
        rows_by_lesson.entry(p.lesson_id).or_default().push((tb.day_of_week, tb.position, p.room_id));
    }

    for (lesson_id, mut rows) in rows_by_lesson {
        let lesson = lesson_by_id[&lesson_id];
        if rows.len() != lesson.hours_per_week as usize {
            return Err(Error::Input(format!(
                "lesson cardinality: lesson {:?} has {} placements, expected {}",
                lesson_id, rows.len(), lesson.hours_per_week
            )));
        }
        rows.sort_unstable_by_key(|(day, pos, _)| (*day, *pos));
        let n = lesson.preferred_block_size as usize;
        let mut day_groups: HashMap<u8, Vec<(u8, RoomId)>> = HashMap::new();
        for (day, pos, room) in rows {
            day_groups.entry(day).or_default().push((pos, room));
        }
        for (day, day_rows) in day_groups {
            if day_rows.len() % n != 0 {
                return Err(Error::Input(format!(
                    "block shape: lesson {:?} on day {} has {} rows, expected multiple of {}",
                    lesson_id, day, day_rows.len(), n
                )));
            }
            // Walk in chunks of `n`: each chunk must be contiguous and one-room.
            for chunk in day_rows.chunks(n) {
                let first_pos = chunk[0].0;
                let first_room = chunk[0].1;
                for (i, (pos, room)) in chunk.iter().enumerate() {
                    if *pos != first_pos + i as u8 {
                        return Err(Error::Input(format!(
                            "block shape: lesson {:?} on day {} has positions {:?}, expected contiguous run of length {}",
                            lesson_id, day, chunk.iter().map(|(p, _)| *p).collect::<Vec<_>>(), n
                        )));
                    }
                    if *room != first_room {
                        return Err(Error::Input(format!(
                            "block shape: lesson {:?} on day {} has rooms {:?}, expected one room per block",
                            lesson_id, day, chunk.iter().map(|(_, r)| *r).collect::<Vec<_>>()
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}
```

The `Entry::Occupied(o) if *o.get() == p.lesson_id` guard is intentional: a single lesson legitimately occupies the same `(class, time_block)`, `(teacher, time_block)`, and `(room, time_block)` once per row (it IS the row), and the validator's job is to catch CROSS-LESSON conflicts, not flag a single placement against itself. The cardinality check below catches genuine duplicate rows (lesson placed at the exact same `(time_block_id, room_id)` twice produces `rows.len() > hours_per_week`).

**Complexity.** O(P) for the first walk (P = total placements), O(P) for the per-lesson assertion (sum of `rows.len()` across all lessons equals P). Memory: three `HashMap`s of size O(P) plus one `HashMap<LessonId, Vec<(...)>>` of size O(L + P). Free in release mode (cfg-gated panic block compiles to nothing); under `cfg(debug_assertions)` adds one extra walk per `solve_with_config` call (the wiring at line 233 already runs in release, so the debug-mode panic is a re-run, not the only run; this is intentional for the loud test failure).

**Why this is the right shape.**

The five concerns share one walk because:
- The lesson lookup and time-block lookup are needed by all of them.
- The duplicate-`(class, tb)` check requires aggregating by `class_id`, which the per-lesson row aggregator also walks (cross-class lessons populate one entry per class).
- The block-shape check requires per-`(lesson, day)` partitioning, which is cheap to derive from the same row aggregator without re-iterating `placements`.

Splitting the validator (e.g., `validate_no_class_overlap` + `validate_no_teacher_overlap` + `validate_no_room_overlap` + `validate_lesson_cardinality` + `validate_block_shape`) would force five separate walks of `placements` and three separate lesson lookups. The bench cost is small (P is ~300 on the dreizügige fixture) but the code-density gain is also small; one function with one walk matches the sibling validators' style and stays grep-friendly.

## Determinism and bench impact

The validator is read-only over `&Problem` and `&[Placement]`. No RNG, no mutation, no I/O. The HashMap iteration order is irrelevant because the validator returns on the first error found and the unit tests assert error substrings rather than ordered error lists. The release-mode validator at `solve.rs:233` adds one O(P) walk per `solve_with_config` call; on the dreizügige fixture (P=294) this is sub-microsecond on a modern x86 box and well below the 20% bench-budget noise floor of `mise run bench`. The cfg-panic at `solve.rs:238` is `#[cfg(debug_assertions)]`-gated and disappears in release builds (which include both the criterion bench and the bake-off bench).

`mise run bench` cost expected to be flat. If the bench shows `>10%` regression (unlikely; the validator does no heap allocation beyond the four HashMaps which size against P, not against `time_blocks` * `rooms` like the solve loop), inline the lookup builders into `solve_with_config` so they're shared with `validate_no_room_hopping` and `validate_daily_caps`. Today each of those validators rebuilds its own lesson-by-id and tb-by-id lookups; sharing the lookups across all three validators is a follow-up optimisation, not a blocker.

Bake-off bench (`mise run bench:bakeoff`) runs in release mode so the cfg-panic is inactive; the release-mode `Result` form at line 233 fires for any backend that produces an illegal schedule and surfaces the failure as `Err` to `solve_with_config`'s caller. The bake-off harness already maps `Err` results to a "0 placements" cell, so the impact on `BENCH_RESULTS.md` is: any Kempe-chain double-booking that previously returned a placements vec with `soft_score = 0` now returns `Err` and the cell drops to a `0` placement count, exposing the bug visually. This is the desired outcome of item 39; the bench refresh that actually re-records `BENCH_RESULTS.md` against the new failure semantics is item 30's job, not this PR's.

## Tests

Ten unit tests inline at the bottom of `solver/solver-core/src/validate.rs`, all using the existing `minimal_problem()` helper plus targeted overrides:

1. `validate_no_double_booking_accepts_well_formed_schedule`: happy path, lesson with `hours_per_week=2, preferred_block_size=2` placed contiguously on one day in one room.
2. `validate_no_double_booking_rejects_class_double_booking`: two lessons sharing a class placed at the same `time_block_id`. Asserts message contains `"double-booking: class"`.
3. `validate_no_double_booking_rejects_class_double_booking_via_cross_class_lesson`: cross-class lesson `[c1, c2]` and single-class lesson on `c1` collide at one `time_block_id`. Asserts message contains `"double-booking: class"` AND mentions `c1`.
4. `validate_no_double_booking_rejects_teacher_double_booking`: two lessons sharing a teacher at the same `time_block_id`. Asserts message contains `"double-booking: teacher"`.
5. `validate_no_double_booking_rejects_room_double_booking`: two lessons (different teacher, no class overlap) at the same `(room_id, time_block_id)`. Asserts message contains `"double-booking: room"`.
6. `validate_no_double_booking_rejects_lesson_cardinality_too_few`: lesson with `hours_per_week=2` placed once. Asserts `"lesson cardinality"` and `"expected 2"`.
7. `validate_no_double_booking_rejects_lesson_cardinality_too_many`: lesson with `hours_per_week=2` placed three times. Asserts `"lesson cardinality"` and `"expected 2"`.
8. `validate_no_double_booking_rejects_block_shape_non_contiguous`: lesson with `hours_per_week=2, preferred_block_size=2` placed at positions `[0, 2]` on one day. Asserts `"block shape"` and `"contiguous run of length 2"`.
9. `validate_no_double_booking_rejects_block_shape_split_across_rooms`: lesson with `hours_per_week=2, preferred_block_size=2` placed at contiguous positions on one day but two different rooms. Asserts `"block shape"` and `"one room per block"`.
10. `validate_no_double_booking_rejects_block_shape_orphan_row`: lesson with `hours_per_week=2, preferred_block_size=2` placed once on Monday and once on Tuesday (each day has 1 row, less than `preferred_block_size`). Asserts `"block shape"` and `"multiple of 2"`.

No new integration tests under `solver-core/tests/`. The wiring through `solve_with_config` covers every existing integration test (`grundschule_smoke.rs`, `daily_caps.rs`, `lahc_property.rs`, `rr_anchor_filter.rs`, `rr_rollback.rs`, `properties.rs`, `score_property.rs`, `ffd_solver_outcome.rs`) automatically. The cfg-panic at line 238 fires during every `solve_with_config` call in test mode; if any of these tests produce an illegal schedule today, that's the Kempe-bug-fires branch from Q4.

If branch 2 fires (Kempe panic on the daily-caps smoke test), the remediating commit 7 adds one regression test `kempe_chain_self_overlap_does_not_double_book` in `solver-core/tests/lahc_property.rs` (or a dedicated `tests/kempe_self_overlap.rs` if the fix grows): a hand-crafted problem with two lessons that share a class and have `preferred_block_size=2`, run through `lahc_rr_kempe`, asserts no double-booking. Today this would panic; with the fix it's green.

## Documentation

- `docs/superpowers/OPEN_THINGS.md`: strip item 39 entirely (no shipped marker). Update the active-sprint header's "Next pickup" line to point at item 40 (correctness phase, P0). Items 40 and 41 stay unchanged in the correctness phase. The bundled fix items (33, plus the lahc.rs slice work which closes item 41 partially) require a re-read: item 33 strips entirely (the rewrite ships); item 41's "replace line 243 with `score_solution(...)`" option is now half-resolved by the lahc.rs slice fix (the slice was contaminating `state.soft_score`; the fix restores the slice so `solution.soft_score = state.soft_score` is now the slice as documented). Item 41's residual concern (LAHC vs cpsat backend reporting different objectives) stays in OPEN_THINGS.
- `solver/CLAUDE.md`: add a sentence to the existing "Per-day caps" bullet referencing the new validator: "A second post-condition validator (`validate_no_double_booking`) covers non-overlap and block-shape invariants symmetrically; both validators run in release as `Result`-returning calls right after `validate_no_room_hopping`, and both panic loud under `cfg(debug_assertions)` so property and integration tests fail fast on a silent hard violation."
- No ADR. Item 39 is a diagnostic that mirrors the existing `validate_no_room_hopping` pattern; the architectural decision (post-condition validators alongside pruning) was implicit in PR #188 (ADR 0033) and PR #186 (the row-keyed rollback) and does not need a fresh ADR.
- No backend or frontend documentation. The validator is invisible to API consumers.

## Acceptance criteria

1. `mise run lint` green (covers `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt -- --check`, the workspace machete sweep, and the unique-fn-names check).
2. `mise run test:rust` green. Includes:
   - Ten new unit tests in `validate.rs::tests` all pass.
   - All existing solver-core integration tests pass.
   - The cfg-panic at `solve.rs:238` does not fire on the existing test surface (Q4 branch 1) OR fires only on the daily-caps smoke test, in which case commit 7 lands the Kempe contains-check fix and tests pass after that commit (Q4 branch 2).
3. `mise run test:py` green. The backend tests should be unaffected (`solver_io.py` change is item 33's, semantic shift in `collect_pinned_placements` may need a regression test update; if so, that test update lands in commit 5 alongside the rewrite).
4. `mise run fe:test` green. The CSS reorder is style-only; no behavioural impact expected.
5. Push triggers `pre-push` (workspace nextest + pytest with coverage + frontend Vitest) and the push only proceeds if everything is green.
6. PR title satisfies `subjectPattern: ^[a-z].+$`: `feat(solver-core): validate_no_double_booking post-condition (sprint 5 item 39)`.
7. PR body includes the validator function signature, the wiring snippet, the bundled-work commit list, the rationale for the bundling, and a "Q4 branch resolved as: ..." line documenting which branch fired.
8. After step 6 of the autopilot, OPEN_THINGS no longer contains item 39, item 33, or any other item the bundled commits closed. Items 40, 41 stay; the active-sprint header's next-pickup line points at item 40.
9. Skill audit (autopilot step 7) passes: `superpowers:using-superpowers`, `superpowers:brainstorming`, `superpowers:writing-plans`, `superpowers:test-driven-development`, `superpowers:subagent-driven-development`, `claude-md-management:revise-claude-md`, `claude-md-management:claude-md-improver`, `fewer-permission-prompts` all show in the session's tool-call history.
