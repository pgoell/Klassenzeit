# CP-SAT seed via Python ortools spec (Sprint 4, items 8-10)

**Sprint program.** Solver feasibility bake-off (active program, Sprint 4: last sprint).
**Phase.** Sprint 4: CP-SAT seed phase via Python `ortools`.
**Goal.** Land CP-SAT as a fourth selectable solver backend (`KZ_SOLVER_BACKEND=cpsat`), refresh `BENCH_RESULTS.md` with all four backends head-to-head on the four bake-off fixtures, document the FFI-direction deviation in ADR 0030, and surface the production-default decision callout for the next cycle.

**Non-goal.** No production-default switch (the post-bench Pareto-frontier read drives that, separate PR). No removal of the `xfail` on `test_seeded_grundschule_solves_with_zero_violations` (post-bake-off item 11). No `peak_memory_kb` / `time_to_first_feasible_ms` columns (deferred follow-up item 19). No CP-SAT objective beyond `Minimize(0)` (the bake-off compares apples-to-apples on the Rust soft-scorer; a CP-SAT optimization objective is post-Sprint-4 if the bench warrants it). No new `ViolationKind` variant. No public `solver_core::SolverBackend` enum (Rust enum unchanged; dispatch lives in Python).

## Context

ADR 0029 records the four-candidate bake-off. Sprints 1-3 (PRs #175, #177, #178) shipped `lahc`, `lahc_rr`, `lahc_rr_kempe`, all native Rust LAHC variants. Sprint 4's job is the fourth backend: CP-SAT (Google OR-Tools) via the Python `ortools` package. The 2026-04-04 algorithm-selection research dismissed CP-SAT on FFI / dependency grounds; ADR 0029 already revised that dismissal in one specific way (Python-side `ortools` sidesteps Rust-side FFI). Sprint 4 makes the revision concrete.

The bake-off measures four metrics per `(fixture, backend, seed)` cell: feasibility rate, hard-violations median, soft-score median (over feasible runs), and total wall-clock median. CP-SAT must produce comparable rows on the existing `BENCH_RESULTS.md` shape, which means the same Rust soft-scorer evaluates all four backends. Soft-constraint encoding inside CP-SAT is rejected as out-of-scope (Q2 of the spec brainstorm); CP-SAT solves only the hard-feasibility CSP and the bench rescores its placements via a new `score_solution_json` PyO3 binding.

Anchor docs: `docs/superpowers/OPEN_THINGS.md` Sprint 4 items 8-10, `docs/adr/0029-solver-feasibility-bake-off.md`, `docs/research/2026-04-04-solver-algorithm-selection.md` Sources 22 + 23, `/tmp/kz-brainstorm/brainstorm.md` (this spec's brainstorm).

## Scope

**In scope.**

- New file `solver/solver-py/python/klassenzeit_solver/cpsat.py` (~250 lines pure Python). Public function `solve_cpsat_json(problem_json: str, deadline_ms: int | None, seed: int = 1) -> str`. Internally builds a CP-SAT model via `ortools.sat.python.cp_model`, solves with `num_search_workers=1` and `random_seed=seed`, marshals the result back into the existing `Solution` wire format. Includes a `__main__` block so the bench harness can invoke it as a subprocess.
- New file `solver/solver-py/python/klassenzeit_solver/__init__.pyi` updated to type the new function. `solver/solver-py/python/klassenzeit_solver/__init__.py` re-exports `solve_cpsat_json` and the new `score_solution_json`.
- New PyO3 binding `score_solution_json(problem_json: str, placements_json: str) -> int` in `solver/solver-py/src/lib.rs`. Wraps `solver_core::score::score_solution` (already `pub` and re-exported from the crate root via `pub use score::score_solution;` in `solver-core/src/lib.rs`) using the production-active `ConstraintWeights` constant introduced in commit 1. Used by `cpsat.py` to populate `Solution.soft_score` post-solve so all four backends get the same Rust scorer.
- New `pub const PRODUCTION_ACTIVE_WEIGHTS: ConstraintWeights` in `solver-core/src/types.rs` (or wherever `ConstraintWeights` lives). The exact same weight set is hardcoded today in three places: `solver-core/src/json.rs::solve_json_with_config`, `solver-bench/src/main.rs::production_active_weights`, and the bench's `BACKEND` cell loop. Centralising as a constant removes one drift surface and makes the new `score_solution_json` binding's weight choice unambiguous.
- Extend `klassenzeit_solver.solve_json_with_config(problem_json, deadline_ms, lahc_rr_period: Optional[int] = None, lahc_kempe_period: Optional[int] = None) -> str`. The two new kwargs default to `None` (no behavior change for existing callers) and let `solver_io.run_solve` pick a backend.
- New backend setting in `backend/src/klassenzeit_backend/core/settings.py`: `solver_backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"] = "lahc"`. Maps to `KZ_SOLVER_BACKEND` env var.
- Rewrite `backend/src/klassenzeit_backend/scheduling/solver_io.run_solve` to dispatch on `app.state.settings.solver_backend`. Each branch calls the right `klassenzeit_solver` function with the right kwargs (lahc / lahc_rr / lahc_rr_kempe through `solve_json_with_config`; cpsat through `solve_cpsat_json`). The match is exhaustive over the `Literal[...]` so `ty` enforces completeness.
- New backend integration test `backend/tests/scheduling/test_solver_io_backend_dispatch.py`. Per Literal value: `monkeypatch.setattr(app.state.settings, "solver_backend", value)`, run `run_solve` against a tiny problem, assert `len(solution["placements"]) > 0`. Plus an explicit `cpsat` case asserting `_solve_cpsat_json` was the dispatched callable.
- Add `ortools` (pinned `>=9.10,<10`) to `solver/solver-py/pyproject.toml`'s `dependencies` via `uv add ortools --package klassenzeit-solver`.
- New backend column `cpsat` in `solver-bench`. The `BenchBackend` enum gains `CpSat`; the FIXTURES × backends loop walks all four. The `cpsat` arm subprocess-invokes `python3 -m klassenzeit_solver.cpsat --problem-file <tmp> --deadline-ms <ms> --seed <n>` and parses the stdout JSON. The bench then calls `solver_core::score_solution` to populate the soft-score column (same code path as the Rust backends; the cpsat subprocess returns soft_score=0 by default and the bench overrides). Mismatch with the LAHC backends in this regard: the LAHC backends compute soft_score in-place; the cpsat backend trusts the bench's post-hoc rescore. Both end up using `solver_core::score_solution` so the comparison stays apples-to-apples.
- Refreshed `solver/solver-core/benches/BENCH_RESULTS.md` (4 fixtures × 4 backends = 16 rows) regenerated by `mise run bench:bakeoff` on the dev host. Committed as part of the PR.
- New ADR `docs/adr/0030-cpsat-dependency-direction.md`. Records the Python-side FFI direction, the ~50 MB image-size cost, the determinism contract, and the alternatives considered.
- `solver/CLAUDE.md` updates: clarify the "thin wrappers only" rule's scope (PyO3 wrappers, not peer Python algorithms), add a "CP-SAT module semantics" bullet (variable encoding, hard-constraint set, INFEASIBLE/UNKNOWN handling, determinism pinning), document the score_solution_json reverse-direction import.
- `backend/CLAUDE.md` updates: add `KZ_SOLVER_BACKEND` reference next to `KZ_SOLVE_DEADLINE_MS`.
- `docs/superpowers/OPEN_THINGS.md` edits: mark Sprint 4 items 8-10 ✅, narrate any deviations between this spec and the OPEN_THINGS-stated shape (the seed kwarg widening, the ViolationKind reuse decision, the post-hoc rescoring path), surface the production-default Pareto-frontier callout as the next-cycle decision, mark item 19 (peak_memory_kb / time_to_first_feasible_ms) as the next pickup.
- `deploy/README.md` note about the ~50 MB image growth and the `KZ_SOLVER_BACKEND` env-var operations contract.

**Out of scope.**

- A CP-SAT objective beyond `Minimize(0)`. Soft-constraint encoding into CP-SAT is a post-Sprint-4 follow-up if `BENCH_RESULTS.md` shows CP-SAT competitive on feasibility but losing on soft-score (the Rust scorer's "0 violations + 0 soft penalty" might be unreachable for CP-SAT-as-CSP).
- A new `ViolationKind` variant. CP-SAT INFEASIBLE / UNKNOWN cases reuse `NoFreeTimeBlock` with a `reason: Some("cpsat: <status>")` per ADR 0027. Surfacing CP-SAT-specific failure modes is a post-bake-off concern if CP-SAT becomes production default.
- Public `solver_core::SolverBackend` enum. Rust enum unchanged; dispatch lives in Python (the bench harness already used a private `BenchBackend` enum for Sprints 1-3 and that pattern continues).
- Removal of `pytest.mark.xfail` on `test_seeded_grundschule_solves_with_zero_violations` (post-bake-off item 11). The xfail comes off only after the production-default decision picks a winner.
- The Pareto-frontier production-default decision itself. This spec ships the bake-off column; the decision lives in a follow-up commit / PR after the bench refresh stabilises.
- `peak_memory_kb` and `time_to_first_feasible_ms` columns (item 19, post-bake-off).
- An end-to-end CI run of `mise run bench:bakeoff`. Multi-hour wall-clock plus host-sensitive numbers; the dev-host refresh stays the canonical workflow.

## CP-SAT model semantics

### Public surface

```python
def solve_cpsat_json(
    problem_json: str,
    deadline_ms: int | None,
    seed: int = 1,
) -> str:
    """
    Solve a Klassenzeit timetable problem via CP-SAT.

    Builds a per-block-anchor binary model, solves under the deadline with the
    given random seed, marshals back into the Solution wire format. On
    INFEASIBLE / UNKNOWN, returns a Solution with no placements and one
    NoFreeTimeBlock violation per lesson, reason="cpsat: <CpSolverStatus>".
    On MODEL_INVALID, raises RuntimeError (programmer bug).
    """
```

`deadline_ms = None` means no time limit (only used by tests on tiny fixtures); otherwise we set `solver.parameters.max_time_in_seconds = deadline_ms / 1000.0`. `seed` defaults to `1` so production callers don't have to thread it.

### Variable encoding

For each lesson `l` with `H = hours_per_week`, `N = preferred_block_size`, `K = H/N` blocks: define `y[l, d, p, r] ∈ {0, 1}` per `(day_of_week d, start_position p, room_id r)` such that

- `(d, p..p+N-1)` are all valid time-block positions on day `d` (the WeekScheme's `time_blocks` list defines which exist).
- room `r ∈ subject.suitable_rooms` for the lesson's subject.
- none of `(d, p+i)` for `i ∈ [0, N)` is in `teacher.blocked_time_blocks` for the lesson's teacher.
- none of `(d, p+i)` is in `room.blocked_time_blocks` for `r`.

Variables that fail any precondition are not created (equivalent to `y = 0`). On the dreizügige fixture pre-pruning, the variable count is roughly `102 lessons × 5 days × 7 positions × 12 rooms ≈ 43k`; post-pruning the actual count is meaningfully lower.

### Hard constraints

One CP-SAT constraint per item below; semantics mirror `solver_core::Error::*` and `ViolationKind::*` rules:

1. **Cardinality.** For each lesson `l`: `sum_{d, p, r} y[l, d, p, r] = K(l)`.
2. **Class non-overlap.** For each `(class c, day d, position p)`: `sum_{l with c ∈ classes(l), p' s.t. p' ≤ p < p' + N(l), r} y[l, d, p', r] ≤ 1`.
3. **Teacher non-overlap.** For each `(teacher t, day d, position p)`: same shape, summed over lessons whose teacher is `t`.
4. **Room non-overlap.** For each `(room r, day d, position p)`: same shape, summed over lessons in room `r`.
5. **Teacher max hours per week.** For each teacher `t`: `sum_{l ∈ teacher's lessons, d, p, r} y[l, d, p, r] · N(l) ≤ teacher.max_hours_per_week`.
6. **Same-room invariant.** For each `(class c, subject s)` group with multiple lessons: introduce auxiliary `z[c, s, r] ∈ {0, 1}` with `sum_r z[c, s, r] = 1` and `y[l, d, p, r] ≤ z[c, s, r]` for every `l ∈ (c, s).lessons`.
7. **Lesson-group co-placement.** For each lesson group `G`, every pair of group members `(l_i, l_j)` and every block index, equate per-`(d, p)`: `sum_r y[l_i, d, p, r] = sum_r y[l_j, d, p, r]`.
8. **Pinned placements (ADR 0027).** For each pinned tuple `(l, d, p, r)`: fix `y[l, d, p, r] = 1`. Cardinality constraint then satisfies the rest of the lesson's blocks via the regular CP-SAT search.

### Objective

```python
model.Minimize(0)
```

Soft-constraint encoding into CP-SAT is intentionally out of scope (see "Out of scope" above and the brainstorm Q2 reply).

### Determinism

```python
solver.parameters.num_search_workers = 1
solver.parameters.random_seed = seed
solver.parameters.log_search_progress = False
```

A property test asserts byte-identical output across two consecutive `solve_cpsat_json` calls with the same `(problem_json, deadline_ms, seed)`.

### Failure modes

CP-SAT's terminal status maps onto the existing `Solution` wire format:

| CpSolverStatus | Mapping |
| --- | --- |
| `OPTIMAL` | `Solution { placements, violations: [], soft_score: <Rust rescore> }` |
| `FEASIBLE` | `Solution { placements, violations: [], soft_score: <Rust rescore> }` (we did not optimize so OPTIMAL is rare; FEASIBLE is the success state) |
| `INFEASIBLE` | `Solution { placements: [], violations: [<one NoFreeTimeBlock per lesson with reason="cpsat: infeasible">], soft_score: 0 }` |
| `UNKNOWN` | Same shape with `reason="cpsat: unknown (deadline)"` |
| `MODEL_INVALID` | Raise `RuntimeError("cpsat: model invalid - bug in cpsat.py")` |

The bench reads `len(solution.violations) == 0` for feasibility; the existing column shape carries through unchanged.

## Backend dispatch

`Settings.solver_backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"] = "lahc"` lands next to `solve_deadline_ms` in `core/settings.py`. The `solver_io.run_solve` body becomes:

```python
async def run_solve(
    problem_json: str, scope_id: UUID | None, input_counts: dict[str, int],
    *, deadline_ms: int | None,
) -> dict:
    settings = ...  # via app.state.settings or a dependency-injected helper
    backend = settings.solver_backend
    started = time.monotonic()
    try:
        match backend:
            case "lahc":
                solution_json = await asyncio.to_thread(
                    _solve_json_with_config, problem_json, deadline_ms,
                )
            case "lahc_rr":
                solution_json = await asyncio.to_thread(
                    _solve_json_with_config, problem_json, deadline_ms,
                    lahc_rr_period=25,
                )
            case "lahc_rr_kempe":
                solution_json = await asyncio.to_thread(
                    _solve_json_with_config, problem_json, deadline_ms,
                    lahc_rr_period=25, lahc_kempe_period=23,
                )
            case "cpsat":
                solution_json = await asyncio.to_thread(
                    _solve_cpsat_json, problem_json, deadline_ms,
                )
    except (ValueError, RuntimeError) as exc:
        ...  # existing error path
    ...  # existing log + json.loads + return
```

The structured-log event `solver.solve.start` and `solver.solve.done` should also include `backend=<backend>` in `extra=` so an operator can grep which backend served a given request.

## Bench harness wiring

```rust
#[derive(Clone, Copy)]
enum BenchBackend {
    Lahc, LahcRr, LahcRrKempe, CpSat,
}

impl BenchBackend {
    fn label(self) -> &'static str { match self { ..., BenchBackend::CpSat => "cpsat" } }
}

const BACKENDS: &[BenchBackend] = &[
    BenchBackend::Lahc, BenchBackend::LahcRr, BenchBackend::LahcRrKempe, BenchBackend::CpSat,
];
```

The `run_cell` function gains a CP-SAT branch that:

1. Serializes the `Problem` to JSON via a new `solver_core` helper or direct `serde_json::to_string`.
2. Writes JSON to a tempfile in `std::env::temp_dir()`.
3. Runs `Command::new("python3").args(["-m", "klassenzeit_solver.cpsat", "--problem-file", &path, "--deadline-ms", &ms, "--seed", &seed_str])`.
4. Parses stdout as a `Solution` (a small `serde_json` `Deserialize` impl already exists in `solver-core` for the wire format; reuse it).
5. Computes soft score via `solver_core::score::score_solution(&problem, &solution.placements)`. (This is the same call cpsat.py would make through the PyO3 binding; calling it here is a small redundancy in the apples-to-apples-comparison commitment but keeps the bench's number authoritative.)

Subprocess errors are recorded as "infeasible (subprocess error: ...)" in the cell so the bench run completes even if Python isn't available; they're never silent.

## File layout (new + modified)

**New:**

- `solver/solver-py/python/klassenzeit_solver/cpsat.py`: CP-SAT impl + `__main__`.
- `solver/solver-py/tests/test_cpsat.py`: unit tests (Q8 of brainstorm).
- `solver/solver-py/tests/test_cpsat_determinism.py`: property test.
- `solver/solver-py/tests/test_score_solution_json.py`: round-trip test for the new PyO3 binding.
- `backend/tests/scheduling/test_solver_io_backend_dispatch.py`: backend dispatch test.
- `docs/adr/0030-cpsat-dependency-direction.md`: the ADR.

**Modified:**

- `solver/solver-py/src/lib.rs`: add `score_solution_json` PyO3 wrapper, widen `solve_json_with_config` signature.
- `solver/solver-py/python/klassenzeit_solver/__init__.py` and `__init__.pyi`: re-export and type the new functions.
- `solver/solver-py/pyproject.toml`: add `ortools >=9.10,<10` dependency.
- `solver/solver-core/src/types.rs`: add `pub const PRODUCTION_ACTIVE_WEIGHTS: ConstraintWeights = ...` literal. Re-export from `lib.rs`.
- `solver/solver-core/src/json.rs`: replace the inline weights literal with `PRODUCTION_ACTIVE_WEIGHTS.clone()` (or by-value if `ConstraintWeights: Copy`).
- `solver/solver-bench/src/main.rs`: replace `production_active_weights()` with `PRODUCTION_ACTIVE_WEIGHTS`.
- `solver/solver-bench/src/main.rs`: add `BenchBackend::CpSat`, the subprocess invocation, the JSON parsing, and the score-solution post-call.
- `solver/solver-core/benches/BENCH_RESULTS.md`: regenerated by `mise run bench:bakeoff`.
- `backend/src/klassenzeit_backend/core/settings.py`: add `solver_backend` field.
- `backend/src/klassenzeit_backend/scheduling/solver_io.py`: replace the single `solve_json_with_config` call with the match-dispatch.
- `backend/.env.example`: document `KZ_SOLVER_BACKEND`.
- `solver/CLAUDE.md`: clarify "thin wrappers only" scope, add CP-SAT semantics bullet, document the cpsat → score_solution_json reverse import.
- `backend/CLAUDE.md`: `KZ_SOLVER_BACKEND` reference.
- `docs/superpowers/OPEN_THINGS.md`: close out Sprint 4 items 8-10, surface the Pareto-frontier callout, mark item 19 next.
- `docs/adr/README.md`: index ADR 0030.
- `deploy/README.md`: ~50 MB image growth and `KZ_SOLVER_BACKEND` operations note.

## Test plan

Five layers, scaling from cheap to expensive:

1. **Unit tests on the model builder** (`tests/test_cpsat.py`):
   - `test_solve_cpsat_json_grundschule_returns_zero_violations`: full demo Grundschule fixture (a Python-side fixture mirror constructed from minimal seed data, or a JSON dump of the Rust `test_fixtures::grundschule_fixture`); 5 s deadline; assert `Solution.violations` is empty.
   - `test_solve_cpsat_json_pinned_placements_round_trip`: pin half the placements; assert all pins survive in the output.
   - `test_solve_cpsat_json_lesson_group_co_placement_preserved`: dreizügige Religion trio is co-placed (same `(day_of_week, position)` per block).
   - `test_solve_cpsat_json_same_room_invariant_holds`: two lessons of the same `(class, subject)` end up in the same room.
   - `test_solve_cpsat_json_doppelstunde_block_contiguity`: two-period lessons land on consecutive positions same day same room.
   - `test_solve_cpsat_json_infeasible_returns_violations_with_reason`: synthesize a problem with insufficient capacity; assert returned `Solution` carries `Violation { kind: "no_free_time_block", reason: "cpsat: ..." }`.
   - `test_solve_cpsat_json_invalid_json_raises_value_error`: malformed JSON input → `ValueError`.

2. **Determinism property test** (`tests/test_cpsat_determinism.py`): two consecutive `solve_cpsat_json(problem, deadline_ms, seed)` calls with the same arguments yield byte-identical outputs.

3. **`score_solution_json` round-trip** (`tests/test_score_solution_json.py`): feed the binding the Solution from a `solve_json` call and assert the scored value matches `Solution.soft_score`.

4. **Backend integration test** (`backend/tests/scheduling/test_solver_io_backend_dispatch.py`): for each `solver_backend` Literal value, `monkeypatch.setattr(app.state.settings, "solver_backend", "<value>")` and exercise `run_solve` against a tiny problem. Asserts the dispatch picks the right code path. The cpsat case asserts non-empty `Solution.placements`.

5. **Bench harness contract test** (inline tests in `solver/solver-bench/src/main.rs`): a unit test that constructs the subprocess command-line for a `BenchBackend::CpSat` cell and asserts the parsed args, without launching a Python subprocess (CI does not have `klassenzeit_solver` installed in the bench's runtime env by default).

**What is NOT tested:** end-to-end `mise run bench:bakeoff` on CI. The bake-off bench runtime is hours, host-sensitive on wall-clock columns, and `BENCH_RESULTS.md` is regenerated on a developer host before merge (same workflow as Sprints 1-3).

## Commit split

Each commit must compile and pass tests on its own (`solver-binding rebuild discipline` per `.claude/CLAUDE.md`).

1. `refactor(solver-core): extract production_active_weights into a public constant`
2. `feat(solver-py): add score_solution_json PyO3 binding`
3. `feat(solver-py): widen solve_json_with_config with lahc period kwargs`
4. `build(solver-py): add ortools to klassenzeit-solver dependencies` (`uv add ortools --package klassenzeit-solver`)
5. `feat(solver-py): solve_cpsat_json CP-SAT seed via ortools`
6. `feat(backend): add solver_backend setting and KZ_SOLVER_BACKEND dispatch`
7. `feat(solver-bench): cpsat backend column + bench refresh`
8. `docs(adr): add 0030 cpsat dependency direction`
9. `docs: close out sprint 4 of solver bake-off`

Subagents share state on commits 1-3 (solver-core / solver-py); they dispatch sequentially. Commit 4 is a one-line `uv add` the main session does inline. Commits 5-9 each run in their own subagent, sequentially, because they touch shared files.

## Risks

1. **CP-SAT model build time on dreizügige.** Variable count is multiplicative pre-pruning. Mitigation: aggressive pruning at variable-creation time (suitability, blocked time blocks, window-fits-on-day). If model-build wall-clock exceeds ~5 s on dreizügige, the cell still completes within the 60 s deadline because the deadline applies to `Solve`, not to model build; flagged in PR if observed.
2. **`ortools` CI wheel availability.** Linux x86_64 wheels ship from the `ortools` PyPI index; Ubuntu CI runners install them without trouble. Pin `ortools >=9.10,<10` to avoid surprise major bumps. Mac / Windows wheels exist but the project's CI is Linux-only.
3. **`ty` type-checking on `ortools.sat.python.cp_model`.** OR-Tools may not ship `py.typed` markers; if `ty` flags `unresolved-import`, add `# ty: ignore[unresolved-import]` on the single `from ortools...` line. If even that fails, fall back to a typed re-export shim.

Lower-tier risks not blocking PR: backend image grows ~50 MB (documented in ADR 0030); bench refresh wall-clock now ~5.3 hours per full refresh (documented in `solver/CLAUDE.md`); the `cpsat.py` → `score_solution_json` reverse import direction needs a `solver/CLAUDE.md` line of context for future readers.

## Success criteria

The bake-off bench refresh produces a `BENCH_RESULTS.md` with four backend rows per fixture; the `cpsat` row at minimum reaches the same feasibility rate as `lahc` on the demo Grundschule, and `mise run bench:bakeoff` completes without subprocess errors. The PR body cites:

- Committed `BENCH_RESULTS.md` diff (4 fixtures × 4 backends = 16 rows, up from 12 today).
- Production-default recommendation based on the Pareto frontier of feasibility / wall-clock / soft-score across the fixture set, surfaced as the next-cycle decision point in OPEN_THINGS.

The xfail on `test_seeded_grundschule_solves_with_zero_violations` (post-bake-off item 11) becomes actionable after this PR; that test stays xfail in this PR.
