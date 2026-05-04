# Smarter FFD ordering + bake-off bench harness spec (Sprint 1, items 3 + 4 + 5)

**Sprint program.** Solver feasibility bake-off (active program, Sprint 1).
**Phase.** Sprint 1: Path A (smarter FFD ordering) + benchmark harness + ADR.
**Goal.** Replace FFD's eligibility metric with a same-room-aware variant (Path A) so the demo Grundschule lock-in fixture solves to zero hard violations under greedy-only, ship the bake-off bench harness that compares candidates across fixtures, and record the bake-off methodology in an ADR.

**Non-goal.** No new optimization-phase backend (R&R, Kempe, CP-SAT all stay deferred to Sprints 2-4). No `KZ_SOLVER_BACKEND` env-var dispatch in solver-py / backend (Sprint 4). No Sek-I stretch fixture (deferred to Beyond-Grundschule program Sprint 2). No `peak_memory_kb` or `time_to_first_feasible_ms` columns in the bench (deferred to Sprint 4).

## Context

PR #173 (diagnostic phase) shipped the deterministic FFD lock-in reproducer (`tests/same_room_property.rs::ffd_locks_in_on_demo_grundschule_and_returns_no_suitable_room`) and the `solver-trace` Cargo feature that explains FFD's per-decision reasoning. The reproducer confirms the failure mode: FFD's current eligibility metric (`free_blocks * suitable_rooms`) overestimates a constrained lesson's flexibility because it counts free time-blocks and suitable rooms separately, ignoring that the same-room hard constraint forces a lesson's full hours-per-week to fit within a single (day, room) family per `(class, subject)` triple. LAHC cannot rescue an FFD `NoSuitableRoom` violation because LAHC moves accepted placements, not unplaced lessons. Anchor research: `docs/research/2026-04-04-solver-algorithm-selection.md`. Anchor sprint: `docs/superpowers/OPEN_THINGS.md` "Active sprint program: Solver feasibility bake-off".

## Scope

**In scope.**

- **Path A (item 5).** New `same_room_eligibility` function in `solver/solver-core/src/ordering.rs` replacing the existing `eligibility`. Counts `(day, room)` pairs where at least `preferred_block_size` consecutive teacher-unblocked, room-suitable, room-unblocked time-blocks exist on `day` for the lesson. Lessons with lower counts sort first. Lesson-id tiebreak preserved.
- **Bench harness (item 3).** New `solver-bench` workspace member at `solver/solver-bench/`. Single binary that runs `solve_with_config` per `(fixture, backend, seed)` cell, captures per-cell aggregates (feasibility n/N, hard violations median, soft score median over feasible runs, FFD wall-clock median, total wall-clock median), and writes the result to `solver/solver-core/benches/BENCH_RESULTS.md`. New mise task `bench:bakeoff`.
- **Shared fixture module.** Refactor `grundschule_fixture` / `zweizuegig_fixture` / `dreizuegig_fixture` out of `solver/solver-core/benches/solver_fixtures.rs` (criterion bench) and `ffd_lock_in_grundschule` out of `solver/solver-core/tests/same_room_property.rs` into a new `pub mod test_fixtures` in `solver-core`, gated behind `feature = "fixtures"` (off by default). Both consumers (the integration test and `solver-bench`) opt in via the feature flag. The criterion bench (`solver_fixtures.rs`) also opts in to share fixtures.
- **Lock-in test flip.** Per the docstring at `same_room_property.rs:875`: rename `ffd_locks_in_on_demo_grundschule_and_returns_no_suitable_room` → `ffd_does_not_lock_in_on_demo_grundschule`, invert the assertion to `assert!(solution.violations.is_empty())`, update the docstring to record that Path A landed.
- **ADR 0029 (item 4).** New `docs/adr/0029-solver-feasibility-bake-off.md` recording the four-candidate methodology, fixture set, metric definitions, deviations from OPEN_THINGS item 3.
- **OPEN_THINGS edits (in step 6 of the autopilot run).** Mark Sprint 1 items 3-5 shipped, narrate the deviations, add an "After bake-off" follow-up for `peak_memory_kb` / `time_to_first_feasible_ms`.

**Out of scope.**

- LAHC move-set knob (`MoveSet::with_rr()` etc.) for Sprint 2's R&R move. The bench's `BenchBackend::Lahc` enum has one variant in Sprint 1; later sprints add variants with their own LAHC config knobs.
- Public `solver_core::SolverBackend` enum. Sprint 1's bench keeps the backend axis private to `solver-bench`. Sprint 4 adds the public enum when CP-SAT requires it.
- `KZ_SOLVER_BACKEND` env-var dispatch in `scheduling/solver_io.run_solve`. Owned by Sprint 4 when there is more than one backend.
- Repair of the zweizuegig bench fixture's `assert!(solution.violations.is_empty())` panic under the same-room hard constraint (OPEN_THINGS item 15). Out of scope; rolled into "After bake-off: deferred quality work".

## Path A: same-room-aware FFD eligibility

### Algorithm

For lesson `L` with class set `C`, subject `S`, hours-per-week `H`, preferred-block-size `N`:

```rust
fn same_room_eligibility(lesson: &Lesson, problem: &Problem, idx: &Indexed) -> u32 {
    let n = lesson.preferred_block_size as usize;
    let mut viable_pairs: u32 = 0;
    for day in distinct_days_in_problem {
        let day_tbs: Vec<&TimeBlock> = sorted_time_blocks_on_day(day);
        for room in &problem.rooms {
            if !idx.room_suits_subject(room.id, lesson.subject_id) { continue; }
            // Sliding window of length n: at least one window must have all n
            // TBs teacher-unblocked AND room-unblocked.
            for window in day_tbs.windows(n) {
                let teacher_ok = window.iter().all(|tb|
                    !idx.teacher_blocked(lesson.teacher_id, tb.id));
                let room_ok = window.iter().all(|tb|
                    !idx.room_blocked(room.id, tb.id));
                if teacher_ok && room_ok {
                    viable_pairs = viable_pairs.saturating_add(1);
                    break; // count the (day, room) pair once
                }
            }
        }
    }
    viable_pairs
}
```

Lower `viable_pairs` = more constrained = sorted first. Tiebreak by lesson_id byte order, unchanged.

### Why this fixes the lock-in

Old metric for `4a-FÖ` on the lock-in fixture: `free_blocks * suitable_rooms = 35 * 3 = 105` (Klassenraum 4a is in `room_blocked_times` so suitable_rooms drops from 4 to 3). That ranks 4a-FÖ comparable to other less-constrained lessons.

New metric for `4a-FÖ`: 5 days × 3 academic Klassenräume × (sliding-window count where the room is unblocked) = 15 raw pairs, minus the (day, klassenraum 4a) cells which are entirely blocked. Net `viable_pairs ≤ 15`, far below e.g. `1a-D` (5 × 4 × full = 20 pairs without any blocking). 4a-FÖ now sorts ahead of less-constrained lessons and gets first pick of (day, room) before sibling lessons commit rooms that 4a-FÖ would have needed.

The metric is also more honest for doppelstunden: a lesson with `H=4, N=2` (block_count=2) needs windows of length 2; the old metric counts free time blocks individually and overstates flexibility.

### Public API surface change

`ordering::ffd_order` signature unchanged (`pub(crate) fn ffd_order(problem: &Problem, idx: &Indexed) -> Vec<usize>`). Internal helper renamed (`eligibility` → `same_room_eligibility`). Backwards-compat: zero impact on callers; FFD ordering is invariant to lesson input order both before and after.

### Tests

1. **Unit tests in `solver/solver-core/src/ordering.rs::tests`.** Replace existing tests where semantics changed; preserve the input-order-invariance test. Add:
   - `ffd_order_places_more_constrained_lesson_first` (renamed from `ffd_order_places_low_eligibility_lesson_first`; fixture updated for the new metric).
   - `ffd_order_doppelstunde_with_one_viable_window_sorts_first` (new: doppelstunde with `N=2`, only one (day, room) pair fits two consecutive teacher-unblocked TBs; ordinary single-period lesson has many viable pairs).
   - `ffd_order_tiebreaks_on_lesson_id_when_eligibility_ties` (existing; fixture may need tightening so eligibility actually ties under the new metric).
   - `ffd_order_returns_every_index_exactly_once` (existing; untouched).
   - `ffd_order_lifts_unqualified_lesson_to_the_front` (existing; the unqualified-teacher case still resolves via the placement loop's qualification check, not the metric).

2. **Property test in `solver/solver-core/tests/properties.rs`.** New `ffd_ordering_invariant_to_input_permutation`: proptest generates a Problem (1..=5 lessons, 1..=3 rooms, 1..=2 days, 1..=4 periods), random-permutes `problem.lessons`, asserts the resulting FFD order produces the same lesson-id sequence after deduplication. (Existing properties.rs may already have a similar test; adapt rather than duplicate.)

3. **Integration test flip in `solver/solver-core/tests/same_room_property.rs`.** `ffd_locks_in_on_demo_grundschule_and_returns_no_suitable_room` → `ffd_does_not_lock_in_on_demo_grundschule`, assertion inverted to `assert!(solution.violations.is_empty(), "expected zero violations after Path A; got {:?}", solution.violations)`. Docstring updated to record Path A's landing.

## Bench harness: `solver-bench`

### Crate layout

```
solver/solver-bench/
  Cargo.toml          # depends on solver-core (workspace path) with feature "fixtures"
  src/
    main.rs           # CLI parsing, measurement loop, markdown emission
```

`Cargo.toml` adds `solver/solver-bench` to workspace `members`. No external runtime dependencies in Sprint 1; `solver-bench` is a thin Rust binary on top of `solver-core`. CLI parsing is manual (~40 lines) to keep the dep graph minimal.

### CLI

```
solver-bench [--budget <duration>] [--seeds <n>] [--fixtures <names>] [--out <path>]
```

| Flag | Default | Notes |
|---|---|---|
| `--budget` | `60s` | Per-seed wall-clock budget passed to `SolveConfig.deadline`. |
| `--seeds` | `20` | Seeds 1..=N. |
| `--fixtures` | all four | Comma-separated subset of `grundschule,zweizuegig,dreizuegig,lock_in`. |
| `--out` | `solver/solver-core/benches/BENCH_RESULTS.md` | Output markdown path. |

Manual arg parsing in `main()`; unknown flag exits 2 with a one-line error.

### Mise task

```toml
[tasks."bench:bakeoff"]
description = "Run the solver feasibility bake-off bench and rewrite BENCH_RESULTS.md"
run         = "cargo run -p solver-bench --release"
```

`--release` matters: greedy is fast even in debug, but a 60-second LAHC budget in debug builds skews per-seed iteration counts and soft-score medians.

### Per-cell measurement protocol

Per `(fixture, backend)` cell:

1. Build the fixture once.
2. Run greedy solve once with the production active-default weights (`class_gap=10, teacher_gap=10, prefer_early_period=1, avoid_first_period=1, prefer_home_room=5, avoid_last_period=1, prefer_late_period=1, class_day_balance=5`) and `deadline: None`. Capture FFD wall-clock ms, FFD-only feasibility (1 if no hard violations else 0), FFD-only hard violations count, FFD-only soft score.
3. For `seed in 1..=N`: run `solve_with_config` with the same weights, `deadline: Some(--budget)`, `seed: <seed>`. Capture total wall-clock ms, feasibility 0/1, hard violations count, soft score.
4. Aggregate per cell: `feasibility = sum(feasibility) / N`, hard violations median (across all N runs), soft score median (over feasible runs only; emit `-` if no run is feasible), total wall-clock median.

`Backend` is `BenchBackend::Lahc` in Sprint 1 (single variant; one match arm forwarding to `solve_with_config`).

### BENCH_RESULTS.md schema

```markdown
# Solver bake-off feasibility bench

<!-- Regenerated by `mise run bench:bakeoff`. Do not hand-edit. -->

| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Soft score (median, feasible) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| grundschule | lahc | 20 | 20/20 | 0 | 0 | 0.05 | 60050 |
| zweizuegig | lahc | 20 | … | … | … | … | … |
| dreizuegig | lahc | 20 | … | … | … | … | … |
| lock_in | lahc | 20 | 20/20 | 0 | … | 0.06 | 60055 |

Refreshed 2026-05-04 on AMD Ryzen 7 3700X 8-Core Processor, Linux 6.8.0-90-generic, rustc 1.93.1.

Refresh with `mise run bench:bakeoff` when a backend changes or a fixture is added. The
bench is host-sensitive on wall-clock columns and host-stable on feasibility / hard-violation
columns.

See `docs/adr/0029-solver-feasibility-bake-off.md` for methodology.
```

The footer (host info) is auto-emitted from `/proc/cpuinfo`, `uname -r`, and `rustc --version` (same shape as `record_solver_bench.sh`).

### Determinism

LAHC seed comes from the seed loop variable. The bench is reproducible across hosts modulo wall-clock variance for the budget; feasibility and hard violations columns are host-stable, wall-clock columns are host-sensitive.

### Tests

- 2-3 unit tests inside `solver-bench/src/main.rs::tests` for the median helper, the markdown row builder, and the CLI parser.
- No integration test of the full measurement loop (the loop is short and reading it is faster than writing test scaffolding for it).

## Shared fixture module: `solver_core::test_fixtures`

### Public API surface

```rust
// solver/solver-core/src/lib.rs
#[cfg(feature = "fixtures")]
pub mod test_fixtures;
```

```rust
// solver/solver-core/src/test_fixtures.rs
//! Hand-coded `Problem` fixtures used by the criterion bench, the
//! bake-off bench, and integration tests. Mirrors the seed builders in
//! `backend/src/klassenzeit_backend/seed/demo_*.py`. Drift is caught by
//! `assert_eq!(lessons.len(), N)` per fixture.

pub fn grundschule_fixture() -> Problem { … }
pub fn zweizuegig_fixture() -> Problem { … }
pub fn dreizuegig_fixture() -> Problem { … }
pub fn ffd_lock_in_grundschule() -> Problem { … }
```

### Cargo.toml feature flag

```toml
# solver/solver-core/Cargo.toml
[features]
default      = ["fixtures"]
solver-trace = []
fixtures     = []
```

The `fixtures` flag has no dependencies; it gates the `pub mod test_fixtures` from production builds. Default-on means solver-core's own integration tests, the criterion bench, and solver-bench all see the module without per-target feature opt-in. solver-py opts out:

```toml
# solver/solver-py/Cargo.toml
[dependencies]
solver-core = { path = "../solver-core", default-features = false }
```

So the maturin-built `klassenzeit_solver` wheel ships without the fixture builders. No production code path ever pulls them in.

### Consumer updates

- `solver/solver-core/benches/solver_fixtures.rs`: drop the inline `grundschule_fixture` / `zweizuegig_fixture` / `dreizuegig_fixture` definitions, import them from `solver_core::test_fixtures`.
- `solver/solver-core/tests/same_room_property.rs`: drop the inline `ffd_lock_in_grundschule` builder, import from `solver_core::test_fixtures`.
- `solver/solver-bench/src/main.rs`: import all four fixtures from `solver_core::test_fixtures`.

## Commit shape

Five primary commits in this order:

1. `refactor(solver-core): extract bench fixtures into shared test_fixtures module`. Pure structural; behavior preserved.
2. `feat(solver-bench): introduce bake-off benchmark harness binary`. New workspace member, new mise task. Imports fixtures from `solver-core::test_fixtures`. No BENCH_RESULTS.md committed.
3. `feat(solver-core): same-room-aware FFD eligibility (Path A)`. `ordering.rs` change + unit test updates + property test + lock-in integration test flip.
4. `bench(solver): record post-Path-A BENCH_RESULTS.md`. Single new file.
5. `docs(adr): record solver feasibility bake-off methodology (ADR 0029)`.

Step 6 of the autopilot run produces additional commits for OPEN_THINGS / autopilot.md / CLAUDE.md edits.

## Risks

- **R1: Path A's metric does not flip the lock-in.** Mitigation: run the lock-in test locally under the new metric before commit 3 lands; iterate the metric formula if needed (e.g., scale by class's room reachability, or add a class-day-balance term).
- **R2: Path A regresses one of the existing 3 fixtures.** Mitigation: run `mise run bench:bakeoff` before commit 4 and assert feasibility=20/20 on every cell; iterate Path A if any cell shows `<20/20`.
- **R3: Path A's FFD wall-clock breaches the 20% perf-regression budget on `mise run bench`.** Mitigation: run the criterion bench, compare to BASELINE.md. New metric is O(days × rooms × window) per lesson; for dreizuegig (~3M ops per FFD pass) it should stay sub-millisecond. Worst case: cache `(day, room) → blocks-fitting-N` once during `Indexed::new`.
- **R4: Bench harness measurement bugs.** Mitigation: unit tests on the helpers; hand-validate the first BENCH_RESULTS.md against ad-hoc `solve_with_config` runs.
- **R5: ADR bakes in metric definitions that change in Sprint 2-4.** Mitigation: phrase the ADR's metric block as "the per-cell aggregate after running each backend with the configured budget", not "the exact column set we ship in Sprint 1". Document the deferred columns explicitly.

## Acceptance criteria

- `cargo nextest run -p solver-core` passes with all unit + property + integration tests, including the renamed `ffd_does_not_lock_in_on_demo_grundschule`.
- `cargo nextest run --workspace` passes (solver-py contract tests, solver-core full suite).
- `mise run lint` passes.
- `mise run bench:bakeoff` produces a `BENCH_RESULTS.md` with feasibility=20/20 on every cell. PR body cites the pre-Path-A run output (run on the commit before commit 3) for comparison.
- `mise run bench` produces a criterion run within 20% of the committed `BASELINE.md` numbers.
- ADR 0029 indexed in `docs/adr/README.md`; OPEN_THINGS Sprint 1 items 3-5 marked shipped with deviation narration.
