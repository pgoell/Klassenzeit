# Cluster: rust-native-solvers

Title: Rust-native solvers (CP / SAT / MIP / metaheuristic) suitable as a `BenchBackend` without Python at all.

Scope reminder: maintained (commits in last 12 months as of 2026-05-08), permissive license preferred (MIT / BSD / Apache-2 / MPL), embeds via cargo, integrates with the existing Rust scorer in `solver-core`.

---

## SQ1: Pumpkin (TU Delft, Rust CP solver)

### Maintenance and release cadence

- Latest crate release: `pumpkin-solver-v0.3.0` published 2026-02-11 [source: https://api.github.com/repos/ConSol-Lab/Pumpkin/releases | GitHub Releases API for ConSol-Lab/Pumpkin | independent].
- Most recent commit on `main`: 2026-05-06 (`chore: Do not collect unnecessarily into solution in Solver`) [source: https://api.github.com/repos/ConSol-Lab/Pumpkin/commits | GitHub commits API for ConSol-Lab/Pumpkin | independent].
- 594 commits on the main branch, 15 releases [source: https://github.com/ConSol-Lab/Pumpkin | GitHub - ConSol-Lab/Pumpkin | independent].
- Repo `pushed_at` 2026-05-07T04:05:06Z, 74 stars, not archived [source: https://api.github.com/repos/ConSol-Lab/Pumpkin | GitHub repo metadata for ConSol-Lab/Pumpkin | independent].

### License

- Dual-licensed Apache-2.0 OR MIT [source: https://github.com/ConSol-Lab/Pumpkin | Pumpkin README license badge | independent]. GitHub API reports `Apache-2.0` as the SPDX tag [source: https://api.github.com/repos/ConSol-Lab/Pumpkin | GitHub repo metadata for ConSol-Lab/Pumpkin | independent].

### Domain features (constraint coverage)

Solver is built on the **Lazy Clause Generation (LCG)** paradigm, the same family that Chuffed pioneered [source: https://github.com/ConSol-Lab/Pumpkin/blob/main/README.md | Pumpkin README | independent]. Top-level public API exposes:

- Global constraints: `cumulative()`, `cumulative_with_options()`, `disjunctive_strict()`, `all_different()`, `element()`, `table()`, `negative_table()` [source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html | docs.rs all-items index for pumpkin-solver | independent].
- Arithmetic: `equals()`, `not_equals()`, `less_than()`, `greater_than()`, `less_than_or_equals()`, `greater_than_or_equals()`, `plus()`, `times()`, `division()`, `absolute()`, `maximum()`, `minimum()` [source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html | docs.rs all-items index for pumpkin-solver | independent].
- Optimisation API: `OptimisationDirection`, `OptimisationStrategy`, `OptimisationProcedure` trait, `SolutionCallback` trait [source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html | docs.rs all-items index for pumpkin-solver | independent].
- Clausal constraints (`clause()`, `conjunction()`) [source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html | docs.rs all-items index for pumpkin-solver | independent].

The `cumulative` (resource scheduling) and `disjunctive` (no-overlap) constraints are first-class: the example `pumpkin-solver/examples/disjunctive_scheduling.rs` builds a no-overlap schedule from start variables plus precedence literals using `pumpkin_constraints::less_than_or_equals(...).reify(&mut solver, literal)` and an `add_clause` to encode "either x ends before y starts, or vice-versa" [source: https://github.com/ConSol-Lab/Pumpkin/blob/main/pumpkin-solver/examples/disjunctive_scheduling.rs | Pumpkin disjunctive_scheduling.rs example | independent]. That is structurally the school-timetabling no-double-booking constraint shape (room, teacher, class disjunctive resources).

Notable absence: the README's first listing of supported constraints does not include `alldifferent` or `table` [source: https://github.com/ConSol-Lab/Pumpkin/blob/main/README.md | Pumpkin README constraint list | independent], though both are exposed in the `all-items` docs index [source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html | docs.rs all-items index for pumpkin-solver | independent]. Inconsistency between README and API is a minor flag for documentation maturity.

### Real-world / competitive evidence

- 2025 MiniZinc Challenge: shared **bronze** in the Fixed Search track, tied with SICStus Prolog. OR-Tools CP-SAT took gold; Choco-solver CP-SAT took silver [source: https://www.minizinc.org/challenge/2025/results/ | MiniZinc Challenge 2025 Results | independent].
- 2024 MiniZinc Challenge: Pumpkin was registered but **received no medal placement** in any of the five tracks (Fixed, Free, Parallel, Open, Local Search) [source: https://www.minizinc.org/challenge/2024/results/ | MiniZinc Challenge 2024 Results | independent]. Year-on-year improvement is real but the solver is one tier behind CP-SAT and Choco at this benchmark.
- 2025 MiniZinc benchmark categories included `EchoSched` (scheduling), `ihtc-2024` (Integrated Healthcare Timetabling Competition), `tsptw`, `groupsplitter`. No purely school-timetabling benchmark was named [source: https://www.minizinc.org/challenge/2025/results/ | MiniZinc Challenge 2025 Results | independent].
- The CP 2025 paper "Unite and Lead: Finding Disjunctive Cliques for Scheduling Problems" by Sidorov, Marijnissen, Demirović benchmarks Pumpkin on **RCPSP** and **RCPSP/max** instances and reports new lower bounds for 16 RCPSP/max instances (closing six) and four RCPSP instances (closing one), plus new upper bounds for two RCPSP/max and four RCPSP instances [source: https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2025.35 | Sidorov, Marijnissen, Demirović CP 2025 — Unite and Lead | independent]. The paper's framing is project scheduling; school timetabling is not addressed [author estimate: based on the abstract which only names RCPSP/RCPSP-max].
- Cited foundational paper: Flippo et al., CP 2024, "A Multi-Stage Proof Logging Framework to Certify the Correctness of CP Solvers" (Pumpkin's distinguishing feature is unsat-proof logging) [source: https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2024.11 | Flippo et al. CP 2024 | independent].

### Python and Rust integration

- Rust: `cargo add pumpkin-solver`. Pure Rust, requires Rust 1.72.1+ to build [source: https://github.com/ConSol-Lab/Pumpkin/blob/main/README.md | Pumpkin README - Building from Source | independent].
- Rust API uses `Solver::default()`, `solver.new_bounded_integer(low, high)`, `solver.add_constraint(...)` with helper functions, then `satisfy()`, `optimise()`, or `get_solution_iterator()` [source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/ | docs.rs landing page for pumpkin_solver | independent].
- Python: PyPI wheel `pumpkin-solver` 0.3.0 released 2026-02-11. **Pre-built wheels are published for cp38 through cp314 on manylinux x86_64** plus aarch64, armv7l, i686, musllinux variants, plus PyPy 3.9-3.11 [source: https://pypi.org/pypi/pumpkin-solver/json | PyPI JSON metadata for pumpkin-solver 0.3.0 | independent]. Python 3.14 is therefore covered out-of-the-box.
- Python binding is built with maturin and pyo3 0.28, requires-python `>=3.8` [source: https://github.com/ConSol-Lab/Pumpkin/blob/main/pumpkin-solver-py/pyproject.toml | Pumpkin pumpkin-solver-py pyproject.toml | independent] [source: https://github.com/ConSol-Lab/Pumpkin/blob/main/pumpkin-solver-py/Cargo.toml | Pumpkin pumpkin-solver-py Cargo.toml | independent].
- Python examples are limited: only `nqueens.py` and `optimisation.py` ship in `pumpkin-solver-py/examples/` [source: https://api.github.com/repos/ConSol-Lab/Pumpkin/contents/pumpkin-solver-py/examples | Pumpkin Python examples directory | independent]. The richer `disjunctive_scheduling` example is only on the Rust side.

### Adversarial: Pumpkin gaps

- The "fixed search" bronze placement at the 2025 MiniZinc Challenge means Pumpkin trails OR-Tools CP-SAT (current Klassenzeit production candidate) by a measurable margin on broad CP benchmarks [source: https://www.minizinc.org/challenge/2025/results/ | MiniZinc Challenge 2025 Results | independent]. Compared to Chuffed (the canonical LCG solver), Pumpkin's documentation does not claim to beat Chuffed on scheduling [author estimate: README and CP 2024 paper claim correctness/proof advantages, not raw speed].

---

## SQ2: Other Rust CP solvers — copper, NuCS (Python), pcp

### copper (`ffminus/copper`)

- Last push: 2024-01-10, 10 stars, MIT, not archived [source: https://api.github.com/repos/ffminus/copper | GitHub repo metadata for ffminus/copper | independent].
- **Out of scope**: the most recent push is more than 12 months before the cluster cutoff (>28 months stale). Latest crate release v0.1.0 (2024-01-06) [source: https://github.com/ffminus/copper | GitHub - ffminus/copper | independent].
- README explicitly disclaims maturity: "Copper is under heavy development. Some exposed APIs are subject to change..." and "cannot rival with mature solvers like Gecode or or-tools" [source: https://github.com/ffminus/copper | Copper README warnings | independent].
- No `alldifferent`, `cumulative`, or other global constraints documented; no Python bindings.

### NuCS (`yangeorget/nucs`) — Python, not Rust-native, but SQ2-adjacent

- Last push: 2026-05-08; 803 commits; 51 releases; 55 stars; MIT [source: https://api.github.com/repos/yangeorget/nucs | GitHub repo metadata for yangeorget/nucs | independent].
- Latest releases: v10.1.0 (2026-04-11), v9.1.3 (2026-03-07) [source: https://api.github.com/repos/yangeorget/nucs/releases | GitHub releases API for yangeorget/nucs | independent].
- Latest PyPI: `NuCS` v10.1.0 published 2026-04-11; `requires_python = ">=3.11"`, **classifiers explicitly list Python 3.11, 3.12, 3.13, 3.14** [source: https://pypi.org/pypi/nucs/json | PyPI JSON metadata for NuCS 10.1.0 | independent].
- Pure Python on Numpy + Numba; "NuCS achieves performance similar to that of solvers written in Java or C/C++" is a self-claim, **no head-to-head benchmark vs OR-Tools is provided** [source: https://github.com/yangeorget/nucs/blob/main/README.md | NuCS README performance claim | practitioner].
- The 14200 solutions to 12-queens are found in <2s on a MacBook M2 (only timing claim) [source: https://medium.com/@yangeorget/nucs-fast-constraint-solving-in-python-9418359c109d | Yan Georget Medium blog | practitioner].
- Propagators include: `alldifferent_propagator`, `gcc_propagator` (global cardinality), `count_eq_propagator`, `count_geq_c_propagator`, `count_leq_c_propagator`, `element_eq_propagator`, `element_l_eq_alldifferent_propagator`, `lexicographic_leq_propagator`, `max_eq_propagator`, `min_eq_propagator`, `no_sub_cycle_propagator`, `permutation_aux_propagator`, `relation_propagator`, `scc_propagator`, `sum_eq_propagator`, `affine_eq_propagator` [source: https://api.github.com/repos/yangeorget/nucs/contents/nucs/propagators | NuCS propagators directory listing | independent]. **No `cumulative` propagator**; the README's `EmployeeSchedulingProblem` example uses booleans plus `ALG_COUNT_EQ_C` / `ALG_COUNT_LEQ_C` rather than a cumulative resource constraint, with a TODO to migrate to GCC [source: https://github.com/yangeorget/nucs/blob/main/nucs/examples/employee_scheduling/employee_scheduling_problem.py | NuCS employee_scheduling example | independent].
- Examples folder: `bibd, car_sequencing, cryptarithmetic, employee_scheduling, golomb, knapsack, langford, latin_square, magic_sequence, magic_square, quasigroup, queens, schur_lemma, social_golfers, sports_tournament_scheduling, sudoku, tsp` [source: https://api.github.com/repos/yangeorget/nucs/contents/nucs/examples | NuCS examples directory listing | independent].
- Caveat: "Since the Python code is compiled and the result cached, performance will always be significantly better when you run your program a second time" — Numba JIT cold-start cost matters for short solves [source: https://github.com/yangeorget/nucs/blob/main/README.md | NuCS README performance note | practitioner].

NuCS belongs more naturally in a Python-cluster than rust-native, but it surfaces here because the brief allows Python-bound C/C++ cores; NuCS is Numpy/Numba-backed Python, which fits the `requires_python = ">=3.11"` pin and the "Python integration" path of `BenchBackend`. **No `cumulative` propagator** is the central school-timetabling fit gap.

### pcp (`ptal/pcp` / libpcp)

- Last push: 2023-11-22, 112 stars, Apache-2.0 [source: https://api.github.com/repos/ptal/pcp | GitHub repo metadata for ptal/pcp | independent].
- **Out of scope**: >12 months stale (no commits in ~18 months as of 2026-05-08).

### u-ras (`iyulab/U-RAS`) — archived

- `archived: true`, last push 2026-02-09, 1 star, MIT [source: https://api.github.com/repos/iyulab/U-RAS | GitHub repo metadata for iyulab/U-RAS | independent].
- **Out of scope**: archived upstream is an explicit exclusion in the brief.

### Summary SQ2

The only Rust CP option meeting the maintenance bar is **Pumpkin**. NuCS is the closest non-Java Python alternative but lacks a cumulative global constraint. `copper`, `pcp`, and `u-ras` all fail the maintenance gate.

---

## SQ3: Rust SAT solvers

### varisat (`jix/varisat`)

- Last push: 2022-11-02, 282 stars, Apache-2.0 [source: https://api.github.com/repos/jix/varisat | GitHub repo metadata for jix/varisat | independent].
- **Out of scope**: ~3.5 years stale.

### splr (`shnarazk/splr`)

- Last push: 2026-05-07, 109 stars [source: https://api.github.com/repos/shnarazk/splr | GitHub repo metadata for shnarazk/splr | independent].
- License: **MPL-2.0** [source: https://api.github.com/repos/shnarazk/splr/contents/LICENSE | splr LICENSE file | independent]. Permissive enough by the brief's MPL allowance.
- "A modern (trail saving, clause subsumption/vivification, learning-rate based selecting, rephrase) CDCL SAT solver in Rust" [source: https://api.github.com/repos/shnarazk/splr | splr description | practitioner].
- Glucose 4.1-derived; "Splr-0.17.0 solved 49 satisfiable problems and 34 unsatisfiable problems in SAT Competition 2021 Benchmarks main track" [source: https://crates.io/crates/splr | crates.io splr description (via search) | practitioner].

### batsat (`c-cube/batsat`)

- Last push: 2026-05-05, 32 stars [source: https://api.github.com/repos/c-cube/batsat | GitHub repo metadata for c-cube/batsat | independent].
- License: derived from MiniSat (RatSat) — non-SPDX explicit (`NOASSERTION`); the LICENSE file shows the MiniSat copyright header [source: https://api.github.com/repos/c-cube/batsat/contents/LICENSE | batsat LICENSE | independent]. MiniSat licensing is permissive (MIT-like), which suggests acceptable but warrants a manual licence read before adoption [author estimate].
- "Because it is fully implemented in Rust, it is a good choice for restricted compilation scenarios like WebAssembly" [source: https://crates.io/crates/rustsat-batsat | rustsat-batsat crates.io description | practitioner].

### rustsat (`chrjabs/rustsat`)

- Last push: 2026-05-03, 67 stars, MIT [source: https://api.github.com/repos/chrjabs/rustsat | GitHub repo metadata for chrjabs/rustsat | independent].
- Latest releases: rustsat-v0.7.5 (2026-01-30) and a coordinated set of solver-binding crates `rustsat-minisat`, `rustsat-kissat`, `rustsat-ipasir`, `rustsat-tools` all at v0.7.5 (2026-01-30) [source: https://api.github.com/repos/chrjabs/rustsat/releases | rustsat releases API | independent].
- "RustSAT provides crates for the state-of-the-art SAT solvers Kissat, CaDiCaL, MiniSat, and Glucose, as well as BatSat" [source: https://arxiv.org/html/2505.15221v1 | RustSAT: A Library For SAT Solving in Rust, SAT 2025 paper | independent]. Active research project paired with peer-reviewed publication.
- A Rust front-end that lets you embed Kissat / CaDiCaL (industry-leading SAT solvers) without writing FFI by hand. License of the underlying engines must be checked per crate (MIT for rustsat itself, but Kissat is MIT, CaDiCaL is MIT — both fine).

### Adversarial: SAT for school timetabling

SAT-encoded timetabling exists in literature: "MaxSAT-based large neighborhood search for high school timetabling" demonstrates SAT/MaxSAT applicability to XHSTT [source: https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | Lemos et al., MaxSAT-based LNS for high school timetabling | independent]. "Recently, exact methods based on integer programming, maxSAT, and constraint programming have proven to be very effective for XHSTT" [source: https://www.researchgate.net/publication/345191580_Optimizing_Student_Course_Preferences_in_School_Timetabling | Optimizing Student Course Preferences in School Timetabling | independent]. However, encoding the soft-constraint cost function (gaps, prefs, balance) into MaxSAT typically inflates clause count by orders of magnitude vs the existing CP-SAT and LAHC encodings [author estimate: standard practitioner result, no specific cite for Klassenzeit-shaped sizes]. The integration cost would be a full re-encoding effort, not a swap-in.

### Summary SQ3

`splr`, `batsat`, and `rustsat` are all maintained. None ship with a scheduling/timetabling DSL; all require the user to encode the problem as CNF (or WCNF for MaxSAT). For a school-timetabling backend, the encoding effort is comparable to writing the CP model from scratch and the encoded problem is much larger than its CP form.

---

## SQ4: Rust metaheuristic frameworks

### argmin (`argmin-rs/argmin`)

- Last push: 2025-11-07, **1249 stars** (largest by far in this cluster) [source: https://api.github.com/repos/argmin-rs/argmin | GitHub repo metadata for argmin-rs/argmin | independent].
- Last release: argmin-v0.11.0 (2025-09-28) [source: https://api.github.com/repos/argmin-rs/argmin/releases | argmin releases | independent].
- License: dual Apache-2.0 / MIT [source: https://github.com/argmin-rs/argmin/blob/main/README.md | argmin README license section | independent].
- Algorithm list (numerical/continuous focus): line searches (backtracking, More-Thuente, Hager-Zhang), trust region, steepest descent, conjugate gradient, Newton/Newton-CG, BFGS, L-BFGS, DFP, SR1, Gauss-Newton, golden-section search, Landweber, Brent's, Nelder-Mead, **Simulated Annealing**, **Particle Swarm Optimization** [source: https://github.com/argmin-rs/argmin/blob/main/README.md | argmin README algorithms list | independent].
- argmin's gravity is **continuous nonlinear optimisation**. The combinatorial / discrete fit is limited to its SA implementation. README does not claim combinatorial-scheduling competence [source: https://github.com/argmin-rs/argmin/blob/main/README.md | argmin README | independent].
- Hosts an external solver compatibility layer (cobyla, egobox-ego) but neither targets discrete scheduling [source: https://github.com/argmin-rs/argmin/blob/main/README.md | argmin README "External solvers" section | independent].

### localsearch (`lucidfrontier45/localsearch`)

- Last push: 2026-02-19, 15 stars, Apache-2.0 [source: https://api.github.com/repos/lucidfrontier45/localsearch | GitHub repo metadata for lucidfrontier45/localsearch | independent].
- Latest version: 0.24.0, requires Rust 1.92+ [source: https://github.com/lucidfrontier45/localsearch/blob/main/README.md | localsearch README | independent].
- Algorithm list (13+ all parallelised with Rayon): Random Search, Hill Climbing, Epsilon-Greedy, Metropolis, **Tabu Search**, **Great Deluge**, Simulated Annealing, Adaptive Annealing, Logistic Annealing, Relative Annealing, Tsallis Relative Annealing, Population Annealing, Parallel Tempering [source: https://github.com/lucidfrontier45/localsearch/blob/main/README.md | localsearch README features section | independent].
- API shape: implement the `OptModel` trait for the user's problem, then pick an `optim::*Optimizer`. The TSP-shaped quickstart in the README uses `OptModel<SolutionType = Vec<f64>, ScoreType = NotNan<f64>>` [source: https://github.com/lucidfrontier45/localsearch/blob/main/README.md | localsearch README quickstart | independent].
- Notable absence: **no LAHC** and **no LNS** in the listed algorithm set. The current Klassenzeit Rust LAHC backend would not be replaced by `localsearch`; only the SA/Tabu/Great-Deluge variants would be additive.

### genevo (`innoave/genevo`)

- Last push: 2024-02-10, 186 stars [source: https://api.github.com/repos/innoave/genevo | GitHub repo metadata for innoave/genevo | independent].
- License: `NOASSERTION` per GitHub (LICENSE file present but not a recognised SPDX) [source: https://api.github.com/repos/innoave/genevo | GitHub repo metadata for innoave/genevo | independent].
- **Borderline-stale**: ~15 months without a push as of 2026-05-08, just outside the 12-month window. Treat as out unless the licence and dormancy can be re-evaluated against project tolerance.

### oxigen (`Martin1887/oxigen`)

- Last push: 2021-07-11, 185 stars, MPL-2.0 [source: https://api.github.com/repos/Martin1887/oxigen | GitHub repo metadata for Martin1887/oxigen | independent].
- **Out of scope**: ~5 years stale.

### metaheuRUSTics (`aryashah2k/metaheuRUSTics`, crate `metaheurustics-rs`)

- Last push: 2025-01-19, 28 stars, MIT [source: https://api.github.com/repos/aryashah2k/metaheuRUSTics | GitHub repo metadata for aryashah2k/metaheuRUSTics | independent].
- **Borderline-stale**: ~16 months without a push as of 2026-05-08, outside the 12-month window.

### Adversarial: pure-metaheuristic-only is the wrong shape

The Klassenzeit production picture already runs LAHC plus FFD seed (`solver-bench` backends `Lahc`, `LahcRr`, `LahcKempe`, `LahcRrKempe`). Adding tabu / SA / Great-Deluge as a third backend would not change the topology — it would be a fifth LAHC variant. The interesting metaheuristic gap is **LNS** (large neighbourhood search) or **ALNS** (adaptive LNS), and there is **no maintained generic Rust ALNS crate**: web search returned only a single project-specific implementation `lcmd65/shift-rostering-alns` [source: https://github.com/lcmd65/shift-rostering-alns | shift-rostering-alns repo | independent], and no widely-adopted `alns` crate exists on crates.io [author estimate: based on absence in WebSearch results].

---

## SQ5: Rust MIP wrappers

### good_lp (`rust-or/good_lp`)

- Last push: 2026-04-07; 438 stars; MIT [source: https://api.github.com/repos/rust-or/good_lp | GitHub repo metadata for rust-or/good_lp | independent].
- Latest release: v1.14.1 (2025-11-12) [source: https://api.github.com/repos/rust-or/good_lp/releases | good_lp releases | independent]. `good_lp` 1.15.x is also referenced in package metadata [source: https://lib.rs/crates/good_lp | lib.rs/crates/good_lp | independent].
- Backend matrix (from the README):

| feature | integer vars | no C compiler | no extra runtime libs | claimed-fast | WASM |
|---------|------|------|------|------|------|
| `coin_cbc` | yes | yes | **no** (needs CBC dynamic lib) | yes | no |
| `highs` | yes | **no** (needs C/C++ compiler) | yes (statically linked, but distro deps may apply) | yes | no |
| `lpsolve` | yes | no | yes | no | no |
| `microlp` | yes | yes | yes | **no** | yes |
| `lp-solvers` | yes | yes | yes | no | no |
| `scip` (`scip_bundled` opt) | yes | yes | yes | yes | no |
| `cplex-rs` | yes | no | yes | yes | no |
| `clarabel` | **no** | yes | yes | yes | yes |

[source: https://github.com/rust-or/good_lp/blob/main/README.md | good_lp README solver matrix | independent]

- Single user-facing API; you swap solvers via Cargo features (`features = ["highs"]`, `default-features = false`).
- Modelling is **LP / MILP only**: "you can maximise `3 * x + y`, but not `3 * x * y`" [source: https://github.com/rust-or/good_lp/blob/main/README.md | good_lp README features and limitations | independent].

### highs / highs-sys (`rust-or/highs`, `rust-or/highs-sys`)

- `rust-or/highs` (safe wrapper): last push 2026-05-02, 31 stars, MIT [source: https://api.github.com/repos/rust-or/highs | GitHub repo metadata for rust-or/highs | independent]. No GitHub Releases tagged.
- `rust-or/highs-sys` (FFI): last push 2026-04-10, 17 stars, license `null` per GitHub API [source: https://api.github.com/repos/rust-or/highs-sys | GitHub repo metadata for rust-or/highs-sys | independent]. Latest version `highs-sys` 1.11.0 published 2025-06-07 [source: https://docs.rs/crate/highs-sys/latest | docs.rs highs-sys latest | independent], with `highs-sys` 1.12.1 referenced in docs.rs source listings [source: https://docs.rs/crate/highs-sys/latest/source/HiGHS/README.md | docs.rs highs-sys 1.12.1 source | independent].
- Build deps: "To build HiGHS, you need at least a C++ compiler and cmake. On Debian, these can be installed with: sudo apt install g++ cmake" [source: https://github.com/rust-or/highs-sys/blob/master/README.md | highs-sys README dependencies | independent]. Optional libz for compressed MPS, runtime needs C++ stdlib.
- HiGHS upstream is MIT [source: https://github.com/ERGO-Code/HiGHS | ERGO-Code/HiGHS | independent].

### microlp (`Specy/microlp`)

- Last push: 2026-02-19, 44 stars, Apache-2.0 [source: https://api.github.com/repos/Specy/microlp | GitHub repo metadata for Specy/microlp | independent].
- "Microlp is a fork of the archived minilp crate" [source: https://github.com/Specy/microlp | microlp README | practitioner].
- Pure Rust, no system dependencies.
- "Models with integer or binary variables are solved using a simple branch & bound method" [source: https://github.com/Specy/microlp | microlp README | practitioner].
- Maturity self-disclosure: "This is an early-stage project. Although the library is already quite powerful and fast, it will probably cycle, lose precision or panic on some harder problems" [source: https://github.com/Specy/microlp | microlp README | practitioner]. Treat as a fallback / WASM target, not a school-timetabling-grade MIP backend.

### russcip (`scipopt/russcip`)

- Last push: 2026-03-29, Apache-2.0 [source: https://api.github.com/repos/scipopt/russcip | GitHub repo metadata for scipopt/russcip | independent].
- Latest release: v0.9.1 (2025-08-26) [source: https://api.github.com/repos/scipopt/russcip/releases/latest | russcip latest release | independent].
- 774 commits, 16 releases, "currently actively developed" with bundled-SCIP feature available via `cargo add russcip --features bundled` [source: https://github.com/scipopt/russcip | russcip README | independent].
- SCIP itself transitioned to **Apache-2.0** (alternative LGPL dual) at SCIP 9.0 [source: https://arxiv.org/html/2402.17702v2 | SCIP Optimization Suite 9.0, Bestuzheva et al. | independent], and SCIP 10.0 (Nov 2025) confirms continued Apache-2.0 licensing [source: https://arxiv.org/html/2511.18580v1 | SCIP Optimization Suite 10.0, Hojny & Besançon | independent].
- Examples shipped with russcip: `bin_packing.rs`, `clique_separator.rs`, `create_and_solve.rs`, `cutting_stock.rs`, `knapsack.rs`, `most_infeasible_branching.rs`, `node_event_handler.rs`, `random_rounding.rs`, `tsp.rs` [source: https://api.github.com/repos/scipopt/russcip/contents/examples | russcip examples directory | independent]. No school-timetabling example, but bin packing + cutting stock are structural cousins.

### coin_cbc

- Default `good_lp` backend; the COIN-OR CBC project itself is EPL-1.0, requires `coinor-cbc coinor-libcbc-dev` system packages [source: https://github.com/rust-or/good_lp/blob/main/README.md | good_lp README cbc section | independent]. Multi-threading is unsafe unless the user enables `singlethread-cbc` or compiles CBC with `CBC_THREAD_SAFE` [source: https://github.com/rust-or/good_lp/blob/main/README.md | good_lp README cbc thread-safety note | independent].

### cp_sat (`KardinalAI/cp_sat`)

- Last push: 2026-04-01, 31 stars, Apache-2.0 [source: https://api.github.com/repos/KardinalAI/cp_sat | GitHub repo metadata for KardinalAI/cp_sat | independent].
- "Rust bindings to the Google CP-SAT constraint programming solver. To use this library, you need a C++ compiler and an installation of google or-tools library files" [source: https://github.com/KardinalAI/cp_sat/blob/main/README.md | cp_sat README | independent].
- **Not a third backend**: this would re-shape the existing CP-SAT backend (currently called from Python via `ortools` per ADR 0030) into a Rust path. Useful infrastructure note, not a candidate.

### Adversarial: MIP fit for school timetabling

CP-SAT vs MIP on timetabling-shaped problems: in a Bucknell University final exam scheduling case study, "in three of four semesters Gurobi obtained slightly better solutions than SCIP, and the final objective values were always within 4% of each other" [source: https://optimization-online.org/wp-content/uploads/2025/09/main-arXiv.pdf | Final Exam Scheduling at Bucknell University, Sep 2025 | independent]. This is exam scheduling (closely related to school timetabling), open-source SCIP within 4% of commercial Gurobi. No direct CP-SAT comparison surfaced in the reachable PDF excerpt.

The 2024 Integrated Healthcare Timetabling Competition (ihtc-2024) third-place hybrid uses **Gurobi (MIP) + Google OR-Tools (CP) + custom Simulated Annealing** in three sequential phases [source: https://arxiv.org/abs/2511.04685 | A hybrid solution approach for the Integrated Healthcare Timetabling Competition 2024 | independent] [source: https://arxiv.org/pdf/2511.04685 | same paper, PDF body | independent]. The pattern (MIP + CP + SA) is the canonical hybrid; **good_lp + Pumpkin + LAHC** would be the OSS, no-Java, Python-or-Rust analogue, but no published evidence of that exact combination on school timetabling exists in the surfaced literature [author estimate].

---

## SQ6: Integration cost — embedding in `solver-core`

### Pumpkin

- Cargo: `cargo add pumpkin-solver` (single dependency, pure Rust, no system deps) [source: https://github.com/ConSol-Lab/Pumpkin/blob/main/README.md | Pumpkin README - Building from Source | independent]. **Lowest integration cost** of any non-LAHC engine surveyed.
- The Rust `Solver` API is direct: `let mut solver = Solver::default(); let x = solver.new_bounded_integer(...);` [source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/ | docs.rs landing page for pumpkin_solver | independent]. Translates directly to a `BenchBackend::Pumpkin` enum variant.
- No vendored C++; no separate runtime; cross-compiles like any pure-Rust crate.

### good_lp + HiGHS feature

- Adds C++ compiler + cmake to build environment. Statically linked HiGHS at runtime, but "highs itself is statically linked and does not require manual installation. However, on some systems, you may have to install dependencies of highs itself" [source: https://github.com/rust-or/good_lp/blob/main/README.md | good_lp README highs note | independent].
- For the Klassenzeit CI runner (already builds Rust toolchain + has g++ for solver-py via maturin), this is incremental complexity but not a new class of dependency [author estimate].

### good_lp + microlp feature

- Pure Rust, but "early-stage project... will probably cycle, lose precision or panic on some harder problems" [source: https://github.com/Specy/microlp | microlp README | practitioner]. Acceptable as a build-everywhere fallback, not as a primary backend.

### russcip with `bundled` feature

- Bundled binary option avoids system SCIP dependency; SCIP itself is large (Apache-2.0). Build cost: pulls SCIP C source/binary at build time; runtime: native shared library. Cross-platform binary distribution complexity is non-zero [author estimate: SCIP is several MB of binary, larger than HiGHS or pumpkin-solver].

### SAT engines (splr, batsat, rustsat-*)

- Pure Rust crates; integration cost in cargo terms is trivial. The expensive part is the **encoding layer**: turning lessons-x-rooms-x-periods plus soft-constraint costs into CNF/WCNF. This dwarfs the binding cost [author estimate].

### localsearch

- Pure Rust, requires Rust 1.92+ [source: https://github.com/lucidfrontier45/localsearch/blob/main/README.md | localsearch README requirements | independent]. Minimal integration cost; high re-implementation cost since LAHC is not provided and the user must implement a domain-specific `OptModel`.

### Compatibility with `solver-bench`

The existing `BenchBackend` enum in `solver/solver-bench/src/main.rs` dispatches Rust-side variants and a Python CP-SAT shim. Adding `BenchBackend::Pumpkin` is the lowest-friction shape: it stays Rust-only, scores via the existing Rust scorer, no `klassenzeit_solver` Python module required. By contrast, any Python-side option (NuCS, py-z3, pumpkin-py) would need a Python peer module mirroring `klassenzeit_solver/cpsat.py` per ADR 0030 precedent [source: file:///home/pascal/Code/Klassenzeit/docs/research/2026-05-08-third-solver-backend-candidates/brief.md | Research brief reference state | author].

---

## Cross-cutting evidence: comparative position vs CP-SAT (current production candidate)

- MiniZinc Challenge 2025: OR-Tools CP-SAT swept gold in Fixed Search, Free Search, Parallel, **and** Local Search; Pumpkin shared bronze in Fixed Search; no other Rust-native CP solver placed [source: https://www.minizinc.org/challenge/2025/results/ | MiniZinc Challenge 2025 Results | independent].
- "Exact methods based on integer programming, maxSAT, and constraint programming have proven to be very effective for XHSTT" — explicit CP / MIP / MaxSAT all viable for high school timetabling [source: https://www.researchgate.net/publication/345191580_Optimizing_Student_Course_Preferences_in_School_Timetabling | Optimizing Student Course Preferences in School Timetabling | independent].
- "Open source solvers like SCIP and HiGHS are available, though they are often not as powerful as commercial solvers" — generic positioning that frames OSS MIP as a tier behind Gurobi/CPLEX, not behind CP-SAT specifically [source: https://d-krupke.github.io/cpsat-primer/ | The CP-SAT Primer, Krupke | practitioner].
- ViolationLS (2024): a Constraint-Based Local Search wrapper that "improves performance" when integrated into CP-SAT's parallel portfolio [source: https://dl.acm.org/doi/10.1007/978-3-031-60597-0_16 | ViolationLS: Constraint-Based Local Search in CP-SAT, CPAIOR 2024 | independent]. Frames hybrid LS-inside-CP as the live frontier when CP plateaus.
- For exam scheduling specifically, "in three of four semesters Gurobi obtained slightly better solutions than SCIP, and the final objective values were always within 4% of each other" [source: https://optimization-online.org/wp-content/uploads/2025/09/main-arXiv.pdf | Bucknell exam scheduling case study | independent].

## Cross-cutting evidence: Java / commercial competitors deliberately scoped out

- FET (Free Timetabling Software) latest: 7.8.5 released 2026-04-11, AGPL v3, C++/Qt [source: https://en.wikipedia.org/wiki/FET_(timetabling_software) | FET Wikipedia | independent]. AGPL is incompatible with the brief's MIT licensing for Klassenzeit (yellow flag flagged in brief). Architecturally an executable, not a library; no documented "fork as library" public path.
- Hexaly (formerly LocalSolver): commercial; "Any commercial use of Trial or Academic licenses is strictly prohibited" [source: https://www.hexaly.com/pricing | Hexaly Pricing | vendor]. Out per the brief's commercial-licence exclusion.
- Chuffed: pioneering LCG solver, C++, Pumpkin's nearest-relative; no published 2025 head-to-head in surfaced results.

---

## Summary table (rust-native-solvers cluster only, restated for brainstorm input)

| Candidate | Solver class | Last push | Last release | License | Python 3.14 wheels | School-TT fit signal |
|---|---|---|---|---|---|---|
| Pumpkin | CP / LCG | 2026-05-06 [source: https://api.github.com/repos/ConSol-Lab/Pumpkin/commits | GitHub commits API | independent] | 0.3.0 (2026-02-11) [source: https://api.github.com/repos/ConSol-Lab/Pumpkin/releases | GitHub releases API | independent] | Apache-2.0 OR MIT [source: https://github.com/ConSol-Lab/Pumpkin | Pumpkin README | independent] | yes (cp314 manylinux) [source: https://pypi.org/pypi/pumpkin-solver/json | PyPI JSON | independent] | cumulative + disjunctive constraints; bronze MZN'25 fixed; RCPSP improvements |
| good_lp + highs | MIP via HiGHS | 2026-05-02 (highs) [source: https://api.github.com/repos/rust-or/highs | GitHub repo API | independent] | 1.14.1 (2025-11-12) [source: https://api.github.com/repos/rust-or/good_lp/releases | GitHub releases | independent] | MIT (good_lp) + MIT (HiGHS) [source: https://github.com/ERGO-Code/HiGHS | ERGO-Code/HiGHS | independent] | n/a (Rust crate; HiGHS has separate Python via highspy) | proven on exam scheduling within 4% of Gurobi [source: https://optimization-online.org/wp-content/uploads/2025/09/main-arXiv.pdf | Bucknell paper | independent] |
| good_lp + scip via russcip | MIP via SCIP | 2026-03-29 (russcip) [source: https://api.github.com/repos/scipopt/russcip | GitHub repo API | independent] | russcip 0.9.1 (2025-08-26) [source: https://api.github.com/repos/scipopt/russcip/releases/latest | russcip release | independent] | Apache-2.0 (russcip + SCIP since 9.0) [source: https://arxiv.org/html/2402.17702v2 | SCIP 9.0 paper | independent] | n/a (Rust); PySCIPOpt separate | mature MIP; SCIP 10.0 cuts production-grade |
| splr | SAT (CDCL) | 2026-05-07 [source: https://api.github.com/repos/shnarazk/splr | GitHub repo API | independent] | crates.io 0.17.x [source: https://crates.io/crates/splr | crates.io splr | practitioner] | MPL-2.0 [source: https://api.github.com/repos/shnarazk/splr/contents/LICENSE | splr LICENSE | independent] | n/a | requires CNF re-encoding; MaxSAT-LNS literature exists for XHSTT |
| rustsat (kissat / cadical / minisat / glucose / batsat) | SAT (multiple) | 2026-05-03 [source: https://api.github.com/repos/chrjabs/rustsat | GitHub repo API | independent] | 0.7.5 (2026-01-30) [source: https://api.github.com/repos/chrjabs/rustsat/releases | rustsat releases | independent] | MIT [source: https://api.github.com/repos/chrjabs/rustsat | GitHub repo API | independent] | n/a | same encoding cost as splr; broader engine choice |
| microlp | MIP (pure Rust) | 2026-02-19 [source: https://api.github.com/repos/Specy/microlp | GitHub repo API | independent] | n/a | Apache-2.0 [source: https://api.github.com/repos/Specy/microlp | GitHub repo API | independent] | n/a | self-described "early-stage", panics on hard problems |
| localsearch | metaheuristic (SA, Tabu, GD, etc.) | 2026-02-19 [source: https://api.github.com/repos/lucidfrontier45/localsearch | GitHub repo API | independent] | 0.24.0 [source: https://github.com/lucidfrontier45/localsearch/blob/main/README.md | README | independent] | Apache-2.0 [source: https://api.github.com/repos/lucidfrontier45/localsearch | GitHub repo API | independent] | n/a | no LAHC; would be 5th LAHC-family variant, not a third class |
| argmin | numerical optimisation (continuous) | 2025-11-07 [source: https://api.github.com/repos/argmin-rs/argmin | GitHub repo API | independent] | 0.11.0 (2025-09-28) [source: https://api.github.com/repos/argmin-rs/argmin/releases | argmin releases | independent] | Apache-2.0 OR MIT [source: https://github.com/argmin-rs/argmin/blob/main/README.md | argmin README | independent] | n/a | continuous focus; SA + PSO present but combinatorial fit weak |
| copper | CP (Rust) | 2024-01-10 [source: https://api.github.com/repos/ffminus/copper | GitHub repo API | independent] | v0.1.0 (2024-01-06) [source: https://github.com/ffminus/copper | copper repo | independent] | MIT | n/a | **fails 12-month maintenance gate** |
| pcp / libpcp | CP (Rust) | 2023-11-22 [source: https://api.github.com/repos/ptal/pcp | GitHub repo API | independent] | n/a | Apache-2.0 [source: https://api.github.com/repos/ptal/pcp | GitHub repo API | independent] | n/a | **fails 12-month maintenance gate** |
| varisat | SAT (Rust) | 2022-11-02 [source: https://api.github.com/repos/jix/varisat | GitHub repo API | independent] | n/a | Apache-2.0 [source: https://api.github.com/repos/jix/varisat | GitHub repo API | independent] | n/a | **fails 12-month maintenance gate** |
| oxigen | GA (Rust) | 2021-07-11 [source: https://api.github.com/repos/Martin1887/oxigen | GitHub repo API | independent] | n/a | MPL-2.0 | n/a | **fails 12-month maintenance gate** |
| genevo | GA (Rust) | 2024-02-10 [source: https://api.github.com/repos/innoave/genevo | GitHub repo API | independent] | n/a | NOASSERTION (LICENSE present, not SPDX) | n/a | borderline-stale (~15 months no push) |
| metaheuRUSTics | metaheuristic suite | 2025-01-19 [source: https://api.github.com/repos/aryashah2k/metaheuRUSTics | GitHub repo API | independent] | n/a | MIT | n/a | borderline-stale (~16 months no push) |
| u-ras | scheduling lib | 2026-02-09 (archived) [source: https://api.github.com/repos/iyulab/U-RAS | GitHub repo API | independent] | n/a | MIT | n/a | **archived** |
| cp_sat (KardinalAI) | OR-Tools CP-SAT bindings | 2026-04-01 [source: https://api.github.com/repos/KardinalAI/cp_sat | GitHub repo API | independent] | n/a | Apache-2.0 | n/a | **not a third backend** — re-shapes existing CP-SAT path |
