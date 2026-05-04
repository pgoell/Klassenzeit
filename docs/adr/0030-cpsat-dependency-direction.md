# 0030: CP-SAT enters via Python ortools, not Rust FFI

- **Status:** Accepted
- **Date:** 2026-05-04

## Context

ADR 0029 introduced the four-backend solver feasibility bake-off. Sprints 1-3 shipped the three Rust LAHC variants; Sprint 4 lands the fourth backend, CP-SAT (Google OR-Tools). The 2026-04-04 algorithm-selection research (`docs/research/2026-04-04-solver-algorithm-selection.md`, Sources 22 and 23) explicitly rejected CP-SAT on FFI / dependency grounds: "CP-SAT requires wrapping a C++ library (OR-Tools) via FFI, losing Rust's compile-time guarantees at the solver boundary." ADR 0029 already noted that the bake-off revises that dismissal in one specific way: CP-SAT enters Klassenzeit through Python `ortools`. Sprint 4 makes that revision concrete.

## Decision

CP-SAT lives entirely on the Python side. The new module `solver/solver-py/python/klassenzeit_solver/cpsat.py` is pure Python and imports `ortools.sat.python.cp_model`. The `ortools` PyPI wheel statically links the C++ CP-SAT engine, so no Rust crate ever links against OR-Tools. Soft scoring runs in Rust post-hoc via the new `score_solution_json` PyO3 binding so all four bake-off backends compare on the same Rust scorer.

## Alternatives considered

- **Direct FFI to OR-Tools C++** via `cxx` or `bindgen`. Rejected; the 2026-04-04 research's "no FFI / no C++" recommendation stands for the Rust side. The static-linked Python wheel sidesteps build-time C++ exposure while keeping the same engine.
- **Skip CP-SAT entirely** (LAHC-only bake-off, three backends). Rejected; ADR 0029's premise is a four-way comparison so future Sek I and Gymnasium fixtures can be re-run against the alternatives without re-implementation.
- **Encode soft constraints into CP-SAT's objective.** Rejected as out-of-scope for Sprint 4. The bake-off compares apples-to-apples on the Rust soft-scorer; a CP-SAT-side optimisation muddles the comparison axis. Re-evaluate post-bench if CP-SAT competes on feasibility but loses on soft-score.

## Consequences

The 2026-04-04 research's Rust-side preference stays intact. The backend Docker image gains roughly 50 MB from the `ortools` wheel; revisited if CP-SAT becomes production default. CP-SAT is deterministic with `num_search_workers=1` plus `random_seed`. The "thin wrappers only" rule in `solver/CLAUDE.md` is clarified: it applies to PyO3 wrappers, not to peer Python algorithms in the same package. A reverse-direction Python-to-Rust call (`cpsat.py` invokes `score_solution_json`) introduces a small surface where module load order matters at runtime; the import lives at the top of `cpsat.py` and is exercised by the bake-off's first run. The full bake-off bench wall-clock grows from roughly 80 minutes to roughly 5.3 hours at full settings (4 fixtures × 4 backends × 20 seeds × 60s); bench remains host-local and does not run in CI, consistent with Sprints 1-3.
