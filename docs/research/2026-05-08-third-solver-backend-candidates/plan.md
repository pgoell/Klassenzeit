# Research Plan: Third Solver Backend Candidates

## Brief

Topic: Strong solver backend candidates for school timetabling that integrate via Python or Rust (no Java), to spike alongside Rust LAHC and CP-SAT for OPEN_THINGS item 56.

Scope: Maintained C / C++ / Rust / Python solver projects with Python or Rust bindings. School-timetabling fit on hard constraints (double-booking, co-placement, room-hopping, daily caps) and soft axes (gaps, prefs, balance). Permissive license preferred. Must satisfy Python 3.14 or Rust toolchain.

Audience: Pascal, sole maintainer, deep solver-domain knowledge. Output drives brainstorm + ADR.

Purpose: Select 1 to 3 candidates for the OPEN_THINGS item 56 spike with maintenance, license, integration cost, and comparative evidence.

## Clusters

### Cluster: mip-lp-backends

Title: Maintained C / C++ MIP / LP solvers with Python bindings (HiGHS, GLPK, CBC, SCIP, others)

Sub-questions:
  - SQ1: Maintenance status, latest release, license, governance for HiGHS, GLPK, CBC, SCIP (Apache-2 since SCIPv9), and any other maintained OSS MIP / LP engine (Clp, Cassowary, etc.).
    - Search angles: "HiGHS solver release 2025 2026", "SCIP Apache 2 license", "COIN-OR CBC maintainer 2025", "GLPK maintainer status", "highspy PyPI", "PySCIPOpt"
    - Source types: project release notes, GitHub Insights, PyPI, JuMP / Pyomo blog, mailing lists
  - SQ2: Python binding quality and Python 3.14 support for each (`highspy`, `swiglpk`, `python-mip`, `PySCIPOpt`, `cylp`).
    - Search angles: "highspy 3.14", "PySCIPOpt python 3.14", "python-mip Python 3.13", "GLPK python wheel"
    - Source types: PyPI release pages, GitHub issues, project docs
  - SQ3: How well does pure MIP / LP fit school timetabling vs CP / SAT? Encoding pain points (no native disjunctive global constraints, big-M blowup, weak LP relaxations on timetable cores). Documented MIP school-timetabling case studies.
    - Search angles: "school timetabling MIP formulation", "Birbas Daskalaki Housos MIP timetabling", "CP-SAT vs Gurobi school timetable", "MIP versus CP timetabling 2024"
    - Source types: OR / scheduling papers, ITC 2007 / 2011 results, JuMP case studies
  - SQ4: Specifically HiGHS — does it have any CP-style or scheduling-friendly extensions? Roadmap?
    - Search angles: "HiGHS roadmap", "HiGHS scheduling indicator constraints"
    - Source types: HiGHS docs, ERGO / GitHub issues
  - SQ5: Rust crates wrapping any of these (`good_lp`, `russcip`, `microlp`, `coin_cbc`, `highs-rs`).
    - Search angles: "rust mip solver crate", "good_lp HiGHS", "russcip", "highs crate maintained"
    - Source types: crates.io, lib.rs, project READMEs

### Cluster: cp-sat-smt-backends

Title: Maintained C / C++ CP, SAT, SMT engines with Python or Rust bindings (non-Java)

Sub-questions:
  - SQ1: Gecode (C++) — maintenance status, license (MIT), Python access (MiniZinc layer, direct bindings), Rust access.
    - Search angles: "Gecode 6.3 release", "Gecode Python", "MiniZinc Gecode", "gecode-rs"
    - Source types: gecode.org, MiniZinc docs, GitHub
  - SQ2: MiniZinc as a portable layer that targets multiple backends (Gecode, Chuffed, OR-Tools, HiGHS). Python binding (`minizinc-python`), Rust binding, Python 3.14 status.
    - Search angles: "MiniZinc 2.8 release", "MiniZinc Chuffed", "minizinc-python 3.13", "minizinc-rust"
    - Source types: minizinc.org, GitHub
  - SQ3: Z3 SMT (C++, MIT) — feasibility for school-scheduling-style integer / cardinality constraints, Python `z3-solver`, Rust `z3` crate. Performance vs CP-SAT on combinatorial scheduling.
    - Search angles: "Z3 solver scheduling performance", "z3-solver Python 3.14", "z3-rs maintained"
    - Source types: Microsoft Research papers, project repo
  - SQ4: PySAT / Kissat / CaDiCaL — SAT-encoded scheduling, MaxSAT for soft constraints. Maintenance, performance evidence.
    - Search angles: "PySAT 0.1.8", "CaDiCaL 2.x release", "MaxSAT timetabling 2024"
    - Source types: GitHub, SAT competition
  - SQ5: Other maintained CP engines beyond Gecode (e.g., Choco-mini in non-Java, JaCoP — Java out, IBM CP Optimizer commercial, LocalSolver / Hexaly commercial).
    - Search angles: "open source constraint solver 2025", "Choco alternative Python", "fzn-cpsat"
    - Source types: project comparisons, recent surveys

### Cluster: rust-native-solvers

Title: Rust-native solvers (CP / SAT / MIP / metaheuristic) suitable as a `BenchBackend` without Python at all

Sub-questions:
  - SQ1: Pumpkin (Delft, Rust CP solver) — maturity, scheduling domain features, license, latest release, real-world use.
    - Search angles: "pumpkin solver Rust", "ConSol-Lab pumpkin", "pumpkin-solver crate"
    - Source types: crates.io, Delft AI repos, papers
  - SQ2: Copper, NuCS, copper-rs, or any other maintained Rust CP solver. License, maturity.
    - Search angles: "Rust constraint programming crate maintained", "copper rust solver", "NuCS solver"
    - Source types: crates.io, lib.rs
  - SQ3: Rust SAT solvers (`varisat`, `splr`, `batsat`, `kissat-rs`) and their suitability for cardinality / scheduling encodings.
    - Search angles: "Rust SAT solver crate 2025 maintained"
    - Source types: crates.io, GitHub
  - SQ4: Rust metaheuristic frameworks (`argmin`, `metaheuristics-rs`, `genevo`, `simanneal-rs`, `oxigen`) — applicability beyond LAHC, fit for school timetabling.
    - Search angles: "Rust simulated annealing crate", "argmin combinatorial", "metaheuristics-rs"
    - Source types: crates.io, project READMEs
  - SQ5: Rust MIP wrappers (`russcip`, `highs`, `coin_cbc`, `microlp`, `good_lp`) maturity and license — overlap with cluster A but framed for direct embedding in `solver-core`.
    - Search angles: "russcip release 2025", "highs rust crate", "good_lp benchmarks"
    - Source types: crates.io, lib.rs
  - SQ6: Integration cost: would a pure-Rust backend require shipping the engine in `solver-core`'s `Cargo.toml`? Build complexity (system deps, vendored C++)?

### Cluster: timetabling-specific-and-localsearch

Title: School-timetabling-specific engines and local-search frameworks beyond LAHC

Sub-questions:
  - SQ1: FET (C++, AGPL) — architecture, applicability as a library vs executable, real-world use, Hessen-style fit.
    - Search angles: "FET timetable open source 2025", "FET algorithm", "FET fork as library"
    - Source types: lalescu.ro, GitHub forks, packagers
  - SQ2: UniTime (Java — out) and Tablix (C, abandoned?) for completeness.
    - Search angles: "UniTime alternatives", "Tablix timetable solver"
    - Source types: project pages, mailing lists
  - SQ3: LocalSolver / Hexaly (commercial, free academic license?) — solver class, school-timetabling track record.
    - Search angles: "Hexaly Optimizer school timetable", "LocalSolver academic license"
    - Source types: vendor docs, case studies
  - SQ4: Modern hybrid local-search frameworks with Python or Rust: ALNS (`alns` Python), `mealpy` (Python metaheuristic library), and any maintained Rust equivalents.
    - Search angles: "ALNS Python school timetable", "mealpy combinatorial", "Rust large neighbourhood search"
    - Source types: PyPI, GitHub
  - SQ5: Custom Rust local search (extending the existing LAHC) with documented stronger neighborhoods (LNS, GA, tabu, partial restart) — what's the precedent for OptaPlanner-like hybrid local search built in Rust without taking on a Java toolchain?
    - Search angles: "OptaPlanner alternative Rust", "Rust LNS scheduling library"
    - Source types: blog posts, GitHub

### Cluster: comparative-evidence-school-timetabling

Title: Empirical and comparative evidence — which solver class wins on school-timetabling-shaped problems?

Sub-questions:
  - SQ1: ITC 2007 / 2011 / XHSTT benchmarks — what solver class wins each track in recent (2022 to 2026) submissions?
    - Search angles: "XHSTT benchmark 2024", "ITC2011 winners", "high school timetabling benchmark CP-SAT"
    - Source types: PATAT proceedings, MISTA, Annals of OR
  - SQ2: Direct head-to-head comparisons of CP-SAT vs HiGHS / SCIP / Gecode / metaheuristic on school-timetabling-shaped problems (2022 to 2026 papers).
    - Search angles: "CP-SAT comparison HiGHS school", "OR-Tools vs SCIP timetabling", "CP-SAT vs metaheuristic timetable"
    - Source types: peer-reviewed OR papers, arXiv
  - SQ3: When CP-SAT plateaus, what tends to break the plateau in literature? (Larger neighbourhoods, MIP hybrid, decomposition, problem-specific heuristics, custom CP propagators?)
    - Search angles: "CP-SAT plateau hybrid solver", "matheuristic timetabling 2024", "Benders decomposition timetabling"
    - Source types: 2024 to 2026 OR / scheduling papers
  - SQ4: Concrete documented cases where switching to or adding HiGHS / SCIP / Gecode / Pumpkin gave measurable wins over CP-SAT or beat OptaPlanner / Timefold.
    - Search angles: "school timetable HiGHS solver wins", "Gecode school timetable", "Pumpkin scheduling experiment"
    - Source types: PATAT, blog posts, theses
  - SQ5: Production-system precedents for the hybrid pattern (FFD + LAHC + external validator). Anything Klassenzeit can learn from?
    - Search angles: "hybrid local search constraint solver production", "OptaPlanner local search plus CP", "matheuristic validator timetabling"
    - Source types: industrial talks, PATAT keynotes
