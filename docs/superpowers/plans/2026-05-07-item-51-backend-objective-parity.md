# Backend objective parity (item 51) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pin the contract that every backend's `Solution.soft_score` equals `score_solution(problem, placements, weights)` on its returned placements, and make each backend's *internal* objective drift from that canonical scorer visible in `BENCH_RESULTS.md`.

**Architecture:** Add a static `BackendObjective` lookup in `solver-core::quality` declaring per-backend optimised / declared-skipped components against a new `QualityComponent` enum. Pin parity at the tail of `solve_with_config_stats` with a `debug_assert_eq!`, mirror the assertion as a Python pytest for CP-SAT, and render a "Backend objectives" section above the bake-off table sourced from the same lookup.

**Tech Stack:** Rust 2021 (`solver-core`, `solver-bench`), PyO3 bindings (`solver-py`), pytest (`solver/solver-py/tests`), `mise`-driven test runners (`mise run test:rust`, `mise run test:py`, `mise run lint`).

Spec: `docs/superpowers/specs/2026-05-07-item-51-backend-objective-parity-design.md`.
Brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

---

## File Structure

**Files created or modified:**

- `solver/solver-core/src/quality.rs` — modified. Add `QualityComponent` enum, `BackendObjective` struct, `backend_objective(name)` lookup, plus four new unit tests.
- `solver/solver-core/src/lib.rs` — modified. Re-export `BackendObjective`, `QualityComponent`, `backend_objective`.
- `solver/solver-core/src/solve.rs` — modified at one line near `solution.soft_score = score::score_solution(...)`. Add `debug_assert_eq!`.
- `solver/solver-core/tests/lahc_property.rs` — modified. Add property test naming the parity contract for grep-discoverability.
- `solver/solver-py/tests/test_cpsat.py` — modified. Add `test_solve_cpsat_json_reported_soft_score_equals_canonical_score`.
- `solver/solver-bench/src/main.rs` — modified. Add `write_backend_objectives_section`, call it between `write_header` and the per-cell render loop, plus an inline unit test.
- `solver/solver-bench/tests/end_to_end.rs` — modified. Extend `supervisor_emits_observability_and_quality_columns` to assert the new section is rendered.
- `solver/solver-core/benches/BENCH_RESULTS.md` — hand-edited once. Insert the static "Backend objectives" section so the file is consistent before the next `mise run bench:bakeoff` regenerates it byte-identically.
- `solver/CLAUDE.md` — modified. Add one bullet to the "Bench workflow" section: production-default ADRs reason from per-component vectors.
- `docs/superpowers/OPEN_THINGS.md` — modified. Append cross-reference to item 47; delete item 51 entry on merge.

Each task below is one self-contained commit. Conventional Commits scope is the crate directory (per `solver/CLAUDE.md`).

---

## Task 1: `QualityComponent` enum + `BackendObjective` lookup table in `solver-core::quality`

**Files:**
- Modify: `solver/solver-core/src/quality.rs`
- Modify: `solver/solver-core/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `solver/solver-core/src/quality.rs`

- [ ] **Step 1: Write the failing tests**

Append to `solver/solver-core/src/quality.rs::tests`:

```rust
#[test]
fn backend_objective_returns_some_for_every_known_backend() {
    for name in ["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"] {
        assert!(
            backend_objective(name).is_some(),
            "backend_objective({name:?}) should return Some; the bench enumerates this name",
        );
    }
}

#[test]
fn backend_objective_returns_none_for_unknown_name() {
    assert!(backend_objective("timefold").is_none());
    assert!(backend_objective("").is_none());
}

#[test]
fn backend_objective_lahc_family_partitions_quality_components() {
    use std::collections::BTreeSet;
    let all: BTreeSet<QualityComponent> = QualityComponent::ALL.iter().copied().collect();
    for name in ["lahc", "lahc_rr", "lahc_rr_kempe"] {
        let bo = backend_objective(name).expect("registered");
        let union: BTreeSet<QualityComponent> =
            bo.optimised.union(&bo.declared_skipped).copied().collect();
        assert_eq!(
            union, all,
            "{name}: optimised ∪ declared_skipped must cover every QualityComponent",
        );
        let intersection: BTreeSet<QualityComponent> =
            bo.optimised.intersection(&bo.declared_skipped).copied().collect();
        assert!(
            intersection.is_empty(),
            "{name}: optimised ∩ declared_skipped must be empty (component cannot be both)",
        );
    }
}

#[test]
fn backend_objective_cpsat_partitions_quality_components() {
    use std::collections::BTreeSet;
    let all: BTreeSet<QualityComponent> = QualityComponent::ALL.iter().copied().collect();
    let bo = backend_objective("cpsat").expect("registered");
    let union: BTreeSet<QualityComponent> =
        bo.optimised.union(&bo.declared_skipped).copied().collect();
    assert_eq!(union, all);
    let intersection: BTreeSet<QualityComponent> =
        bo.optimised.intersection(&bo.declared_skipped).copied().collect();
    assert!(intersection.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p solver-core quality::tests::backend_objective`
Expected: FAIL (`cannot find function backend_objective`, `cannot find type QualityComponent`).

- [ ] **Step 3: Implement the enum, struct, and lookup**

Add to `solver/solver-core/src/quality.rs` (anywhere above `tests`; canonical placement: just below the `QualityReport` block):

```rust
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// One canonical soft-objective axis. Every backend declares which of these
/// it optimises in its internal acceptance criterion or model objective via
/// [`BackendObjective`]. `HardViolations` and `UnplacedHours` from
/// [`QualityReport`] are intentionally excluded: they are pruned during
/// search rather than optimised, so it is meaningless to ask which backend
/// "optimises" them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QualityComponent {
    /// `weights.class_gap`-weighted axis from `score::class_gap_cost`.
    ClassGap,
    /// `weights.teacher_gap`-weighted axis from `score::teacher_gap_cost`.
    TeacherGap,
    /// `weights.class_day_balance`-weighted axis from `score::class_day_balance_cost`.
    ClassDayBalance,
    /// `weights.prefer_home_room`-weighted axis from `score::home_room_penalty`.
    HomeRoom,
    /// `weights.prefer_early_period`-weighted axis (per-placement
    /// `subject.prefer_early_period * tb.position`).
    PreferEarly,
    /// `weights.avoid_first_period`-weighted axis at `tb.position == 0`.
    AvoidFirst,
    /// `weights.avoid_last_period`-weighted axis at the last position of the day.
    AvoidLast,
    /// `weights.prefer_late_period`-weighted axis from
    /// `subject.prefer_late_period * (max_position_for_day - tb.position)`.
    PreferLate,
}

impl QualityComponent {
    /// Every variant, in [`PartialOrd`] order. Used by tests and by the
    /// bench renderer to enumerate components deterministically.
    pub const ALL: [QualityComponent; 8] = [
        QualityComponent::ClassGap,
        QualityComponent::TeacherGap,
        QualityComponent::ClassDayBalance,
        QualityComponent::HomeRoom,
        QualityComponent::PreferEarly,
        QualityComponent::AvoidFirst,
        QualityComponent::AvoidLast,
        QualityComponent::PreferLate,
    ];

    /// Lower-snake_case label suitable for markdown rendering and as a
    /// stable identifier in tests.
    pub fn label(self) -> &'static str {
        match self {
            QualityComponent::ClassGap => "class_gap",
            QualityComponent::TeacherGap => "teacher_gap",
            QualityComponent::ClassDayBalance => "class_day_balance",
            QualityComponent::HomeRoom => "home_room",
            QualityComponent::PreferEarly => "prefer_early",
            QualityComponent::AvoidFirst => "avoid_first",
            QualityComponent::AvoidLast => "avoid_last",
            QualityComponent::PreferLate => "prefer_late",
        }
    }
}

/// Per-backend objective declaration. Describes which canonical
/// [`QualityComponent`]s the backend's *internal* search loop or model
/// objective optimises today, plus the components it explicitly does not
/// optimise. The bench renders this above the bake-off table so reviewers
/// see internal-objective drift instead of staring at a collapsed `Soft
/// score` column.
///
/// The declarations describe today's reality, not the desired end state.
/// As items 48 / 52 / 54 land, each one moves entries from `declared_skipped`
/// into `optimised` in its own commit.
#[derive(Debug)]
pub struct BackendObjective {
    /// Backend identifier matching `solver-bench`'s `--backend` argument
    /// (`"lahc"`, `"lahc_rr"`, `"lahc_rr_kempe"`, `"cpsat"`).
    pub name: &'static str,
    /// Canonical components this backend's internal acceptance criterion
    /// (LAHC) or model objective (CP-SAT) actually steers toward.
    pub optimised: BTreeSet<QualityComponent>,
    /// Canonical components this backend explicitly does not include in
    /// its internal objective today. Their value is still recomputed
    /// post-solve by `quality_report(...)` and contributes to
    /// `Solution.soft_score`, so a backend can score badly on a skipped
    /// axis without that being a bug.
    pub declared_skipped: BTreeSet<QualityComponent>,
    /// One-sentence rationale tying each declaration back to the OPEN_THINGS
    /// item that closes the gap (item 48 for cpsat; item 52 for LAHC's slice).
    pub notes: &'static str,
}

/// Looks up the [`BackendObjective`] for a registered backend. Returns
/// `None` for unknown names; bench callers treat that as a registration bug.
pub fn backend_objective(name: &str) -> Option<&'static BackendObjective> {
    BACKEND_OBJECTIVES
        .get_or_init(build_backend_objectives)
        .iter()
        .find(|bo| bo.name == name)
}

static BACKEND_OBJECTIVES: OnceLock<Vec<BackendObjective>> = OnceLock::new();

fn build_backend_objectives() -> Vec<BackendObjective> {
    use QualityComponent::*;
    let lahc_optimised: BTreeSet<QualityComponent> =
        [ClassGap, TeacherGap, PreferEarly, AvoidFirst, AvoidLast, PreferLate]
            .into_iter()
            .collect();
    let lahc_skipped: BTreeSet<QualityComponent> =
        [HomeRoom, ClassDayBalance].into_iter().collect();
    let lahc_notes = "LAHC slice is class_gap + teacher_gap + subject_pref \
                      (see solve.rs:291-292); item 52 widens it; item 54 \
                      adds class-day-balance to the search hot path.";
    let cpsat_optimised: BTreeSet<QualityComponent> = BTreeSet::new();
    let cpsat_skipped: BTreeSet<QualityComponent> =
        QualityComponent::ALL.iter().copied().collect();
    let cpsat_notes = "Today minimises 0 (cpsat.py); item 48 ports the \
                       canonical objective into the CP-SAT model.";
    vec![
        BackendObjective {
            name: "lahc",
            optimised: lahc_optimised.clone(),
            declared_skipped: lahc_skipped.clone(),
            notes: lahc_notes,
        },
        BackendObjective {
            name: "lahc_rr",
            optimised: lahc_optimised.clone(),
            declared_skipped: lahc_skipped.clone(),
            notes: "Inherits LAHC's slice; R&R recreate ranks by soft delta \
                    after item 49.",
        },
        BackendObjective {
            name: "lahc_rr_kempe",
            optimised: lahc_optimised,
            declared_skipped: lahc_skipped,
            notes: lahc_notes,
        },
        BackendObjective {
            name: "cpsat",
            optimised: cpsat_optimised,
            declared_skipped: cpsat_skipped,
            notes: cpsat_notes,
        },
    ]
}
```

Then re-export from `solver/solver-core/src/lib.rs`:

```rust
pub use quality::{
    backend_objective, quality_report, BackendObjective, QualityComponent, QualityReport,
};
```

(Append `BackendObjective`, `QualityComponent`, `backend_objective` to whatever the existing `pub use quality::{...}` line says. The existing line today is `pub use quality::{quality_report, QualityReport};` — replace it with the line above.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p solver-core quality::tests::backend_objective`
Expected: PASS (4 tests).

Then run the full crate:

Run: `cargo nextest run -p solver-core`
Expected: PASS (no other test should regress).

- [ ] **Step 5: Commit**

```bash
git add solver/solver-core/src/quality.rs solver/solver-core/src/lib.rs
mise exec -- git commit -m "feat(solver-core): QualityComponent enum + BackendObjective lookup table"
```

---

## Task 2: `debug_assert_eq!` parity guard at end of `solve_with_config_stats` + property test

**Files:**
- Modify: `solver/solver-core/src/solve.rs:294-298`
- Modify: `solver/solver-core/tests/lahc_property.rs`

- [ ] **Step 1: Write the failing property test**

Append to `solver/solver-core/tests/lahc_property.rs` (look for the existing `proptest! { ... }` block over `lahc_small_problem`; add the new property inside the same block):

```rust
proptest! {
    /// Item 51 acceptance #1: every backend's reported `Solution.soft_score`
    /// must equal `score_solution(problem, placements, weights)` on its
    /// returned placements. This is the property-test form of the
    /// `debug_assert_eq!` at the tail of `solve_with_config_stats`; named
    /// for grep-discoverability.
    #[test]
    fn solve_with_config_stats_solution_soft_score_equals_score_solution(
        problem in lahc_small_problem(),
        seed in any::<u64>(),
    ) {
        let config = SolveConfig {
            seed,
            max_iterations: Some(64),
            deadline: None,
            ..SolveConfig::default()
        };
        let (solution, _stats) = solve_with_config_stats(&problem, &config);
        let canonical = score_solution(&problem, &solution.placements, &config.weights);
        prop_assert_eq!(solution.soft_score, canonical);
    }
}
```

If `score_solution` is not already imported at the top of `lahc_property.rs`, add: `use solver_core::score_solution;` (the function is re-exported; verify with `grep "pub use" solver/solver-core/src/lib.rs`). If `SolveConfig` lacks `Default` for the `..` syntax, replace `..SolveConfig::default()` with explicit fields, copying from a sibling test in the same file.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p solver-core --test lahc_property solve_with_config_stats_solution_soft_score_equals_score_solution`
Expected: FAIL (test does not yet exist on disk if you wrote it as a new file; or PASS if `solve.rs:296` already enforces the equality, which it does today — see below).

If the test PASSES on the first run, that is fine. The test exists as a regression guard; it does not need to fail on master. Move directly to step 3.

- [ ] **Step 3: Add the `debug_assert_eq!` in `solve_with_config_stats`**

Open `solver/solver-core/src/solve.rs` and find the line:

```rust
solution.soft_score =
    crate::score::score_solution(problem, &solution.placements, &config.weights);
```

(Today this is around line 296 inside `solve_with_config_stats`.) Append immediately after, before the function returns:

```rust
debug_assert_eq!(
    solution.soft_score,
    crate::score::score_solution(problem, &solution.placements, &config.weights),
    "Solution.soft_score must equal score_solution(problem, placements, weights) for every backend; \
     see docs/superpowers/specs/2026-05-07-item-51-backend-objective-parity-design.md (item 51)",
);
```

- [ ] **Step 4: Run the property test and the full solver-core suite**

Run: `cargo nextest run -p solver-core --test lahc_property`
Expected: PASS.

Run: `cargo nextest run -p solver-core`
Expected: PASS (the debug-assert is no-op on equality and equality holds today).

- [ ] **Step 5: Commit**

```bash
git add solver/solver-core/src/solve.rs solver/solver-core/tests/lahc_property.rs
mise exec -- git commit -m "test(solver-core): pin Solution.soft_score == score_solution post-solve"
```

---

## Task 3: CP-SAT regression test in `solver-py`

**Files:**
- Modify: `solver/solver-py/tests/test_cpsat.py`

- [ ] **Step 1: Write the failing test**

Append to `solver/solver-py/tests/test_cpsat.py` (after the existing tests; reuse the small-fixture helpers `_cpsat_uuid`, `_cpsat_trivial_one_lesson_problem`, `_cpsat_doppelstunde_problem` already defined in the file):

```python
def test_solve_cpsat_json_reported_soft_score_equals_canonical_score():
    """Item 51 acceptance #1 (CP-SAT): the reported `soft_score` on a
    returned solution must equal `score_solution_json(problem, placements)`.

    Tautological today (cpsat.py computes `soft_score` via
    `score_solution_json`), but the test is a regression guard against any
    future swap of the post-solve scorer for an internal CP-SAT objective
    expression. Item 48 ports the canonical objective into the model itself;
    even after that lands, this assertion still holds because the *reported*
    score on the returned placements is a function of the placements alone.
    """
    from klassenzeit_solver import score_solution_json

    problem_json = _cpsat_doppelstunde_problem()
    out_json = solve_cpsat_json(problem_json, deadline_ms=2000, seed=0)
    out = json.loads(out_json)
    canonical = score_solution_json(problem_json, json.dumps(out["placements"]))
    assert out["soft_score"] == canonical
```

If `score_solution_json` is not exported by `klassenzeit_solver/__init__.py`, import it from `klassenzeit_solver._rust import score_solution_json` (the lower-level binding mentioned in `solver/CLAUDE.md`).

- [ ] **Step 2: Run test to verify it fails**

Run: `mise run test:py -- solver/solver-py/tests/test_cpsat.py::test_solve_cpsat_json_reported_soft_score_equals_canonical_score -v`
Expected: FAIL with "function not defined" (test does not yet exist) or PASS if the regression guard happens to be satisfied today (which it is). Either way, proceed to step 3.

- [ ] **Step 3: No implementation needed**

The test is a regression guard against a swap that has not happened. No code change is required beyond the test itself.

- [ ] **Step 4: Run the test to verify it passes**

Run: `mise run test:py -- solver/solver-py/tests/test_cpsat.py::test_solve_cpsat_json_reported_soft_score_equals_canonical_score -v`
Expected: PASS.

Then run the full `test_cpsat.py` and the binding suite:

Run: `mise run test:py -- solver/solver-py/tests/test_cpsat.py -v`
Expected: PASS (existing tests should not regress).

- [ ] **Step 5: Commit**

```bash
git add solver/solver-py/tests/test_cpsat.py
mise exec -- git commit -m "test(solver-py): cpsat reports canonical score on returned placements"
```

---

## Task 4: "Backend objectives" section above the bake-off table

**Files:**
- Modify: `solver/solver-bench/src/main.rs`
- Modify: `solver/solver-bench/tests/end_to_end.rs`
- Modify: `solver/solver-core/benches/BENCH_RESULTS.md`

- [ ] **Step 1: Write the failing inline test**

Append to `solver/solver-bench/src/main.rs::tests` (alongside `write_header_includes_three_new_columns` and `write_row_renders_quality_columns`):

```rust
#[test]
fn write_backend_objectives_section_renders_all_four_backends() {
    let mut out = String::new();
    write_backend_objectives_section(&mut out, &BenchBackend::ALL);
    assert!(
        out.contains("## Backend objectives"),
        "section header missing: {out}",
    );
    for backend in BenchBackend::ALL {
        assert!(
            out.contains(backend.label()),
            "backend {} missing from rendered section: {out}",
            backend.label(),
        );
    }
    assert!(
        out.contains("Optimised") && out.contains("Declared skipped"),
        "objectives table missing column headers: {out}",
    );
    assert!(
        out.contains("class_gap, teacher_gap, prefer_early, avoid_first, avoid_last, prefer_late"),
        "lahc family optimised set rendered incorrectly: {out}",
    );
    assert!(
        out.contains("(none)"),
        "cpsat optimised set should render as (none) today: {out}",
    );
}
```

Also append to `solver/solver-bench/tests/end_to_end.rs::supervisor_emits_observability_and_quality_columns` (after the existing `Late-period ratio (median)` assertion):

```rust
    assert!(
        body.contains("## Backend objectives"),
        "missing Backend objectives section: {body}",
    );
    assert!(
        body.contains("lahc_rr_kempe"),
        "missing lahc_rr_kempe row in objectives section: {body}",
    );
    assert!(
        body.contains("class_gap, teacher_gap, prefer_early, avoid_first, avoid_last, prefer_late"),
        "missing lahc-family optimised set in objectives section: {body}",
    );
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p solver-bench`
Expected: FAIL (`write_backend_objectives_section` does not exist; the integration test fails because the section is missing from rendered markdown).

- [ ] **Step 3: Implement `write_backend_objectives_section`**

Add to `solver/solver-bench/src/main.rs` (just above `write_header`):

```rust
/// Render the static "Backend objectives" section that lives above the
/// bake-off table. Sourced from `solver_core::backend_objective(name)`;
/// renders one row per known backend showing optimised / declared-skipped
/// canonical components plus a one-sentence rationale. Item 51.
fn write_backend_objectives_section(out: &mut String, backends: &[BenchBackend]) {
    out.push_str("## Backend objectives\n\n");
    out.push_str(
        "Each backend's *internal* acceptance criterion or model objective optimises \
         the listed canonical components. Components in `declared_skipped` are not \
         part of the backend's own search loop today; they are still recomputed \
         post-solve by `quality_report(...)` and contribute to the `Soft score` \
         column, so a backend can score badly on a skipped axis without that being \
         a bug. Items 48, 52, 54 move skipped components into `optimised`.\n\n",
    );
    out.push_str("| Backend | Optimised | Declared skipped | Notes |\n");
    out.push_str("| --- | --- | --- | --- |\n");
    for backend in backends {
        let label = backend.label();
        let bo = solver_core::backend_objective(label)
            .unwrap_or_else(|| panic!("backend_objective({label:?}) must be registered"));
        let render_set = |s: &std::collections::BTreeSet<solver_core::QualityComponent>| {
            if s.is_empty() {
                "(none)".to_string()
            } else {
                s.iter()
                    .map(|c| c.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            label,
            render_set(&bo.optimised),
            render_set(&bo.declared_skipped),
            bo.notes,
        ));
    }
    out.push('\n');
}
```

Wire the call site: inside `run_supervisor` (around the existing `write_header(&mut markdown);` call), insert immediately after `write_header`:

```rust
    write_backend_objectives_section(&mut markdown, &BenchBackend::ALL);
```

- [ ] **Step 4: Run the bench tests**

Run: `cargo nextest run -p solver-bench --bin solver-bench`
Expected: PASS (the inline unit test passes).

Run: `cargo nextest run -p solver-bench --test end_to_end`
Expected: PASS (the supervisor end-to-end test now sees the new section).

- [ ] **Step 5: Hand-edit `BENCH_RESULTS.md` so the static section is consistent before next regeneration**

Open `solver/solver-core/benches/BENCH_RESULTS.md`. Insert the rendered section between the top `# Solver bake-off feasibility bench` heading + `<!-- Regenerated by ... -->` line and the `| Fixture | Backend | ...` table header. The exact text:

```markdown
## Backend objectives

Each backend's *internal* acceptance criterion or model objective optimises the listed canonical components. Components in `declared_skipped` are not part of the backend's own search loop today; they are still recomputed post-solve by `quality_report(...)` and contribute to the `Soft score` column, so a backend can score badly on a skipped axis without that being a bug. Items 48, 52, 54 move skipped components into `optimised`.

| Backend | Optimised | Declared skipped | Notes |
| --- | --- | --- | --- |
| lahc | class_gap, teacher_gap, prefer_early, avoid_first, avoid_last, prefer_late | class_day_balance, home_room | LAHC slice is class_gap + teacher_gap + subject_pref (see solve.rs:291-292); item 52 widens it; item 54 adds class-day-balance to the search hot path. |
| lahc_rr | class_gap, teacher_gap, prefer_early, avoid_first, avoid_last, prefer_late | class_day_balance, home_room | Inherits LAHC's slice; R&R recreate ranks by soft delta after item 49. |
| lahc_rr_kempe | class_gap, teacher_gap, prefer_early, avoid_first, avoid_last, prefer_late | class_day_balance, home_room | LAHC slice is class_gap + teacher_gap + subject_pref (see solve.rs:291-292); item 52 widens it; item 54 adds class-day-balance to the search hot path. |
| cpsat | (none) | class_day_balance, class_gap, home_room, prefer_early, prefer_late, teacher_gap, avoid_first, avoid_last | Today minimises 0 (cpsat.py); item 48 ports the canonical objective into the CP-SAT model. |

```

(Note the components inside each set are rendered in `BTreeSet` order, which is the `PartialOrd` order on `QualityComponent`. The order above matches `QualityComponent::ALL`. Verify after Task 1's commit by running `cargo nextest run -p solver-bench solver_bench::tests::write_backend_objectives_section`.)

If the unit test produces a different exact ordering than what is hand-edited above, copy the unit-test output verbatim into `BENCH_RESULTS.md` (the test output is the source of truth; this plan describes intent).

- [ ] **Step 6: Run lint to catch markdown / clippy issues**

Run: `mise run lint:rust`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add solver/solver-bench/src/main.rs solver/solver-bench/tests/end_to_end.rs solver/solver-core/benches/BENCH_RESULTS.md
mise exec -- git commit -m "feat(solver-bench): render Backend objectives section in BENCH_RESULTS.md"
```

---

## Task 5: solver/CLAUDE.md rule + OPEN_THINGS cross-reference + delete item 51 entry

**Files:**
- Modify: `solver/CLAUDE.md`
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Add the bench-workflow bullet to `solver/CLAUDE.md`**

Open `solver/CLAUDE.md`. Find the "Bench workflow" section (it begins with `## Bench workflow` and includes the "Two artifacts, two purposes" bullet). Append a new bullet immediately after the "Two artifacts, two purposes" bullet:

```markdown
- **Production-default ADRs reason from per-component vectors.** ADR 0031, 0032, and any future production-default ADR (e.g., the planned ADR 0035 under OPEN_THINGS item 47) must compare backends against the per-component columns in BENCH_RESULTS.md (`class_gap_h`, `teacher_gap_h`, `home_room_miss`, `day_balance`, plus the four schedule-quality predicates), not just the `Soft score` collapsed scalar. The "Backend objectives" section above the table makes drift between an internal objective and the canonical scorer legible; an ADR that picks a backend whose internal objective declared-skips half the axes solely because its post-hoc soft score is lower is reading the wrong column. Item 51.
```

- [ ] **Step 2: Cross-reference the rule in OPEN_THINGS item 47**

Open `docs/superpowers/OPEN_THINGS.md`. Find item 47 ("ADR 0032 production-default revisit (item 42 follow-up)"). Append a sentence to the end of the bullet's text, before the line break:

```
... in one atomic commit. ADR 0035 must reason from per-component vectors per the rule landed in solver/CLAUDE.md under item 51.
```

(Locate the existing trailing fragment "in one atomic commit." and append the new clause.)

- [ ] **Step 3: Delete OPEN_THINGS item 51**

Same file. Find the item 51 bullet (line begins `51. **Make every backend optimize and report the same objective.**`). Delete the entire bullet (one paragraph). Per the autopilot rule, completed work is removed from OPEN_THINGS; PR description and `git log` are the canonical record.

- [ ] **Step 4: Verify markdown lints**

Run: `mise run lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add solver/CLAUDE.md docs/superpowers/OPEN_THINGS.md
mise exec -- git commit -m "docs(solver): production-default ADRs reason from component vectors (item 51)"
```

---

## Self-review

Spec coverage check (run before opening the PR):

- Spec section "In scope" / `QualityComponent` enum + `BackendObjective` lookup — Task 1.
- Spec section "In scope" / parity assertion in `solve_with_config_stats` — Task 2.
- Spec section "In scope" / property test in `tests/lahc_property.rs` — Task 2.
- Spec section "In scope" / Python regression test — Task 3.
- Spec section "In scope" / `write_backend_objectives_section` + lockstep tests — Task 4.
- Spec section "In scope" / hand-edit `BENCH_RESULTS.md` — Task 4 step 5.
- Spec section "In scope" / `solver/CLAUDE.md` addendum — Task 5.
- Spec section "In scope" / OPEN_THINGS item 47 cross-reference + item 51 deletion — Task 5.

No spec requirement is unmapped to a task. No `TBD`, `TODO`, `implement later`, or "appropriate error handling" placeholders. Type names (`QualityComponent`, `BackendObjective`, `backend_objective`, `write_backend_objectives_section`) are consistent across tasks.
