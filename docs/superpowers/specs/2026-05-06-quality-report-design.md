# Backend-neutral `QualityReport` and objective contract (item 50)

**Sprint program.** Solver feasibility correctness + observability (active program), follow-ups bucket (`## Open solver follow-ups`).
**Phase.** Open follow-up: item 50 (P0).
**Goal.** Replace "is `Solution.soft_score` good?" with one shared component vector that every backend's output is evaluated against. The vector lives in `solver-core` so `score_solution` is testable against it; `solver-bench` renders the load-bearing components per backend; the backend's `quality_checks.py` documents how its `QualityIssue.kind` Literal maps onto the same dimensions.

**Non-goal.** No backend objective rewiring (item 51). No LAHC acceptance switch (item 52). No CP-SAT objective port (item 48). No `BENCH_RESULTS.md` refresh in this PR (5 h cost; queued for the maintainer). No Python-side `quality_report_json` binding. No `QualityIssue.kind` Literal renames (wire-format change).

## Context

`Solution.soft_score: u32` (`solver/solver-core/src/types.rs:355-373`) is the single weighted scalar every backend reports. It collapses six cost axes (class_gap, teacher_gap, prefer_home_room, class_day_balance, plus four subject-timing sub-axes) into one number. ADR 0031 and ADR 0032 use this scalar to choose the production default; OPEN_THINGS items 47, 48, 51, 52 all flag it as the wrong granularity for the decisions that depend on it.

Two existing structures already expose partial breakdowns:

- `solver/solver-core/src/score.rs::score_solution` walks per-axis subtotals internally (class_gap, teacher_gap, subject_pref, class_day_balance, home_room) and sums them into the `u32` return. The subtotals are not exposed.
- `solver/solver-bench/src/quality.rs::QualityReport` is a four-field predicate report (`worst_spread`, `worst_home_room_ratio`, `total_interior_gaps`, `late_period_ratio`) with thresholds, used by the bake-off bench's `Quality (pass / 4)` column. Predicate-style: each field is `Some/None` and gates a pass/fail count, not a per-axis cost.
- `backend/src/klassenzeit_backend/scheduling/quality_checks.py::QualityIssue.kind` is `Literal["room_hop", "imbalance", "home_room_miss", "day_too_long", "interior_gap"]`. Soft-issue records keyed by aggregate, used by the demo Grundschule integration test.

Item 50 wants the per-axis breakdown promoted to a public solver-core type, made the single source of truth, and rendered in `solver-bench` so cross-backend comparisons see component drift instead of one collapsed number. The acceptance criteria are:

1. Rust scoring exposes the report.
2. `score_solution` is either derived from it or tested against it.
3. `solver-bench` renders the core components per backend.
4. Backend `quality_checks.py` names map to the same dimensions.

Anchor items: `docs/superpowers/OPEN_THINGS.md` items 50, 51, 52, 48 (lineage).
Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- Add `solver/solver-core/src/quality.rs` with:
  - `pub struct QualityReport` carrying eleven fields (one per cost axis component, plus `weighted_score`). `#[derive(Default, Debug, Clone, PartialEq, Eq)]`.
  - `pub fn quality_report(problem: &Problem, placements: &[Placement], violations: &[Violation], weights: &ConstraintWeights) -> QualityReport`. Walks the same partitions `score_solution` walks; populates each subtotal; returns the struct with `weighted_score = sum_of_weighted_subtotals`.
  - Module rustdoc explaining the eleven fields, the relationship to `score_solution`, and the additive-extension shape (new axis = new field; `Default` keeps existing call sites compiling).
- Wire `quality.rs` into `solver-core/src/lib.rs`: `pub mod quality;` and re-export `pub use quality::{QualityReport, quality_report};`.
- Unit tests in `solver/solver-core/src/quality.rs::tests`:
  - `quality_report_default_returns_zeros`.
  - `quality_report_hard_violations_equals_violation_count`.
  - `quality_report_unplaced_hours_equals_expected_minus_placed`.
  - `quality_report_class_gap_hours_matches_score_helper`.
  - `quality_report_teacher_gap_hours_matches_score_helper`.
  - `quality_report_class_day_balance_matches_score_helper`.
  - `quality_report_home_room_misses_counts_per_member_class`.
  - `quality_report_prefer_early_units_match_score_helper`.
  - `quality_report_avoid_first_units_match_score_helper`.
  - `quality_report_avoid_last_units_match_score_helper`.
  - `quality_report_prefer_late_units_match_score_helper`.
  - `quality_report_weighted_score_equals_score_solution_on_grundschule`.
- Property test in `solver/solver-core/tests/quality_property.rs`:
  - `quality_report_weighted_score_matches_score_solution`. Generator builds an `lahc_small_problem`-shaped fixture (reuses the generator from `tests/lahc_property.rs` if its visibility allows; otherwise duplicates the `prop_compose!` shape with a unique helper name per the global "unique function names" rule). Asserts the equality `quality_report(&p, &pls, &[], &w).weighted_score == score_solution(&p, &pls, &w)` for every shrunk input.
  - 5×128 `PROPTEST_CASES=128 PROPTEST_SEED={1..5}` sweep before commit per `solver/CLAUDE.md` discipline.
- Rename the predicate-style report in `solver/solver-bench/src/quality.rs`:
  - `pub struct QualityReport` → `pub struct QualityPredicates`.
  - `pub fn evaluate_quality(...)` → `pub fn evaluate_quality_predicates(...)`.
  - `pub fn quality_pass_count(report: &QualityReport)` → `pub fn quality_pass_count(report: &QualityPredicates)`.
  - Update every call site in `solver-bench/src/main.rs` (the `quality_reports: Vec<...>` collections, the `aggregate_quality_medians` helper, the `evaluate_quality` calls; existing tests pass post-rename without semantic change).
- Extend `solver/solver-bench/src/main.rs::CellResult` with nine new median fields. Two of the eleven `QualityReport` fields already have equivalents on `CellResult` and stay as-is:
  - existing `hard_violations_median: u32` already mirrors `QualityReport.hard_violations`.
  - existing `soft_score_median: Option<u32>` already mirrors `QualityReport.weighted_score` (since `Solution.soft_score` is set to `score_solution(...)` at the end of every `solve_with_config`, and the property test pins `quality_report.weighted_score == score_solution`).

  New fields:
  - `unplaced_hours_median: Option<u32>`.
  - `class_gap_hours_median: Option<u32>`.
  - `teacher_gap_hours_median: Option<u32>`.
  - `class_day_balance_cost_median: Option<u32>`.
  - `home_room_misses_median: Option<u32>`.
  - `prefer_early_units_median: Option<u32>`.
  - `avoid_first_units_median: Option<u32>`.
  - `avoid_last_units_median: Option<u32>`.
  - `prefer_late_units_median: Option<u32>`.
- Wire `solver_core::quality_report` into the per-cell loop in `solver-bench/src/main.rs::run_lahc_seed` and `run_cpsat_seed` (or equivalent helpers): collect a `Vec<solver_core::QualityReport>` alongside the existing `Vec<solver_bench::quality::QualityPredicates>`, derive medians per field via a new helper `aggregate_component_medians(&[QualityReport]) -> ComponentMedians`. The helper mirrors the existing `aggregate_quality_medians` shape.
- Render four new columns in `solver-bench/src/main.rs::render_markdown` (or its current name): `class_gap_h`, `teacher_gap_h`, `home_room_miss`, `day_balance`. Column placement: between the existing `Soft score` and `Quality (pass/4)` columns so the cost-driver axes sit next to the aggregate score they derive from. Existing columns and their order otherwise unchanged.
- Update the existing markdown render unit tests in `solver-bench/src/main.rs::tests` (the `format_cell_*` and `render_markdown_*` tests) to assert the new column headers and one realistic synthesized-CellResult render.
- Add a JSON-shape test `cell_result_serialises_eleven_quality_report_medians` that round-trips a fully-populated `CellResult` so the cell-child to supervisor wire stays self-consistent.
- Add a module-level docstring block to `backend/src/klassenzeit_backend/scheduling/quality_checks.py` mapping each `QualityIssue.kind` Literal value onto its `solver_core::QualityReport` field. Mapping table:

  | `QualityIssue.kind` | `QualityReport` field      | Notes                                                                                                  |
  | ------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------ |
  | `imbalance`         | `class_day_balance_cost`   | Same axis. Predicate-style threshold (`max_spread`) vs. raw-count L1 metric.                            |
  | `home_room_miss`    | `home_room_misses`         | Same axis. Predicate-style ratio threshold vs. raw mismatch count.                                     |
  | `interior_gap`      | `class_gap_hours`          | Same axis. Predicate-style per-class threshold (`max_gaps_per_class`) vs. global sum.                  |
  | `day_too_long`      | `avoid_last_units` (loose) | Closest soft component. Predicate's `max_position` is sharper than the soft `avoid_last_period` axis.   |
  | `room_hop`          | (none)                     | Hard constraint; pruned in production via `validate_no_room_hopping`. No soft component.               |

- Delete OPEN_THINGS item 50 entirely (no "Shipped" annotation; OPEN_THINGS is for OPEN items only).
- Update OPEN_THINGS items 51, 52, 48 cross-references to drop "after item 50" notes if any remain after deletion.
- Update auto-memory `project_roadmap_status.md` next-pickup pointer (rolls forward to item 51 or item 48, whichever the maintainer picks next; item 51 directly depends on item 50).
- Add a brief entry to `solver/CLAUDE.md` documenting the `QualityReport` location, its relationship to `score_solution`, and the additive-extension shape for future axes (teacher bad windows, pin disruption cost).

**Out of scope.**

- Item 51 (every backend optimises and reports the same objective). Contract is defined here; backend-side parity work is item 51's PR.
- Item 52 (LAHC accept on canonical objective). Same: needs the contract first.
- Item 48 (CP-SAT objective port). Own PR.
- `BENCH_RESULTS.md` refresh. Adding columns is cheap; populating them at production cell shape costs `mise run bench:bakeoff --budget 60s --seeds 20` ≈ 5 hours wall-clock. Maintainer runs it at refresh cadence; the columns render `None`-shaped placeholders (`-`) for now or stay populated by whatever stale numbers the existing rows carry until refresh.
- Python-side `quality_report_json` PyO3 binding. No Python consumer needs the breakdown today (`Solution.soft_score` already crosses the boundary via `score_solution_json` for cpsat). Add later if a consumer materialises.
- `QualityIssue.kind` Literal value renames. Documentation-only mapping; renames touch `Pydantic schemas`, `frontend/src/lib/api-types.ts` regen via `mise run fe:types`, and any future quality-issue endpoint consumers. Out of scope for the contract PR.
- ADR. The decision is a typed-vector promotion of an existing internal structure; ADR 0001 (workspace layout), ADR 0002 (solver-core / solver-py split), and ADR 0029 (bake-off methodology) already cover the architectural surface. A future ADR may cite this PR if item 51's backend rewiring proves load-bearing on production-default decisions.

## Deliverables

- `solver/solver-core/src/quality.rs` (new): struct + function + unit tests.
- `solver/solver-core/src/lib.rs`: `pub mod quality;` plus re-exports.
- `solver/solver-core/tests/quality_property.rs` (new): property test.
- `solver/solver-bench/src/quality.rs`: rename `QualityReport` → `QualityPredicates`.
- `solver/solver-bench/src/main.rs`: extended `CellResult`, new aggregation helper, new column rendering, updated render tests.
- `backend/src/klassenzeit_backend/scheduling/quality_checks.py`: module-level docstring with mapping table.
- `solver/CLAUDE.md`: brief entry on the `QualityReport` location and extension shape.
- `docs/superpowers/OPEN_THINGS.md`: delete item 50 entry; cross-reference cleanups.

## Test plan

**Per commit:**

1. **`feat(solver-core): add QualityReport component vector`.**
    - Twelve unit tests (one per axis + Default + the score equality on grundschule).
    - One property test (`quality_property::quality_report_weighted_score_matches_score_solution`) with a 5×128 PROPTEST_CASES sweep before commit.
    - `cargo nextest run -p solver-core` plus `mise run lint:rust`.
2. **`refactor(solver-bench): rename QualityReport → QualityPredicates`.**
    - Existing tests pass post-rename without modification (semantic-equivalent).
    - `cargo nextest run -p solver-bench` plus `mise run lint`.
3. **`feat(solver-bench): render QualityReport components per backend`.**
    - `cell_result_serialises_nine_new_quality_report_medians` (JSON round-trip with the new fields).
    - `aggregate_component_medians_returns_per_field_medians` (median helper sanity).
    - Updated existing `format_cell_*` / `render_markdown_*` tests to assert four new column headers + realistic synthesized values.
    - `cargo nextest run -p solver-bench --bin solver-bench` plus `cargo nextest run -p solver-bench --test end_to_end` (the integration test that asserts shape; updates if the column count assertion needs the new headers).
    - `mise run lint`.
4. **`docs(backend): map quality_checks.py kinds to QualityReport components`.**
    - Module is pure docstring; no test impact.
    - `mise run lint:py` is enough.

**Workspace gates (pre-push):**

- `mise run lint`.
- `mise run test:rust`.
- `mise run test:py` (greedy-only, `KZ_SOLVE_DEADLINE_MS=0`; the new docstring is read-only and does not change behaviour).
- `mise exec -- git push -u origin feat/solver-core-quality-report` (lefthook runs `cargo nextest run --workspace`, `uv run pytest`, and the frontend Vitest suite before the push).

**Bench gate:**

- This PR does not change algorithm performance. The new `quality_report` is invoked from the bench (cold path post-solve) and from tests; it is not on the LAHC hot path. `mise run bench` (criterion) is therefore not a release-blocking step. If a maintainer runs it for sanity, the four-fixture greedy + LAHC numbers should track within the 20-percent regression budget; cite results if relevant.

## Risks and mitigations

- **Field-cascade risk on `Problem` literals.** None: this PR adds a new struct in its own file; `Problem` is unchanged.
- **Property-test flakiness on the new generator.** Mitigated by the 5×128 sweep before commit. If a seed pins a counterexample, commit the `tests/quality_property.proptest-regressions` entry alongside the test in the same commit.
- **Column-count drift in `BENCH_RESULTS.md` parsers.** No external parser today; the file is read by humans + ADR follow-ups. The test suite renders synthesized cells, so column shifts surface there before they hit the maintainer.
- **`QualityPredicates` rename causing import churn.** Bench-internal only (`solver-bench/src/quality.rs` and `main.rs`). No consumer outside the bench imports `solver_bench::quality::QualityReport` today.
- **`solver/CLAUDE.md` decay.** New entry placed near the existing scoring documentation block. A future PR that adds a new axis (item 12 lands `prefer_late` on FÖ; item 51 ports the report into CP-SAT's objective) updates the same block in lockstep.

## Acceptance check (item 50)

- `pub struct QualityReport` plus `pub fn quality_report(...)` exposed from `solver_core` — yes, commit 1.
- `score_solution` derived from or tested against the report — yes, property test asserts equality (commit 1).
- `solver-bench` renders core components per backend — yes, four columns added (`class_gap_h`, `teacher_gap_h`, `home_room_miss`, `day_balance`); nine new fields serialised in `CellResult` (plus the two existing ones, `hard_violations_median` and `soft_score_median`, that already mirror their `QualityReport` counterparts) for future column promotion (commit 3).
- Backend `quality_checks.py` names map to the same dimensions — yes, module-level mapping table (commit 4).
