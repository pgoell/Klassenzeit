# Synthesis (Iteration 1)

## Thesis

For the OPEN_THINGS item 56 spike, Pascal should bake off **(1) Pumpkin** as the primary candidate, **(2) PySAT + RC2 / TT-Open-WBO-Inc MaxSAT** as the secondary candidate, and **(3) good_lp + HiGHS (or russcip + SCIP) MIP** as the tertiary candidate, in that priority order; the load-bearing reason is that Pumpkin uniquely combines (a) a different solver class than both LAHC and CP-SAT (LCG-CP with first-class `cumulative` and `disjunctive` globals), (b) a pure-Rust integration that fits `BenchBackend` without a Python peer module, and (c) the same intellectual lineage as the only public solver with documented XHSTT-best-result wins outside MIP (Demirović's MaxSAT-LNS), while MaxSAT and MIP cover the two solver classes with the strongest empirical track record on XHSTT and ITC2019 respectively when CP-SAT plateaus.

## Argument Structure

1. **Solver-class diversity is the only lever that matters when CP-SAT and LAHC plateau on the same axis.** Adding another LP-based engine, another CDCL-only SAT solver, or another LAHC variant would not produce a new failure mode. The literature converges on three classes that beat CP-SAT on educational timetabling under specific regimes: pure MIP via fix-and-optimize matheuristic (ITC2019 winner DSUM), MaxSAT-based LNS (Demirović XHSTT new-best results), and external local search atop a CP/MIP backbone (IHTC2024 podium).
2. **Maintenance, license, and Python 3.14 / Rust toolchain compatibility are gates, not nice-to-haves.** Several otherwise interesting candidates (FET, Tablix, copper, varisat, oxigen, genevo) fail on these gates and must be excluded before scoring on technical fit.
3. **Integration cost into the existing `BenchBackend` enum and the canonical Rust scorer (ADR 0029, ADR 0030) is materially lower for pure-Rust crates than for engines requiring a Python peer module or a subprocess shim.** Pumpkin is uniquely lowest-friction; PySAT and HiGHS are mid-friction (Python peer module mirroring `klassenzeit_solver/cpsat.py`); FET via subprocess and Hexaly via paid license are highest-friction or out.

## Claims by Sub-Question

### SQ1: Which solver classes / engines pass the maintenance, license, and toolchain gates?

- Claim: **Pumpkin (Rust LCG-CP, TU Delft) passes all gates.** v0.3.0 released 2026-02-11; main commits as recent as 2026-05-06; dual Apache-2.0 / MIT; pure Rust crate `pumpkin-solver`; PyPI `pumpkin-solver` ships cp314 manylinux wheels; PyO3 bindings via maturin; cumulative + disjunctive + all_different + element + table globals exposed at the Rust API.
  - Sources: [crates.io/crates/pumpkin-solver | pumpkin-solver crates.io | independent], [github.com/ConSol-Lab/Pumpkin | Pumpkin README | independent], [pypi.org/pypi/pumpkin-solver/json | PyPI JSON | independent], [docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html | docs.rs all-items | independent].
  - Notes: Bronze in MiniZinc Challenge 2025 fixed-search track; tied with SICStus Prolog; CP 2025 paper "Unite and Lead" demonstrates new RCPSP/RCPSP-max bounds. Lead author (Demirović) co-authored the foundational MaxSAT-LNS-on-XHSTT papers, which is direct school-timetabling pedigree even though Pumpkin itself ships no XHSTT example.
- Claim: **PySAT + Kissat / CaDiCaL + RC2 / TT-Open-WBO-Inc MaxSAT passes all gates.** python-sat 1.9.dev2 (2026-03-05) ships cp314 wheels; MIT; bundles CaDiCaL 1.9.5 / Kissat 4.0.4; RC2 was top-ranked in MaxSAT Evaluations 2018-2019; TT-Open-WBO-Inc backed MaxSAT-Eval 2023 / 2024 winners.
  - Sources: [pypi.org/project/python-sat/ | python-sat PyPI | independent], [pysathq.github.io/updates/ | PySAT updates | independent], [github.com/alexander-nadel-academic/tt-open-wbo-inc/ | TT-Open-WBO-Inc | independent].
  - Notes: TT-Open-WBO-Inc has no PyPI release; would ship as a C++ binary alongside Klassenzeit, not as a Python wheel. Encoding cost is significant; cardinality encodings can blow up treewidth Ω(n).
- Claim: **good_lp + HiGHS passes all gates with caveats.** good_lp 1.15.1 (2026-04-07), MIT; HiGHS 1.14.0 (2026-04-06), MIT. HiGHS-Rust statically links HiGHS C++ but requires g++ + cmake at build. Python: highspy 1.14.0 ships cp314 wheels.
  - Sources: [github.com/rust-or/good_lp | good_lp README | independent], [github.com/ERGO-Code/HiGHS/releases | HiGHS Releases | independent], [pypi.org/project/highspy/ | highspy PyPI | independent].
- Claim: **russcip + SCIP passes all gates.** russcip 0.9.1 (2025-08-26), Apache-2.0; SCIP 10.0 (Nov 2025), Apache-2.0; bundled feature avoids system SCIP install.
  - Sources: [github.com/scipopt/russcip | russcip README | independent], [arxiv.org/html/2511.18580v1 | SCIP 10.0 paper | independent].
- Claim: **SolverForge (Rust, Apache-2.0, OptaPlanner-like LAHC framework) passes the maintenance gate but is NOT a third solver class.** v0.11.1 (2026-05-05); 990 commits; algorithms include Late Acceptance, Tabu, SA, Great Deluge, Step Counting Hill Climbing.
  - Sources: [github.com/SolverForge/solverforge | SolverForge README | practitioner], [solverforge.org/about/ | SolverForge About | practitioner].
  - Notes: Klassenzeit's existing LAHC bench already covers most of this catalogue. Adding SolverForge would be a fifth LAHC-family variant, not a third solver class.
- Claim: **Z3 SMT passes all gates** but trails CP-SAT on scheduling. z3-solver 4.16.0.0 (2026-02-19), MIT; `z3` Rust crate 0.20.0 active. Demirović-Musliu showed Z3-bitvector achieves feasible solutions in 21/23 XHSTT instances and three optima in 24-hour runs but is "not competitive" vs heuristics.
  - Sources: [pypi.org/project/z3-solver/ | z3-solver PyPI | independent], [pmc.ncbi.nlm.nih.gov/articles/PMC5411413/ | Z3-bitvector XHSTT paper | independent].
- Claim: **CPMpy (Python, Apache-2) is a portable layer over OR-Tools, Gurobi, Z3, PySAT, MiniZinc, plus Pumpkin and Choco-via-PyChoco backends.** v0.10.0 (2026-01-19), 3 gold + 1 silver in XCSP3 2024-2025.
  - Sources: [github.com/CPMpy/cpmpy | CPMpy README | independent].
  - Notes: A meta-strategy: ship a CPMpy-fronted backend that internally swaps Pumpkin / Z3 / Gurobi-via-CPMpy for one bake-off run.
- Claim: **MiniZinc 2.9.7 (April 2026) passes the toolchain gate** but its Python binding is a pure-Python wrapper around the system `minizinc` binary; deployment must ship a tens-to-hundreds-of-MB compiler.
  - Sources: [www.minizinc.org/downloads/ | MiniZinc Downloads | vendor], [github.com/MiniZinc/minizinc-python/releases | minizinc-python releases | independent].
- Claim: **The following candidates FAIL one or more gates and are excluded:** Gecode (gecode-python frozen 2012, no Rust crate); GLPK 5.0 (December 2020 last release, GPL); CBC (maintenance-mode, EPL not strictly permissive); FET (AGPL-3.0 yellow flag, executable-only architecture, subprocess-and-XML integration); UniTime (Java); Tablix (~2009 last release); TimeFinder (Java + abandoned); Hexaly (commercial $29-49K/yr); copper (last push 2024-01-10, ~28 months stale); pcp (~18 months stale); varisat (~3.5 years stale); oxigen (~5 years stale); genevo (~15 months stale, NOASSERTION license); metaheurustics-rs (~16 months stale, continuous-only); netaheuristics (1 star, scope incomplete); microlp ("early-stage, panics on hard problems"); ALNS (last release 2024-10-21, on the wrong side of the 12-month gate by release-date; main-branch activity unverified).
  - Sources: [github.com/Gecode/gecode/releases | Gecode releases | independent], [pypi.org/project/gecode-python/ | gecode-python PyPI | independent], [www.gnu.org/software/glpk/ | GLPK GNU Project | independent], [github.com/coin-or/Cbc/releases | Cbc releases | independent], [www.tablix.org/ | Tablix | independent], [timefinder.sourceforge.net/ | TimeFinder | practitioner], [www.hexaly.com/pricing | Hexaly Pricing | vendor], [www.vendr.com/buyer-guides/localsolver | Vendr LocalSolver pricing | journalism], [api.github.com/repos/ffminus/copper | copper repo metadata | independent], [api.github.com/repos/jix/varisat | varisat repo metadata | independent], [api.github.com/repos/Martin1887/oxigen | oxigen repo metadata | independent].
- Contradictions: Gecode releases page shows 6.2.0 (2024-04) but a separate WebFetch summary said "latest release dates back over five years"; the releases page wins as the authoritative source. ALNS release-date gate vs main-branch activity is unresolved without a fresh commit-log check.

### SQ2: Which solver class is best supported by school-timetabling-specific empirical evidence?

- Claim: **MIP-with-matheuristic has the strongest 2019-2024 competition track record on educational timetabling.** ITC2019 was won by DSUM with Gurobi 8.1.1 + fix-and-optimize MIP matheuristic; 5 instances proven optimal.
  - Sources: [dsumsoftware.com/itc2019/ | DSUM ITC 2019 | practitioner], [ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00724-y.html | Holm graph-based MIP | independent].
- Claim: **MaxSAT-based LNS produced new XHSTT best-known upper bounds.** Demirović & Musliu (2017) modified Open-WBO and "managed to compute four new best known upper bounds for high school timetabling problems."
  - Sources: [www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | Demirović-Musliu MaxSAT-LNS | independent].
- Claim: **CP-SAT (the current Klassenzeit production candidate) sweeps gold at the MiniZinc Challenge** in 2023, 2024, and 2025 across Fixed, Free, Parallel, and Local Search categories.
  - Sources: [www.minizinc.org/challenge/2025/results/ | MiniZinc Challenge 2025 | independent], [www.minizinc.org/challenge/2024/results/ | MiniZinc Challenge 2024 | independent], [www.minizinc.org/challenge/2023/results/ | MiniZinc Challenge 2023 | independent].
- Claim: **Pure CP-SAT and pure ILP both fail on real-world German high schools at scale.** "Out of 18 instances, solutions were found for only 10 instances (55% success rate)" with Gurobi + 6h limit on the Falkner formulation; "even after 6 hours of runtime, it could only find solutions that are nowhere near satisfactory."
  - Sources: [arxiv.org/html/2407.16898v1 | Falkner et al. German school timetabling | independent].
- Claim: **The hybrid CP + MIP + SA pattern is the dominant production-grade recipe.** IHTC2024 third-place team Twente used "mixed-integer programming, constraint programming, and simulated annealing in a 3-phase solution approach"; second-place SDU-IMADA used "local-search-based meta-heuristic algorithm implemented in Python and C++"; first-place v777v was entirely heuristic.
  - Sources: [arxiv.org/abs/2511.04685 | Twente IHTC2024 paper | independent], [roar-net.eu/news/ihtc-2024-best-oss-prize/ | SDU-IMADA Open-Source Prize | practitioner], [www.sciencedirect.com/science/article/pii/S3050784725000157 | IHTC2024 competition report | independent].
- Claim: **No 2022-2026 paper documents a measurable win for HiGHS, SCIP, Gecode, or Pumpkin over CP-SAT on a school-timetabling-shaped benchmark.** This is a negative finding from saturated searches.
  - Sources: [highs.dev/ | HiGHS home (negative finding) | independent], [www.mdpi.com/2079-3197/13/1/10 | 95-paper IP review | independent].
  - Notes: This is a critical caveat for the thesis. The case for any candidate over CP-SAT is structural (different solver class to break a plateau) rather than empirically pre-validated.
- Contradictions: ITC2011 was "dominated by metaheuristic methods" (GOAL won with hybrid LS); ITC2019 was won by pure MIP; IHTC2024 was won by pure heuristic. The dominant solver class shifts by problem variant. For Klassenzeit's Hessen Grundschule + Sek-I/II shape, neither extreme fully predicts.

### SQ3: What are the integration costs into `BenchBackend` and the Rust scorer?

- Claim: **Pumpkin has the lowest integration cost.** `cargo add pumpkin-solver`; pure Rust; no system deps; direct Rust API (`Solver::default()`, `new_bounded_integer`, `add_constraint`); a `BenchBackend::Pumpkin` variant fits the existing enum without a Python peer module or a subprocess shim.
  - Sources: [github.com/ConSol-Lab/Pumpkin/blob/main/README.md | Pumpkin README building | independent], [docs.rs/pumpkin-solver/latest/pumpkin_solver/ | docs.rs landing | independent].
  - Notes: ADR 0030 precedent (CP-SAT via `ortools` Python wheel + Rust scorer) does not need to be re-applied for a pure-Rust backend.
- Claim: **PySAT + MaxSAT integration cost is medium.** Python wheel ships cp314; bundled CaDiCaL / Kissat avoids extra system installs; integration mirrors `klassenzeit_solver/cpsat.py`. The encoding cost of WCNF for Klassenzeit's soft-constraint set (gaps, prefs, balance) is the dominant work, not the binding plumbing.
  - Sources: [pypi.org/project/python-sat/ | python-sat PyPI | independent], [pysathq.github.io/docs/html/api/card.html | PySAT cardinality | independent].
- Claim: **good_lp + HiGHS integration cost is medium.** Adds g++ + cmake to the build environment, statically linked HiGHS at runtime. CI runner already builds Rust toolchain plus solver-py via maturin, so this is incremental rather than a new class of dependency. MIP encoding for Klassenzeit's hard + soft constraints will need big-M reformulations of disjunctive globals.
  - Sources: [github.com/rust-or/good_lp/blob/main/README.md | good_lp README | independent], [github.com/rust-or/highs-sys/blob/master/README.md | highs-sys README | independent].
- Claim: **russcip + SCIP integration cost is medium-high.** Bundled binary feature available; SCIP itself is several MB of binary, larger than HiGHS or pumpkin-solver.
  - Sources: [github.com/scipopt/russcip | russcip README | independent].
- Claim: **FET integration cost is high.** AGPL-yellow; subprocess via `fet-cl --inputfile=...`; XML round-trip; unreliable exit codes ("0 sometimes means error"); requires parsing `result.txt`. Architecture is executable-only; no `libfet` decoupling exists.
  - Sources: [www.timetabling.de/manual/FET-manual.en.html | FET Manual | practitioner], [manpages.ubuntu.com/manpages/bionic/man1/fet-cl.1.html | fet-cl(1) | independent], [lalescu.ro/liviu/fet/doc/en/faq.html | FET FAQ | practitioner].
- Claim: **CPMpy meta-backend integration cost is medium.** A single Python peer module fronts multiple solvers (Pumpkin / Z3 / Gurobi-via-CPMpy / OR-Tools-via-CPMpy). Useful for de-risked exploration but adds an indirection layer that Klassenzeit can avoid by binding to Pumpkin directly.
  - Sources: [github.com/CPMpy/cpmpy | CPMpy README | independent].

### SQ4: What does the comparative-evidence cluster say about CP-SAT plateau-breakers specifically?

- Claim: **Fix-and-optimize matheuristic over a MIP model is the single most-cited plateau-breaker for educational timetabling.** Fonseca et al. on XHSTT-2014: matheuristic over an alternative MIP formulation provided "four new best known lower bounds and improved eleven best known solutions." DSUM ITC2019 winner uses the same pattern.
  - Sources: [www.sciencedirect.com/science/article/abs/pii/S0377221717302242 | Fonseca matheuristic | independent], [dsumsoftware.com/itc2019/ | DSUM ITC2019 | practitioner].
- Claim: **MaxSAT-based LNS is the second most-cited plateau-breaker.** Demirović-Musliu set new XHSTT best-known upper bounds; UniCorT (J. Scheduling 2022) used TT-Open-WBO-Inc to solve all ITC2019 instances by 2022.
  - Sources: [www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | MaxSAT-LNS XHSTT | independent], [link.springer.com/article/10.1007/s10951-021-00695-6 | UniCorT | independent], [patatconference.org/patat2022/proceedings/PATAT_2022_paper_29.pdf | ITC2019 UniTime MaxSAT | independent].
- Claim: **CP with hot-start and phase-saving (improving CP-SAT-shaped solvers themselves) is the third plateau-breaker.** "A drastic improvement in performance can be achieved by including solution-based phase saving... and hot start approaches where existing heuristic methods produce a starting point for the CP solver."
  - Sources: [link.springer.com/chapter/10.1007/978-3-319-93031-2_10 | CP hot-start XHSTT | independent].
  - Notes: This favours feeding LAHC seeds into CP-SAT/Pumpkin, not adding a new solver class.
- Claim: **CP-SAT internal LNS already runs.** "CP-SAT schedules its LNS strategies using a simple round-robin method." Adding an external LNS layer is a different lever (custom destroy/repair on Klassenzeit's domain).
  - Sources: [d-krupke.github.io/cpsat-primer/09_lns.html | CP-SAT Primer LNS | practitioner].
- Claim: **CP-SAT beats MIP on small-to-medium scheduling problems but Gurobi beats CP-SAT on pure linear-integer problems.** Per Perron (Google maintainer): "On CP problems, CP-SAT beats Gurobi"; "On linear integer problems, CP-SAT beats SCIP, is not far from CPLEX, and sometimes wins against Gurobi, but not often."
  - Sources: [egon.cheme.cmu.edu/ewo/docs/CP-SAT%20and%20OR-Tools.pdf | Perron CP-SAT and OR-Tools | practitioner], [schedulingseminar.com/presentations/SchedulingSeminar_LaurentPerron.pdf | Perron CP-SAT for scheduling | vendor].
- Claim: **Open-source MIP gap to commercial is one order of magnitude.** "For MIP problems, open source solvers like HiGHS, CBC, and SCIP perform about the same, while commercial solvers (CPLEX, XPRESS, and Gurobi) are about two orders of magnitude faster."
  - Sources: [github.com/ERGO-Code/HiGHS/discussions/1683 | HiGHS Discussion 1683 | practitioner], [plato.asu.edu/ftp/milp.html | Mittelmann MILP benchmark | independent].
  - Notes: The DSUM ITC2019 win used commercial Gurobi; whether HiGHS or SCIP is "good enough" for Klassenzeit's instance sizes is the central empirical question for the spike.

### SQ5: What does the Rust-native solvers cluster say?

- Claim: **Pumpkin is the only Rust-native CP solver passing the maintenance gate.** Copper, pcp, varisat, oxigen, genevo all fail; argmin is continuous-only; localsearch lacks LAHC and LNS; SolverForge is a LAHC-family framework rather than a third solver class.
  - Sources: [api.github.com/repos/ConSol-Lab/Pumpkin/commits | Pumpkin commits | independent], [api.github.com/repos/ffminus/copper | copper repo | independent], [api.github.com/repos/argmin-rs/argmin | argmin repo | independent].
- Claim: **Rust SAT crates (splr, batsat, rustsat) are maintained but require full WCNF re-encoding** of Klassenzeit's soft-constraint set. Encoding cost dwarfs binding cost.
  - Sources: [api.github.com/repos/shnarazk/splr | splr repo | independent], [github.com/chrjabs/rustsat | rustsat README | independent], [arxiv.org/html/2505.15221v1 | RustSAT SAT 2025 paper | independent].
- Claim: **Rust MIP wrappers (good_lp, highs, russcip) are maintained.** good_lp 1.15.1 fronts CBC / HiGHS / microlp / SCIP / clarabel via Cargo features.
  - Sources: [github.com/rust-or/good_lp/blob/main/README.md | good_lp README | independent], [api.github.com/repos/rust-or/highs | highs repo | independent], [api.github.com/repos/scipopt/russcip | russcip repo | independent].

## Reconciled Contradictions

1. **Gecode maintenance: "5 years stale" vs "2024-04 release"** — Two researcher snapshots conflict. The GitHub releases page is authoritative and shows 6.2.0 in April 2024, plus 6.1.1 (Feb 2024) and 6.1.0 (Oct 2023). A development 6.3.0 line is in Guix patch-set; Debian packaging activity continued into 2025. Verdict: Gecode upstream is **slow but not stale**. The disqualifier is not maintenance but the **frozen-since-2012 Python binding** (`gecode-python`) and the absence of any maintained Rust crate; Klassenzeit cannot integrate Gecode without writing its own binding.

2. **MIP-vs-metaheuristic dominance on educational timetabling** — ITC2011 was won by metaheuristic (GOAL, hybrid LS); ITC2019 was won by pure MIP (DSUM, Gurobi + fix-and-optimize); IHTC2024 was won by pure heuristic (v777v) with second-place using MIP as a feasible-solution generator. The 2025 IP-review counts CPLEX (47), Gurobi (11), CP-SAT (1) across 95 university-timetabling papers. Verdict: **The dominant class shifts per problem variant.** Klassenzeit's Hessen Grundschule shape is closer to ITC2011 (hard constraints + soft preferences over a small instance) than ITC2019 (university block scheduling at scale). The Falkner et al. result that pure MIP fails on 18 real German high schools with 6h Gurobi confirms that MIP is not a free win; the value of adding a MIP backend is to compute lower bounds and to drive a fix-and-optimize loop, not to replace LAHC.

3. **CP-SAT plateau-breaker recommendations** — Three options coexist in the literature: matheuristic over MIP (Fonseca, DSUM), MaxSAT-LNS (Demirović), and external local search atop CP/MIP (IHTC2024 Twente). They are not mutually exclusive but they imply different third-backend choices. Verdict: **Pumpkin spans MaxSAT and CP through its LCG architecture** (Pumpkin can solve SAT and MaxSAT problems via its FlatZinc + WCNF frontends per its README) and is co-authored by the same researcher who pioneered MaxSAT-LNS-on-XHSTT. Thus picking Pumpkin partially covers the CP and MaxSAT plateau-breakers in a single integration; MIP remains a separate gap that good_lp + HiGHS or russcip + SCIP fills.

4. **PyChoco / GraalVM "no Java" interpretation** — Brief excludes pure-Java engines. PyChoco compiles Choco-solver to a native shared library via GraalVM; "no JVM at runtime" but the build pipeline pulls Java. The cluster files surface this but do not commit to a verdict. Verdict: **Out under the strictest reading of "no Java" because the build chain is not portable and the maintenance burden of GraalVM native-image is non-trivial**; PyChoco wheel availability is also unconfirmed for Python 3.13/3.14.

5. **ALNS maintenance gate** — Last release v7.0.0 on 2024-10-21 is over 12 months from 2026-05-08. Verdict: **Out under strict reading of the gate**, though main-branch commits may be more recent. The gate-failure call is conservative; if Pascal wants ALNS specifically, a fresh `git log` check is the cheapest way to flip the verdict. ALNS is also Python-only with a custom-encoding cost similar to MaxSAT, so its differentiator vs PySAT-LNS is small.

6. **SCIP vs HiGHS choice for a MIP backend** — Bucknell exam-scheduling case study: "Gurobi obtained slightly better solutions than SCIP, and the final objective values were always within 4% of each other." Mittelmann MILP benchmark: HiGHS solves 162/240 instances vs SCIP 136-150/240. Verdict: **HiGHS wins on benchmark coverage; SCIP wins on the educational-timetabling-shaped Bucknell case.** For Klassenzeit, **HiGHS is preferred for the spike** because (a) MIT license is strictly more permissive than SCIP's Apache-2.0 (within Klassenzeit's policy band, neither is a problem; HiGHS aligns with the existing solver-core MIT/Apache-2 dual licence shape), (b) HiGHS-Rust statically links from a single MIT C++ tree, (c) `highspy` is available and ships cp314 wheels, (d) HiGHS's Feasibility Jump primal heuristic added in v1.11.0 is exactly the plateau-breaker the brief calls out.

## Gaps and Thin Evidence

1. **No published Pumpkin-on-XHSTT benchmark.** Pumpkin's published benchmarks are RCPSP / RCPSP-max (CP 2025 "Unite and Lead"), N-queens, BIBD. School-timetabling fit is inferred from (a) the disjunctive_scheduling.rs example matching the no-double-booking shape and (b) the lead author's MaxSAT-XHSTT track record. The spike itself would be the first public Pumpkin-on-Klassenzeit-shape benchmark. The mitigation is the bake-off: ADR 0029's methodology is designed to surface this kind of empirical question.

2. **No 2022-2026 head-to-head of HiGHS vs CP-SAT on school timetabling.** Saturated searches across "HiGHS school timetabling case study" returned no result. The Bucknell exam-scheduling case ranks Gurobi over SCIP within 4%; HiGHS-vs-Gurobi on that problem is unknown. The MaxSAT-LNS literature does not benchmark against HiGHS.

3. **CP-SAT plateau characterization for Klassenzeit is hypothetical.** The brief specifies the spike trigger as "Rust LAHC and CP-SAT both plateau on the same quality axis." That trigger has not yet fired. The synthesis cannot tell which axis would plateau first (gaps? balance? makespan-equivalent?); the answer determines whether MIP (good for proven-optimal lower bounds) or Pumpkin (good for unsat proofs and disjunctive cliques) is the right tool. The bake-off design needs to record per-axis solution quality, not just overall.

4. **Python 3.14 wheel coverage is incomplete for Python-side candidates.** ALNS, mealpy, jMetalPy, CPMpy, PyChoco, MiniZinc-python, minilp variants do not list cp314 in PyPI classifiers, even where pure-Python wheels work in practice. The cluster's "Python 3.14 wheel readiness" tables show that **Pumpkin (cp314 manylinux), PySAT (cp314), highspy (cp314), and z3-solver (py3 generic)** are the four with confirmed cp314 wheels. This narrows the Python-side field considerably.

5. **No evidence of a MIP-as-validator hybrid in Klassenzeit's exact problem shape.** The IHTC2024 pattern (CP for feasibility + MIP for lower bounds + SA for quality) is the closest precedent. Whether the Klassenzeit FFD+LAHC seed plus a MIP-validator upgrade would beat the current LAHC+CP-SAT pair is the question the spike must answer; literature offers a precedent but no quantification.

6. **License nuance for FET subprocess separation is not legally settled.** "Mere aggregation" via subprocess is the practitioner consensus but FSF guidance treats coupling on a case-by-case axis. Klassenzeit cannot rely on this without OSS counsel; FET is therefore an inferior third-backend candidate even setting aside the integration friction.

## Outliers / Counter-Evidence

1. **CP-SAT may already be the practical ceiling for OSS school-timetabling.** Perron's claim that "On CP problems, CP-SAT beats Gurobi" plus the MiniZinc Challenge sweeps 2023-2025 means a third backend may not produce a measurable timetabling-specific win on Klassenzeit's scale. The synthesis acknowledges this risk: the bake-off may well conclude that LAHC + CP-SAT is the production ceiling and the third backend is dropped. That outcome is itself useful (it would close item 56 with a "no addition" ADR analogous to ADR 0035 on Timefold).

2. **SolverForge-style external LAHC framework is the lowest-risk addition but the lowest-information addition.** It would generate another Rust LAHC variant rather than testing a different solver class. The brief's framing ("when CP-SAT plateaus") rules this out as the primary spike target; it remains a footnote-worthy "if you wanted to replace the in-house LAHC with a more featureful framework" path.

3. **CPMpy as a single backend that internally fronts multiple engines** is a viable but inferior alternative to a direct Pumpkin integration. The indirection layer adds a Python peer module dependency for a problem (Klassenzeit shape) that Pumpkin's Rust API can solve without it. CPMpy makes more sense as a parallel exploration than as the spike target.

4. **MiniZinc as a portability layer** is attractive on paper (one model, four backends including CP-SAT, Gecode, HiGHS, Chuffed) but its Python binding is a subprocess shell-out, the bundled compiler is heavy, and the fzn-cp-sat backend has known binding-quality issues (status UNKNOWN bug, missing-shared-library bug). For a third-backend spike, this is more friction than direct Pumpkin or PySAT integration.

5. **Hexaly is a legitimate technical fit but a hard license-out.** Multiple vendor benchmarks show Hexaly outperforming OR-Tools / Gurobi / CPLEX on JSSP at very large scale, and YDUQS (Brazilian universities) chose Hexaly for school timetabling after benchmarking against four alternatives. The brief's no-commercial rule excludes it; flagging this here so the rejection is conscious rather than accidental.

## Source Inventory

| Source | Title | Tag | Used in claims |
|---|---|---|---|
| https://github.com/ConSol-Lab/Pumpkin | Pumpkin README | independent | SQ1 (Pumpkin maintenance/license/API), SQ3 (integration cost), SQ5 |
| https://crates.io/crates/pumpkin-solver | pumpkin-solver crates.io | independent | SQ1 (Pumpkin release cadence) |
| https://pypi.org/pypi/pumpkin-solver/json | pumpkin-solver PyPI metadata | independent | SQ1 (cp314 wheel coverage) |
| https://docs.rs/pumpkin-solver/latest/pumpkin_solver/all.html | Pumpkin docs.rs all-items | independent | SQ1 (constraint coverage) |
| https://www.minizinc.org/challenge/2025/results/ | MiniZinc Challenge 2025 | independent | SQ2 (CP-SAT gold sweep, Pumpkin bronze) |
| https://www.minizinc.org/challenge/2024/results/ | MiniZinc Challenge 2024 | independent | SQ2 (Pumpkin no medal, CP-SAT gold sweep) |
| https://www.minizinc.org/challenge/2023/results/ | MiniZinc Challenge 2023 | independent | SQ2 |
| https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2025.35 | Sidorov et al. CP 2025 "Unite and Lead" | independent | Gaps (Pumpkin RCPSP wins) |
| https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2024.11 | Flippo et al. CP 2024 proof logging | independent | SQ1 (Pumpkin academic backing) |
| https://pypi.org/project/python-sat/ | python-sat PyPI | independent | SQ1 (PySAT cp314 wheels) |
| https://pysathq.github.io/updates/ | PySAT updates | independent | SQ1 (PySAT bundled CaDiCaL/Kissat) |
| https://pysathq.github.io/docs/html/api/rc2.html | PySAT RC2 docs | independent | SQ1 (RC2 MaxSAT track record) |
| https://github.com/alexander-nadel-academic/tt-open-wbo-inc/ | TT-Open-WBO-Inc | independent | SQ1 (MaxSAT Eval winners) |
| https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | Demirović-Musliu MaxSAT-LNS | independent | SQ4 (XHSTT new best-known), Thesis |
| https://dbai.tuwien.ac.at/staff/musliu/emird.pdf | Demirović PhD thesis: SAT-based Approaches for the General High School Timetabling Problem | independent | SQ1 (Pumpkin lead-author lineage), Thesis |
| https://link.springer.com/article/10.1007/s10951-021-00695-6 | UniCorT J. Scheduling | independent | SQ4 (MaxSAT plateau-breaker) |
| https://patatconference.org/patat2022/proceedings/PATAT_2022_paper_29.pdf | UniTime ITC2019 MaxSAT | independent | SQ4 |
| https://github.com/rust-or/good_lp | good_lp README | independent | SQ1, SQ3 (good_lp + HiGHS) |
| https://github.com/ERGO-Code/HiGHS/releases | HiGHS releases | independent | SQ1 (HiGHS 1.14, MIT) |
| https://pypi.org/project/highspy/ | highspy PyPI | independent | SQ1 (cp314 wheels) |
| https://github.com/scipopt/russcip | russcip README | independent | SQ1, SQ3 (russcip + SCIP bundled) |
| https://arxiv.org/html/2511.18580v1 | SCIP 10.0 paper | independent | SQ1 (SCIP Apache-2.0) |
| https://github.com/scipopt/PySCIPOpt/blob/master/RELEASE.md | PySCIPOpt RELEASE | independent | SQ1 (cp314 wheels for SCIP) |
| https://plato.asu.edu/ftp/milp.html | Mittelmann MILP benchmark | independent | Reconciled #6 (HiGHS vs SCIP) |
| https://github.com/ERGO-Code/HiGHS/discussions/1683 | HiGHS discussion 1683 | practitioner | SQ4 (open-source MIP gap) |
| https://optimization-online.org/wp-content/uploads/2025/09/main-arXiv.pdf | Bucknell exam scheduling | independent | Reconciled #6 (SCIP vs Gurobi within 4%) |
| https://dsumsoftware.com/itc2019/ | DSUM ITC 2019 | practitioner | SQ2, SQ4 (MIP-matheuristic ITC2019 win) |
| https://ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00724-y.html | Holm graph-based MIP | independent | SQ2 |
| https://ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00728-8.html | Lemos parallelized matheuristic ITC2019 | independent | SQ4 |
| https://www.sciencedirect.com/science/article/pii/S0305054814001816 | Fix-and-optimize XHSTT | independent | SQ4 |
| https://www.sciencedirect.com/science/article/abs/pii/S0377221717302242 | Fonseca matheuristic XHSTT-2014 | independent | SQ4 |
| https://www.sciencedirect.com/science/article/pii/S3050784725000157 | IHTC2024 competition report | independent | SQ2 |
| https://arxiv.org/abs/2511.04685 | Twente IHTC2024 hybrid | independent | SQ2, SQ4 (CP+MIP+SA hybrid) |
| https://roar-net.eu/news/ihtc-2024-best-oss-prize/ | SDU-IMADA Open-Source prize | practitioner | SQ2 |
| https://arxiv.org/html/2407.16898v1 | Falkner German school timetabling | independent | SQ2 (pure ILP fails 10/18 schools) |
| https://schedulingseminar.com/presentations/SchedulingSeminar_LaurentPerron.pdf | Perron CP-SAT for scheduling | vendor | SQ4 (CP-SAT vs Gurobi/SCIP) |
| https://egon.cheme.cmu.edu/ewo/docs/CP-SAT%20and%20OR-Tools.pdf | Perron CP-SAT and OR-Tools | practitioner | SQ4 |
| https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2023.3 | Perron CP 2023 invited talk | vendor | SQ4 |
| https://d-krupke.github.io/cpsat-primer/ | CP-SAT Primer | practitioner | SQ4 (CP-SAT internal LNS, alternatives) |
| https://link.springer.com/chapter/10.1007/978-3-319-93031-2_10 | CP hot-start XHSTT | independent | SQ4 |
| https://link.springer.com/chapter/10.1007/978-3-031-74209-5_5 | ASP-LNPS course timetabling | independent | SQ4 |
| https://www.sciencedirect.com/science/article/pii/S0377221722005641 | Ceschia et al. EJOR 2023 | independent | SQ2 |
| https://arxiv.org/abs/2201.07525 | Ceschia preprint | independent | SQ2 |
| https://www.researchgate.net/publication/364582707 | SLR metaheuristics XHSTT | independent | SQ2 |
| https://www.mdpi.com/2079-3197/13/1/10 | 95-paper IP review | independent | SQ2 |
| https://www.utwente.nl/en/eemcs/dmmp/hstt/ | DMMP HSTT XHSTT archive | independent | SQ2 |
| https://link.springer.com/article/10.1007/s10951-014-0405-x | Kristiansen IP for HSTT | independent | SQ2 |
| https://github.com/CPMpy/cpmpy | CPMpy README | independent | SQ3 (CPMpy meta-backend) |
| https://github.com/Z3Prover/z3 | Z3 README | independent | SQ1 (Z3 MIT) |
| https://pypi.org/project/z3-solver/ | z3-solver PyPI | independent | SQ1 (Z3 wheel) |
| https://crates.io/crates/z3 | z3 Rust crate | independent | SQ1 (z3-rs maintained) |
| https://pmc.ncbi.nlm.nih.gov/articles/PMC5411413/ | Z3-bitvector XHSTT | independent | SQ1 (Z3 21/23 feasible) |
| https://github.com/Gecode/gecode/releases | Gecode releases | independent | Reconciled #1 |
| https://pypi.org/project/gecode-python/ | gecode-python PyPI | independent | Reconciled #1 (binding frozen 2012) |
| https://lalescu.ro/liviu/fet/ | FET home | practitioner | SQ1 (FET AGPL) |
| https://www.timetabling.de/manual/FET-manual.en.html | FET Manual | practitioner | SQ3 (FET CLI integration) |
| https://lalescu.ro/liviu/fet/doc/en/faq.html | FET FAQ | practitioner | SQ3 (no library API) |
| https://manpages.ubuntu.com/manpages/bionic/man1/fet-cl.1.html | fet-cl(1) | independent | SQ3 |
| https://www.gnu.org/licenses/agpl-3.0.html | AGPL-3.0 | independent | SQ1 (FET license implication) |
| https://www.tablix.org/ | Tablix | practitioner | SQ1 (out: maintenance) |
| https://timefinder.sourceforge.net/ | TimeFinder | practitioner | SQ1 (out: abandoned) |
| https://www.hexaly.com/pricing | Hexaly Pricing | vendor | Outliers (commercial-only) |
| https://www.hexaly.com/benchmarks/hexaly-vs-cp-optimizer-vs-or-tools-on-the-job-shop-scheduling-problem-jssp | Hexaly JSSP benchmark | vendor | Outliers |
| https://www.vendr.com/buyer-guides/localsolver | Vendr LocalSolver pricing | journalism | Outliers |
| https://github.com/SolverForge/solverforge | SolverForge README | practitioner | SQ1 (SolverForge gates), Outliers |
| https://solverforge.org/about/ | SolverForge About | practitioner | SQ1 |
| https://github.com/N-Wouda/ALNS | ALNS README | practitioner | SQ1 (ALNS edge), Reconciled #5 |
| https://pypi.org/project/alns/ | alns PyPI | independent | SQ1 |
| https://joss.theoj.org/papers/10.21105/joss.05028 | ALNS JOSS | independent | Reconciled #5 |
| https://api.github.com/repos/argmin-rs/argmin | argmin repo metadata | independent | SQ5 |
| https://github.com/argmin-rs/argmin | argmin README | practitioner | SQ5 (continuous-only) |
| https://github.com/lucidfrontier45/localsearch | localsearch README | independent | SQ5 (no LAHC) |
| https://api.github.com/repos/Martin1887/oxigen | oxigen repo metadata | independent | SQ1 (out) |
| https://api.github.com/repos/innoave/genevo | genevo repo metadata | independent | SQ1 (borderline-stale) |
| https://api.github.com/repos/jix/varisat | varisat repo metadata | independent | SQ1 (out) |
| https://api.github.com/repos/shnarazk/splr | splr repo metadata | independent | SQ5 (Rust SAT) |
| https://github.com/chrjabs/rustsat | rustsat README | independent | SQ5 |
| https://arxiv.org/html/2505.15221v1 | RustSAT SAT 2025 | independent | SQ5 |
| https://api.github.com/repos/ffminus/copper | copper repo | independent | SQ1 (out) |
| https://crates.io/crates/copper | copper crates.io | practitioner | SQ1 |
| https://github.com/yangeorget/nucs | NuCS README | independent | SQ1 (NuCS no cumulative) |
| https://pypi.org/pypi/nucs/json | NuCS PyPI metadata | independent | SQ1 |
| https://www.minizinc.org/downloads/ | MiniZinc Downloads | vendor | Outliers (MiniZinc 2.9.7) |
| https://github.com/MiniZinc/minizinc-python/releases | minizinc-python releases | independent | Outliers |
| https://docs.minizinc.dev/en/stable/installation.html | MiniZinc install | independent | Outliers (bundled solvers) |
| https://github.com/google/or-tools/issues/4398 | fzn-cp-sat UNKNOWN bug | independent | Outliers |
| https://github.com/chocoteam/pychoco | PyChoco README | independent | Reconciled #4 |
| https://www.graalvm.org/latest/reference-manual/native-image/optimizations-and-performance/ | GraalVM native-image perf | vendor | Reconciled #4 |
| https://github.com/arminbiere/cadical | CaDiCaL README | independent | SQ1 (CaDiCaL bundled in PySAT) |
| https://satcompetition.github.io/2025/satcomp25slides.pdf | SAT Competition 2025 results | independent | SQ1 (CaDiCaL first place) |
| https://www.cs.cmu.edu/~csd-phd-blog/2024/cardinality-constraints/ | Cardinality treewidth | independent | SQ3 (PySAT encoding cost) |
| https://github.com/coin-or/Cbc/releases | Cbc releases | independent | SQ1 (CBC maintenance-mode) |
| https://lists.gnu.org/archive/html/info-gnu/2020-12/msg00007.html | GLPK 5.0 release | independent | SQ1 (out) |
| https://www.gnu.org/software/glpk/ | GLPK GNU Project | independent | SQ1 (out) |
| https://en.wikipedia.org/wiki/COIN-OR | COIN-OR Wikipedia | independent | SQ1 (CBC EPL) |
| https://highs.dev/ | HiGHS home | vendor / independent | SQ1 (HiGHS workshop), SQ4 (negative finding) |
| https://highs.dev/assets/HiGHS_funding_proposal.pdf | HiGHS funding proposal | vendor | SQ1 (parallel roadmap) |
| https://link.springer.com/article/10.1007/s12532-023-00234-8 | Feasibility Jump MPC | independent | SQ4 (HiGHS Feasibility Jump) |
| https://dev.ampl.com/solvers/highs/options.html | HiGHS AMPL options | vendor | SQ1 (HiGHS no native CP) |
