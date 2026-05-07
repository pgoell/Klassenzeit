# Backend objective parity contract (item 51)

**Sprint program.** Solver feasibility correctness + observability (active program), follow-ups bucket (`## Open solver follow-ups`).
**Phase.** Open follow-up: item 51 (P0).
**Goal.** Pin the contract that every backend's `Solution.soft_score` equals `score_solution(problem, placements, weights)` on its returned placements, and make each backend's *internal* objective drift from that canonical scorer visible in `BENCH_RESULTS.md` rather than hidden behind a post-hoc rescore.

**Non-goals.** No change to LAHC's hot-path slice (item 52). No CP-SAT objective port (item 48). No class-day-balance during search (item 54). No Timefold spike (item 55). No `Solution` / `SolveStats` wire-format change. No `mise run bench:bakeoff` regeneration (5 h cost; the Backend objectives section is static and hand-edited into the existing file so it is consistent before the next refresh).

## Context

`solver/solver-core/src/solve.rs:296` (item 41) overwrites `solution.soft_score = score_solution(problem, &solution.placements, &config.weights)` at the tail of `solve_with_config_stats`, so every Rust LAHC variant (`lahc`, `lahc_rr`, `lahc_rr_kempe`) reports the canonical scorer's value by construction. `solver/solver-py/python/klassenzeit_solver/cpsat.py:69` mirrors that for CP-SAT via the `score_solution_json` PyO3 binding (ADR 0030 / item 41).

Two facts make item 51 necessary even though parity holds today:

1. **No regression guard.** Nothing fails if a future PR reverts `solve.rs:296` to the LAHC running-slice value, or if `cpsat.py` swaps `score_solution_json` for an internal CP-SAT objective expression. The drift would surface only as a confusing BENCH_RESULTS.md column the next time someone refreshed.
2. **Internal objective drift is invisible.** LAHC's running slice is `class_gap + teacher_gap + subject_pref` (`solve.rs:291-292`); CP-SAT's model objective is `Minimize(0)` (`cpsat.py`, item 48 follow-up). Both backends *report* the canonical score post-solve but *optimise* against very different internal targets. A reviewer staring at `cpsat 349 vs lahc 90` on `BENCH_RESULTS.md` reads "CP-SAT lost on the canonical objective" without seeing that CP-SAT was not steering toward that objective at all.

Item 51's three acceptance bullets address both gaps:

1. Each backend has a test or harness assertion that its reported objective matches the canonical report on returned placements.
2. `BENCH_RESULTS.md` shows the same component columns for all backends.
3. Production-default ADRs choose from component vectors, not just `Soft score`.

Bullet 2 is already structurally true (every cell renders the same nine `QualityReport` median fields per `quality_report(...)`); the work for bullet 2 is making *internal-objective drift* legible above the table. Bullet 1 needs a new debug-assert and a new pytest. Bullet 3 needs a one-line solver/CLAUDE.md rule + an OPEN_THINGS item 47 cross-reference.

Anchor items: `docs/superpowers/OPEN_THINGS.md` items 51, 50 (lineage), 52, 48, 54, 55, 47.
Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- New `solver/solver-core/src/quality.rs` additions:
  - `pub enum QualityComponent { ClassGap, TeacherGap, ClassDayBalance, HomeRoom, PreferEarly, AvoidFirst, AvoidLast, PreferLate }`. Eight variants, one per soft-objective axis. `Default` is not derived (no zero variant). `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]`. `Hard violations` and `Unplaced hours` are intentionally excluded: they are pruned during search, not optimised.
  - `pub struct BackendObjective { pub name: &'static str, pub optimised: BTreeSet<QualityComponent>, pub declared_skipped: BTreeSet<QualityComponent>, pub notes: &'static str }`. `'static` so the lookup table is `const`-friendly. `BTreeSet` so iteration order is deterministic across renderings.
  - `pub fn backend_objective(name: &str) -> Option<&'static BackendObjective>`. Static table populated for `lahc`, `lahc_rr`, `lahc_rr_kempe`, `cpsat`. Behind a `OnceLock<HashMap<&'static str, BackendObjective>>` (or `phf` if pulled into the workspace later) initialised at first call. Adding a new backend (Timefold, etc.) requires registering it here, otherwise the bench renders `(unknown)` and a unit test fails.
- Initial `BackendObjective` table (reflects today's reality, not aspirational):
  - `lahc` / `lahc_rr` / `lahc_rr_kempe`: `optimised = {ClassGap, TeacherGap, PreferEarly, AvoidFirst, AvoidLast, PreferLate}`, `declared_skipped = {HomeRoom, ClassDayBalance}`, notes string references item 52 (LAHC slice widens) and item 54 (class-day-balance during search).
  - `cpsat`: `optimised = {}` (today CP-SAT minimises `0`), `declared_skipped = {ClassGap, TeacherGap, ClassDayBalance, HomeRoom, PreferEarly, AvoidFirst, AvoidLast, PreferLate}`, notes string references item 48.
- Re-export from `solver-core/src/lib.rs`: `pub use quality::{BackendObjective, QualityComponent, backend_objective};`.
- Unit tests in `solver/solver-core/src/quality.rs::tests`:
  - `backend_objective_returns_some_for_every_known_backend` (string match on each of the four names).
  - `backend_objective_returns_none_for_unknown_name`.
  - `backend_objective_optimised_and_declared_skipped_partition_quality_components_for_lahc_family` (asserts the union covers every `QualityComponent` variant; intersection is empty).
  - `backend_objective_optimised_and_declared_skipped_partition_quality_components_for_cpsat`.
- Parity assertion in `solver/solver-core/src/solve.rs::solve_with_config_stats`:

  ```rust
  // After: solution.soft_score = score_solution(problem, &solution.placements, &config.weights);
  debug_assert_eq!(
      solution.soft_score,
      score::score_solution(problem, &solution.placements, &config.weights),
      "Solution.soft_score must equal score_solution(problem, placements, weights) for every backend; \
       see docs/superpowers/specs/2026-05-07-item-51-backend-objective-parity-design.md",
  );
  ```

  The assert is a no-op in release builds, identical to the existing `validate_no_double_booking` debug-only pattern.
- Property-test addition in `solver/solver-core/tests/lahc_property.rs`: a fresh test that runs `solve_with_config_stats` against a generated problem under random seed and asserts `solution.soft_score == score::score_solution(problem, &solution.placements, &config.weights)` on the returned placements. The debug-assert already enforces this internally; the property test names the contract for grep-discoverability.
- New Python regression test in `solver/solver-py/tests/test_cpsat.py`:

  ```python
  def test_solve_cpsat_json_reported_soft_score_equals_canonical_score():
      problem_json = json.dumps(<small fixture: 1 class, 4 lessons, 5 TBs>)
      out = json.loads(solve_cpsat_json(problem_json, deadline_ms=2000, seed=0))
      canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
      assert out["soft_score"] == canonical
  ```

  Tautological today (CP-SAT computes `soft_score` via `score_solution_json`); regression guard against any future swap.
- New "Backend objectives" section above the table in `solver/solver-core/benches/BENCH_RESULTS.md`:

  ```markdown
  ## Backend objectives

  Each backend's *internal* acceptance criterion or model objective optimises
  the listed canonical components. Components in `declared_skipped` are not
  part of the backend's own search loop today; they are still recomputed
  post-solve by `quality_report(...)` and contribute to the `Soft score`
  column, so a backend can score badly on a skipped axis without that being
  a bug. Items 48, 52, 54 move skipped components into `optimised`.

  | Backend       | Optimised                                                                | Declared skipped                  | Notes                                                                  |
  | ---           | ---                                                                       | ---                                | ---                                                                    |
  | lahc          | class_gap, teacher_gap, prefer_early, avoid_first, avoid_last, prefer_late| home_room, class_day_balance      | LAHC slice excludes home_room and day-balance; item 52 widens it.      |
  | lahc_rr       | (same as lahc)                                                            | (same as lahc)                    | Inherits LAHC's slice; R&R recreate uses scarcity ordering, not soft delta. |
  | lahc_rr_kempe | (same as lahc)                                                            | (same as lahc)                    | (same as lahc)                                                          |
  | cpsat         | (none)                                                                    | class_gap, teacher_gap, class_day_balance, home_room, prefer_early, avoid_first, avoid_last, prefer_late | Today minimises `0`; item 48 ports the canonical objective. |
  ```

  Rendered by `solver-bench/src/main.rs` via a new `write_backend_objectives_section(out, &["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"])` helper that calls `solver_core::backend_objective(name)` per backend. Hand-edit the existing file in commit 4 so the markdown is consistent before the next `mise run bench:bakeoff`.
- Lockstep tests for the bench rendering:
  - Inline `write_backend_objectives_section_renders_all_four_backends` in `solver/solver-bench/src/main.rs::tests`.
  - Extension to `solver/solver-bench/tests/end_to_end.rs::supervisor_emits_observability_and_quality_columns` asserting the rendered markdown contains `## Backend objectives` and a row per known backend.
- Documentation:
  - `solver/CLAUDE.md` bench-workflow addendum: "Production-default ADRs reason from per-component vectors. ..." (one bullet, references ADR 0031 / 0032 and the planned ADR 0035 under OPEN_THINGS item 47).
  - `docs/superpowers/OPEN_THINGS.md` item 47: append a sentence cross-referencing the new rule.
  - `docs/superpowers/OPEN_THINGS.md` item 51: deleted on merge (per the Active sprint program's archive convention; the rationale lives in PR description and `git log`).

**Out of scope.**

- LAHC slice changes (item 52).
- CP-SAT internal objective port (item 48).
- Class-day-balance during search (item 54).
- Timefold spike (item 55).
- Any change to `Solution`, `SolveStats`, the FFI wire format, or Pydantic / Zod schemas.
- ADR template edits (intentionally rejected during brainstorming Q5 as too generic).
- A new ADR (intentionally rejected; the rule is solver/CLAUDE.md plus an OPEN_THINGS cross-reference).
- Regenerating `BENCH_RESULTS.md` via `mise run bench:bakeoff` (5 h cost; the Backend objectives section is static and hand-edited; the next refresh reproduces identical content).

## Architecture

### Static lookup, not per-call data

`BackendObjective` describes *backend identity*, not *per-call result*. Two LAHC runs at different seeds produce different placements but the same set of optimised components. The data lives behind `solver_core::backend_objective(name) -> Option<&'static BackendObjective>` so:

- The bench reads it once per cell at render time.
- The Python CP-SAT side never sees it (no FFI exposure needed; bench knows by `--backend cpsat` string).
- Adding a new backend = registering a `BackendObjective` in solver-core; nothing on the Python side or the Pydantic schema changes.

### Parity assertion shape

`solve_with_config_stats` is the single end of every Rust solve path: `solve()`, `solve_json_with_config(_, deadline_ms)`, the bench, integration tests, and property tests all reach the same function. Adding `debug_assert_eq!` here pins parity for every Rust caller in dev / test builds without paying any cost in release.

For CP-SAT, the equivalent end-of-pipeline is `cpsat.py::solve_cpsat_json`. The pytest in `test_cpsat.py` exercises that function on a small fixture; the assertion `out["soft_score"] == score_solution_json(...)` is run *outside* the function (vs. an `assert` *inside* `cpsat.py`, which would be tautological since the line above just computed `soft_score` from `score_solution_json`).

### Bench rendering

`solver/solver-bench/src/main.rs` writes the bake-off markdown. Today its row writer renders one row per `(fixture, backend, seed)` cell with 17 columns; the column structure is uniform across backends by construction (see `aggregate_component_medians` and `write_row`). Adding a "Backend objectives" section above the table is a new top-level write; the table itself is unchanged. The section iterates a `&[&str]` slice of bench-known backend names and calls `solver_core::backend_objective(name)` for each; `(unknown)` rendered for any name without a registered declaration causes the inline test to fail (so the bench cannot silently ship a backend whose objective is undeclared).

### Failure modes

- The `debug_assert_eq!` fires only on real drift between `solve.rs:296` and the persisted `solution.soft_score`. With current code, both sides compute the same expression; the assert is identically true.
- The Python pytest fires only on real drift between `cpsat.py`'s post-solve scorer and the canonical scorer.
- A new backend registered without a `BackendObjective` entry causes both an inline solver-core unit test (`backend_objective_returns_some_for_every_known_backend`) and the bench's `write_backend_objectives_section_renders_all_four_backends` test to fail.

### Determinism

The static lookup uses a `BTreeMap<&'static str, BackendObjective>` (or a sorted match arm) so iteration is byte-stable. `BTreeSet<QualityComponent>` preserves enum order via `PartialOrd` / `Ord` derives. The rendered markdown is byte-identical across runs.

## Testing strategy

- Unit (solver-core/src/quality.rs::tests):
  - `backend_objective_returns_some_for_every_known_backend` (each of `lahc`, `lahc_rr`, `lahc_rr_kempe`, `cpsat`).
  - `backend_objective_returns_none_for_unknown_name`.
  - `backend_objective_lahc_family_partitions_quality_components` (union covers all variants, intersection empty).
  - `backend_objective_cpsat_partitions_quality_components`.
- Integration / property (solver-core/tests/):
  - Property test `solution_soft_score_equals_score_solution_post_solve` over generated problems / random seeds. Asserts the equality the debug-assert pins. Lives in `tests/lahc_property.rs` to share the existing `prop_compose!` `lahc_small_problem` generator.
  - The existing `quality_property::quality_report_weighted_score_matches_score_solution` (item 50) continues to pin the cross-axis sum.
- Python (solver/solver-py/tests/test_cpsat.py):
  - `test_solve_cpsat_json_reported_soft_score_equals_canonical_score` on a 1-class / 4-lesson / 5-TB fixture, deadline 2000 ms, seed 0.
- Bench (solver/solver-bench/src/main.rs::tests + tests/end_to_end.rs):
  - `write_backend_objectives_section_renders_all_four_backends` (inline; checks markdown structure for each name).
  - `supervisor_emits_observability_and_quality_columns` extension: assert `## Backend objectives` is present in the rendered file and that each known backend appears as a row.

All tests run under `mise run test` (workspace-wide). No new mise task required.

## Risks

- **`debug_assert_eq!` could trip on master if the equality is somehow not actually true today.** Reading `solve.rs:296` and `cpsat.py:69` shows it is identically true by construction. If the assert does fire, that is a pre-existing correctness bug surfaced as a side benefit and is fixed in the same PR (likely a one-line fix or a test fixture issue, given the scope).
- **`BENCH_RESULTS.md` hand-edit drift.** A maintainer regenerating the file via `mise run bench:bakeoff` after this PR must produce the same `## Backend objectives` section. The regenerator's `write_backend_objectives_section` produces the exact markdown we hand-edited (same input table, same render code), so a byte diff between the hand-edited file and a future regenerated file is `0`.
- **Test wall-clock.** The new property test runs `solve_with_config_stats` once per case; default proptest cases is 32 (bench-size fixtures). The Rust LAHC small-problem generator already produces sub-second cases, so total test cost stays under 5 s.
- **CP-SAT pytest determinism.** `solve_cpsat_json` with `seed=0` and a tiny fixture is deterministic per ADR 0030. A 2000 ms deadline gives plenty of headroom; the test should never flake.

## Mapping back to OPEN_THINGS acceptance

| Acceptance bullet | Deliverable |
| --- | --- |
| "test or harness assertion that its reported objective matches the canonical report on returned placements" | `debug_assert_eq!` in `solve_with_config_stats` (Rust LAHC family); property test in `tests/lahc_property.rs`; `test_solve_cpsat_json_reported_soft_score_equals_canonical_score` (CP-SAT). |
| "`BENCH_RESULTS.md` shows the same component columns for all backends" | Already true; new "Backend objectives" section above the table makes internal-objective drift legible. |
| "production-default ADRs choose from component vectors, not just `Soft score`" | solver/CLAUDE.md bench-workflow rule addendum; OPEN_THINGS item 47 cross-references the rule. |
