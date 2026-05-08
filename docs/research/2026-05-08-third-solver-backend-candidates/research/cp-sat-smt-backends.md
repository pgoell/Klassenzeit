# Cluster: CP / SAT / SMT Backends (non-Java)

Cluster slug: `cp-sat-smt-backends`. Generated 2026-05-08 for OPEN_THINGS item 56. Evidence-only notes; no synthesis.

---

## SQ1 — Gecode (C++): maintenance, license, Python and Rust access

### Maintenance status

- Latest GitHub release on the official repo is **Gecode 6.2.0**, dated **April 12, 2024** (per the GitHub releases page) `[source: https://github.com/Gecode/gecode/releases | Releases · Gecode/gecode | independent]`. Earlier 6.x point releases land in February 2024 (6.1.1) and October 2023 (6.1.0).
- A separate WebFetch summary of the same repo reported the "latest release shown is from 2019" with "limited recent activity" and "41 open issues and 11 pull requests" `[source: https://github.com/Gecode/gecode | GitHub - Gecode/gecode | independent]`. The two snapshots conflict; the releases page (2024 entries) is the authoritative source.
- A Guix patch series targets a development version "6.3.0" `[source: https://issues.guix.gnu.org/70087 | [PATCH 1/4] gnu: gecode: Update to development version 6.3.0 | independent]`, suggesting an unreleased 6.3 line in development.
- Debian package tracking shows Gecode 6.2.0-7 accepted into unstable on **February 11, 2025** (downstream packaging activity, not upstream release) `[source: https://tracker.debian.org/pkg/gecode | gecode - Debian Package Tracker | independent]`.

### License

- Gecode license is **MIT** `[source: https://github.com/Gecode/gecode | GitHub - Gecode/gecode | independent]`.

### Python binding

- The PyPI package `gecode-python` (Cython binding) was last released as **0.27 on March 27, 2012** `[source: https://pypi.org/project/gecode-python/ | gecode-python · PyPI | independent]`.
- Documentation on Libraries.io describes the binding's surface ("All constants used in Gecode are available with the same names... all gecode constraints are available except 'extensional'... half-reification supported since v4.0.0") `[source: https://libraries.io/pypi/gecode-python | gecode-python 0.27 on PyPI - Libraries.io | independent]`.
- No PyPI release for `gecode-python` since 2012; project description still labels it "a work in progress" with no recent activity.

### Rust binding

- No maintained `gecode-rs` crate surfaced under the standard search angles; only third-party FlatZinc bridges via MiniZinc.
- Indirect access exists via MiniZinc, which bundles Gecode as a backend (see SQ2).

### Competition presence

- Gecode entered MiniZinc Challenge 2024 as an organizer reference solver ("a C++ FD solver"), ineligible for prizes `[source: https://www.minizinc.org/challenge/2024/results/ | MiniZinc - Challenge 2024 Results | independent]`.

---

## SQ2 — MiniZinc as a portable layer

### MiniZinc release cadence

- MiniZinc 2.9.x is the current stable line. Latest release surfaced is **MiniZinc 2.9.7 on April 30, 2024** (with 2.9.6 on April 24, 2024 and 2.9.5 on January 23, 2024) `[source: https://github.com/MiniZinc/libminizinc/releases | MiniZinc/libminizinc Releases | independent]`. The 2.8.x line shows 2.8.7 on October 2, 2022 and 2.8.6 on September 26, 2022 (release dates as listed by the WebFetch summary).
- A 2024 OR-Tools issue references "MiniZinc 2.8.7" being current at that time `[source: https://github.com/google/or-tools/issues/4398 | fzn-cp-sat: Status UNKNOWN... | independent]`.

### Bundled solver list

- The MiniZinc binary bundle ships **Gecode, Chuffed, COIN-OR CBC, HiGHS, and OR-Tools CP-SAT** as built-in backends `[source: https://docs.minizinc.dev/en/stable/installation.html | 1.2. Installation — The MiniZinc Handbook | independent]`.

### Python binding (`minizinc-python` / pip `minizinc`)

- Latest tagged release **0.10.0 on February 25, 2024**, requires Python ≥ 3.8 and MiniZinc ≥ 2.6.0; the 0.10.0 release dropped Python 3.7 and renamed the `timeout` argument to `time_limit` `[source: https://github.com/MiniZinc/minizinc-python/releases | Releases · MiniZinc/minizinc-python | independent]`.
- The 0.10.0 wheel filename is `minizinc-0.10.0-py3-none-any.whl` (pure-Python, generic `py3` tag), which means it loads on any Python 3.x including 3.13 and 3.14, but there is no `cp314` ABI tag because the package is not C-extension-based; it shells out to a system MiniZinc executable `[source: https://pypi.org/project/minizinc/ | minizinc · PyPI | independent]`.
- The package requires the MiniZinc compiler to be installed separately from the Python wheel `[source: https://python.minizinc.dev/en/latest/getting_started.html | Getting Started — MiniZinc Python 0.10.0 documentation | independent]`.

### Performance evidence

- The MiniZinc Challenge 2024 results show **OR-Tools CP-SAT swept gold in Fixed, Free, and Parallel categories**; PicatSAT and Choco-CP placed silver/bronze; **Gecode and Chuffed were entered as organizer reference solvers (ineligible for medals); GCS not listed; Pumpkin entered competitively** `[source: https://www.minizinc.org/challenge/2024/results/ | MiniZinc - Challenge 2024 Results | independent]`.
- The 2024 Challenge benchmark set includes a `train-scheduling` problem ("a real minimization problem" using `all_different`, `cumulative`, `disjunctive`) — a scheduling-flavored instance `[source: https://www.minizinc.org/challenge/2024/results/ | MiniZinc - Challenge 2024 Results | independent]`.
- Pumpkin earned **Bronze in the 2025 MiniZinc Challenge fixed search track** per its GitHub page `[source: https://github.com/ConSol-Lab/Pumpkin | GitHub - ConSol-Lab/Pumpkin | independent]`.

### License

- The MiniZinc bundle carries multiple licenses; libminizinc itself is MPL-2.0 (per docs.minizinc.dev project metadata; not directly enumerated in the WebFetch result, listed here as the standard MPL-2.0 used by IDS / project pages).

### Rust binding

- No first-party Rust binding for MiniZinc surfaced. Direct integration would shell out to `minizinc` like the Python binding does.

---

## SQ3 — Z3 SMT (C++, MIT)

### Maintenance & release cadence

- Latest release on PyPI is **z3-solver 4.16.0.0, released February 19, 2026**, with prior 2025 releases at 4.15.4.0 (Oct 29, 2025), 4.15.3.0 (Aug 16, 2025), 4.15.1.0 (Jun 8, 2025), 4.15.0.0 (May 10, 2025), 4.14.0.0 (Feb 18, 2025) `[source: https://pypi.org/project/z3-solver/ | z3-solver · PyPI | independent]`.
- GitHub release page confirms **Z3 4.16.0** on **February 19, 2026** plus 4.15.x series in 2025 `[source: https://github.com/Z3Prover/z3/releases | Releases · Z3Prover/z3 | independent]`.

### License

- Z3 is **MIT licensed** `[source: https://github.com/Z3Prover/z3 | GitHub - Z3Prover/z3 | independent]`. The PyPI package metadata also lists "MIT License" `[source: https://pypi.org/project/z3-solver/ | z3-solver · PyPI | independent]`.

### Python binding

- The `z3-solver` PyPI package supports Python 3 generally; wheels carry the `py3` tag (no per-cp ABI tag in the WebFetch dump, suggesting generic abi3 wheels) `[source: https://pypi.org/project/z3-solver/ | z3-solver · PyPI | independent]`.
- The Python binding is exhaustive (officially supported by Microsoft Research; surfaces SMT-LIB2, Optimize, BitVec, Int, Real theories) `[source: https://github.com/Z3Prover/z3 | GitHub - Z3Prover/z3 | independent]`.

### Rust binding

- The community `z3` crate (under `prove-rs/z3.rs`) is actively maintained: **0.20.0 within 8 days of the search**, with a 1-2 month cadence, ~64 K downloads/month, used by 31 dependent crates `[source: https://crates.io/crates/z3 | z3 - crates.io: Rust Package Registry | independent]`. The low-level `z3-sys 0.10.4` was published **December 27, 2025** `[source: https://docs.rs/crate/z3-sys/latest | z3-sys 0.10.4 - Docs.rs | independent]`. The crate provides high-level + low-level Rust bindings on top of `z3-sys` `[source: https://github.com/prove-rs/z3.rs | GitHub - prove-rs/z3.rs | independent]`.

### Scheduling fit

- Z3's optimization layer (νZ) "comprises a MaxSMT module... implemented as a satellite theory solver in Z3's SMT core" `[source: https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/nbjorner-nuz.pdf | νZ - An Optimizing SMT Solver | vendor]`.
- νZ implements multiple MaxSAT engines (WMax, MaxRes, BCD2, MaxHS); the "basic approach to MaxSMT works remarkably well in many cases... but it falls flat on its face in most large scale benchmark applications circulating in the MaxSAT community", motivating MaxRes-based alternatives `[source: https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/nbjorner-nuz.pdf | νZ - An Optimizing SMT Solver | vendor]`.
- Z3 supports lexicographic, Pareto-front, and independent multi-objective optimization over integers, reals, and bit-vectors `[source: https://microsoft.github.io/z3guide/docs/optimization/intro/ | Introduction | Online Z3 Guide | vendor]`.
- For job-shop scheduling: "Z3's performance on job shop scheduling problems has been noted as remaining far from the performance of CPOPTIMIZER" and "users have observed order of magnitude speedups on the same formulations when switching from integer arithmetic to bit-vectors" `[source: https://news.ycombinator.com/item?id=21104748 | Hacker News - I have used the Z3 solver | practitioner]`.
- The Demirović-Musliu bitvector formulation reports that "Using Z3 with 24-hour timeout, the approach found feasible solutions in all instances except two, achieving optimal solutions for three instances (Brazil1, GreeceHighSchool, FinlandESchool)" out of 23 XHSTT instances modeled, and "overall performance remained 'not competitive' compared to state-of-the-art heuristic methods" `[source: https://pmc.ncbi.nlm.nih.gov/articles/PMC5411413/ | Modeling high school timetabling with bitvectors - PMC | independent]`.
- Demirović and Musliu used Z3 v4.4.2 with the wmax engine "because it is the only active solver that supports optimization over bitvectors", with 10-minute local-search and 24-hour SMT time limits `[source: https://www.ncbi.nlm.nih.gov/pmc/articles/PMC5411413/ | Modeling high school timetabling with bitvectors | independent]`.

### Adversarial / dissenting

- General SMT scaling caveats: "As formulas grow larger and more complex, solving time can explode exponentially. SMT solvers struggle with formulas having many quantifiers, nonlinear arithmetic, and deeply nested data structures or functions"; "Many SMT procedures depend on heuristics... the same problem can take seconds or hours depending on formulation, and minor syntactic changes may drastically affect performance" `[source: https://www.cs.umd.edu/class/fall2025/cmsc433/Solving_SAT_and_SMT_Problems_Using_Z3.html | 12 Solving SAT and SMT Problems Using Z3 | independent]`.
- A direct Z3 vs Gurobi comparison on rostering / scheduling found that "MILP solver generally performs better when the problem is highly constrained or infeasible, while the SMT solver performs better otherwise" `[source: https://www.researchgate.net/publication/329612297_SMT_Solvers_for_Job-Shop_Scheduling_Problems_Models_Comparison_and_Performance_Evaluation | SMT Solvers for Job-Shop Scheduling Problems | independent]`.

---

## SQ4 — PySAT / Kissat / CaDiCaL — SAT-encoded scheduling

### PySAT (`python-sat`)

- Latest version on PyPI is **1.9.dev2 on March 5, 2026**; PyPI ships CPython wheels for **3.8, 3.9, 3.10, 3.11, 3.12, 3.13, 3.14**; license **MIT** `[source: https://pypi.org/project/python-sat/ | python-sat · PyPI | independent]`.
- 1.8.dev releases throughout 2025 (1.8.dev23 in Sep 2025, 1.8.dev29 in Feb 2026, 1.8.dev30 in Feb 2026) `[source: https://libraries.io/pypi/python-sat | python-sat 1.8.dev14 on PyPI - Libraries.io | independent]`.
- Cardinality encodings supported: pairwise, sequential counter, sortnetwrk, cardnetwrk, bitwise, ladder, totalizer, mtotalizer, kmtotalizer, native; all C++ implementations behind a Python wrapper `[source: https://pysathq.github.io/docs/html/api/card.html | Cardinality encodings (pysat.card) — PySAT 1.9.dev2 documentation | independent]`.
- Treewidth caveat for cardinality encodings: "the naive encoding can increase the treewidth by the number of variables in the cardinality constraint" and "Adding a k-cardinality totalizer constraint to a formula with n variables increases the treewidth up to Ω(n)" `[source: https://www.cs.cmu.edu/~csd-phd-blog/2024/cardinality-constraints/ | CMU CSD PhD Blog - Encoding Cardinality Constraints in Automated Reasoning | independent]`.
- PySAT integrates "a number of state-of-the-art Boolean satisfiability (SAT) solvers and a few types of cardinality and pseudo-Boolean encodings" — list includes integration with SAT cores and the `aiger`, `approxmc`, `cryptosat`, `pblib` extras `[source: https://github.com/pysathq/pysat | GitHub - pysathq/pysat | independent]`.

### CaDiCaL

- Latest GitHub release **CaDiCaL 3.0.0 on December 23, 2025**, with 34 total releases; license **MIT** `[source: https://github.com/arminbiere/cadical | GitHub - arminbiere/cadical | independent]`.
- "CaDiCaL has begun to replace MiniSat in numerous applications, most prominently cvc5"; recommended citation is the CaDiCaL 2.0 CAV'24 tool paper `[source: https://link.springer.com/chapter/10.1007/978-3-031-65627-9_7 | CaDiCaL 2.0 | Springer Nature Link | independent]`.
- Available as command-line tool and library (`libcadical.a`); README does not mention Python or Rust bindings directly.
- Rust binding via `cadical` crate: "stand-alone crate that contains both the C++ source code of the CaDiCaL (version 1.9.5) incremental SAT solver together with its Rust binding... compiled and statically linked during the build process" `[source: https://github.com/mmaroti/cadical-rs | GitHub - mmaroti/cadical-rs | independent]`. Crate license follows CaDiCaL itself (MIT).
- SAT Competition 2025 main sequential UNSAT track: **CaDiCaL-SC2025 placed first (161 instances)**, Kissat-VSA second (160), AE-Kissat-bump third (159) `[source: https://satcompetition.github.io/2025/satcomp25slides.pdf | The Results of SAT Competition 2025 | independent]`.

### Kissat

- Kissat enters SAT Competition 2024 alongside CaDiCaL, Gimsatul, IsaSAT (per SAT Competition 2025 entry document) `[source: https://cca.informatik.uni-freiburg.de/papers/BiereFallerFleuryFroleyksPollitt-SAT-Competition-2025-solvers.pdf | CaDiCaL, Gimsatul, IsaSAT and Kissat Entering the SAT Competition 2025 | independent]`.
- Python wrapping is indirect: `passagemath-kissat` ships pip wheels containing the Kissat executable for CPython 3.11 / 3.12 on Linux / macOS `[source: https://pypi.org/project/passagemath-kissat/10.5.7/ | passagemath-kissat · PyPI | independent]`. PySAT also has an open issue requesting Kissat integration `[source: https://github.com/pysathq/pysat/issues/61 | Include kissat · Issue #61 · pysathq/pysat | independent]`.

### RustSAT

- RustSAT is a Rust SAT-prototyping library with **v0.7.5 on January 30, 2026**; license MIT; 158 total releases; supports CaDiCaL, Kissat, Glucose, MiniSat, plus IPASIR-compliant solvers `[source: https://github.com/chrjabs/rustsat | GitHub - chrjabs/rustsat | independent]`.
- Includes cardinality (Totalizer), pseudo-Boolean (GeneralizedTotalizer, BinaryAdder, DynamicPolyWatchdog) encodings; experimental Python bindings on PyPI under `rustsat`; MSRV 1.87.0 `[source: https://github.com/chrjabs/rustsat | GitHub - chrjabs/rustsat | independent]`.
- Published as the SAT 2025 paper "RustSAT: A Library for SAT Solving in Rust", offering "interfaces to various state-of-the-art SAT solvers available with a unified Rust API" `[source: https://arxiv.org/abs/2505.15221 | [2505.15221] RustSAT: A Library For SAT Solving in Rust | independent]`.
- Since the 0.7.0 release, "Totalizer and GeneralizedTotalizer encodings provide functionality for certifying the correctness of the generated CNF encoding by producing a proof in VeriPB format" `[source: https://arxiv.org/html/2505.15221v1 | RustSAT: A Library For SAT Solving in Rust | independent]`.

### MaxSAT / school-timetabling evidence

- MaxSAT-based LNS for high school timetabling: a 2017 paper "modified the open-source maxSAT solver Open-WBO to support an exhaustive insertion strategy. Using this algorithm, researchers managed to compute four new best known upper bounds" on XHSTT instances `[source: https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | MaxSAT-based large neighborhood search for high school timetabling - ScienceDirect | independent]`.
- "Recently exact methods based on integer programming, maxSAT and constraint programming have proven to be very effective for high school timetabling (XHSTT)" `[source: https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | MaxSAT-based large neighborhood search for high school timetabling | independent]`.
- ITC 2019 university course timetabling: "A MaxSAT-based solver could solve all instances by 2022, compared to 18 instances during the competition" — the MaxSAT-based approach overtook the competition winner with later refinements `[source: https://patatconference.org/patat2022/proceedings/PATAT_2022_paper_29.pdf | ITC 2019: Results Using the UniTime Solver | independent]`.
- ITC 2019 winner used a MIP-based matheuristic; refinement combined MaxSAT solving with local search `[source: https://link.springer.com/article/10.1007/s10951-023-00801-w | Real-world university course timetabling at the International Timetabling Competition 2019 | independent]`.
- UniCorT (Journal of Scheduling, 2022) is "an iterative university course timetabling tool with MaxSAT" using TT-Open-WBO-Inc; it solves timetabling iteratively with MaxSAT, addressing memory constraints via iterative MaxSAT calls `[source: https://link.springer.com/article/10.1007/s10951-021-00695-6 | Introducing UniCorT: an iterative university course timetabling tool with MaxSAT | independent]`.
- TT-Open-WBO-Inc backed the winners of MaxSAT Evaluation 2023 (NuWLS-c-2023) and 2024 (SPB-MaxSAT-c) in all four anytime categories `[source: https://github.com/alexander-nadel-academic/tt-open-wbo-inc/ | GitHub - alexander-nadel-academic/tt-open-wbo-inc | independent]`.
- No PyPI release surfaced for Open-WBO or TT-Open-WBO-Inc; they are released as C++ command-line tools.

---

## SQ5 — Other maintained CP engines beyond Gecode

### Choco (Java) and PyChoco (GraalVM native build)

- Choco-solver itself is **Java**, BSD-3-Clause, latest stable **6.0.0 on May 5, 2026** with 5.0.0-beta.1 (Feb 17, 2025) and 4.10.18 (Jan 27, 2025) `[source: https://github.com/chocoteam/choco-solver/releases | Releases · chocoteam/choco-solver | independent]`. Pure-Java; "explicitly requires JDK 8+ and Maven" `[source: https://github.com/chocoteam/choco-solver | GitHub - chocoteam/choco-solver | independent]`.
- **PyChoco** ships a "native-build of the original Java Choco-solver library, in the form of a shared library, which means that it can be used without any JVM. This native-build is created with GraalVM native-image tool" `[source: https://github.com/chocoteam/pychoco | GitHub - chocoteam/pychoco | independent]`.
- PyChoco latest is **0.2.4 on September 28, 2025**, BSD-3-Clause, supports Python ≥ 3.6, automatically built 64-bit wheels for Linux / Windows / macOS, Choco-solver 4.10.18 underlying `[source: https://github.com/chocoteam/pychoco | GitHub - chocoteam/pychoco | independent]`.
- Earlier docs say "Automatically built 64-bit wheels are available for Python 3.6, 3.7, 3.8, 3.9, and 3.10 on Linux, Windows and MacOSX" — Python 3.13/3.14 wheel availability not confirmed in the surfaced sources `[source: https://pychoco.readthedocs.io/en/latest/ | Pychoco — pychoco 0.1.1 documentation | independent]`.
- PyChoco published a JOSS paper in 2025: "pychoco: all-inclusive Python bindings for the Choco-solver constraint programming library" `[source: https://zenodo.org/records/17219306 | pychoco JOSS paper | independent]`.
- GraalVM native-image trade-off: "a Spring Boot microservice that took 3-4 seconds to start on the JVM now boots in under 100 milliseconds as a native image, while consuming 75% less memory at runtime" `[source: https://www.javacodegeeks.com/2026/02/graalvm-native-image-javas-answer-to-rusts-startup-speed.html | GraalVM Native Image | independent]`. GraalVM native-image cost: "may not achieve the same peak runtime performance as a well-optimized JVM JIT-compiled application" for long-running solvers `[source: https://www.graalvm.org/latest/reference-manual/native-image/optimizations-and-performance/ | Optimizations and Performance | vendor]`.
- Choco-solver "is used by the academy for teaching and research and by the industry to solve real-world problems, such as program verification, smart grid management, timetabling, scheduling and routing"; Cumulative is "replaced by state-of-the-art implementation of TimeTabling and OverloadChecking" `[source: https://choco-solver.org/overview/ | Overview of Choco-solver | independent]`.
- MiniZinc Challenge 2024: **Choco-solver CP-SAT silver in Fixed track**; **Choco-solver CP bronze in Parallel** `[source: https://www.minizinc.org/challenge/2024/results/ | MiniZinc - Challenge 2024 Results | independent]`.

### Pumpkin (Rust CP solver)

- Latest release **pumpkin-solver-v0.3.0 on February 11, 2026**; dual-licensed **Apache-2.0 / MIT**; 90.8% Rust, 594 commits, 74 stars, 28 forks `[source: https://github.com/ConSol-Lab/Pumpkin | GitHub - ConSol-Lab/Pumpkin | independent]`.
- Crate `pumpkin-solver 0.3.0` on crates.io updated "2 months ago" relative to a May 2026 search `[source: https://crates.io/crates/pumpkin-solver/0.1.3 | pumpkin-solver - crates.io | independent]`.
- Bindings: "Native Rust library via cargo, Python bindings (pumpkin-solver-py), Command-line interface supporting CNF/WCNF and FlatZinc, MiniZinc backend integration" `[source: https://github.com/ConSol-Lab/Pumpkin | GitHub - ConSol-Lab/Pumpkin | independent]`.
- Scheduling-relevant globals: Cumulative, Disjunctive (NoOverlap / Unary Resource), AllDifferent, Element, Linear inequalities `[source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/ | pumpkin_solver - Rust docs | independent]`.
- API surface: "high-level API with low-level access. Users interact through a Solver struct... declarative functions; Multiple solving modes (satisfaction, optimization, enumeration)... 100% of the crate is documented" `[source: https://docs.rs/pumpkin-solver/latest/pumpkin_solver/ | pumpkin_solver - Rust docs | independent]`.
- "Pumpkin has a FlatZinc frontend, but can also be used to solve SAT and MaxSAT problems... unique feature... can produce a certificate of unsatisfiability as part of the solving process without significant overhead" `[source: https://www.minizinc.org/challenge/2024/description_pumpkin.txt | description_pumpkin.txt | independent]`.
- "The solver is reasonably competitive, although depending on the problem, other state-of-the-art solvers may be better" — author commentary on competitiveness `[source: https://www.minizinc.org/challenge/2024/description_pumpkin.txt | description_pumpkin.txt | independent]`.
- Academic backing: CP 2024 paper "A Multi-Stage Proof Logging Framework to Certify the Correctness of CP Solvers", CP 2025 paper "Conflict Analysis Based on Cutting-Planes for Constraint Programming"; AAAI 2026 publication; Bronze medal in 2025 MiniZinc Challenge fixed search track `[source: https://github.com/ConSol-Lab/Pumpkin | GitHub - ConSol-Lab/Pumpkin | independent]`.
- Contributors at TU Delft ConSol Lab: Emir Demirović, Maarten Flippo, Imko Marijnissen, Konstantin Sidorov, Jeff Smits `[source: https://www.minizinc.org/challenge/2024/description_pumpkin.txt | description_pumpkin.txt | independent]`.
- Same lead author (Emir Demirović) co-authored the Z3-bitvector / MaxSAT XHSTT papers — direct connection to school-timetabling background `[source: https://dbai.tuwien.ac.at/staff/musliu/emird.pdf | SAT-based Approaches for the General High School Timetabling Problem PhD THESIS | independent]`.

### Glasgow Constraint Solver (GCS)

- C++ CP solver with proof logging (VeriPB 3.0 format); MIT license; 1,211 commits; **C++23 compiler required (GCC 13+, CMake 3.21+)** `[source: https://github.com/ciaranm/glasgow-constraint-solver | GitHub - ciaranm/glasgow-constraint-solver | independent]`.
- Python binding `gcspy` (separate repo `mmcilree/gcspy`): "primarily intended for use by the CPMpy modelling library", `pip3 install gcspy`, **MPL-2.0**, only 7 commits, 0 stars, no published releases on GitHub yet, "in active development rather than a stable release phase" `[source: https://github.com/mmcilree/gcspy | GitHub - mmcilree/gcspy | independent]`.
- Solver scope: "Constraint satisfaction problems; Constraint optimization with objective maximization/minimization; XCSP and MiniZinc input formats (though marked as 'extremely minimal')"; README itself states "this is a work in progress, with no stable API or design... the code that is here should mostly work, but there is a lot missing" `[source: https://github.com/ciaranm/glasgow-constraint-solver | GitHub - ciaranm/glasgow-constraint-solver | independent]`.

### CPMpy (Python modelling layer over CP/MIP/SMT/SAT solvers)

- Latest release **0.10.0 on January 19, 2026**; license Apache-2.0; requires Python ≥ 3.10 `[source: https://github.com/CPMpy/cpmpy | GitHub - CPMpy/cpmpy | independent]`.
- Supported solvers (per docs): CP — OR-Tools (default), IBM CP Optimizer, Choco (via PyChoco), **Glasgow GCS, Pumpkin**, MiniZinc + solvers; ILP — SCIP, Gurobi, CPLEX; SMT/SAT — Z3, PySAT; Other — Hexaly, Exact (PB), PySDD `[source: https://github.com/CPMpy/cpmpy | GitHub - CPMpy/cpmpy | independent]`.
- Includes Cumulative scheduling global; the docs show flexible job-shop scheduling examples with makespan and energy optimization `[source: https://cpmpy.readthedocs.io/en/latest/ | CPMpy: Constraint Programming and Modeling in Python — CPMpy 0.9.24 documentation | independent]`.
- "CPMpy participated in both the 2024 and 2025 XCSP3 competition, twice making its solvers win 3 gold and 1 silver medal" `[source: https://github.com/CPMpy/cpmpy | GitHub - CPMpy/cpmpy | independent]`.
- Maintained by Tias Guns and collaborators; ERC-funded `[source: https://pypi.org/project/cpmpy/ | cpmpy · PyPI | independent]`.

### NuCS (pure-Python CSP/COP)

- Latest release **v10.1.0 on April 11, 2026**; MIT license; 99.9% Python; 51 releases, 803 commits, 55 stars `[source: https://github.com/yangeorget/nucs | GitHub - yangeorget/nucs | independent]`.
- Performance via NumPy + Numba: "NuCS achieves performance similar to that of solvers written in Java or C/C++"; "Most propagators in NuCS are global (aka n-ary) and implement state-of-art propagation algorithms" `[source: https://github.com/yangeorget/nucs | GitHub - yangeorget/nucs | independent]`.
- Published reference benchmarks: 12-queens (14,200 solutions), BIBD optimization, Golomb ruler `[source: https://github.com/yangeorget/nucs | GitHub - yangeorget/nucs | independent]`. No school-timetabling case study surfaced in the search results.
- Dependencies: Python 3.x, NumPy 2.4.2, Numba 0.65; Numba support of Python 3.14 not confirmed in surfaced sources.

### copper (Rust CP)

- Rust crate, MIT license; "still quite early in its development and cannot rival with mature solvers like Gecode or or-tools"; "currently supports a limited number of variable types and constraints" `[source: https://docs.rs/copper/latest/copper/ | copper - Rust | independent]`.
- The crates.io WebFetch returned no detail; metadata most-recent date observed elsewhere is January 2024 (suggests dormancy relative to Pumpkin's cadence).

### cvc5 (SMT, C++)

- Latest release **cvc5-1.3.4 on May 7, 2026**; license **BSD 3-Clause**; 13,899 commits on main, 36 releases, active CI/CD `[source: https://github.com/cvc5/cvc5 | GitHub - cvc5/cvc5 | independent]`.
- Python API "built on top of cvc5's C++ API using Cython and makes all of cvc5's features accessible to Python users", with a higher-level Pythonic layer on top `[source: https://www-cs.stanford.edu/~preiner/publications/2022/BarbosaBBKLMMMN-TACAS22.pdf | cvc5: A Versatile and Industrial-Strength SMT Solver | independent]`.
- No first-party Rust binding surfaced; no scheduling-specific MaxSMT extensions surfaced in the search results.
- cvc5 has competed at SMT-COMP 2021 and 2022 `[source: https://www-cs.stanford.edu/~preiner/publications/2022/BarbosaBBKLMMMN-TACAS22.pdf | cvc5: A Versatile and Industrial-Strength SMT Solver | independent]`.

### Yuck (FlatZinc local search)

- Yuck is **Scala / JVM** — out of scope for this cluster (JVM excluded by brief). Won several silver and gold medals at past MiniZinc Challenges `[source: https://github.com/informarte/yuck | GitHub - informarte/yuck | independent]`.

### IBM CP Optimizer / Hexaly

- IBM CP Optimizer is **commercial** (CPMpy supports it as a backend, "license required") `[source: https://github.com/CPMpy/cpmpy | GitHub - CPMpy/cpmpy | independent]`.
- Hexaly is **commercial**; academic licenses are free and unlimited but commercial deployment requires a paid quote (no public free tier for production) `[source: https://www.hexaly.com/pricing | Pricing | Hexaly | vendor]`.
- Hexaly was used by YDUQS, "a leading Brazilian group of universities and schools", for timetabling and resource allocation after benchmarking against Gurobi, Cplex, CP Optimizer, OR-Tools `[source: https://www.hexaly.com/pricing | Pricing | Hexaly | vendor]`.
- Hexaly vs OR-Tools CP-SAT 9.14 / Gurobi / CPLEX / CP Optimizer on JSSP: "Hexaly's results after 10 minutes were 8.5% better than CP Optimizer's after 6 hours, while OR-Tools, Gurobi, and Cplex failed to deliver feasible solutions within 6 hours" on the very-large-scale benchmark `[source: https://www.hexaly.com/benchmarks/hexaly-vs-cp-optimizer-vs-or-tools-on-the-job-shop-scheduling-problem-jssp | Hexaly vs CP Optimizer vs OR-Tools on the Job Shop Scheduling Problem | vendor]`.

---

## Cross-cutting evidence: solver-class wins on school-timetabling-shaped problems

- Demirović-Musliu's bitvector SMT approach achieved feasible solutions in 21/23 modeled XHSTT instances and three optima (Brazil1, GreeceHighSchool, FinlandESchool) in 24-hour SMT runs; "overall performance remained 'not competitive' compared to state-of-the-art heuristic methods" `[source: https://pmc.ncbi.nlm.nih.gov/articles/PMC5411413/ | Modeling high school timetabling with bitvectors - PMC | independent]`.
- "Exact methods based on integer programming, maxSAT and constraint programming have proven to be very effective for high school timetabling (XHSTT)" — survey-level claim in the MaxSAT-LNS paper `[source: https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | MaxSAT-based large neighborhood search for high school timetabling | independent]`.
- ITC 2019: "Five of their solutions to the ITC 2019 instances are proven optimal" by the DSUM team; the winning approach combined a MIP model with MaxSAT and local search `[source: https://link.springer.com/article/10.1007/s10951-023-00801-w | Real-world university course timetabling at the International Timetabling Competition 2019 | independent]`.
- MiniZinc Challenge 2024: **OR-Tools CP-SAT swept gold in Fixed, Free, Parallel**; PicatSAT silver in Free / Parallel; Choco-solver CP-SAT silver Fixed, Choco-solver CP bronze Parallel; Pumpkin entered competitively but did not medal in 2024 `[source: https://www.minizinc.org/challenge/2024/results/ | MiniZinc - Challenge 2024 Results | independent]`. Pumpkin medaled (Bronze, Fixed track) in 2025 `[source: https://github.com/ConSol-Lab/Pumpkin | GitHub - ConSol-Lab/Pumpkin | independent]`.
- The CP-SAT primer's "Alternatives" section frames the field: MIP "best for network problems with linear constraints"; CP "ideal for scheduling and problems with complex constraints like AllDifferent"; SAT "efficient for boolean feasibility problems, handling millions of variables. Less specialized than CP-SAT but surprisingly capable for optimization with clever encoding"; SMT "advanced layer above SAT, handling mathematical formulas with theories... useful in automated theorem proving and verification"; meta-heuristics "offer quick solutions but typically underperform against advanced solvers for solution quality" `[source: https://d-krupke.github.io/cpsat-primer/03_big_picture.html | Alternatives - The CP-SAT Primer | practitioner]`.
- For boolean feasibility problems, the CP-SAT primer specifically calls out **PySAT** as a Python interface ("a Python library under MIT license that provides a nice interface to many SAT-solvers and allows you to switch between different solvers without changing your code") and **Z3** as the canonical SMT recommendation `[source: https://d-krupke.github.io/cpsat-primer/03_big_picture.html | Alternatives - The CP-SAT Primer | practitioner]`.
- In differential cryptanalysis benchmarks (a non-timetabling but related discrete-search domain): "Kissat dominated the SAT category, Yices2 the SMT pool, Gurobi in MILP and Chuffed won CP" `[source: https://eprint.iacr.org/2024/105.pdf | Differential cryptanalysis with SAT, SMT, MILP, and CP | independent]`.

---

## Cross-cutting evidence: Python 3.14 wheel readiness

- **OR-Tools 9.15.6755 on January 14, 2026** ships cp314 + cp314t (free-threaded) wheels for manylinux x86-64, manylinux aarch64, Windows, macOS `[source: https://github.com/google/or-tools/releases | Releases · google/or-tools | independent]` `[source: https://pypi.org/project/ortools/ | ortools · PyPI | independent]`. Open issue #4859 from October 2025 is now resolved; before this, no cp314 wheels were available `[source: https://github.com/google/or-tools/issues/4859 | Add official Python 3.14 wheels for OR-Tools · Issue #4859 | independent]`.
- **z3-solver 4.16.0.0** ships generic `py3` wheels (works on any Python 3.x) `[source: https://pypi.org/project/z3-solver/ | z3-solver · PyPI | independent]`.
- **python-sat (PySAT) 1.9.dev2** explicitly ships cp314 wheels alongside cp310 / cp311 / cp312 / cp313 `[source: https://pypi.org/project/python-sat/ | python-sat · PyPI | independent]`.
- **highspy** (HiGHS Python binding) ships cp314 wheels for manylinux, Windows, macOS in the 1.14.x line `[source: https://pypi.org/project/highspy/ | highspy · PyPI | independent]`.
- **minizinc-python 0.10.0** ships `py3-none-any.whl` (pure-Python wrapper around system MiniZinc), so it loads on 3.13 / 3.14 but requires the MiniZinc binary to be installed separately `[source: https://pypi.org/project/minizinc/ | minizinc · PyPI | independent]`.
- **CPMpy 0.10.0** requires Python ≥ 3.10; its wheel format is pure-Python, so 3.13/3.14 compatibility depends on its solver dependencies (e.g., `ortools`, `z3-solver`) `[source: https://pypi.org/project/cpmpy/ | cpmpy · PyPI | independent]`.
- **PyChoco 0.2.4**: surfaced docs only confirm wheels for Python 3.6 to 3.10; Python 3.13/3.14 wheel availability not confirmed in surfaced sources `[source: https://pychoco.readthedocs.io/en/latest/ | Pychoco — pychoco 0.1.1 documentation | independent]`.
- **gecode-python 0.27** has not been updated since 2012; no CPython 3.13/3.14 wheels `[source: https://pypi.org/project/gecode-python/ | gecode-python · PyPI | independent]`.

---

## Cross-cutting evidence: Rust binding readiness

- **`z3` crate (prove-rs/z3.rs)**: actively maintained, **0.20.0**, ~64 K downloads/month, 31 dependents; `z3-sys 0.10.4` Dec 27, 2025 `[source: https://crates.io/crates/z3 | z3 - crates.io: Rust Package Registry | independent]` `[source: https://docs.rs/crate/z3-sys/latest | z3-sys 0.10.4 - Docs.rs | independent]`.
- **`pumpkin-solver` crate**: pure Rust, **0.3.0** Feb 2026, dual Apache-2.0/MIT, scheduling globals natively in-tree `[source: https://crates.io/crates/pumpkin-solver/0.1.3 | pumpkin-solver - crates.io | independent]`.
- **`cadical` crate (mmaroti/cadical-rs)**: bundles CaDiCaL 1.9.5 source + Rust binding, statically linked, MIT `[source: https://github.com/mmaroti/cadical-rs | GitHub - mmaroti/cadical-rs | independent]`.
- **`rustsat` crate**: 0.7.5, MIT, MSRV 1.87, integrates CaDiCaL / Kissat / Glucose / MiniSat with cardinality + pseudo-Boolean encodings; Python bindings published as `rustsat` on PyPI `[source: https://github.com/chrjabs/rustsat | GitHub - chrjabs/rustsat | independent]`.
- **`copper` crate**: early-stage, MIT, "limited number of variable types and constraints" `[source: https://docs.rs/copper/latest/copper/ | copper - Rust | independent]`.
- **No Rust binding for Gecode, MiniZinc, Choco, GCS, NuCS, CPMpy, cvc5** surfaced in the search results.

---

## Adversarial / dissenting evidence (Round 3)

### On Z3 / SMT for scheduling

- "Nonlinear integer arithmetic (with multiplication of variables) is undecidable. For such cases, solvers use heuristics or incomplete methods" — Z3's integer optimization correctness depends on encoding `[source: https://www.cs.umd.edu/class/fall2025/cmsc433/Solving_SAT_and_SMT_Problems_Using_Z3.html | 12 Solving SAT and SMT Problems Using Z3 | independent]`.
- Z3 MaxSMT "falls flat on its face in most large scale benchmark applications circulating in the MaxSAT community" with the basic engine `[source: https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/nbjorner-nuz.pdf | νZ - An Optimizing SMT Solver | vendor]`.
- "Z3's performance on job shop scheduling problems has been noted as remaining far from the performance of CPOPTIMIZER" `[source: https://news.ycombinator.com/item?id=21104748 | Hacker News - Z3 job scheduling | practitioner]`.

### On PySAT

- Cardinality encodings increase treewidth: "Adding a k-cardinality totalizer constraint to a formula with n variables increases the treewidth up to Ω(n)"; "the naive encoding can increase the treewidth by the number of variables in the cardinality constraint" `[source: https://www.cs.cmu.edu/~csd-phd-blog/2024/cardinality-constraints/ | CMU CSD PhD Blog | independent]`.
- PySAT does not bundle Kissat by default — the long-standing issue #61 is still open `[source: https://github.com/pysathq/pysat/issues/61 | Include kissat · Issue #61 · pysathq/pysat | independent]`.

### On Pumpkin

- Pumpkin's own MiniZinc Challenge 2024 description: "The solver is reasonably competitive, although depending on the problem, other state-of-the-art solvers may be better" `[source: https://www.minizinc.org/challenge/2024/description_pumpkin.txt | description_pumpkin.txt | independent]`.
- Pumpkin did not medal in MiniZinc Challenge 2024; only earned Bronze in the 2025 Fixed track `[source: https://github.com/ConSol-Lab/Pumpkin | GitHub - ConSol-Lab/Pumpkin | independent]`.

### On PyChoco / GraalVM native build

- GraalVM native-image "may not achieve the same peak runtime performance as a well-optimized JVM JIT-compiled application... for long-running applications, the traditional JIT compiler might still provide benefits" — relevant for solver workloads that run for minutes `[source: https://www.graalvm.org/latest/reference-manual/native-image/optimizations-and-performance/ | Optimizations and Performance | vendor]`.

### On Gecode

- Per the WebFetch summary of `github.com/Gecode/gecode`, "the latest official release dates back over five years" (a partial-data snapshot conflicting with the releases page that shows 2024 entries) — release cadence is at minimum slow, with 6.2.0 (Apr 2024) the most recent stable `[source: https://github.com/Gecode/gecode | GitHub - Gecode/gecode | independent]` `[source: https://github.com/Gecode/gecode/releases | Releases · Gecode/gecode | independent]`.
- The actively-maintained Python binding is not maintained by the upstream Gecode team and last shipped in 2012 `[source: https://pypi.org/project/gecode-python/ | gecode-python · PyPI | independent]`.

### On Choco / pychoco

- Choco-solver core requires JDK 8+ and Maven (Java toolchain); pychoco is the only no-JVM access path and depends on the `choco-solver-capi` GraalVM native-image build pipeline `[source: https://github.com/chocoteam/choco-solver | GitHub - chocoteam/choco-solver | independent]`.

### On MiniZinc

- The Python binding is a pure-Python wrapper that requires a separately-installed MiniZinc binary; deployments must ship the MiniZinc compiler (which itself bundles solver binaries totaling tens-to-hundreds of MB) `[source: https://python.minizinc.dev/en/latest/getting_started.html | Getting Started — MiniZinc Python 0.10.0 documentation | independent]`.
- Known issue (Oct 2024, OR-Tools 9.11 + MiniZinc 2.8.7): minimization with timeout but without `--all-solutions` returns status UNKNOWN even when CP-SAT found feasible solutions `[source: https://github.com/google/or-tools/issues/4398 | fzn-cp-sat: Status UNKNOWN... | independent]`. The fzn-cp-sat backend has had recurring binding-quality issues `[source: https://github.com/MiniZinc/libminizinc/issues/945 | OR-Tools CP-SAT FlatZinc backend fails to run due to missing shared library | independent]`.

---

## Summary of license / Python 3.14 / Rust crate matrix (collected for cluster)

| Engine | License | Latest release | Python 3.14 wheel | Rust crate | Scheduling globals available |
|---|---|---|---|---|---|
| Gecode 6.2.0 | MIT | 2024-04-12 `[source: https://github.com/Gecode/gecode/releases]` | No (gecode-python frozen 2012) | None maintained | Cumulative, AllDifferent, NoOverlap (via FlatZinc/MZN) |
| MiniZinc 2.9.7 | (MPL-2.0 per project) | 2024-04-30 `[source: https://github.com/MiniZinc/libminizinc/releases]` | py3-none-any wrapper (needs system MZN binary) | None | All MZN globals via backend |
| Z3 4.16.0 | MIT | 2026-02-19 `[source: https://github.com/Z3Prover/z3/releases]` | py3 generic wheels `[source: https://pypi.org/project/z3-solver/]` | `z3` 0.20.0 active `[source: https://crates.io/crates/z3]` | Optimize / MaxSMT (no scheduling-specific globals) |
| PySAT 1.9.dev2 | MIT | 2026-03-05 `[source: https://pypi.org/project/python-sat/]` | cp314 wheels `[source: https://pypi.org/project/python-sat/]` | (RustSAT covers same SAT cores) | Cardinality / pseudo-Boolean encodings only |
| CaDiCaL 3.0.0 | MIT | 2025-12-23 `[source: https://github.com/arminbiere/cadical]` | via PySAT extras | `cadical` (mmaroti) `[source: https://github.com/mmaroti/cadical-rs]` | None (raw SAT) |
| RustSAT 0.7.5 | MIT | 2026-01-30 `[source: https://github.com/chrjabs/rustsat]` | experimental `rustsat` PyPI | yes, native | Cardinality / PB encodings, MaxSAT |
| Choco 6.0.0 | BSD-3 | 2026-05-05 `[source: https://github.com/chocoteam/choco-solver/releases]` | No (Java only) | None | Cumulative, NoOverlap, AllDifferent |
| PyChoco 0.2.4 | BSD-3 | 2025-09-28 `[source: https://github.com/chocoteam/pychoco/releases]` | unconfirmed (≤3.10 documented) | None | Inherits Choco scheduling globals |
| Pumpkin 0.3.0 | Apache-2 / MIT | 2026-02-11 `[source: https://github.com/ConSol-Lab/Pumpkin]` | via `pumpkin-solver-py` | `pumpkin-solver` 0.3.0 native Rust `[source: https://crates.io/crates/pumpkin-solver/0.1.3]` | Cumulative, Disjunctive, AllDifferent, Element |
| GCS / gcspy | MIT / MPL-2.0 | gcspy: pre-release | unconfirmed | None | "extremely minimal" MZN coverage; under development `[source: https://github.com/ciaranm/glasgow-constraint-solver]` |
| CPMpy 0.10.0 | Apache-2 | 2026-01-19 `[source: https://github.com/CPMpy/cpmpy]` | requires Py ≥ 3.10 | None | Cumulative + decomposition fallback |
| NuCS 10.1.0 | MIT | 2026-04-11 `[source: https://github.com/yangeorget/nucs]` | depends on Numba 0.65 / NumPy 2.4.2 support | None | n-ary global propagators |
| cvc5 1.3.4 | BSD-3 | 2026-05-07 `[source: https://github.com/cvc5/cvc5]` | Cython binding | None | None scheduling-specific |
| copper (Rust) | MIT | dormant since ~Jan 2024 `[source: https://docs.rs/copper/latest/copper/]` | n/a | yes (early) | Limited; "cannot rival mature solvers" |

(The license + date strings above are sourced inline; the table is a compact restatement of the per-engine subsections, not new claims.)
