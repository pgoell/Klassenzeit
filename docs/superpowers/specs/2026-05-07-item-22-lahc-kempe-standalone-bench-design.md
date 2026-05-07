# lahc_kempe standalone bench backend (item 22)

**Sprint program.** Solver feasibility correctness + observability (active program), follow-ups bucket (`## Open solver follow-ups`).
**Phase.** Open follow-up: item 22 (P1).
**Goal.** Add a fourth LAHC bake-off backend column, `lahc_kempe`, that runs Kempe-without-R&R so the marginal Kempe contribution becomes legible above the composed `lahc_rr_kempe` row.

**Non-goals.** No solver-core algorithm change. No new `SolveConfig` field (Kempe-only is already expressible via `lahc_rr_period: None, lahc_kempe_period: Some(_)`). No production-default flip; the `Settings.solver_backend` enum stays `lahc_rr_kempe` and the future ADR 0035 (item 47) consumes the new column rather than this PR. No `BENCH_RESULTS.md` regeneration; that 5+ h cost belongs to the existing item 44 refresh, which now also picks up the new column. No new property test (`lahc_rr_kempe_never_decreases_placement_count` already pins the Kempe-move invariant; standalone Kempe exercises the same `kempe_attempt` code path with R&R off).

## Context

The 2026-05-06 production bake-off refresh shows `lahc_rr_kempe` matching `lahc_rr` cell-for-cell on every fixture (zweizuegig 1108 vs 1119, dreizuegig 2414 vs 2434, lock_in 606 vs 609); the Kempe contribution is currently illegible from atop R&R. `solver/CLAUDE.md` documents the suspected dynamic: R&R's full ruin-and-recreate breaks the home-room invariant aggressively (worst_home_med collapses 0.50 → 0.05 on zweizuegig, 0.04 → 0.00 on dreizuegig vs plain `lahc`), while Kempe swaps lessons between days at the same per-day position and room, so Kempe alone should preserve `worst_home_med` close to the plain `lahc` baseline.

Today the bench enumerates four backends in `solver/solver-bench/src/main.rs` (`Lahc`, `LahcRr`, `LahcRrKempe`, `CpSat`); `BenchBackend::ALL` is a const slice consumed by the cell-plan generator (`main.rs:247`, `flat_map((name, _)| ALL.iter().map(move |b| (*name, *b)))`) and the objectives renderer (`write_backend_objectives_section_renders_all_four_backends` test at `main.rs:1594`). The dispatch site at `main.rs:438` chooses `lahc_rr_period` and `lahc_kempe_period` per variant; the composed `LahcRrKempe` already uses `(Some(25), Some(23))`, so isolating Kempe means dispatching `(None, Some(23))` for the new variant.

`solver_core::quality::build_backend_objectives` enumerates four `BackendObjective` rows; `backend_objective(name)` returns `None` for an unknown backend, which `solver-bench`'s renderer treats as a registration bug (`main.rs:843` + the inline tests). Two unit tests in `solver/solver-core/src/quality.rs::tests` iterate the literal slice `["lahc", "lahc_rr", "lahc_rr_kempe"]` to verify partition correctness; the new variant must land in those slices in the same commit as the `BackendObjective` row.

After items 52 + 54 shipped on 2026-05-07, every LAHC variant declares `optimised: ALL` and `declared_skipped: empty` in its `BackendObjective` row because LAHC's accept-time canonical-score covers the full `QualityComponent` set. The new `lahc_kempe` row clones that shape; no per-variant divergence in `optimised` / `declared_skipped`.

The end_to_end smoke at `solver/solver-bench/tests/end_to_end.rs:18` spawns the supervisor without an explicit `--backends` flag, so `BenchBackend::ALL` automatically picks up the new variant; the smoke's row-substring assertions need a sibling check for `lahc_kempe`. Inline tests in `main.rs::tests` use synthetic plans and only assert on the variants their plans contain; they are unaffected by the addition.

Anchor items: `docs/superpowers/OPEN_THINGS.md` items 22 (this), 21 (`lahc_rr_period` + `RR_K` tuning, sister follow-up; the OPEN_THINGS line "Sweep before item 22" is about producing meaningful comparison data, not a code dependency), 47 (ADR 0035 production-default revisit, consumes the new column), 44 (BENCH_RESULTS.md refresh; this PR amends 44's body to acknowledge the new column).
Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run).

## Scope

**In scope.**

- `solver/solver-core/src/quality.rs`:
  - One new `BackendObjective` row in `build_backend_objectives`, name `"lahc_kempe"`, cloning `lahc_optimised` / `lahc_skipped` / `lahc_notes` from the existing LAHC family. Insertion order between `"lahc_rr"` and `"lahc_rr_kempe"` so the rendered `## Backend objectives` table reads in the same progression as `BENCH_RESULTS.md`'s data table.
  - Both unit tests grow `"lahc_kempe"` in their literal slices: `backend_objective_returns_some_for_every_known_backend` and `backend_objective_lahc_family_partitions_quality_components`.
- `solver/solver-bench/src/main.rs`:
  - `BenchBackend::LahcKempe` enum variant.
  - `label()` returns `"lahc_kempe"`. `parse()` accepts `"lahc_kempe"`.
  - `BenchBackend::ALL` becomes `[Lahc, LahcRr, LahcKempe, LahcRrKempe, CpSat]` (single-move variants grouped before the composed variant, CP-SAT last).
  - The dispatch match at `main.rs:438` adds `BenchBackend::LahcKempe => (None, Some(23u32))`.
  - The inline test `write_backend_objectives_section_renders_all_four_backends` is renamed to `write_backend_objectives_section_renders_every_registered_backend` (the loop already iterates `BenchBackend::ALL`, so the body needs no change; only the name and any inline doc lose their "four" specificity).
- `solver/solver-bench/tests/end_to_end.rs`:
  - One new `body.contains("lahc_kempe")` assertion alongside the existing `body.contains("lahc_rr_kempe")` assertion in `supervisor_emits_observability_and_quality_columns`. Confirms the supervisor accepts the new backend label and renders its objectives row.
- `docs/superpowers/OPEN_THINGS.md`:
  - Item 22 deleted (per autopilot step 6: when an item ships, it is removed from OPEN_THINGS entirely).
  - Item 44 body amended to read "...refresh `BENCH_RESULTS.md` at production cell shape now that items 12 and 22 have shipped" so the next refresh picks up both axis activations.
  - Item 21's body retains its "Sweep before item 22" note as historical context (item 21 still open) but no edit is needed since this PR ships item 22 without item 21.
  - Item 47's body retains its "once items 21 + 22 ship" reference; item 22 ships here, item 21 still pending.
  - The "Active sprint program" header line at OPEN_THINGS.md:9 is amended to drop "+ items 21 + 22" from the "Next pickup" sentence (item 21 stays open as the residual pickup option).
  - Active sprint header's running narrative gets one sentence: "Item 22 shipped on 2026-05-07: `BenchBackend::LahcKempe` standalone column (`lahc_rr_period: None, lahc_kempe_period: Some(23)`, matching `LahcRrKempe`'s Kempe period for clean isolation); BENCH_RESULTS.md refresh deferred to item 44."

**Out of scope.**

- Production-default flip (item 47, ADR 0035).
- `lahc_rr_period` / `RR_K` sweep (item 21, sister follow-up).
- `mise run bench:bakeoff` production-cell-shape regeneration (item 44).
- Promoting `KEMPE_MAX_CHAIN` to a `SolveConfig` knob (item 23).
- Lesson-group co-swap inside Kempe (item 24).

## Approach

Mechanical column-add. The two-commit split lines up with the documented Conventional Commits scope rule in `solver/CLAUDE.md` ("Use the crate directory as Conventional Commits scope"):

1. **`feat(solver-core): register lahc_kempe BackendObjective`** — `solver/solver-core/src/quality.rs` adds the row to `build_backend_objectives` (clone of `lahc_rr_kempe`'s declaration, same `lahc_notes` constant), and both `backend_objective_*` tests grow `"lahc_kempe"` in their literal slices. Self-contained: `cargo nextest run -p solver-core` exercises the new row.
2. **`feat(solver-bench): wire lahc_kempe BenchBackend variant (item 22)`** — `solver/solver-bench/src/main.rs` adds `BenchBackend::LahcKempe`, updates `label` / `parse` / `ALL`, dispatches `(lahc_rr_period: None, lahc_kempe_period: Some(23u32))`; renames the inline `renders_all_four_backends` test; `solver/solver-bench/tests/end_to_end.rs` row assertion grows the sibling `lahc_kempe` check. The `cargo machete` lint allows the order because commit 1's row is looked up by string at runtime; the library has no static reference.

Plus the docs commits (autopilot step 6): `docs(open-things): delete shipped item 22 and amend items 9, 44 (item 22)`, plus any CLAUDE.md / settings deltas the improvement-pass skills propose, plus auto-memory refresh.

## Acceptance

- `cargo nextest run -p solver-core` green; the two `backend_objective_*` tests now assert on five backends (lahc family of four plus cpsat).
- `cargo nextest run -p solver-bench --bin solver-bench` green; the inline tests pick up the new variant (the renamed `write_backend_objectives_section_renders_every_registered_backend` test iterates `BenchBackend::ALL`).
- `cargo nextest run -p solver-bench --test end_to_end` green; the supervisor smoke at `--budget 200ms --seeds 1 --fixtures grundschule` runs five backends instead of four and the markdown body contains `lahc_kempe`.
- `mise run lint` green; clippy match-exhaustiveness silent across `BenchBackend` matches in `main.rs`.
- One pre-merge smoke `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig --out /tmp/lahc-kempe-smoke.md` confirms the new column renders alongside the others without panic. Numbers are noisy at 5s/4 seeds; the gate is "the cell ran and emitted a CellResult", not the soft-score column itself.
- Item 22 entry deleted from OPEN_THINGS.md; item 44 body amended to acknowledge the new column.
- PR body cites the `solver/CLAUDE.md` "matching the period for clean isolation" rationale (Q2 in the brainstorm).

## Risk and rollout

- **Match exhaustiveness sweep.** Adding `BenchBackend::LahcKempe` triggers clippy non-exhaustive-match warnings at every `match` site in `main.rs`. Two known sites (`label`, `parse`); a third at the dispatch (`main.rs:438`). Sweep at task time via `cargo clippy -p solver-bench --tests -- -D warnings`.
- **Renamed inline test.** `write_backend_objectives_section_renders_all_four_backends` becomes `write_backend_objectives_section_renders_every_registered_backend`. `scripts/check_unique_fns.py` (cross-language unique-function-name lint) requires the new name to be globally unique across `.rs` files; `rg -n 'fn write_backend_objectives_section_renders_every_registered_backend'` to confirm pre-commit. The old name is unique today so the rename should land cleanly.
- **End_to_end smoke wall-clock.** The supervisor smoke today runs four backends at `--budget 200ms --seeds 1`; adding a fifth adds ~200ms wall-clock plus subprocess spawn overhead. CP-SAT in this smoke fails with `ModuleNotFoundError` (per `solver/CLAUDE.md` "cargo nextest does not propagate the uv venv to subprocess `python3`"); LAHC backends complete in well under 200ms each. New `LahcKempe` cell behaves identically to `LahcRrKempe` from the smoke's perspective. No CI budget impact.
- **Backend objective registration timing.** Splitting commits across crates risks the second commit failing CI alone if the first is reverted; the bench renderer panics on `backend_objective("lahc_kempe").unwrap()`. Mitigation: the two commits ship in one PR (squash-merge per repo convention); the merge commit contains both edits atomically.
- **Comparability of standalone Kempe wall-clock vs composed Kempe wall-clock.** `LahcKempe` runs Kempe at period 23 with no R&R overhead; `LahcRrKempe` runs Kempe at period 23 + R&R at period 25. A reader comparing the two backends' `total_ms_median` columns may attribute Kempe's contribution to "Kempe is fast" when the actual signal is "no R&R overhead". The PR body should call this out so the next bake-off refresh annotation is honest.
- **No solver-core algorithm change; no `Problem`/`SolveConfig` field cascade; no `solver-py` or backend Pydantic mirror; no Python deps moving.** The 20% criterion bench budget does not apply (no hot-path edits); pre-push pytest will be unaffected.
