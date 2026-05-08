# Research Brief: Third Solver Backend Candidates (item 56)

## Topic

Identify and evaluate strong solver backend candidates for school timetabling that could ship as a third backend alongside Rust LAHC variants and CP-SAT (current production candidates). Output is the input to a fresh brainstorm + ADR (numbered 0036+) when OPEN_THINGS item 56's trigger fires.

## Scope

- **Domain**: school timetabling (Hessen Grundschule and Sek-I/II fixtures). Hard constraints: room/teacher/class double-booking, lesson-group co-placement, no room-hopping, daily caps. Soft axes: class gaps, teacher gaps, subject preferences, home-room preference, class day balance, prefer-late-period.
- **Integration constraint (hard)**: the candidate MUST integrate via Python (PyPI wheel) or Rust (cargo crate). Pure-Java engines are out (item 56 explicit exclusion; ADR 0035 rejects Timefold). C / C++ cores with Python bindings are allowed and precedented (CP-SAT enters via the `ortools` wheel per ADR 0030).
- **Integration shape**: must fit `BenchBackend` dispatch in `solver/solver-bench/src/main.rs` and the canonical `QualityReport` evaluator (ADR 0030 uniform-scorer contract). Apples-to-apples scoring on the existing Rust scorer is the comparison axis.
- **Maintenance**: only currently maintained projects (commits in last 12 months, non-archived upstream).
- **License**: permissive (MIT / BSD / Apache-2 / MPL) preferred. GPL is a yellow flag for the project's MIT licensing, not an automatic exclusion. Commercial / non-commercial-only licenses are out.
- **Solver classes in scope**: constraint programming, MILP / LP, SAT / SMT, local search / metaheuristic frameworks, Rust-native solvers, school-timetabling-specific engines. The named candidates in item 56 (HiGHS, GLPK, Choco) are starting points, not a final list.
- **Trigger context**: spike runs only when Rust LAHC (post item 47 production-default revisit) and CP-SAT both plateau on the same quality axis. Research output should still surface candidates ready to evaluate when the trigger fires.

## Audience

Pascal (sole maintainer, deep familiarity with Rust + Python + CP / MIP solver landscape). Reads ADRs and uses research output to drive a brainstorm + bake-off. Output should be technically dense, opinionated where evidence supports it, and explicit about uncertainty where it does not.

## Purpose

Select 1 to 3 strong candidates for the OPEN_THINGS item 56 spike, with enough evidence that the brainstorm can pick the winner without a second round of research. For each candidate: maintenance status, license, integration cost (binding quality, build complexity, deployment footprint), expected fit for school-timetabling hard / soft constraints, and any published evidence of how it compares to CP-SAT on similar problems.

## Out of scope

- Re-evaluating Timefold or Choco (Java path closed by ADR 0035 + item 56's explicit "no Java").
- Re-evaluating CP-SAT or LAHC (already in production / bench).
- Production deployment design for the chosen candidate (deferred to the spike PR).
- Tuning of an already-chosen candidate.

## Reference state (as of 2026-05-08)

- ADR 0029: bake-off methodology, four backends (`lahc`, `lahc_rr`, `lahc_rr_kempe`, `cpsat`).
- ADR 0030: CP-SAT lives Python-side via `ortools`, no Rust FFI; uniform Rust scorer.
- ADR 0035: Timefold rejected (archived Python binding, version conflict, Java cost).
- `solver/solver-bench/src/main.rs`: `BenchBackend` enum with five variants today (`Lahc`, `LahcRr`, `LahcKempe`, `LahcRrKempe`, `CpSat`).
- `solver/solver-py/python/klassenzeit_solver/cpsat.py`: precedent for a Python-side peer module that calls back into Rust for scoring via `score_solution_json`.
- Python pinned to 3.14.2 (mise.toml). Any candidate's Python binding must support 3.14 or have a roadmap there.
