# Third Solver Backend Candidates: A Pumpkin-First Bake-Off Plan

*Date: 2026-05-08*

## Executive Summary

For the OPEN_THINGS item 56 spike, Klassenzeit should bake off **(1) Pumpkin** as the primary candidate, **(2) PySAT with RC2 or TT-Open-WBO-Inc MaxSAT** as the secondary candidate, and **(3) good_lp + HiGHS (or russcip + SCIP) MIP** as the tertiary candidate, in that priority order. The load-bearing reason is that Pumpkin uniquely combines three properties no other gate-passing engine offers together: a different solver class than both LAHC and CP-SAT (lazy-clause-generating CP with first-class `cumulative` and `disjunctive` globals), a pure-Rust integration that fits the existing `BenchBackend` enum without a Python peer module, and intellectual lineage to the only public solver line with documented XHSTT-best-result wins outside MIP. MaxSAT and MIP cover the two solver classes with the strongest empirical track record on XHSTT and ITC2019 respectively when CP-SAT plateaus.

The structural argument matters more than any single benchmark. The current candidate pair, Rust LAHC plus CP-SAT (per ADR 0029 and ADR 0030), shares no failure mode that another LAHC variant or another LP-based engine could expose. The 2017 to 2025 literature converges on three classes that beat CP-SAT under specific regimes: pure MIP via fix-and-optimize matheuristic (ITC2019 winner DSUM, with Gurobi 8.1.1), MaxSAT-based large neighborhood search (Demirović and Musliu's XHSTT new-best-known upper bounds), and external local search atop a CP or MIP backbone (IHTC2024 podium teams). Pumpkin spans the first and second of those by virtue of its LCG architecture and its lead author's MaxSAT-LNS pedigree; HiGHS or SCIP fills the third.

The honest caveat is that no 2022 to 2026 paper documents a measurable win for HiGHS, SCIP, Gecode, or Pumpkin over CP-SAT on a school-timetabling benchmark. The case for any candidate over CP-SAT here is structural (a different solver class to break a plateau) rather than empirically pre-validated. The recommendation is therefore conditional: ship the spike if and when LAHC and CP-SAT plateau on the same quality axis on Klassenzeit's instance shape, and accept upfront that one defensible outcome is closing item 56 with a "no addition" ADR analogous to ADR 0035 on Timefold.

## Table of Contents

- [Executive Summary](#executive-summary)
- [Introduction](#introduction)
- [Methodology](#methodology)
- [What Matters Most](#what-matters-most)
- [Supporting Evidence](#supporting-evidence)
- [Analysis & Insights](#analysis--insights)
- [Limitations & Open Problems](#limitations--open-problems)
- [Future Outlook](#future-outlook)
- [Conclusions & Practical Starting Point](#conclusions--practical-starting-point)
- [References](#references)

## Introduction

Klassenzeit's solver stack today is a Rust LAHC core with a CP-SAT bake-off candidate, both governed by ADR 0029 (the four-backends bake-off) and ADR 0030 (CP-SAT via the `ortools` Python wheel with a uniform Rust scorer). ADR 0035 closed the most recent third-backend question by rejecting Timefold on archived-Python and Java-toolchain grounds. OPEN_THINGS item 56 reopens the search under a stricter trigger: the spike fires only when LAHC and CP-SAT plateau on the same quality axis, and the chosen backend must integrate without violating the no-Java rule, the permissive-license preference, the 12-month maintenance gate, or the Python 3.14.2 toolchain pin.

The relevant solver classes inside that gate are constraint programming (CP), mixed-integer linear programming (MILP) and pure linear programming (LP), Boolean satisfiability and its weighted variant (SAT and MaxSAT), satisfiability modulo theories (SMT), local-search frameworks beyond pure LAHC, Rust-native engines that can drop directly into `solver-core`, and school-timetabling-specific tools. C and C++ engines with maintained Python or Rust bindings are admissible (the precedent is CP-SAT itself, which ships as a C++ kernel inside the `ortools` wheel).

The thesis: pick Pumpkin as the primary spike target, queue PySAT-MaxSAT and HiGHS or SCIP as the secondary and tertiary fallbacks, and treat the bake-off as the empirical step that decides whether a third class is justified at all.

## Methodology

The research drew from five clusters, each scoped to a perspective on the third-backend question. The synthesis sub-questions (SQ1 through SQ5) collapse the cluster sub-questions into a smaller set: SQ1 (gate compliance) absorbs cluster sub-questions on maintenance, license, and toolchain across MIP/LP, CP/SAT/SMT, Rust-native, and timetabling-specific engines; SQ2 (school-timetabling-specific empirical evidence) maps to the comparative-evidence cluster's competition and head-to-head sub-questions; SQ3 (integration cost) draws from the Rust-native cluster's embedding sub-question and the binding-quality sub-questions across the other clusters; SQ4 (CP-SAT plateau-breakers) is the comparative-evidence cluster's plateau sub-question with cross-references to MaxSAT and matheuristic findings elsewhere; SQ5 (Rust-native specifics) is the rust-native-solvers cluster verbatim.

Source credibility tags travel with every claim. Repository metadata, peer-reviewed papers, and standards documentation are tagged `[independent]`. Practitioner blogs, project READMEs maintained by the engine's primary contributors, and competition write-ups from participating teams are tagged `[practitioner]`. Vendor materials (Hexaly's benchmarks, Perron's CP-SAT slide decks delivered as Google authorship) are tagged `[vendor]`. Industry analyst content is tagged `[journalism]` or `[consulting]` as appropriate. Where two sources contradict, the synthesis picks the more authoritative one and flags the loser explicitly: GitHub releases pages outrank summarized WebFetch snippets when judging maintenance freshness, for example.

The research limitation worth foregrounding: no published Pumpkin-on-XHSTT or HiGHS-on-Klassenzeit benchmark exists. The case for the third backend rests on inferred fit (the shape of Pumpkin's `disjunctive_scheduling.rs` example, the lead author's prior XHSTT track record) plus structural diversity, not on a head-to-head number that a citation alone could supply. The bake-off itself is what would close that gap.

## What Matters Most

### 1. Solver-class diversity is the only lever that matters once CP-SAT and LAHC share a plateau

Adding another LP-based engine, another CDCL-only SAT solver, or another LAHC variant would not produce a new failure mode. The literature converges on three classes that beat CP-SAT on educational timetabling under specific regimes: pure MIP with fix-and-optimize matheuristic (DSUM's ITC2019 win using Gurobi 8.1.1, [DSUM ITC 2019](https://dsumsoftware.com/itc2019/) `[practitioner]`); MaxSAT-based LNS (four new XHSTT best-known upper bounds in [Demirović and Musliu 2017](https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927) `[independent]`); and external local search atop a CP or MIP backbone (the IHTC2024 podium, including the [Twente three-phase CP+MIP+SA paper](https://arxiv.org/abs/2511.04685) `[independent]` and the [SDU-IMADA Open-Source Prize](https://roar-net.eu/news/ihtc-2024-best-oss-prize/) `[practitioner]`).

The implication: any third backend should sit in a different solver class than CP-SAT and LAHC. A second metaheuristic framework (SolverForge, ALNS) loses on this axis even before its other gates are evaluated.

### 2. Pumpkin is the only candidate that simultaneously passes every gate and adds a genuinely different solver class with the lowest integration cost

Pumpkin (TU Delft, [crates.io/pumpkin-solver](https://crates.io/crates/pumpkin-solver) `[independent]`) released v0.3.0 on 2026-02-11, with main commits as recent as 2026-05-06 ([Pumpkin commits API](https://api.github.com/repos/ConSol-Lab/Pumpkin/commits) `[independent]`). License: Apache-2.0 with MIT alternative; both fit Klassenzeit's license band cleanly. Wheels: `pumpkin-solver` ships cp314 manylinux wheels per [PyPI metadata](https://pypi.org/pypi/pumpkin-solver/json) `[independent]`. API surface: cumulative, disjunctive, all_different, element, and table globals are exposed at the Rust API per [docs.rs all-items](https://docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html) `[independent]`. Integration is `cargo add pumpkin-solver`; no system dependencies, no Python peer module, no subprocess shim. A `BenchBackend::Pumpkin` variant fits the existing enum exactly the way the existing Rust LAHC variants do, which is materially less work than the `klassenzeit_solver/cpsat.py` shape ADR 0030 had to invent for CP-SAT.

The lead-author lineage is the structural twist. Emir Demirović, the Pumpkin lead, co-authored the Demirović-Musliu MaxSAT-LNS-on-XHSTT line ([SAT-based Approaches for the General High School Timetabling Problem PhD thesis](https://dbai.tuwien.ac.at/staff/musliu/emird.pdf) `[independent]`) and the Z3-bitvector XHSTT paper ([PMC5411413](https://pmc.ncbi.nlm.nih.gov/articles/PMC5411413/) `[independent]`). This is intellectual lineage rather than load-bearing thesis evidence: Pumpkin itself ships no XHSTT example. The point is that the only researcher line with documented XHSTT new-best-known wins outside MIP is the same line that built Pumpkin, and Pumpkin's LCG-CP architecture is the natural successor toolkit. The Pumpkin CP 2025 paper "Unite and Lead" reports new RCPSP and RCPSP-max bounds ([Sidorov et al.](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2025.35) `[independent]`), confirming current research-grade competence on disjunctive scheduling.

### 3. PySAT with RC2 or TT-Open-WBO-Inc is the right secondary because MaxSAT-LNS owns the only XHSTT plateau-breaker in the literature outside MIP

`python-sat` 1.9.dev2 (2026-03-05) ships cp314 wheels and bundles CaDiCaL 1.9.5 plus Kissat 4.0.4 (per [PyPI](https://pypi.org/project/python-sat/) `[independent]` and [PySAT updates](https://pysathq.github.io/updates/) `[independent]`). License is MIT. RC2 was top-ranked across MaxSAT Evaluations 2018 and 2019; TT-Open-WBO-Inc backed MaxSAT Evaluation 2023 and 2024 winners ([repo](https://github.com/alexander-nadel-academic/tt-open-wbo-inc/) `[independent]`). The empirical anchor is [Demirović and Musliu 2017](https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927) `[independent]`: a modified Open-WBO "managed to compute four new best known upper bounds for high school timetabling problems." The follow-up [UniCorT in Journal of Scheduling 2022](https://link.springer.com/article/10.1007/s10951-021-00695-6) `[independent]` used TT-Open-WBO-Inc to solve all ITC2019 instances by 2022.

The friction is encoding cost, not binding plumbing. Klassenzeit's soft-constraint set (gaps, preferences, balance) requires WCNF cardinality encodings that can blow up treewidth at order-n scale per [CMU's cardinality-treewidth note](https://www.cs.cmu.edu/~csd-phd-blog/2024/cardinality-constraints/) `[independent]`. TT-Open-WBO-Inc has no PyPI release; integration would mean shipping a C++ binary alongside Klassenzeit, comparable in friction to the FET subprocess pattern but with a much cleaner license posture.

### 4. good_lp with HiGHS is the right tertiary; russcip with SCIP is a defensible alternative

The MIP track-record evidence is overwhelmingly DSUM-shaped, but DSUM used commercial Gurobi. The Mittelmann benchmark gap is one order of magnitude: open-source MIP solvers (HiGHS, CBC, SCIP) "perform about the same, while commercial solvers (CPLEX, XPRESS, and Gurobi) are about two orders of magnitude faster" per the [HiGHS discussion 1683](https://github.com/ERGO-Code/HiGHS/discussions/1683) `[practitioner]`, with HiGHS solving 162/240 instances vs SCIP 136 to 150/240 on [Mittelmann's MILP benchmark](https://plato.asu.edu/ftp/milp.html) `[independent]`. Bucknell's exam-scheduling case study, however, found "Gurobi obtained slightly better solutions than SCIP, and the final objective values were always within 4% of each other" ([optimization-online preprint](https://optimization-online.org/wp-content/uploads/2025/09/main-arXiv.pdf) `[independent]`).

For Klassenzeit the verdict is HiGHS first. Both MIT and Apache-2.0 are permissive within Klassenzeit's policy band, so license is not a tiebreaker. The tiebreakers are: HiGHS-Rust statically links from a single MIT C++ tree; `highspy` ships cp314 wheels per [PyPI](https://pypi.org/project/highspy/) `[independent]`; and HiGHS's Feasibility Jump primal heuristic added in v1.11.0 per [Feasibility Jump MPC paper](https://link.springer.com/article/10.1007/s12532-023-00234-8) `[independent]` is exactly the plateau-breaker the brief calls out. russcip is the fallback if SCIP's tighter MIP bound and richer parameterization matter on Klassenzeit-shaped instances, where the case study evidence is thinner but more directly comparable.

### 5. The negative finding that no published HiGHS-vs-CP-SAT school-timetabling benchmark exists is itself load-bearing

Saturated searches across 2022 to 2026 returned no measurable win for HiGHS, SCIP, Gecode, or Pumpkin over CP-SAT on a school-timetabling-shaped benchmark. The 95-paper [IP review on MDPI](https://www.mdpi.com/2079-3197/13/1/10) `[independent]` counts CPLEX 47 times, Gurobi 11 times, CP-SAT once across university-timetabling work. Perron's claim that "On CP problems, CP-SAT beats Gurobi" ([Perron CP-SAT and OR-Tools](https://egon.cheme.cmu.edu/ewo/docs/CP-SAT%20and%20OR-Tools.pdf) `[practitioner]`) and the MiniZinc Challenge sweeps reinforce that CP-SAT may already be the OSS practical ceiling on school-timetabling shapes. **Based solely on saturated-search absence, not on independently corroborated negative experiments**, the case for any candidate over CP-SAT here is structural rather than benchmark-driven. This is the central reason to scope item 56 as a conditional spike with a no-addition ADR as a permissible outcome.

### 6. Several otherwise interesting candidates fail gates and must be excluded before scoring

Gecode upstream is slow but not stale (releases page shows 6.2.0 in April 2024); the disqualifier is the frozen-since-2012 [`gecode-python`](https://pypi.org/project/gecode-python/) `[independent]` binding and the absence of any maintained Rust crate, so Klassenzeit cannot integrate it without writing its own binding. GLPK 5.0 (December 2020 last release) is GPL. CBC is in maintenance-mode and EPL is not strictly inside Klassenzeit's permissive band. FET is AGPL-3.0 with an executable-only architecture; per the [FET FAQ](https://lalescu.ro/liviu/fet/doc/en/faq.html) `[practitioner]` and [fet-cl(1) manpage](https://manpages.ubuntu.com/manpages/bionic/man1/fet-cl.1.html) `[independent]`, integration is via subprocess and XML round-trip with unreliable exit codes. UniTime, Tablix, and TimeFinder are Java or abandoned. Hexaly is commercial at $29K to $49K per year per [Hexaly Pricing](https://www.hexaly.com/pricing) `[vendor]`. The Rust crates copper (~28 months stale per [GitHub API](https://api.github.com/repos/ffminus/copper) `[independent]`), pcp, varisat (~3.5 years stale), oxigen (~5 years stale), genevo, metaheurustics-rs, netaheuristics, and microlp all fail the maintenance or scope gate.

ALNS is the closest gate-failure call. v7.0.0 was released 2024-10-21, just over the 12-month gate by release date. This is a release-date-only check; a fresh `git log` query against main could flip the verdict. ALNS is also Python-only with a custom-encoding cost similar to MaxSAT, so its differentiator versus PySAT-LNS is small.

## Supporting Evidence

### Maintenance, license, and toolchain gates (SQ1)

**Pumpkin passes every gate.** Cargo crate `pumpkin-solver`, dual Apache-2.0 / MIT, v0.3.0 on 2026-02-11, main HEAD commits dated 2026-05-06. PyPI wheel `pumpkin-solver` ships cp314 manylinux. PyO3 bindings via maturin; constraint coverage includes cumulative, disjunctive, all_different, element, table per [docs.rs all-items](https://docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html) `[independent]`. Bronze in MiniZinc Challenge 2025 fixed-search track per [results page](https://www.minizinc.org/challenge/2025/results/) `[independent]`, tied with SICStus Prolog. CP 2025 paper "Unite and Lead" demonstrates new RCPSP and RCPSP-max bounds.

**PySAT plus Kissat or CaDiCaL plus RC2 or TT-Open-WBO-Inc passes every gate.** `python-sat` 1.9.dev2 (2026-03-05), MIT, cp314 wheels, bundles CaDiCaL 1.9.5 and Kissat 4.0.4. RC2 top-ranked in MaxSAT Evaluations 2018 to 2019. TT-Open-WBO-Inc backed MaxSAT-Eval 2023 and 2024 winners but has no PyPI release; would ship as a C++ binary.

**good_lp plus HiGHS passes every gate with caveats.** good_lp 1.15.1 (2026-04-07), MIT; HiGHS 1.14.0 (2026-04-06), MIT; HiGHS-Rust statically links HiGHS C++ but requires g++ and cmake at build. `highspy` 1.14.0 ships cp314 wheels.

**russcip plus SCIP passes every gate.** russcip 0.9.1 (2025-08-26), Apache-2.0; SCIP 10.0 (November 2025), Apache-2.0 since v9 per the [SCIP 10.0 paper](https://arxiv.org/html/2511.18580v1) `[independent]`; the bundled feature avoids system SCIP install.

**Z3 SMT passes every gate but trails CP-SAT on scheduling.** z3-solver 4.16.0.0 (2026-02-19), MIT; `z3` Rust crate 0.20.0 active. [Demirović and Musliu's Z3-bitvector study](https://pmc.ncbi.nlm.nih.gov/articles/PMC5411413/) `[independent]` found feasible solutions for 21 of 23 XHSTT instances and three optima in 24-hour runs but called the approach "not competitive" versus heuristics. Still a defensible plateau-probe; lower priority than Pumpkin or MaxSAT.

**CPMpy passes every gate as a meta-layer.** v0.10.0 (2026-01-19), Apache-2; 3 gold + 1 silver in XCSP3 2024 to 2025; backends include Pumpkin, Choco-via-PyChoco, Z3, PySAT, OR-Tools, MiniZinc, Gurobi. As a single ported backend that fronts multiple engines it is interesting; as the spike target it adds an indirection layer Klassenzeit can avoid by binding to Pumpkin directly.

**MiniZinc passes the toolchain gate but loses on integration shape.** MiniZinc 2.9.7 was released in April 2026 per the official [downloads page](https://www.minizinc.org/downloads/) `[vendor]`. Its Python binding is a pure-Python wrapper around the system `minizinc` binary; deployment must ship a tens-to-hundreds-of-MB compiler. The fzn-cp-sat backend has known status-UNKNOWN bugs ([or-tools issue 4398](https://github.com/google/or-tools/issues/4398) `[independent]`).

**SolverForge passes the maintenance gate but is not a third solver class.** v0.11.1 (2026-05-05), 990 commits, Apache-2.0; LAHC, Tabu, SA, Great Deluge, Step Counting Hill Climbing per [SolverForge README](https://github.com/SolverForge/solverforge) `[practitioner]`. Klassenzeit's existing LAHC bench already covers most of this catalogue. Adding SolverForge would be a fifth LAHC-family variant.

### School-timetabling-specific empirical evidence (SQ2)

**MIP-with-matheuristic owns the strongest 2019 to 2024 educational-timetabling competition track record.** ITC2019 was won by DSUM with Gurobi 8.1.1 plus fix-and-optimize MIP, with five instances proven optimal per [DSUM ITC 2019](https://dsumsoftware.com/itc2019/) `[practitioner]` and [Holm graph-based MIP in J. Scheduling](https://ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00724-y.html) `[independent]`. [Lemos's parallelized matheuristic](https://ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00728-8.html) `[independent]` is the same shape on the same problem.

**MaxSAT-based LNS produced new XHSTT best-known upper bounds.** Per [Demirović and Musliu 2017](https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927) `[independent]`, four new best-known upper bounds were computed.

**CP-SAT swept gold at the MiniZinc Challenge in 2023, 2024, and 2025.** Per-year accuracy: [MiniZinc Challenge 2023](https://www.minizinc.org/challenge/2023/results/) `[independent]` ran the Fixed, Free, Parallel, and Local Search tracks; [2024](https://www.minizinc.org/challenge/2024/results/) `[independent]` ran Fixed, Free, Open, and Local Search; [2025](https://www.minizinc.org/challenge/2025/results/) `[independent]` returned to Fixed, Free, Parallel, and Local Search. CP-SAT topped its tracks in each year.

**Pure CP-SAT and pure ILP both fail on real-world German high schools at scale.** Per [Falkner et al. on German school timetabling](https://arxiv.org/html/2407.16898v1) `[independent]`, "Out of 18 instances, solutions were found for only 10 instances (55% success rate)" with Gurobi at a 6h limit; "even after 6 hours of runtime, it could only find solutions that are nowhere near satisfactory." This is the most directly comparable benchmark to Klassenzeit's instance shape, and it disqualifies pure MIP as a drop-in replacement for the LAHC core.

**The hybrid CP plus MIP plus SA pattern is the dominant production-grade recipe.** IHTC2024 third-place team Twente used "mixed-integer programming, constraint programming, and simulated annealing in a 3-phase solution approach" per their [arXiv preprint](https://arxiv.org/abs/2511.04685) `[independent]`. Second-place SDU-IMADA used "local-search-based meta-heuristic algorithm implemented in Python and C++" per the [Open-Source Prize page](https://roar-net.eu/news/ihtc-2024-best-oss-prize/) `[practitioner]`. First-place v777v was entirely heuristic per the [IHTC2024 competition report](https://www.sciencedirect.com/science/article/pii/S3050784725000157) `[independent]`.

**No 2022 to 2026 paper documents a measurable win for HiGHS, SCIP, Gecode, or Pumpkin over CP-SAT on a school-timetabling benchmark.** This is a negative finding from saturated searches across [HiGHS](https://highs.dev/) `[vendor]` and the 95-paper [IP review on MDPI](https://www.mdpi.com/2079-3197/13/1/10) `[independent]`.

### Integration costs (SQ3)

**Pumpkin's integration cost is the lowest possible.** `cargo add pumpkin-solver`; pure Rust; no system deps; direct Rust API (`Solver::default()`, `new_bounded_integer`, `add_constraint`); `BenchBackend::Pumpkin` fits the existing enum without a Python peer module or subprocess shim per [Pumpkin README building section](https://github.com/ConSol-Lab/Pumpkin/blob/main/README.md) `[independent]` and [docs.rs landing](https://docs.rs/pumpkin-solver/latest/pumpkin_solver/) `[independent]`.

**PySAT plus MaxSAT integration cost is medium.** Wheel ships cp314; bundled CaDiCaL and Kissat avoid extra system installs; integration mirrors `klassenzeit_solver/cpsat.py`. The encoding cost of WCNF for Klassenzeit's soft-constraint set dominates over binding plumbing per [PySAT cardinality docs](https://pysathq.github.io/docs/html/api/card.html) `[independent]`.

**good_lp plus HiGHS integration cost is medium.** Adds g++ and cmake to the build environment; statically linked HiGHS at runtime per [highs-sys README](https://github.com/rust-or/highs-sys/blob/master/README.md) `[independent]`. The CI runner already builds the Rust toolchain plus solver-py via maturin, so the build complexity is incremental rather than a new class of dependency. MIP encoding for Klassenzeit's hard plus soft constraints will need big-M reformulations of disjunctive globals.

**russcip plus SCIP integration cost is medium-high.** Bundled binary feature available; SCIP itself is several MB of binary, larger than HiGHS or pumpkin-solver.

**FET integration cost is high.** AGPL-yellow; subprocess via `fet-cl --inputfile=...`; XML round-trip; unreliable exit codes ("0 sometimes means error"); requires parsing `result.txt`. No `libfet` decoupling exists per [FET Manual](https://www.timetabling.de/manual/FET-manual.en.html) `[practitioner]`.

### CP-SAT plateau-breaker analysis (SQ4)

**Fix-and-optimize matheuristic over a MIP model is the single most-cited plateau-breaker.** [Fonseca et al. on XHSTT-2014](https://www.sciencedirect.com/science/article/abs/pii/S0377221717302242) `[independent]` reported "four new best known lower bounds and improved eleven best known solutions." DSUM ITC2019 uses the same pattern. The [fix-and-optimize XHSTT paper](https://www.sciencedirect.com/science/article/pii/S0305054814001816) `[independent]` is the original methodology citation.

**MaxSAT-based LNS is the second most-cited plateau-breaker.** Demirović-Musliu set new XHSTT best-known upper bounds; UniCorT used TT-Open-WBO-Inc to solve all ITC2019 instances by 2022 per [PATAT 2022 paper 29](https://patatconference.org/patat2022/proceedings/PATAT_2022_paper_29.pdf) `[independent]`.

**CP with hot-start and phase-saving is the third plateau-breaker.** Per [CP hot-start XHSTT chapter](https://link.springer.com/chapter/10.1007/978-3-319-93031-2_10) `[independent]`, "A drastic improvement in performance can be achieved by including solution-based phase saving... and hot start approaches where existing heuristic methods produce a starting point for the CP solver." This favours feeding LAHC seeds into CP-SAT or Pumpkin, not adding a new solver class. The [ASP-LNPS course timetabling chapter](https://link.springer.com/chapter/10.1007/978-3-031-74209-5_5) `[independent]` is a related precedent for ASP-side LNS.

**CP-SAT internal LNS already runs.** Per the [CP-SAT Primer](https://d-krupke.github.io/cpsat-primer/09_lns.html) `[practitioner]`, "CP-SAT schedules its LNS strategies using a simple round-robin method." Adding an external LNS layer is a different lever (custom destroy/repair on Klassenzeit's domain).

**CP-SAT beats MIP on small-to-medium scheduling problems but Gurobi beats CP-SAT on pure linear-integer problems.** Per Perron, the OR-Tools maintainer at Google ([Perron CP-SAT and OR-Tools](https://egon.cheme.cmu.edu/ewo/docs/CP-SAT%20and%20OR-Tools.pdf) `[practitioner]` and [Perron CP-SAT for scheduling](https://schedulingseminar.com/presentations/SchedulingSeminar_LaurentPerron.pdf) `[vendor]`): "On CP problems, CP-SAT beats Gurobi"; "On linear integer problems, CP-SAT beats SCIP, is not far from CPLEX, and sometimes wins against Gurobi, but not often."

### Rust-native landscape (SQ5)

**Pumpkin is the only Rust-native CP solver passing the maintenance gate.** Per [argmin metadata](https://api.github.com/repos/argmin-rs/argmin) `[independent]`, argmin is continuous-only; [localsearch README](https://github.com/lucidfrontier45/localsearch) `[independent]` documents no LAHC; SolverForge is a LAHC-family framework. Copper, pcp, varisat, oxigen, genevo all fail maintenance per their respective [GitHub API metadata records](https://api.github.com/repos/ffminus/copper) `[independent]`.

**Rust SAT crates are maintained but require full WCNF re-encoding.** splr ([metadata](https://api.github.com/repos/shnarazk/splr) `[independent]`), batsat, rustsat ([README](https://github.com/chrjabs/rustsat) `[independent]`, [SAT 2025 paper](https://arxiv.org/html/2505.15221v1) `[independent]`) are alive; encoding cost dwarfs binding cost for any of them.

**Rust MIP wrappers are maintained.** good_lp 1.15.1 fronts CBC, HiGHS, microlp, SCIP, and clarabel via Cargo features per [good_lp README](https://github.com/rust-or/good_lp/blob/main/README.md) `[independent]`; `highs` repo ([metadata](https://api.github.com/repos/rust-or/highs) `[independent]`) and `russcip` ([metadata](https://api.github.com/repos/scipopt/russcip) `[independent]`) are active.

## Analysis & Insights

The most important pattern across the 2017 to 2025 literature is that the dominant solver class shifts per problem variant. ITC2011 was won by metaheuristic (GOAL, hybrid local search). ITC2019 was won by pure MIP (DSUM, Gurobi plus fix-and-optimize). IHTC2024 was won by pure heuristic (v777v), with second-place using MIP only as a feasible-solution generator. The 95-paper IP review counts CPLEX 47, Gurobi 11, CP-SAT 1 across university-timetabling work, which says more about what tooling is available in OR groups than about what wins.

For Klassenzeit's Hessen Grundschule plus Sek-I/II shape, the closer precedent is ITC2011 (hard constraints plus soft preferences over a small instance) than ITC2019 (university block scheduling at scale). Falkner et al.'s German-school result is the most damning for any "swap LAHC for MIP" hypothesis: pure MIP fails on real Hessen-shaped instances even at 6h Gurobi. The value of adding a MIP backend for Klassenzeit is therefore not to replace LAHC but to compute lower bounds and to drive a fix-and-optimize loop. That makes good_lp+HiGHS a tertiary fit, not a primary one.

The second pattern is that Pumpkin's single integration partially covers two of the three plateau-breakers in the literature. Pumpkin can solve SAT and MaxSAT problems via its FlatZinc and WCNF frontends per its README, and the LCG-CP architecture is a direct CP plateau-breaker. The lead-author lineage is intellectual rather than load-bearing, but it is the right kind of intellectual lineage: the same researcher who pioneered MaxSAT-LNS-on-XHSTT now owns the toolkit that bundles LCG-CP, MaxSAT, and SAT into one binding.

The third pattern is that the conventional wisdom mostly gets the third-backend choice wrong by reaching for Choco or Timefold (both Java) or for MiniZinc (compiler ship) or for Hexaly (commercial) when the engineering brief says no Java, no commercial, and no heavy build chain. Inside that brief the field is much smaller than the "OSS solver landscape" panorama suggests. Pumpkin is essentially uncontested on the joint constraint of pure-Rust integration, school-timetabling-shaped global constraints, and active research-grade development.

If a decision-maker has 5 minutes: pick Pumpkin first because it is the only candidate that delivers a different solver class with the lowest integration cost; queue PySAT-MaxSAT and HiGHS or SCIP as fallbacks because they own the MaxSAT-LNS and matheuristic plateau-breakers respectively; and accept that one defensible outcome of the bake-off is no third backend at all.

## Limitations & Open Problems

**No published Pumpkin-on-XHSTT benchmark exists.** Pumpkin's published benchmarks are RCPSP and RCPSP-max ("Unite and Lead"), N-queens, and BIBD. School-timetabling fit is inferred from the `disjunctive_scheduling.rs` example matching the no-double-booking shape and the lead author's MaxSAT-XHSTT track record. The spike itself would be the first public Pumpkin-on-Klassenzeit-shape benchmark. The mitigation is the bake-off: ADR 0029's methodology is designed to surface this kind of empirical question.

**No 2022 to 2026 head-to-head of HiGHS versus CP-SAT on school timetabling exists.** Saturated searches across "HiGHS school timetabling case study" returned no result. The Bucknell exam-scheduling case ranks Gurobi over SCIP within 4%; HiGHS-versus-Gurobi on that problem is unknown. The MaxSAT-LNS literature does not benchmark against HiGHS.

**CP-SAT plateau characterization for Klassenzeit is hypothetical.** The brief specifies the spike trigger as "Rust LAHC and CP-SAT both plateau on the same quality axis." That trigger has not yet fired. The synthesis cannot tell which axis would plateau first (gaps? balance? makespan-equivalent?); the answer determines whether MIP (good for proven-optimal lower bounds) or Pumpkin (good for unsat proofs and disjunctive cliques) is the right tool first. The bake-off design must record per-axis solution quality, not just overall.

**Python 3.14 wheel coverage is incomplete for Python-side candidates.** ALNS, mealpy, jMetalPy, CPMpy, PyChoco, MiniZinc-python, and minilp variants do not list cp314 in PyPI classifiers, even where pure-Python wheels work in practice. The four with confirmed cp314 wheels are Pumpkin (cp314 manylinux), PySAT (cp314), highspy (cp314), and z3-solver (py3 generic). This narrows the Python-side field considerably.

**No evidence of a MIP-as-validator hybrid in Klassenzeit's exact problem shape.** The IHTC2024 pattern (CP for feasibility, MIP for lower bounds, SA for quality) is the closest precedent. Whether the Klassenzeit FFD+LAHC seed plus a MIP-validator upgrade would beat the current LAHC plus CP-SAT pair is the question the spike must answer; literature offers a precedent but no quantification.

**License nuance for FET subprocess separation is not legally settled.** "Mere aggregation" via subprocess is the practitioner consensus but FSF guidance treats coupling on a case-by-case axis. Klassenzeit cannot rely on this without OSS counsel; FET is therefore an inferior third-backend candidate even setting aside the integration friction.

**Pumpkin's MiniZinc Challenge 2024 absence is an open question.** Pumpkin won bronze in 2025 fixed-search but did not appear on the 2024 medal table. Whether this reflects a non-submission, a different track set, or a regression is unverified from the synthesis sources alone.

## Future Outlook

Cut. No falsifiable predictions are supported by the evidence; the bake-off itself is the falsification step.

## Conclusions & Practical Starting Point

The thesis stands. Pumpkin is the right primary spike target because it is the only candidate that simultaneously passes every gate (Apache-2.0 / MIT, cp314 wheels, Rust 2024 toolchain, fresh main commits, active research-grade releases), adds a different solver class than CP-SAT and LAHC (LCG-CP with first-class disjunctive and cumulative globals), integrates at the lowest possible cost (`cargo add pumpkin-solver` into the existing `BenchBackend` enum, no Python peer module, no subprocess shim), and traces back to the only researcher line with documented XHSTT new-best-known wins outside MIP. PySAT with RC2 or TT-Open-WBO-Inc is the right secondary because MaxSAT-LNS owns the only XHSTT plateau-breaker outside MIP and the integration is well-precedented (`klassenzeit_solver/cpsat.py` shape). good_lp with HiGHS is the right tertiary because matheuristic over MIP owns the ITC2019 podium and HiGHS has a Feasibility Jump primal heuristic ready for use.

Recommendations prioritized by impact:

1. **Pre-commit to a Pumpkin spike** in the next ADR (numbered 0036 or higher, after `ls docs/adr/*.md | sort | tail -1` confirms availability). Scope: a `BenchBackend::Pumpkin` variant added to `solver/solver-bench/src/main.rs`, exercising the same instance set as the existing LAHC and CP-SAT runs. Success criterion: per-axis solution-quality dominance over CP-SAT on at least one Klassenzeit-shaped axis (gaps, balance, makespan-equivalent) at matched wall-clock budget.
2. **Hold PySAT and HiGHS as queued candidates.** Do not pre-build their `klassenzeit_solver/*.py` peer modules until the Pumpkin spike either (a) confirms the third-backend value proposition and exposes the failure mode the next backend should target, or (b) delivers a no-addition outcome.
3. **Record per-axis bake-off results, not just overall scoreboard.** The literature shows the dominant solver class shifts per problem variant; the spike's value is in characterizing Klassenzeit's plateau axis, not in declaring a winner per se.
4. **Treat a no-third-backend ADR as a first-class outcome.** ADR 0035 on Timefold is the precedent. The bake-off may well conclude that LAHC plus CP-SAT is the production ceiling for Klassenzeit's shape.

### Starting from zero

The realistic starting position is: Klassenzeit has LAHC plus CP-SAT in the bake-off, no measured plateau yet on production-shape instances, no third-backend Python or Rust integration in tree. The first concrete step is not to integrate Pumpkin. It is to instrument the existing two-backend bake-off so that "plateau on the same axis" is a measurable event rather than a felt one. That means:

1. Run the existing LAHC plus CP-SAT bake-off across Klassenzeit's full instance set with a fixed wall-clock budget per backend per instance.
2. Record per-axis solution quality (gaps, balance, makespan-equivalent, hard-constraint slack) at fixed time checkpoints (1 minute, 5 minutes, 15 minutes, 60 minutes).
3. Define "plateau on the same axis" operationally: both backends fail to improve a specific axis by more than a threshold percent over the last half of the budget, on more than a threshold fraction of instances.
4. If and only if step 3 fires, open the Pumpkin spike using this report's ADR-input recommendation.
5. If step 3 does not fire, file the no-addition ADR (analogous to ADR 0035) and close item 56.

This sequence costs less than building any third backend speculatively, and it forces the spike trigger to be earned rather than asserted. It is also the only sequence that protects against the most likely failure mode: shipping a third backend that delivers no measurable Klassenzeit-shape win because CP-SAT was already at the OSS practical ceiling for this problem.

## References

| Source | Title | Tag |
|---|---|---|
| https://github.com/ConSol-Lab/Pumpkin | Pumpkin README | independent |
| https://crates.io/crates/pumpkin-solver | pumpkin-solver crates.io | independent |
| https://pypi.org/pypi/pumpkin-solver/json | pumpkin-solver PyPI metadata | independent |
| https://docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html | Pumpkin docs.rs all-items | independent |
| https://docs.rs/pumpkin-solver/latest/pumpkin_solver/ | Pumpkin docs.rs landing | independent |
| https://github.com/ConSol-Lab/Pumpkin/blob/main/README.md | Pumpkin README building section | independent |
| https://api.github.com/repos/ConSol-Lab/Pumpkin/commits | Pumpkin commits API | independent |
| https://www.minizinc.org/challenge/2025/results/ | MiniZinc Challenge 2025 | independent |
| https://www.minizinc.org/challenge/2024/results/ | MiniZinc Challenge 2024 | independent |
| https://www.minizinc.org/challenge/2023/results/ | MiniZinc Challenge 2023 | independent |
| https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2025.35 | Sidorov et al. CP 2025 "Unite and Lead" | independent |
| https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2024.11 | Flippo et al. CP 2024 proof logging | independent |
| https://dbai.tuwien.ac.at/staff/musliu/emird.pdf | Demirović PhD thesis: SAT-based Approaches for the General High School Timetabling Problem | independent |
| https://pypi.org/project/python-sat/ | python-sat PyPI | independent |
| https://pysathq.github.io/updates/ | PySAT updates | independent |
| https://pysathq.github.io/docs/html/api/rc2.html | PySAT RC2 docs | independent |
| https://pysathq.github.io/docs/html/api/card.html | PySAT cardinality docs | independent |
| https://github.com/alexander-nadel-academic/tt-open-wbo-inc/ | TT-Open-WBO-Inc | independent |
| https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | Demirović and Musliu MaxSAT-LNS | independent |
| https://link.springer.com/article/10.1007/s10951-021-00695-6 | UniCorT in Journal of Scheduling | independent |
| https://patatconference.org/patat2022/proceedings/PATAT_2022_paper_29.pdf | UniTime ITC2019 MaxSAT (PATAT 2022 paper 29) | independent |
| https://github.com/rust-or/good_lp | good_lp README | independent |
| https://github.com/rust-or/good_lp/blob/main/README.md | good_lp README (main branch) | independent |
| https://github.com/ERGO-Code/HiGHS/releases | HiGHS releases | independent |
| https://github.com/rust-or/highs-sys/blob/master/README.md | highs-sys README | independent |
| https://api.github.com/repos/rust-or/highs | rust-or/highs repo metadata | independent |
| https://pypi.org/project/highspy/ | highspy PyPI | independent |
| https://github.com/scipopt/russcip | russcip README | independent |
| https://api.github.com/repos/scipopt/russcip | russcip repo metadata | independent |
| https://arxiv.org/html/2511.18580v1 | SCIP 10.0 paper | independent |
| https://github.com/scipopt/PySCIPOpt/blob/master/RELEASE.md | PySCIPOpt RELEASE | independent |
| https://plato.asu.edu/ftp/milp.html | Mittelmann MILP benchmark | independent |
| https://github.com/ERGO-Code/HiGHS/discussions/1683 | HiGHS Discussion 1683 | practitioner |
| https://optimization-online.org/wp-content/uploads/2025/09/main-arXiv.pdf | Bucknell exam scheduling preprint | independent |
| https://dsumsoftware.com/itc2019/ | DSUM ITC 2019 | practitioner |
| https://ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00724-y.html | Holm graph-based MIP, J. Scheduling | independent |
| https://ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00728-8.html | Lemos parallelized matheuristic ITC2019 | independent |
| https://www.sciencedirect.com/science/article/pii/S0305054814001816 | Fix-and-optimize XHSTT | independent |
| https://www.sciencedirect.com/science/article/abs/pii/S0377221717302242 | Fonseca matheuristic XHSTT-2014 | independent |
| https://www.sciencedirect.com/science/article/pii/S3050784725000157 | IHTC2024 competition report | independent |
| https://arxiv.org/abs/2511.04685 | Twente IHTC2024 hybrid | independent |
| https://roar-net.eu/news/ihtc-2024-best-oss-prize/ | SDU-IMADA Open-Source Prize | practitioner |
| https://arxiv.org/html/2407.16898v1 | Falkner et al. German school timetabling | independent |
| https://schedulingseminar.com/presentations/SchedulingSeminar_LaurentPerron.pdf | Perron CP-SAT for scheduling | vendor |
| https://egon.cheme.cmu.edu/ewo/docs/CP-SAT%20and%20OR-Tools.pdf | Perron CP-SAT and OR-Tools | practitioner |
| https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2023.3 | Perron CP 2023 invited talk | vendor |
| https://d-krupke.github.io/cpsat-primer/ | CP-SAT Primer | practitioner |
| https://d-krupke.github.io/cpsat-primer/09_lns.html | CP-SAT Primer LNS chapter | practitioner |
| https://link.springer.com/chapter/10.1007/978-3-319-93031-2_10 | CP hot-start XHSTT | independent |
| https://link.springer.com/chapter/10.1007/978-3-031-74209-5_5 | ASP-LNPS course timetabling | independent |
| https://www.sciencedirect.com/science/article/pii/S0377221722005641 | Ceschia et al. EJOR 2023 | independent |
| https://arxiv.org/abs/2201.07525 | Ceschia preprint | independent |
| https://www.researchgate.net/publication/364582707 | SLR metaheuristics XHSTT | independent |
| https://www.mdpi.com/2079-3197/13/1/10 | 95-paper IP review | independent |
| https://www.utwente.nl/en/eemcs/dmmp/hstt/ | DMMP HSTT XHSTT archive | independent |
| https://link.springer.com/article/10.1007/s10951-014-0405-x | Kristiansen IP for HSTT | independent |
| https://github.com/CPMpy/cpmpy | CPMpy README | independent |
| https://github.com/Z3Prover/z3 | Z3 README | independent |
| https://pypi.org/project/z3-solver/ | z3-solver PyPI | independent |
| https://crates.io/crates/z3 | z3 Rust crate | independent |
| https://pmc.ncbi.nlm.nih.gov/articles/PMC5411413/ | Z3-bitvector XHSTT (PMC5411413) | independent |
| https://github.com/Gecode/gecode/releases | Gecode releases | independent |
| https://pypi.org/project/gecode-python/ | gecode-python PyPI | independent |
| https://lalescu.ro/liviu/fet/ | FET home | practitioner |
| https://www.timetabling.de/manual/FET-manual.en.html | FET Manual | practitioner |
| https://lalescu.ro/liviu/fet/doc/en/faq.html | FET FAQ | practitioner |
| https://manpages.ubuntu.com/manpages/bionic/man1/fet-cl.1.html | fet-cl(1) | independent |
| https://www.gnu.org/licenses/agpl-3.0.html | AGPL-3.0 | independent |
| https://www.tablix.org/ | Tablix | practitioner |
| https://timefinder.sourceforge.net/ | TimeFinder | practitioner |
| https://www.hexaly.com/pricing | Hexaly Pricing | vendor |
| https://www.hexaly.com/benchmarks/hexaly-vs-cp-optimizer-vs-or-tools-on-the-job-shop-scheduling-problem-jssp | Hexaly JSSP benchmark | vendor |
| https://www.vendr.com/buyer-guides/localsolver | Vendr LocalSolver pricing | journalism |
| https://github.com/SolverForge/solverforge | SolverForge README | practitioner |
| https://solverforge.org/about/ | SolverForge About | practitioner |
| https://github.com/N-Wouda/ALNS | ALNS README | practitioner |
| https://pypi.org/project/alns/ | alns PyPI | independent |
| https://joss.theoj.org/papers/10.21105/joss.05028 | ALNS JOSS | independent |
| https://api.github.com/repos/argmin-rs/argmin | argmin repo metadata | independent |
| https://github.com/argmin-rs/argmin | argmin README | practitioner |
| https://github.com/lucidfrontier45/localsearch | localsearch README | independent |
| https://api.github.com/repos/Martin1887/oxigen | oxigen repo metadata | independent |
| https://api.github.com/repos/innoave/genevo | genevo repo metadata | independent |
| https://api.github.com/repos/jix/varisat | varisat repo metadata | independent |
| https://api.github.com/repos/shnarazk/splr | splr repo metadata | independent |
| https://github.com/chrjabs/rustsat | rustsat README | independent |
| https://arxiv.org/html/2505.15221v1 | RustSAT SAT 2025 | independent |
| https://api.github.com/repos/ffminus/copper | copper repo | independent |
| https://crates.io/crates/copper | copper crates.io | practitioner |
| https://github.com/yangeorget/nucs | NuCS README | independent |
| https://pypi.org/pypi/nucs/json | NuCS PyPI metadata | independent |
| https://www.minizinc.org/downloads/ | MiniZinc Downloads | vendor |
| https://github.com/MiniZinc/minizinc-python/releases | minizinc-python releases | independent |
| https://docs.minizinc.dev/en/stable/installation.html | MiniZinc install | independent |
| https://github.com/google/or-tools/issues/4398 | or-tools issue 4398 (fzn-cp-sat UNKNOWN bug) | independent |
| https://github.com/chocoteam/pychoco | PyChoco README | independent |
| https://www.graalvm.org/latest/reference-manual/native-image/optimizations-and-performance/ | GraalVM native-image performance | vendor |
| https://github.com/arminbiere/cadical | CaDiCaL README | independent |
| https://satcompetition.github.io/2025/satcomp25slides.pdf | SAT Competition 2025 results | independent |
| https://www.cs.cmu.edu/~csd-phd-blog/2024/cardinality-constraints/ | Cardinality treewidth blog post | independent |
| https://github.com/coin-or/Cbc/releases | Cbc releases | independent |
| https://lists.gnu.org/archive/html/info-gnu/2020-12/msg00007.html | GLPK 5.0 release announcement | independent |
| https://www.gnu.org/software/glpk/ | GLPK GNU Project | independent |
| https://en.wikipedia.org/wiki/COIN-OR | COIN-OR Wikipedia | independent |
| https://highs.dev/ | HiGHS home | vendor |
| https://highs.dev/assets/HiGHS_funding_proposal.pdf | HiGHS funding proposal | vendor |
| https://link.springer.com/article/10.1007/s12532-023-00234-8 | Feasibility Jump MPC | independent |
| https://dev.ampl.com/solvers/highs/options.html | HiGHS AMPL options | vendor |
