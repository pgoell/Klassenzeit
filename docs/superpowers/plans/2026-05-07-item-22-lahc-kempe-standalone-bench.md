# lahc_kempe Standalone Bench Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a fourth LAHC bake-off backend column, `lahc_kempe` (Kempe-without-R&R), so the marginal Kempe contribution becomes legible in `BENCH_RESULTS.md`.

**Architecture:** Two-commit, two-crate change. Commit 1 (solver-core) registers a `BackendObjective` row for the new backend so the renderer can look it up. Commit 2 (solver-bench) adds the `BenchBackend::LahcKempe` enum variant, wires `label`/`parse`/`ALL`/dispatch (`lahc_rr_period: None, lahc_kempe_period: Some(23u32)` to mirror `LahcRrKempe`'s Kempe period for clean isolation), renames an inline test whose specificity becomes outdated, and adds a substring assertion to the end_to_end smoke. No solver-core algorithm change. No `Settings.solver_backend` flip. No `BENCH_RESULTS.md` regeneration.

**Tech Stack:** Rust 1.85, `solver-core` library + `solver-bench` binary crate, `cargo nextest`, lefthook pre-commit (clippy + cargo fmt + cargo machete + check_unique_fns).

**Spec:** `docs/superpowers/specs/2026-05-07-item-22-lahc-kempe-standalone-bench-design.md`.
**Brainstorm:** `/tmp/kz-brainstorm/brainstorm.md` (this run).

---

## File Map

- **Modify** `solver/solver-core/src/quality.rs:375-414` — extend `build_backend_objectives` and the two unit tests in the inline `tests` module.
- **Modify** `solver/solver-bench/src/main.rs:38-65` — add enum variant, update `label`/`parse`/`ALL`.
- **Modify** `solver/solver-bench/src/main.rs:438-443` — extend the dispatch `match` for the new variant.
- **Modify** `solver/solver-bench/src/main.rs:1593-1622` — rename the inline test whose name pins "four backends".
- **Modify** `solver/solver-bench/tests/end_to_end.rs:18-90` — add a `body.contains("lahc_kempe")` substring assertion.

No new files. No deletions in this PR (item 22 deletion in OPEN_THINGS happens in autopilot step 6, not here).

---

## Task 1: Register `lahc_kempe` BackendObjective in solver-core

**Files:**
- Modify: `solver/solver-core/src/quality.rs:375-414` (`build_backend_objectives` body) and `solver/solver-core/src/quality.rs:655-700` (the two `backend_objective_*` lahc-family tests).

- [ ] **Step 1.1: Add `"lahc_kempe"` to the literal slice in `backend_objective_returns_some_for_every_known_backend`**

In `solver/solver-core/src/quality.rs`, replace the existing slice literal in the test:

```rust
    #[test]
    fn backend_objective_returns_some_for_every_known_backend() {
        for name in ["lahc", "lahc_rr", "lahc_kempe", "lahc_rr_kempe", "cpsat"] {
            assert!(
                backend_objective(name).is_some(),
                "backend_objective({name:?}) should return Some; the bench enumerates this name",
            );
        }
    }
```

- [ ] **Step 1.2: Add `"lahc_kempe"` to the literal slice in `backend_objective_lahc_family_partitions_quality_components`**

In `solver/solver-core/src/quality.rs`, replace the existing slice literal in the lahc-family partition test:

```rust
    #[test]
    fn backend_objective_lahc_family_partitions_quality_components() {
        // ... existing setup ...
        for name in ["lahc", "lahc_rr", "lahc_kempe", "lahc_rr_kempe"] {
            let bo = backend_objective(name).expect("registered");
            // ... existing assertions unchanged ...
        }
    }
```

(Re-read the surrounding lines first; only the slice literal changes — preserve every other line in the test.)

- [ ] **Step 1.3: Run the failing tests to confirm RED**

```bash
cargo nextest run -p solver-core backend_objective_
```

Expected: both tests fail. The first fails on `backend_objective("lahc_kempe").is_some()` returning `false` (no registration). The second fails on `backend_objective("lahc_kempe").expect("registered")` panicking.

- [ ] **Step 1.4: Add the `BackendObjective` row in `build_backend_objectives`**

In `solver/solver-core/src/quality.rs`, insert one new row in the `vec![...]` literal of `build_backend_objectives`, between the `"lahc_rr"` row and the `"lahc_rr_kempe"` row, so the table reads in the same progression as the bench's `BenchBackend::ALL` ordering will:

```rust
        BackendObjective {
            name: "lahc_kempe",
            optimised: lahc_optimised.clone(),
            declared_skipped: lahc_skipped.clone(),
            notes: lahc_notes,
        },
```

(The function declares `lahc_optimised` and `lahc_skipped` as local `BTreeSet` values and `lahc_notes` as a `&str`. Cloning matches the `"lahc"` row's style; the final `"lahc_rr_kempe"` row consumes the originals without `.clone()`.)

- [ ] **Step 1.5: Run the previously failing tests to confirm GREEN**

```bash
cargo nextest run -p solver-core backend_objective_
```

Expected: both tests pass. The full solver-core test set should also stay green:

```bash
cargo nextest run -p solver-core
```

Expected: every test passes.

- [ ] **Step 1.6: Commit**

```bash
git add solver/solver-core/src/quality.rs
git commit -m "feat(solver-core): register lahc_kempe BackendObjective (item 22)"
```

The commit-msg hook will run `cog verify`; the message satisfies `feat(scope): description` Conventional Commits shape.

---

## Task 2: Wire `BenchBackend::LahcKempe` in solver-bench

**Files:**
- Modify: `solver/solver-bench/src/main.rs:38-65` (enum + `label`/`parse`/`ALL`).
- Modify: `solver/solver-bench/src/main.rs:438-443` (dispatch match).
- Modify: `solver/solver-bench/src/main.rs:1593-1622` (test rename).
- Modify: `solver/solver-bench/tests/end_to_end.rs:18-90` (substring assertion).

- [ ] **Step 2.1: Add `body.contains("lahc_kempe")` assertion to the end_to_end smoke**

In `solver/solver-bench/tests/end_to_end.rs`, immediately after the existing `body.contains("lahc_rr_kempe")` assertion in `supervisor_emits_observability_and_quality_columns`, add a sibling check:

```rust
    assert!(
        body.contains("lahc_kempe"),
        "missing lahc_kempe row in objectives section: {body}",
    );
```

(`body.contains("lahc_kempe")` is a substring match; "lahc_rr_kempe" ALSO contains "lahc_kempe" as a suffix substring, so this check would pass spuriously even with the old four-backend `ALL`. Make the assertion specific by anchoring on the markdown table cell delimiter that the renderer emits.)

Replace the assertion with the cell-delimited form:

```rust
    assert!(
        body.contains("| lahc_kempe |"),
        "missing lahc_kempe row in objectives section (between table cell delimiters): {body}",
    );
```

(The `## Backend objectives` table at `solver-bench/src/main.rs:867` formats each row as `| {} | ... |` with single-space padding around `label`. The `lahc_rr_kempe` row's first cell renders `| lahc_rr_kempe |`, so the `| lahc_kempe |` substring won't match it.)

- [ ] **Step 2.2: Run the end_to_end smoke to confirm RED**

```bash
cargo nextest run -p solver-bench --test end_to_end
```

Expected: `supervisor_emits_observability_and_quality_columns` fails on the new assertion because `BenchBackend::ALL` doesn't include `LahcKempe` yet, so the renderer never emits a `| lahc_kempe |` row.

- [ ] **Step 2.3: Add the `BenchBackend::LahcKempe` enum variant and update `label`, `parse`, `ALL`**

In `solver/solver-bench/src/main.rs`, replace the enum + impl block at lines 38-65:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchBackend {
    Lahc,
    LahcRr,
    LahcKempe,
    LahcRrKempe,
    CpSat,
}

impl BenchBackend {
    fn label(self) -> &'static str {
        match self {
            BenchBackend::Lahc => "lahc",
            BenchBackend::LahcRr => "lahc_rr",
            BenchBackend::LahcKempe => "lahc_kempe",
            BenchBackend::LahcRrKempe => "lahc_rr_kempe",
            BenchBackend::CpSat => "cpsat",
        }
    }
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "lahc" => Ok(Self::Lahc),
            "lahc_rr" => Ok(Self::LahcRr),
            "lahc_kempe" => Ok(Self::LahcKempe),
            "lahc_rr_kempe" => Ok(Self::LahcRrKempe),
            "cpsat" => Ok(Self::CpSat),
            other => Err(format!("unknown backend '{other}'")),
        }
    }
    const ALL: [Self; 5] = [
        Self::Lahc,
        Self::LahcRr,
        Self::LahcKempe,
        Self::LahcRrKempe,
        Self::CpSat,
    ];
}
```

(`ALL` length goes from 4 to 5; the type annotation `[Self; 4]` becomes `[Self; 5]`. Variant ordering groups the two single-move variants between plain LAHC and the composed variant per the spec's Approach section.)

- [ ] **Step 2.4: Extend the dispatch match at `main.rs:438`**

In `solver/solver-bench/src/main.rs`, replace the dispatch match arm block:

```rust
    let (lahc_rr_period, lahc_kempe_period) = match backend {
        BenchBackend::Lahc => (None, None),
        BenchBackend::LahcRr => (Some(25u32), None),
        BenchBackend::LahcKempe => (None, Some(23u32)),
        BenchBackend::LahcRrKempe => (Some(25u32), Some(23u32)),
        BenchBackend::CpSat => unreachable!("cpsat dispatched above"),
    };
```

(The new arm uses `Some(23u32)` for `lahc_kempe_period` so the standalone Kempe runs at the same period the composed `LahcRrKempe` uses internally; the only difference between the two backends is "R&R off", which is the experimental contrast item 22 wants to measure. Per the brainstorm Q2.)

- [ ] **Step 2.5: Rename the inline test whose name pins "four backends"**

In `solver/solver-bench/src/main.rs:1593`, replace the test name and the assertion message:

```rust
    #[test]
    fn write_backend_objectives_section_renders_every_registered_backend() {
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
            out.contains(
                "class_gap, teacher_gap, class_day_balance, home_room, prefer_early, avoid_first, avoid_last, prefer_late"
            ),
            "lahc family optimised set rendered incorrectly (item 52: lahc accepts on full canonical): {out}",
        );
        assert!(
            out.contains("(none)"),
            "cpsat optimised set should render as (none) today: {out}",
        );
    }
```

(Loop body unchanged; only the function name updates. `scripts/check_unique_fns.py` requires the new name to be globally unique; `rg -n 'fn write_backend_objectives_section_renders_every_registered_backend'` should return zero matches before this edit.)

- [ ] **Step 2.6: Run cargo nextest on solver-bench to confirm GREEN**

```bash
cargo nextest run -p solver-bench --bin solver-bench
cargo nextest run -p solver-bench --test end_to_end
```

Expected: all tests pass. The end_to_end smoke now spawns the supervisor with `--budget 200ms --seeds 1 --fixtures grundschule`, which iterates `BenchBackend::ALL` (5 backends), and the rendered markdown body contains a `| lahc_kempe |` cell in the `## Backend objectives` table.

- [ ] **Step 2.7: Run the workspace lint sweep**

```bash
mise run lint:rust
```

Expected: green. This runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo clippy --workspace --all-targets --all-features -- -D warnings` plus `cargo machete`. Catches:
- Match-exhaustiveness regressions (every `match` over `BenchBackend` now requires the new arm).
- Format drift on the inserted lines.
- A stray `#[allow(dead_code)]` accidentally inherited from the test rename (none expected).

- [ ] **Step 2.8: Commit**

```bash
git add solver/solver-bench/src/main.rs solver/solver-bench/tests/end_to_end.rs
git commit -m "feat(solver-bench): wire lahc_kempe BenchBackend variant (item 22)"
```

---

## Task 3: Pre-merge bake-off smoke verification

**Files:** none modified — verification only.

This task confirms the new column renders end-to-end without panic at a tiny budget; it does NOT regenerate the production `BENCH_RESULTS.md` (that is item 44's deferred 5+ h refresh).

- [ ] **Step 3.1: Run `mise run solver:rebuild` to refresh the editable wheel**

```bash
mise run solver:rebuild
```

Expected: `uvx maturin develop --uv -m solver/solver-py/Cargo.toml` rebuilds the `klassenzeit_solver` wheel in seconds. Required because the smoke's `cpsat` cell subprocesses into `python3 -m klassenzeit_solver.cpsat`; a stale wheel would confuse triage if the smoke surfaced a CP-SAT cell anomaly.

- [ ] **Step 3.2: Run the bake-off smoke at `--budget 5s --seeds 4 --fixtures grundschule,zweizuegig --out /tmp/lahc-kempe-smoke.md`**

```bash
mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig --out /tmp/lahc-kempe-smoke.md
```

Expected wall-clock: ~3-5 minutes (10 cells = 2 fixtures × 5 backends, 5s × 4 seeds = 20s wall-clock per cell at the LAHC arms, ~30-60s per CP-SAT cell). The supervisor exits 0 even if individual cells are 0/4 feasible (cpsat at 5s is famously thin); the gate is "every cell emitted a CellResult JSON without panic", not feasibility.

- [ ] **Step 3.3: Confirm the new `lahc_kempe` row renders in `/tmp/lahc-kempe-smoke.md`**

```bash
grep -E '^\| (grundschule|zweizuegig) \| lahc_kempe \|' /tmp/lahc-kempe-smoke.md
```

Expected: two matching lines (one per fixture). If grep returns no match, the new variant did not dispatch; re-read Task 2 step 2.3-2.4 for a missed edit.

Also confirm the `## Backend objectives` section renders the new row:

```bash
grep -E '^\| lahc_kempe \|' /tmp/lahc-kempe-smoke.md
```

Expected: one matching line.

- [ ] **Step 3.4: Sanity-check `worst_home_room_ratio_median` on the new `lahc_kempe` rows**

The brainstorm's Q1 hypothesis: "Kempe alone preserves `worst_home_med` (Kempe swaps lessons between days, keeping the per-day position and room) so `lahc_kempe` should not show the home-room collapse that `lahc_rr*` does." At 5s × 4 seeds the data is noisy, but the directional signal should already be visible: `lahc_kempe`'s `worst_home_med` column should be closer to plain `lahc`'s than to `lahc_rr`'s.

```bash
grep -E '^\| (grundschule|zweizuegig) \|' /tmp/lahc-kempe-smoke.md
```

Read the output; record the directional observation in the PR body. If `lahc_kempe`'s `worst_home_med` matches `lahc_rr`'s (counter to hypothesis), surface the surprise in the PR body so the next bake-off refresh annotation reads honestly. Do NOT block the PR on the directional check; production data lives at item 44's full refresh, not at this smoke.

- [ ] **Step 3.5: Commit any uncommitted edits**

```bash
git status
```

Expected: working tree clean. (No file edits in Task 3; this step exists only to confirm the smoke didn't accidentally leave stash debris or a regenerated file behind.)

---

## Self-Review

**Spec coverage check.**

- Spec scope bullet 1 (`solver-core/src/quality.rs` row + tests): Task 1.
- Spec scope bullet 2 (`solver-bench/src/main.rs` enum + label/parse/ALL/dispatch + rename): Task 2 steps 2.3, 2.4, 2.5.
- Spec scope bullet 3 (`solver-bench/tests/end_to_end.rs` substring assertion): Task 2 step 2.1.
- Spec acceptance "pre-merge bake-off smoke": Task 3.
- Spec scope bullet 4 (`OPEN_THINGS.md` item 22 deletion + item 44 amend + active-sprint header amend): handled by autopilot step 6, NOT in this plan. Plan's responsibility ends at the implementation commits.
- Spec acceptance "PR body cites the matching-period rationale": handled at PR open time (autopilot step 7), NOT in this plan.

**Placeholder scan.** No "TBD", no "TODO", no "implement later", no "fill in details", no vague error-handling stubs. Every step has either a concrete code block or a concrete shell command with expected output.

**Type consistency.** `BenchBackend::LahcKempe` is the variant name across all three task references (2.3, 2.4, 2.5). `lahc_kempe` is the string label across the parse/label/test assertions. `lahc_optimised` / `lahc_skipped` / `lahc_notes` are the local-binding names in `build_backend_objectives` (verified in the read of `quality.rs:375-414`); Task 1 step 1.4's code block uses them consistently.

**Crate-boundary commit ordering.** Task 1 is solver-core; Task 2 is solver-bench. The bench renderer at `main.rs:855` calls `solver_core::backend_objective(label).unwrap_or_else(|| panic!(...))`, so commit 2 (solver-bench wiring) depends on commit 1 (solver-core registration). The plan dispatches Task 1 before Task 2; subagent-driven-development executes them sequentially because they share state (the `lahc_kempe` registration is the contract Task 2 relies on). Cherry-picking commit 2 alone to a different branch would panic the renderer; the squash-merge convention on master makes this irrelevant for the merge commit.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-07-item-22-lahc-kempe-standalone-bench.md`.

Per autopilot non-negotiables and the project's "Subagents mandatory" feedback memory: this plan is executed via **Subagent-Driven Development** (`superpowers:subagent-driven-development`). Each task dispatches a fresh `general-purpose` subagent. Tasks 1 and 2 share state (Task 2's bench renderer panics if Task 1's registration is missing) so they run **sequentially**, one agent at a time. Task 3 (verification) runs after Task 2 lands.
