# Maintained C / C++ MIP / LP solvers with Python bindings (HiGHS, GLPK, CBC, SCIP, others)

Research cluster: `mip-lp-backends`. Cluster scope per `plan.md`: maintained C / C++ MIP / LP engines with Python or Rust bindings; license; Python 3.14 status; school-timetabling fit; HiGHS-specific roadmap; Rust-side wrappers.

Note on cluster scope drift: while researching the explicit MIP/LP question (SQ1-SQ5), several adjacent candidates surfaced that the brief explicitly listed in other clusters (Pumpkin, MiniZinc, PySAT/MaxSAT, FET, PyChoco, CPMpy, ALNS). They are noted at the end under "Adjacent candidates surfaced during MIP/LP search" with the same source-citation discipline so the brainstorm has them in one place; full coverage of those other clusters would still need their own cluster runs.

## SQ1: Maintenance, latest release, license, governance

### HiGHS
HiGHS v1.14.0 was released April 6, 2026; v1.13.1 February 11, 2026; v1.13.0 February 4, 2026; v1.12.0 October 25, 2024; v1.11.0 June 6, 2024 [source: https://github.com/ERGO-Code/HiGHS/releases | Releases - ERGO-Code/HiGHS | independent]. License is MIT for the standard binaries; the optional HiPO interior-point component is Apache-2.0 [source: https://github.com/ERGO-Code/HiGHS/releases | Releases - ERGO-Code/HiGHS | independent]. HiGHS is developed at the University of Edinburgh / ERGO group; the third HiGHS workshop is scheduled for Edinburgh on 1 June 2026 alongside SIAM OP26 [source: https://highs.dev/ | HiGHS - High-performance parallel linear optimization software | vendor]. HiGHS handles LP, MIP, and (since v1.14.0) convex QP [source: https://github.com/ERGO-Code/HiGHS/releases | Releases - ERGO-Code/HiGHS | independent].

### SCIP
SCIP 9.0 (February 2024) and SCIP 10.0 (November 2025) ship under the Apache-2.0 license; the suite has been Apache-2.0 since SCIP 8.0.3 [source: https://github.com/scipopt/PySCIPOpt | scipopt/PySCIPOpt README | independent] [source: https://arxiv.org/html/2511.18580v1 | The SCIP Optimization Suite 10.0 | independent]. SCIP 10 adds an "Exact mode" for solving rational MILPs without floating-point precision, an IIS (irreducible infeasible system) finder, four new event types (`TYPECHANGED`, `IMPLTYPECHANGED`, `DUALBOUNDIMPROVED`, `GAPUPDATED`), and `writeStatisticsJson` [source: https://github.com/scipopt/PySCIPOpt/blob/master/RELEASE.md | PySCIPOpt RELEASE.md | independent]. Governed by Zuse Institute Berlin (ZIB) [source: https://pyscipopt.readthedocs.io/_/downloads/en/stable/pdf/ | PySCIPOpt docs PDF | independent].

### COIN-OR CBC
Cbc 2.10.13 was released March 11, 2026; 2.10.12 August 20, 2024; 2.10.11 October 25, 2023; 2.10.10 April 18, 2023; 2.10.9 April 12, 2023 [source: https://github.com/coin-or/Cbc/releases | Releases - coin-or/Cbc | independent]. Recent releases focus on compiler-warning fixes and "unused-but-set variables", consistent with maintenance-mode rather than feature development [source: https://github.com/coin-or/Cbc/releases | Releases - coin-or/Cbc | independent]. Cbc is governed by John Forrest, Ted Ralphs, Stefan Vigerske, Haroldo Gambini Santos and the rest of the Cbc team [source: https://www.coin-or.org/Cbc/ | CBC User Guide | independent]. CBC is a COIN-OR project; the canonical license for the project is the Eclipse Public License (EPL) per its hosting under coin-or [source: https://github.com/coin-or/Cbc | coin-or/Cbc | independent].

### GLPK
GLPK 5.0 was released December 16, 2020 with the copyright transferred to the Free Software Foundation and several routines disabled to fix licensing problems [source: https://lists.gnu.org/archive/html/info-gnu/2020-12/msg00007.html | glpk 5.0 release information | independent]. The GNU Project page lists Andrew Makhorin as maintainer; the page itself is dated 2012 [source: https://www.gnu.org/software/glpk/ | GLPK - GNU Project | independent]. License is GPL [source: https://en.wikipedia.org/wiki/GNU_Linear_Programming_Kit | GNU Linear Programming Kit - Wikipedia | independent]. No 5.x release after 5.0 (2020) was located in five years of search; this is a yellow flag for active maintenance.

### Other open-source MIP/LP engines
The good_lp Rust crate enumerates eight backends (CBC default, HiGHS, microlp, lpsolve, lp-solvers, SCIP, CPLEX, Clarabel), where Clarabel is "free Apache 2.0 ... written in Rust" but does not support integer variables [source: https://docs.rs/good_lp/latest/good_lp/solvers/index.html | good_lp::solvers - Rust | independent].

## SQ2: Python binding quality and Python 3.14 support

### highspy
highspy 1.14.0 was released April 6, 2026 with wheels for CPython 3.8 through 3.14 (cp314-cp314 manylinux i686, x86_64, aarch64, and win_amd64) [source: https://pypi.org/project/highspy/ | highspy - PyPI | independent]. License: MIT. The HiGHS C++ library no longer needs to be installed separately; only `numpy` is a runtime dep [source: https://pypi.org/project/highspy/ | highspy - PyPI | independent]. Python 3.14 wheels were added in v1.13.0 (February 2026) [source: https://github.com/ERGO-Code/HiGHS/releases | Releases - ERGO-Code/HiGHS | independent]. highspy is "a thin set of pybind11 wrappers to HiGHS" [source: https://pypi.org/project/highspy/ | highspy - PyPI | independent].

### PySCIPOpt
PySCIPOpt 6.1.0 was released February 5, 2026 with Python 3.14 wheels for manylinux and macOS, including a `cp314t` free-threaded variant [source: https://github.com/scipopt/PySCIPOpt/blob/master/RELEASE.md | PySCIPOpt RELEASE.md | independent]. Pre-built binary wheels are uploaded for Linux x86_64, Windows x86_64, macOS x86_64, and macOS Apple Silicon [source: https://pyscipopt.readthedocs.io/en/stable/install.html | PySCIPOpt Installation Guide | independent]. PySCIPOpt 6.1.0 raised the minimum NumPy requirement to 1.19.0 [source: https://github.com/scipopt/PySCIPOpt/releases | PySCIPOpt Releases | independent]. Apache-2.0 inherited from SCIP since v8.0.3 [source: https://github.com/scipopt/PySCIPOpt | scipopt/PySCIPOpt | independent].

### python-mip
python-mip 1.17.1 was released March 2, 2024; minimum Python is 3.10; tested with Python 3.10, 3.11, 3.12, 3.13 and PyPy 3.11; CBC binaries are now distributed via the `cbcbox` PyPI package; HiGHS solver support was added in v1.16.0-pre and v1.17 [source: https://github.com/coin-or/python-mip/releases | Releases - coin-or/python-mip | independent]. License: EPL-2.0 [source: https://github.com/coin-or/python-mip | coin-or/python-mip | independent]. Python 3.14 is not listed as tested as of the v1.17.1 release notes [source: https://github.com/coin-or/python-mip/releases | Releases - coin-or/python-mip | independent]. Maintained by Haroldo Santos (UFOP) and Sebastian Heger; HiGHS integration led by Robert Schwarz [source: https://github.com/coin-or/python-mip/releases | Releases - coin-or/python-mip | independent].

### z3-solver (SMT, surfaced for completeness)
z3-solver 4.16.0.0 was uploaded February 19, 2026 [source: https://pypi.org/project/z3-solver/ | z3-solver - PyPI | independent]. Microsoft Research project; MIT license [source: https://github.com/Z3Prover/z3 | Z3Prover/z3 | independent]. Python 3.14 wheel availability not explicitly confirmed in the search results.

### PySAT
python-sat 1.9.dev2 has uploads dated March 5, 2026 with Python 3.14 support [source: https://pysathq.github.io/updates/ | PySAT - Updates | independent]. Bundles CaDiCaL (rel-1.0.3, rel-1.5.3, rel-1.9.5) and Kissat (rel-4.0.4); the Kissat wrapper does not support incrementality [source: https://pysathq.github.io/updates/ | PySAT - Updates | independent]. The bundled RC2 MaxSAT solver "participated in the MaxSAT Evaluations 2018 and 2019 where, surprisingly, it was ranked first in two complete categories: unweighted and weighted" [source: https://pysathq.github.io/docs/html/api/examples/rc2.html | RC2 MaxSAT solver - PySAT | independent].

### swiglpk / GLPK Python
GLPK has Python bindings via the `glpk` PyPI package, but the upstream GLPK project itself has had no 5.x point release since GLPK 5.0 in December 2020 [source: https://lists.gnu.org/archive/html/info-gnu/2020-12/msg00007.html | glpk 5.0 release information | independent] [source: https://pypi.org/project/glpk/ | glpk - PyPI | independent].

## SQ3: How well does pure MIP/LP fit school timetabling vs CP/SAT?

### MIP is the dominant approach in the literature
Across 95 integer programming-based university-timetabling models (1990 to 2023), the most-used solvers are CPLEX (47), Gurobi (11), Lingo (5), Open Solver (4), C++ GLPK (4), AIMMS (2), GAMS (2), XPRESS (2), CELCAT (1), AMPL (1), and Google OR-Tools CP-SAT (1) [source: https://www.mdpi.com/2079-3197/13/1/10 | From Integer Programming to Machine Learning ... University Timetabling | independent].

### CP-SAT vs MIP qualitative claims
On the OR-Tools maintainer's recommendation: "CP-SAT preferred if you can run more than 8 cores and if the problem does not need continuous variables, SCIP otherwise" [source: https://github.com/google/or-tools/discussions/3969 | MIP solver choice 2023 | practitioner]. From the same maintainer in another talk: on the MiniZinc benchmark, "Gurobi is considered slightly better on pure linear problems, while CP-SAT is better on all CP problems"; "On CP problems, CP-SAT beats Gurobi"; "On linear integer problems, CP-SAT beats SCIP, is not far from CPLEX, and sometimes wins against Gurobi, but not often" [source: https://egon.cheme.cmu.edu/ewo/docs/CP-SAT%20and%20OR-Tools.pdf | CP-SAT and OR-Tools - Laurent Perron | practitioner].

### CP-SAT is competitive on combinatorial scheduling
CP-SAT has interval and `add_no_overlap` / `add_cumulative` global constraints suited to scheduling; "the cumulative constraint enforces that for all t, the sum of demands for overlapping intervals must not exceed the capacity" [source: https://github.com/google/or-tools/blob/stable/ortools/sat/docs/scheduling.md | OR-Tools scheduling docs | vendor]. CP-SAT is "competitive or better than state of the art on academic benchmarks for scheduling, and is better than commercial solvers on small to medium scheduling problems" [source: https://schedulingseminar.com/presentations/SchedulingSeminar_LaurentPerron.pdf | CP-SAT for scheduling - Laurent Perron | vendor].

### Open-source MIP gap to commercial
"For MIP problems, open source solvers like HiGHS, CBC, and SCIP perform about the same, while commercial solvers (CPLEX, XPRESS, and Gurobi) are about two orders of magnitude faster" [source: https://github.com/ERGO-Code/HiGHS/discussions/1683 | HiGHS Discussion #1683 | practitioner]. On Mittelmann's benchmarks the gap from HiGHS to Gurobi is "about one order of magnitude, and not much more for SCIP and Cbc" [source: https://github.com/ERGO-Code/HiGHS/discussions/1683 | HiGHS Discussion #1683 | practitioner]. On the Mittelmann MILP benchmark (April 27, 2026 results, 240 instances, 2-hour time limit, AMD Ryzen 9 5900X / 12 cores / 128 GB), COPT scaled-geometric-mean ratio is 1.0 (baseline), HiGHS is 7.36; instance-solve counts: COPT 219/240 (91%), Optverse 210 (88%), XSMOO 174 (73%), HiGHS 162 (68%), SCIP 136-150 (57-63%) [source: https://plato.asu.edu/ftp/milp.html | Mittelmann MILP benchmark | independent].

### Recent timetabling competition pattern
ITC 2019 was won by a "parallelized matheuristic" by Mikkelsen and Holm using a graph-based MIP model with a fix-and-optimize matheuristic; the full MIP model was used to compute lower bounds [source: https://orbit.dtu.dk/en/publications/winning-the-international-timetabling-competition-2019/ | Winning the International Timetabling Competition 2019 - DTU | independent] [source: https://www.itc2019.org/papers/itc2019-holm.pdf | A MIP based approach for ITC 2019 | independent]. In the 2024 Integrated Healthcare Timetabling Competition, of 26 single-category teams there were "16 MILP, two CP and eight metaheuristic approaches"; 18 teams used Gurobi for MILP and four used CP-SAT (OR-Tools); the winner used MILP with Gurobi [source: https://www.sciencedirect.com/science/article/pii/S3050784725000157 | The Integrated Healthcare Timetabling competition 2024 | independent].

### MaxSAT-based timetabling has also won submissions
"A MaxSAT-based large neighborhood search algorithm has been proposed for high school timetabling" combining local search with MaxSAT-based LNS [source: https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | MaxSAT-based large neighborhood search for high school timetabling | independent]. UniCorT, an iterative university course timetabling tool, uses the TT-Open-WBO-Inc MaxSAT solver and was published in J. Scheduling 2022 [source: https://link.springer.com/article/10.1007/s10951-021-00695-6 | Introducing UniCorT - J. Scheduling 2022 | independent].

### State-of-the-art survey (Ceschia et al., 2022)
Ceschia, Di Gaspero, Schaerf, "Educational timetabling: Problems, benchmarks, and state-of-the-art results", European Journal of Operational Research, vol 308 pp 1-18 (2022/2023): identifies six "standard" formulations, reports best-known upper/lower bounds, and notes that "exact methods based on integer programming, MaxSAT and constraint programming have proven to be very effective for XHSTT" [source: https://www.sciencedirect.com/science/article/pii/S0377221722005641 | Educational timetabling: Problems, benchmarks, and state-of-the-art results | independent] [source: https://arxiv.org/abs/2201.07525 | arXiv:2201.07525 (preprint of the same paper) | independent].

## SQ4: HiGHS roadmap and CP-style features

HiGHS has no native CP-style global constraints in its core engine; its interface layer (the AMPL MP library) supports indicator constraints, logical or/forall, and a `numberof` operator that can be reformulated to MILP at the modeling layer [source: https://dev.ampl.com/solvers/highs/options.html | HiGHS Options - AMPL Resources | vendor].

The Feasibility Jump primal heuristic was added to HiGHS in v1.11.0 (June 2024) and is reported as designation "J" in solver output [source: https://github.com/ERGO-Code/HiGHS/releases | Releases - ERGO-Code/HiGHS | independent]. Feasibility Jump "won 1st place in the MIP 2022 Computational Competition" and "now runs by default on FICO Xpress Solver 9.0" [source: https://link.springer.com/article/10.1007/s12532-023-00234-8 | Feasibility Jump: an LP-free Lagrangian MIP heuristic - MPC | independent].

HiGHS has been "limited opportunities for exploiting parallel computing" with planned greater multi-core use; the funding proposal explicitly targets future commercial-funded paid support in MIP and parallelisation of tree search [source: https://highs.dev/assets/HiGHS_funding_proposal.pdf | Optimization solvers: the missing link - HiGHS funding proposal | vendor]. v1.12.0 added a HiPO factorisation-based interior point solver with multi-threaded capabilities; v1.13.0 added singleton column stuffing in MIP presolve and IIS detection; v1.14.0 extended HiPO to convex QP [source: https://github.com/ERGO-Code/HiGHS/releases | Releases - ERGO-Code/HiGHS | independent].

## SQ5: Rust crates wrapping these MIP/LP engines

### good_lp
good_lp 1.15.1 was released April 7, 2026 [source: https://crates.io/crates/good_lp/versions | good_lp versions - crates.io | independent]. License: MIT [source: https://docs.rs/crate/good_lp/latest | good_lp 1.15.1 - Docs.rs | independent]. Default backend is `coin_cbc`; backends behind feature flags include `highs`, `microlp`, `lpsolve`, `lp-solvers`, `scip`, `cplex-rs`, `clarabel` [source: https://docs.rs/good_lp/latest/good_lp/solvers/index.html | good_lp::solvers - Rust | independent]. Cargo invocation pattern: `good_lp = { version = "*", features = ["your solver feature name"], default-features = false }` [source: https://github.com/rust-or/good_lp | rust-or/good_lp | independent]. Build complexity for HiGHS: "you will need a C compiler, but you shouldn't have to install any additional library on linux (it depends only on the C++ standard library)"; HiGHS is statically linked from the `highs` crate [source: https://github.com/rust-or/good_lp | rust-or/good_lp | independent]. SCIP via good_lp has an opt-in `scip_bundled` feature for a precompiled binary [source: https://github.com/rust-or/good_lp | rust-or/good_lp | independent]. microlp is a "fork of minilp ... pure rust solver which works out of the box without installing anything else" but is "slower than other solvers"; the original minilp is unmaintained and good_lp uses microlp instead [source: https://docs.rs/good_lp | good_lp - Rust | independent].

### highs (Rust crate)
`highs` v2.1.0 wraps HiGHS via `highs-sys ^1.12.0`; license MIT; "Safe rust binding to the HiGHS linear programming solver"; supports both `RowProblem` and `ColProblem` build styles plus MIP [source: https://docs.rs/highs/latest/highs/ | highs - Rust | independent].

### russcip (Rust SCIP wrapper)
russcip v0.9.1 was released August 26, 2025; license Apache-2.0; SCIP can be vendored via `cargo add russcip --features bundled`; the project has 774 commits and 16 releases; maintained by Mohammed Ghannam under the scipopt org [source: https://github.com/scipopt/russcip | scipopt/russcip | independent]. The crate "exposes access to SCIP's C-API through the ffi module" [source: https://github.com/scipopt/russcip | scipopt/russcip | independent].

### coin_cbc / lp-solvers / clarabel
coin_cbc is good_lp's default backend; clarabel is "a free Apache 2.0 linear programming solver written in Rust that doesn't support integer variables but is fast and easy to install" [source: https://docs.rs/good_lp/latest/good_lp/solvers/index.html | good_lp::solvers - Rust | independent]. lp-solvers is a generic dispatcher to external LP/MIP CLI binaries [source: https://docs.rs/good_lp/latest/good_lp/solvers/index.html | good_lp::solvers - Rust | independent].

## Adjacent candidates surfaced during MIP/LP search

### Pumpkin (Rust LCG CP solver, TU Delft)
Pumpkin v0.3.0 was released February 11, 2026 on crates.io as `pumpkin-solver` [source: https://crates.io/crates/pumpkin-solver | pumpkin-solver - crates.io | independent]. Dual-licensed Apache-2.0 OR MIT [source: https://github.com/ConSol-Lab/Pumpkin | ConSol-Lab/Pumpkin | independent]. The repository has 594 commits on main; Python bindings are available as the `pumpkin-solver` PyPI package [source: https://github.com/ConSol-Lab/Pumpkin | ConSol-Lab/Pumpkin | independent]. PyPI package version 0.3.0, released February 11, 2026, with wheels for CPython 3.8 through 3.14 (including PyPy 3.9-3.11) on Windows, macOS (10.12+ x86-64; 11.0+ ARM64), and Linux (manylinux + musllinux); built with maturin 1.11.5, confirming PyO3 bindings [source: https://pypi.org/project/pumpkin-solver/ | pumpkin-solver - PyPI | independent]. Global constraints supported: cumulative, disjunctive (no-overlap / unary resource), element, all-different, plus arithmetic (linear, multiplication, division, max, min, abs) [source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/ | pumpkin_solver - Rust | independent]. The CP'24 paper "A Multi-Stage Proof Logging Framework to Certify the Correctness of CP Solvers" by Flippo, Sidorov, Marijnissen, Smits, Demirović describes Pumpkin's proof logging [source: https://github.com/ConSol-Lab/Pumpkin | ConSol-Lab/Pumpkin | independent]. CPMpy supports Pumpkin as a backend [source: https://cpmpy.readthedocs.io/en/latest/api/solvers/pumpkin.html | CPMpy pumpkin interface | independent].

### MiniZinc (model layer)
MiniZinc 2.9.7 was released April 30, 2026 [source: https://www.minizinc.org/downloads/ | MiniZinc Downloads | vendor]. minizinc-python 0.10.0 was released February 25, 2025; requires Python 3.8+; Python 3.14 not explicitly confirmed [source: https://github.com/MiniZinc/minizinc-python/releases | minizinc-python releases | independent].

### Gecode (CP, MIT)
Gecode 6.2.0 (April 2019) is the latest release on the GitHub releases page; main repo has 5,620 commits but the most recent tagged release is from 2019, which is a yellow flag for active maintenance under the Gecode/gecode upstream [source: https://github.com/Gecode/gecode | Gecode/gecode | independent]. Used by SAP in S/4HANA for Advanced Variant Configuration [source: https://community.sap.com/t5/enterprise-resource-planning-blog-posts-by-sap/sap-uses-gecode-an-award-winning-constraint-solver-in-s-4hana-for-advanced/ba-p/13370310 | SAP uses Gecode | vendor]. License MIT [source: https://en.wikipedia.org/wiki/Gecode | Gecode - Wikipedia | independent]. Python binding `gecode-python` is third-party with limited recent activity [source: https://pypi.org/project/gecode-python/ | gecode-python - PyPI | independent].

### CPMpy (Python CP modeling layer)
CPMpy 0.9.24 was released October 6, 2025; Apache-2.0 [source: https://cpmpy.readthedocs.io/_/downloads/en/latest/pdf/ | CPMpy Release 0.9.24 | independent]. Backends: OR-Tools (default, CP-SAT), IBM CP Optimizer, Choco, Glasgow GCS, Pumpkin, MiniZinc + solvers, plus Gurobi and CPLEX [source: https://github.com/CPMpy/cpmpy/blob/master/docs/solvers.md | cpmpy solvers.md | independent]. "CPMpy participated in both the 2024 and 2025 XCSP3 competition, twice making its solvers win 3 gold and 1 silver medal" [source: https://github.com/CPMpy/cpmpy | CPMpy/cpmpy | independent].

### PyChoco (GraalVM-native binding to Choco)
PyChoco 0.2.4 was released September 2025 with Choco-solver 4.10.18 bundled as a GraalVM native-image shared library; "the pychoco library uses a native-build of the original Java Choco-solver library, in the form of a shared library, which means that it can be used without any JVM" [source: https://github.com/chocoteam/pychoco | chocoteam/pychoco | independent]. License: BSD-3-Clause [source: https://github.com/chocoteam/pychoco | chocoteam/pychoco | independent]. Wheels for Python 3.6+ on Linux, Windows, macOS [source: https://github.com/chocoteam/pychoco | chocoteam/pychoco | independent]. Note: the integration model technically satisfies "no Java at runtime" since GraalVM AOT-compiles to a native shared library, but the build pipeline pulls Java; whether this counts as "no Java" under the brief's hard constraint is a judgement call.

### FET (timetabling-specific, AGPL)
FET 7.8.5 was released April 11, 2026; FET 7.0.0 was released January 20, 2025 with algorithm-improvement constraint handling [source: https://lalescu.ro/liviu/fet/news.html | FET News | independent]. License: GNU AGPL v3 [source: https://lalescu.ro/liviu/fet/ | FET - Free Timetabling Software | independent]. AGPL is a yellow flag per the brief's license policy. The algorithm "is heuristic, probably simulating the manual procedure of finding a timetable" and was discovered on June 24, 2007 (the project began on October 31, 2002 using a genetic algorithm) [source: https://lalescu.ro/liviu/fet/doc/en/generation-algorithm-description.html | FET Timetable Generation Algorithm | independent]. FET is GUI/CLI executable, not a library [source: https://lalescu.ro/liviu/fet/ | FET - Free Timetabling Software | independent].

### Tablix
Tablix has been "inactive since around 2009" with content "archived in a read-only form" [source: https://www.tablix.org/ | Tablix | independent]. Out of scope for the brief's "maintained" criterion.

### UniTime
UniTime is open source and Java-based; the constraint solver extensions are LGPL and the application is GPL [source: https://www.unitime.org/ | UniTime | independent]. Out of scope for the brief's "no Java" hard constraint.

### Hexaly (formerly LocalSolver)
Hexaly is commercial; "Academic licenses are FREE for educational and fundamental research purposes" but "Any commercial use of Trial or Academic licenses is strictly prohibited"; commercial licensing is "UNLIMITED model with flexible engagement periods of 3 or 12 months. Exact pricing requires contacting their sales team" [source: https://www.hexaly.com/pricing | Hexaly Pricing | vendor]. Out of scope for the brief's "no commercial-only" rule. YDUQS (Brazilian universities) use Hexaly for timetabling after benchmarking against Gurobi, CPLEX, CP Optimizer, and OR-Tools [source: https://www.hexaly.com/announcements/localsolver-10-0?redirect | Hexaly 10.0 announcement | vendor].

### ALNS (Python LNS framework)
alns v7.0.0 was released October 21, 2024; MIT license; 197 commits; 621 GitHub stars [source: https://github.com/N-Wouda/ALNS | N-Wouda/ALNS | independent]. JOSS publication: "ALNS: a Python implementation of the adaptive large neighbourhood search metaheuristic" [source: https://joss.theoj.org/papers/10.21105/joss.05028 | ALNS - JOSS | independent]. School-timetabling is not a documented use case in the project's README [source: https://github.com/N-Wouda/ALNS | N-Wouda/ALNS | independent]. Python 3.14 status: not explicitly confirmed in the repo content fetched.

### Rust SAT crates
RustSAT was presented at SAT 2025 [source: https://arxiv.org/html/2505.15221v1 | RustSAT: A Library For SAT Solving in Rust | independent]. Available crates wrap Kissat, CaDiCaL, MiniSat, Glucose; BatSat is a pure-Rust MiniSat reimplementation accessible via `rustsat-batsat` [source: https://docs.rs/rustsat/latest/rustsat/solvers/index.html | rustsat::solvers - Rust | independent]. `rustsat-batsat` had a recent update October 2025 [source: https://crates.io/crates/rustsat-batsat | rustsat-batsat - crates.io | independent]. `varisat` is a CDCL SAT solver in pure Rust available as library and CLI [source: https://github.com/jix/varisat | jix/varisat | independent]. `splr` is a modern Rust SAT solver based on Glucose 4.1 (latest version 0.17.0) [source: https://crates.io/crates/splr | splr - crates.io | independent].

### Copper (Rust CP)
Copper is "still quite early in its development", crates.io page "last updated in January 2024", MIT license; "limited variable types and constraints compared to mature solvers like Gecode or or-tools" [source: https://crates.io/crates/copper | copper - crates.io | independent]. Maintenance is a yellow flag.

### NuCS (Python CP)
NuCS is "a Python constraint programming library ... 100% written in Python", powered by Numpy and Numba; most recent GitHub activity August 2025; 53-55 stars [source: https://github.com/yangeorget/nucs | yangeorget/nucs | independent]. No Rust bindings.

## Counter-evidence and caveats

### CP-SAT may already capture the practical wins
On CP problems CP-SAT beats Gurobi/SCIP, and on linear-integer problems CP-SAT "beats SCIP, is not far from CPLEX, and sometimes wins against Gurobi, but not often" [source: https://egon.cheme.cmu.edu/ewo/docs/CP-SAT%20and%20OR-Tools.pdf | CP-SAT and OR-Tools - Laurent Perron | practitioner]. This suggests adding HiGHS or SCIP as a third backend, when CP-SAT is already present, may not produce a measurable timetabling-specific win on hard instances; the differentiator is more likely a different solver class (CP with LCG, MaxSAT, or matheuristic) than another LP-based engine.

### Small-instance regime where HiGHS competes
"For small and modest sized instances, HiGHS's performance is comparable with that of Gurobi"; commercial solvers pull ahead on larger instances [source: https://www.researchgate.net/figure/Comparison-of-open-source-solvers-HiGHS-GLPK-and-Cbc-to-commercial-solver-Gurobi-on_fig1_360478503 | Comparison of open source solvers HiGHS, GLPK and Cbc | independent].

### Real-world wins when MIP is part of the recipe
ITC 2019 was won by a parallel matheuristic built on a MIP model with fix-and-optimize over the MIP, plus MIP-based lower bounds [source: https://orbit.dtu.dk/en/publications/winning-the-international-timetabling-competition-2019/ | Winning ITC 2019 - DTU | independent]. The 2024 Integrated Healthcare Timetabling competition winner used MILP with Gurobi; commercial Gurobi was used by 18 of 26 single-category teams [source: https://www.sciencedirect.com/science/article/pii/S3050784725000157 | Integrated Healthcare Timetabling competition 2024 | independent]. This is an existence proof that MIP+matheuristic can beat metaheuristics on educational/healthcare timetabling, but the dominant successes use commercial Gurobi rather than HiGHS or SCIP.

### CBC velocity slowdown
Cbc 2.10.13 (March 2026) is the first 2.10.x point release since 2.10.12 in August 2024; the changelog focuses on compiler-warning fixes [source: https://github.com/coin-or/Cbc/releases | Releases - coin-or/Cbc | independent]. This is consistent with a stable, mature project but not active feature development.

### GLPK staleness
Five years between point releases (5.0 in December 2020, no 5.x successor located) and a project page last touched in 2012 [source: https://www.gnu.org/software/glpk/ | GLPK - GNU Project | independent] [source: https://lists.gnu.org/archive/html/info-gnu/2020-12/msg00007.html | glpk 5.0 release | independent] make GLPK a no-go under the brief's "maintained" criterion.

### Rust ecosystem caveat
good_lp is a modeling layer, not a solver: "the library is 'not a solver'—it provides an abstraction layer over existing solver implementations" [source: https://docs.rs/crate/good_lp/latest | good_lp 1.15.1 - Docs.rs | independent]. Picking good_lp still requires picking an underlying engine (HiGHS, CBC, SCIP, microlp), and that engine's build complexity dominates the integration cost.
