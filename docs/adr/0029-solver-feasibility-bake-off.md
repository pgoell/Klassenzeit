# 0029: Solver feasibility bake-off methodology

**Status.** Accepted, 2026-05-04.

## Context

The 2026-04-04 solver-algorithm-selection research (`docs/research/2026-04-04-solver-algorithm-selection.md`) committed Klassenzeit to a pure-Rust FFD greedy + LAHC architecture and explicitly dismissed CP-SAT on FFI / dependency grounds. PR #173 (diagnostic phase) showed that the resulting solver flakes ~50% of runs on the demo Grundschule when the same-room hard constraint is on, and that LAHC cannot rescue an FFD `NoSuitableRoom` violation because LAHC moves accepted placements rather than re-placing failed ones. The active sprint program (`docs/superpowers/OPEN_THINGS.md`) revises the original "one decision, three candidate paths" framing into a head-to-head bake-off across four feasibility-improving candidates.

## Decision

Four solver backends ship as selectable options. Each is shipped fully; none are deferred mid-program. The bake-off bench compares them on a shared fixture set; the production default is the candidate that wins the Pareto frontier of (feasibility rate, time-to-first-feasible, soft score) at fixture-relevant time budgets, weighted toward feasibility.

| Backend | Sprint | Description |
| --- | --- | --- |
| `lahc` | 1 (this PR) | Current FFD greedy + LAHC, with Path A's same-room-aware FFD ordering (see below). |
| `lahc_rr` | 2 | Adds ruin-and-recreate as a third LAHC move type. |
| `lahc_rr_kempe` | 3 | Adds Kempe-chain swaps as a fourth LAHC move type. |
| `cpsat` | 4 | CP-SAT via `ortools` (Python). |

**Path A: same-room-aware FFD ordering.** A one-time change to `ordering::ffd_order` (not a runtime toggle); every later backend inherits it. The new eligibility metric counts `(day, room)` pairs where at least `preferred_block_size` consecutive teacher-unblocked, room-unblocked time blocks exist on `day` for the lesson's subject. Replaces the prior `free_blocks * suitable_rooms` product, which double-counted independent dimensions and ignored that the same-room hard constraint forces a lesson's full hours into one (day, room) family per `(class, subject)` triple.

**Bench harness.** New `solver-bench` workspace member at `solver/solver-bench/`. Single binary that runs `solve_with_config` per `(fixture, backend, seed)` cell against the production active-default `ConstraintWeights` and writes a markdown table to `solver/solver-core/benches/BENCH_RESULTS.md`. Mise task: `bench:bakeoff`.

**Fixtures.** Four: the existing `grundschule`, `zweizuegig`, `dreizuegig` (mirrors of `backend/.../seed/demo_*.py`), plus the `lock_in` reproducer extracted from `tests/same_room_property.rs::ffd_lock_in_grundschule`. The Sek-I stretch fixture from OPEN_THINGS item 3 is rolled into the Beyond-Grundschule program's Sprint 2 (where it is built natively as the primary fixture, not as a one-row "no regression" check on this bench).

**Per-cell metrics.** Twenty seeds (`seed in 1..=20`), 60-second per-seed wall-clock budget. Per cell, the harness captures: feasibility count (out of 20), hard-violations median, soft-score median (over feasible runs only), FFD wall-clock median, total wall-clock median.

**Deviations from OPEN_THINGS item 3.**

- `peak_memory_kb` and `time_to_first_feasible_ms` are deferred to Sprint 4. Klassenzeit problem sizes are sub-megabyte working sets so memory is not a feasibility signal for `lahc`; it will matter for CP-SAT in Sprint 4 where `ortools`'s allocation behavior is meaningful. `time_to_first_feasible_ms` for FFD + LAHC collapses to `ffd_ms_median` because feasibility is determined at end-of-FFD; CP-SAT is the first backend where the metric has independent meaning.
- Median-only stats; no p95 in Sprint 1. Twenty samples per cell give meaningful medians; p95 columns add row-width without proportionate signal until a long-tail issue surfaces in Sprints 2-4.
- Sek-I stretch fixture deferred (above).

**CI integration.** Bench does not run in CI. Algorithm-phase PRs cite the relevant `BENCH_RESULTS.md` diff in the PR body. The criterion bench (`mise run bench`) and `BASELINE.md` continue to gate perf regressions; the bake-off bench and `BENCH_RESULTS.md` gate feasibility regressions.

## Consequences

- The 2026-04-04 research's "no FFI / no C++" preference is partially revised. CP-SAT enters via `ortools` (pip-installed Python wrapper around the C++ CP-SAT engine), so the C++ dependency lives entirely on the Python side; no Rust FFI cost. Deployment ships ~50 MB more in the backend image. ADR added in Sprint 4 records this in detail (this ADR introduces the broader methodology).
- `solver_core::test_fixtures` becomes a default-on Cargo feature in solver-core. solver-py opts out via `default-features = false`. Production wheels do not ship the fixture builders.
- The criterion bench (`solver_fixtures.rs`) and the bake-off bench (`solver-bench`) share fixture builders through `solver_core::test_fixtures`. Drift between the two benches is now structurally impossible.
- Path A is unconditional after this PR. Future sprints cannot toggle FFD ordering at runtime; the bake-off compares optimization-phase candidates only, on top of a fixed Path A construction phase.

## Alternatives considered

- **Single-PR ADR per backend.** Rejected: the methodology is shared across four backends and benefits from one anchor document. Sprint-specific decisions (CP-SAT FFI direction, R&R RNG draw count) get their own ADRs as they land.
- **Path A as a runtime toggle.** Rejected: OPEN_THINGS commits to "one-time change, every later backend inherits". Adding a runtime toggle would surface a public API knob whose only purpose is one-PR review; before/after comparison is captured by the git-history pattern that `BASELINE.md` already uses.
- **Public `solver_core::SolverBackend` enum in Sprint 1.** Rejected: only one variant exists today, and a public enum changes once per added backend; the bench's private `BenchBackend` is sufficient for Sprint 1. Sprint 4 adds the public enum when CP-SAT requires it for `KZ_SOLVER_BACKEND` env-var dispatch.
- **Add the Sek-I stretch fixture in this PR.** Rejected: ~200-400 lines of hand-built fixture for one row of "no regression" data Sprint 1 does not depend on. Beyond-Grundschule Sprint 2 already plans the same fixture as a primary deliverable.
