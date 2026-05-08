# Cluster: Timetabling-Specific and Local-Search Backends

Scope: school-timetabling-specific engines (FET, UniTime, Tablix, TimeFinder, Hexaly), modern hybrid local-search frameworks accessible from Python or Rust (ALNS, mealpy, jMetalPy, argmin, oxigen, genevo, metaheurustics-rs, netaheuristics, SolverForge), and precedent for OptaPlanner-style hybrid local search built without a Java toolchain.

## SQ1 — FET (C++, AGPLv3)

### Identity, license, latest release

- "FET is free software for automatically scheduling the timetable of a school, high-school or university... It is licensed under the GNU Affero General Public License version 3" [source: https://lalescu.ro/liviu/fet/ | FET — Free Timetabling Software | practitioner].
- Latest release: **FET-7.8.5, released 11 April 2026** [source: https://lalescu.ro/liviu/fet/ | FET — Free Timetabling Software | practitioner].
- Written in C++ with Qt; cross-platform (Linux, Windows, macOS) [source: https://en.wikipedia.org/wiki/FET_(timetabling_software) | FET (timetabling software) — Wikipedia | independent].
- Originally "Free Evolutionary Timetabling," initiated in 2002 by Liviu Lalescu [source: https://en.wikipedia.org/wiki/FET_(timetabling_software) | FET (timetabling software) — Wikipedia | independent].

### Algorithm

- Heuristic, **not evolutionary or pure local search**: "The algorithm is heuristic, probably simulating the manual procedure of finding a timetable" [source: https://lalescu.ro/liviu/fet/doc/en/generation-algorithm-description.html | FET — Timetable Generation Algorithm Description | practitioner].
- Discovered in 2007 (introduced in v5.0.0) and replaced an earlier genetic algorithm: "the evolutionary algorithm was only able to solve easy timetables... In summer 2007 the big breakthrough was done. A new heuristic algorithm (based on recursive swapping of activities) was able to solve difficult timetables in a few minutes" [source: https://lalescu.ro/liviu/fet/doc/en/generation-algorithm-description.html | FET — Timetable Generation Algorithm Description | practitioner].
- Recursive-swapping mechanic: place activities sorted by difficulty; if no slot is free, recursively try swapping conflicting activities into other slots up to a bounded recursion depth (default 14) [source: https://lalescu.ro/liviu/fet/doc/en/generation-algorithm-description.html | FET — Timetable Generation Algorithm Description | practitioner].
- Resembles ejection-chain / ejection-tree algorithms rather than LAHC/SA/CP-SAT.

### Architecture: library vs executable

- **No library API.** Distributed as a desktop GUI (`fet`) and a headless CLI (`fet-cl`) [source: https://manpages.ubuntu.com/manpages/bionic/man1/fet-cl.1.html | fet-cl(1) — Ubuntu Manpages | independent].
- Integration shape from FAQ: "If you are a programmer, the file `src/engine/messageboxes.cpp` contains the implementation of various messages, which you can modify to catch them in your program. You can also catch the end of the program (successful or unsuccessful) in the file `src/interface/fet.cpp` in the command-line code part" — i.e., suggested integration is forking the source [source: https://lalescu.ro/liviu/fet/doc/en/faq.html | FET — Frequently asked questions | practitioner].
- CLI invocation: `fet-cl --inputfile=filename.fet [options]` [source: https://manpages.ubuntu.com/manpages/bionic/man1/fet-cl.1.html | fet-cl(1) — Ubuntu Manpages | independent].
- Output: `activities_timetable.xml` plus per-student / per-teacher XML and HTML; logs in `outputdir/logs/result.txt` [source: https://www.timetabling.de/manual/FET-manual.en.html | FET Manual — timetabling.de | practitioner].
- Exit-code semantics are unreliable for programmatic detection: "FET command line has two types of return values (0 or 1), where value 1 means error, but value 0 sometimes means success and sometimes means error (when time is exceeded or aborted)" — practitioners parse `result.txt` instead [source: https://www.timetabling.de/manual/FET-manual.en.html | FET Manual — timetabling.de | practitioner].
- No `libfet`-style fork found; web search for "libfet" returned no decoupled library [author estimate based on web search returning only forks of the full Qt application: karandit/fet, rodolforg/fet, foradian/FET, fet-project/fet, all of which build the full executable].

### Forks and maintenance signals

- `fet-project/fet` GitHub: 16 commits on master, AGPL-3.0, no active community signals (0 forks of itself shown) [source: https://github.com/fet-project/fet | GitHub — fet-project/fet | practitioner].
- `rodolforg/fet`: "Unofficial repository... `dev` branch contains modifications," AGPL-3.0, 1,001 commits on dev, 11 forks, 9 stars, 0 issues, 0 PRs [source: https://github.com/rodolforg/fet | GitHub — rodolforg/fet | practitioner].
- `karandit/fet`: latest release v5.35.1 from January 2018, 73 commits total, "largely inactive" [source: https://github.com/karandit/fet | GitHub — karandit/fet | practitioner].
- `foradian/FET`: 1 commit, CLI-only fork [source: https://github.com/foradian/FET | GitHub — foradian/FET | practitioner].
- The official upstream (lalescu.ro) is the active source; forks lag.

### License implication for Klassenzeit (MIT codebase)

- AGPL-3.0 obligates source disclosure for network-served users of the combined work, not just distributed binaries [source: https://www.gnu.org/licenses/agpl-3.0.html | GNU Affero General Public License v3 | independent].
- "An aggregate is a compilation of a covered work with other separate and independent works that are not by their nature extensions of the covered work and are not combined to form a larger program. Inclusion of a covered work in an aggregate does not cause the [AGPL] license to apply to the other parts of the aggregate" [source: https://www.gnu.org/licenses/agpl-3.0.html | GNU Affero General Public License v3 | independent].
- Practitioner consensus on subprocess separation: "Using subprocess calls or isolated microservices is often recommended since it counts as 'mere aggregation,' not linking, thus avoiding GPL obligations" [source: https://licensecheck.io/blog/viral-licenses-explained | Viral Licenses Explained: GPL, AGPL, and Copyleft | consulting]. Note: this remains a contested boundary; FSF guidance still treats tightly coupled subprocesses on the case-by-case axis [author estimate — consult OSS counsel before relying on subprocess isolation as a license firewall].
- Klassenzeit's brief flags GPL/AGPL as "yellow flag, not auto-exclusion" (see `brief.md`).

### School-timetabling fit

- Native concepts: years/groups (e.g., "a German primary school has years 1 to 4, and a year contains... several groups (classes)"); rooms with type and capacity; teacher max hours per day; students set max hours per day [source: https://www.timetabling.de/manual/FET-manual.en.html | FET Manual — timetabling.de | practitioner].
- Operating modes: "Official" (standard week), "Mornings-Afternoons" (e.g., Morocco, Algeria), "Block planning" (IB schools), "Terms" (Finnish schools) [source: https://en.wikipedia.org/wiki/FET_(timetabling_software) | FET (timetabling software) — Wikipedia | independent].
- Wikipedia notes deployment in Morocco, Algeria, North America, Finland [source: https://en.wikipedia.org/wiki/FET_(timetabling_software) | FET (timetabling software) — Wikipedia | independent]. No specific Hessen/Grundschule case study surfaced.
- Reported runtimes: "FET is able to solve a complicated timetable in maximum 5-20 minutes. For simpler timetables, it may take a shorter time, under 5 minutes (in some cases, a matter of seconds)" — vendor self-report, no peer benchmark [source: https://www.timetabling.de/manual/FET-manual.en.html | FET Manual — timetabling.de | practitioner].

## SQ2 — UniTime, Tablix, TimeFinder

### UniTime (Java — out of scope per brief)

- "Comprehensive University Timetabling System" hosted at github.com/UniTime/unitime [source: https://github.com/UniTime/unitime | GitHub — UniTime/unitime | practitioner]. Java; excluded by item 56's no-Java rule.

### Tablix (C, abandoned)

- "Tablix is a powerful free software kernel implementing a parallel genetic algorithm... but is specially optimized for timetabling. Input and output is in form of specially formatted XML files" [source: https://www.tablix.org/articles/about/ | Tablix: What is Tablix? | practitioner].
- Last release: **0.3.5** [source: https://www.freshports.org/math/tablix/ | FreshPorts math/tablix | independent]. Tablix.org's most recent visible activity dates from October 2009 [source: https://archiveos.org/tablix-on-morphix/ | Tablix on Morphix — ArchiveOS | independent]. **Out per maintenance gate** (commits in last 12 months).
- No GitHub mirror found in search results [author estimate — search for "tablix github" returned no project repo; Debian and FreshPorts are the only living packagers].

### TimeFinder (Java, abandoned)

- "TimeFinder is not maintained any longer!" — explicit statement on the project page, which redirects users to UniTime or FET [source: https://timefinder.sourceforge.net/ | TimeFinder — Free Your Timetabling | practitioner].
- Java; last JAR `timefinder-2009-v4.jar` from 2010 [source: https://timefinder.sourceforge.net/ | TimeFinder — Free Your Timetabling | practitioner]. **Out per language and maintenance gates.**

## SQ3 — Hexaly / LocalSolver (commercial)

### Status

- LocalSolver rebranded to Hexaly. "The old LocalSolver APIs disappear completely [in Hexaly 14.0]... The old Python, Java and C# APIs will be maintained for another 1 year (for Hexaly 13.0 and Hexaly 13.5)" [source: https://www.hexaly.com/announcements/now-we-are-hexaly | Now we are Hexaly | vendor].
- Python wheel via private index: `pip install hexaly -i https://pip.hexaly.com`; "Python library requires Python >= 3.6"; **no explicit confirmation of Python 3.14 wheel** [source: https://www.hexaly.com/docs/last/installation/pythonsetup.html | Hexaly — Python Setup | vendor].

### License & cost

- **Commercial-only for non-academic use.** Pricing not publicly listed: "Hexaly does not currently have any pricing plans listed publicly" [source: https://www.trustradius.com/products/hexaly/pricing | Hexaly Pricing 2025 — TrustRadius | journalism].
- Recent transaction data on the prior LocalSolver brand: "the minimum price for LocalSolver is around $29,000, the maximum price is approximately $49,000, and the average cost is about $39,000 annually" [source: https://www.vendr.com/buyer-guides/localsolver | LocalSolver Software Pricing & Plans 2025 — Vendr | journalism].
- Free tiers: 1-month full trial; **academic license free, renewable forever** for educational and fundamental research purposes; no community / individual non-commercial license [source: https://www.hexaly.com/pricing | Hexaly Pricing | vendor].

### Track record on school timetabling

- "A comprehensive benchmark against Gurobi, Cplex, CP Optimizer, and OR-Tools led to choosing Hexaly to power a timetabling and resource allocation solution for YDUQS, a leading Brazilian group of universities and schools" — vendor case study, no methodology disclosed [source: https://www.hexaly.com/ | Hexaly home | vendor].

### Klassenzeit fit

- **Out by Klassenzeit's commercial-only-licenses-out rule** in the brief, unless the project pivots to an academic posture (which it has not). Klassenzeit is MIT-licensed and self-hosted; Hexaly's pricing band per Vendr ($29K–$49K/yr) is incompatible with sole-maintainer hobby economics [author estimate, restating the brief's exclusion criterion against the cited price band].

## SQ4 — Modern hybrid local-search frameworks (Python / Rust)

### ALNS (Python, MIT)

- "Adaptive large neighbourhood search (and more!) in Python." MIT-licensed; author Niels Wouda; latest release **v7.0.0 on 21 October 2024** [source: https://pypi.org/project/alns/ | alns · PyPI | independent].
- Python support: `Python <4.0, >=3.9`; classifiers list 3.9 through 3.13. **No explicit 3.14 support yet.** Dependencies: numpy + matplotlib (optional MABWiser for multi-armed bandit operator selection) [source: https://pypi.org/project/alns/ | alns · PyPI | independent].
- Releases history shows no 2025 or 2026 release as of the search date; latest is v7.0.0 (Oct 2024) [source: https://github.com/N-Wouda/ALNS/releases | ALNS Releases on GitHub | practitioner].
- Active in 2024: v7.0.0 migrated to NumPy `Generator`; v6.0.0 added logo and renamed acceptance criteria; v5.3.x added adaptive threshold acceptance and multi-armed bandit operator selection [source: https://github.com/N-Wouda/ALNS/releases | ALNS Releases on GitHub | practitioner].
- Published JOSS paper: "ALNS: a Python implementation of the adaptive large neighbourhood search metaheuristic," Wouda & Lan [source: https://joss.theoj.org/papers/10.21105/joss.05028 | JOSS: ALNS Python implementation | independent].
- **RCPSP example in repo**: PSPLib instance j9041_6 (90 jobs, 4 resources). Initial 172 → improved to 141; optimal makespan known to be in [123, 135] [source: https://alns.readthedocs.io/en/latest/examples/resource_constrained_project_scheduling_problem.html | ALNS RCPSP example | practitioner].
- **Maintenance gate edge case**: 12-month window from 2026-05-08 means last release Oct 2024 is just over the 18-month mark — **fails the strict "commits in last 12 months" rule** as stated in the brief, unless main-branch commits (not releases) are within 12 months [author estimate — releases page shows no 2025/2026 entries; need to verify main-branch commit recency before relying on this candidate].
- Used for timetabling-shaped problems in literature: "Effective adaptive large neighborhood search for a firefighters timetabling problem" (Journal of Heuristics, 2023) [source: https://link.springer.com/article/10.1007/s10732-023-09519-6 | Springer J. Heuristics | independent].

### MEALPY (Python, MIT)

- "World's largest Python library [for] meta-heuristic algorithms"; **233 algorithms** (206 official + 27 custom); MIT-licensed [source: https://pypi.org/project/mealpy/ | mealpy · PyPI | independent].
- Latest release **v3.0.3 on 16 August 2025**; classifiers list up to Python 3.13; **3.14 not confirmed** [source: https://pypi.org/project/mealpy/ | mealpy · PyPI | independent].
- 2026 paper applying MEALPY to railway timetabling: "An approach to the timetabling problem in deregulated railway markets based on metaheuristic algorithms" [source: https://doi.org/10.1177/10692509251392410 | Munoz-Valero et al., 2026 | independent]. Continuous/permutation problem framing, not direct school timetabling.
- Bias: most algorithms target **continuous** numerical optimization; combinatorial scheduling fit requires custom encoding [author estimate based on the algorithm catalogue's heavy nature-inspired-continuous slant].

### jMetalPy (Python, MIT)

- "A framework for single/multi-objective optimization with metaheuristics," MIT-licensed [source: https://github.com/jMetal/jMetalPy | GitHub — jMetal/jMetalPy | practitioner].
- Latest release **v1.9.0 on 29 October 2025**; supports Python 3.11–3.12 per badges (3.13 / 3.14 not advertised) [source: https://github.com/jMetal/jMetalPy | GitHub — jMetal/jMetalPy | practitioner].
- Algorithms include local search, GA, evolution strategy, simulated annealing, NSGA-II, NSGA-III, MOEA/D, SPEA2, IBEA [source: https://jmetal.github.io/jMetalPy/index.html | jMetalPy docs | practitioner].
- Permutation encodings exist (recently added multi-objective TSP); no dedicated school-timetabling problem class [source: https://jmetal.github.io/jMetalPy/index.html | jMetalPy docs | practitioner].

### Rust crates

- **argmin** (Apache-2 OR MIT): "Numerical optimization in pure Rust." Latest **v0.11.0, 28 September 2025**; 39 releases [source: https://github.com/argmin-rs/argmin | GitHub — argmin-rs/argmin | practitioner]. Includes Simulated Annealing via the `Anneal` trait [source: https://docs.rs/argmin/latest/argmin/solver/simulatedannealing/trait.Anneal.html | argmin Anneal trait docs | practitioner]. **No tabu search, no LNS, no LAHC** in the algorithm catalogue [source: https://github.com/argmin-rs/argmin | GitHub — argmin-rs/argmin | practitioner]. Combinatorial-permutation usage requires user-supplied `Anneal::anneal` neighborhood; no first-class permutation example surfaced [author estimate based on the documented algorithm list and the absence of permutation examples in argmin's main page].
- **oxigen** (MPL-2.0): "Fast, parallel, extensible and adaptable genetic algorithms framework." Latest release **v2.2.2, 28 February 2021**; **last commit ~July 2021** per arewelearningyet [source: https://www.arewelearningyet.com/metaheuristics/ | Are We Learning Yet? — Metaheuristics | independent], [source: https://github.com/Martin1887/oxigen | GitHub — Martin1887/oxigen | practitioner]. **Out per maintenance gate.**
- **genevo** (Apache-2 / MIT): "Execute genetic algorithm (GA) simulations." Latest release **v0.7.1, 13 March 2022** — outside 12-month window [source: https://github.com/innoave/genevo | GitHub — innoave/genevo | practitioner]. Examples: knapsack, N-queens, infinite monkey theorem; no scheduling [source: https://github.com/innoave/genevo | GitHub — innoave/genevo | practitioner]. **Out per maintenance gate.**
- **metaheurustics-rs** (`aryashah2k/metaheuRUSTics`, MIT): published January 2025, 12 commits, 0 issues / 0 PRs, no releases [source: https://github.com/aryashah2k/metaheuRUSTics | GitHub — aryashah2k/metaheuRUSTics | practitioner]. Algorithms: PSO, DE, GA, SA, ACGWO, ABCO, GWO, FA — all **continuous-optimization** benchmarks (Sphere, Ackley, Rosenbrock); no combinatorial scheduling [source: https://github.com/aryashah2k/metaheuRUSTics | GitHub — aryashah2k/metaheuRUSTics | practitioner].
- **netaheuristics** (`DaPurr/Netaheuristics`, MIT): "Metaheuristics framework for Rust"; planned algorithms include "Variable Neighborhood Search, Simulated Annealing, Large Neighborhood Search and their adaptive variants"; only 45 commits, 1 star, 0 forks; activity status unclear [source: https://github.com/DaPurr/Netaheuristics | GitHub — DaPurr/Netaheuristics | practitioner]. Stale per arewelearningyet [source: https://www.arewelearningyet.com/metaheuristics/ | Are We Learning Yet? — Metaheuristics | independent]. **Likely fails maintenance gate.**
- **metaheuristics** (crates.io): last published 2022-07-16 [source: https://www.arewelearningyet.com/metaheuristics/ | Are We Learning Yet? — Metaheuristics | independent]. Out.
- **metaheuristics-nature**: "A collection of nature-inspired metaheuristic algorithms" — continuous-optimization-oriented per the description and naming convention [author estimate from the lib.rs framing surfaced via WebSearch; direct fetch returned 403].

## SQ5 — OptaPlanner-style hybrid local search built without Java

### SolverForge (Rust, Apache-2.0)

- **Direct OptaPlanner spiritual successor in pure Rust.** "SolverForge is a constraint programming framework and solver written in Rust... optimizes planning and scheduling problems using metaheuristic algorithms" [source: https://github.com/SolverForge/solverforge | GitHub — SolverForge/solverforge | practitioner].
- License: **Apache-2.0**. Latest release **v0.11.1, 5 May 2026**; 990 commits on main; 41 releases; 54 stars, 2 forks; requires Rust 1.95+ [source: https://github.com/SolverForge/solverforge | GitHub — SolverForge/solverforge | practitioner].
- Algorithm catalogue overlaps Klassenzeit's existing LAHC bench almost exactly: "Hill Climbing, Simulated Annealing, Tabu Search, **Late Acceptance**, Great Deluge, Step Counting Hill Climbing, Diversified Late Acceptance" plus exhaustive search (Branch and Bound) and partitioned search [source: https://github.com/SolverForge/solverforge | GitHub — SolverForge/solverforge | practitioner].
- Architecture is OptaPlanner-shaped: "constraint streams API — a declarative, composable way to express rules that reads like a pipeline of filters and transformations... using `for_each`, `filter`, `join`, or `group`... `penalize` or `reward` to affect the score" [source: https://solverforge.org/docs/solverforge/constraints/ | SolverForge — Constraints | practitioner].
- Performance posture: "zero-erasure architecture that eliminates trait objects, runtime dispatch, and hidden allocations for deterministic performance"; "Typical throughput is 300k-1M moves/second depending on constraint complexity for scheduling" — vendor self-report [source: https://solverforge.org/about/ | SolverForge — About | practitioner].
- Real-world deployment claim: "Working like a charm, A+" testimonial from a pathologist at The Ottawa Hospital [source: https://github.com/SolverForge/solverforge | GitHub — SolverForge/solverforge | practitioner].
- Examples shipped: graph coloring (`scalar-graph-coloring`), TSP (`list-tsp`), job shop (`mixed-job-shop`), N-queens (`nqueens`), employee scheduling, meeting scheduling [source: https://github.com/SolverForge/solverforge | GitHub — SolverForge/solverforge | practitioner], [source: https://solverforge.org/docs/getting-started/employee-scheduling/ | SolverForge — Employee Scheduling | practitioner]. **No dedicated school-timetabling example shipped.** The XHSTT-shaped instance would need to be modeled by Klassenzeit.
- **No Python bindings.** Cargo-only; would integrate via `solver-core` directly, similar to how Klassenzeit owns the LAHC bench backends [source: https://github.com/SolverForge/solverforge | GitHub — SolverForge/solverforge | practitioner].
- Maturity caveats: 2 forks, 54 stars, sub-1.0 version (v0.11.1), young project [source: https://github.com/SolverForge/solverforge | GitHub — SolverForge/solverforge | practitioner]. Klassenzeit would be an early adopter [author estimate from the star/fork/version metrics].

### Other Rust CP frameworks (mentioned for completeness, primarily covered in cluster `rust-native-solvers`)

- **copper** (MIT): "constraint programming solver," but "still quite early in its development, it cannot rival with mature solvers like Gecode or or-tools... Copper currently supports a limited number of variable types and constraints" [source: https://crates.io/crates/copper | copper crates.io | practitioner]. Limited variable types: binary plus weight/capacity constraints; "is not blazingly fast (yet)" [source: https://docs.rs/copper/latest/copper/ | copper Rust docs | practitioner]. Out for hybrid-local-search-on-school-timetabling fit [author estimate from the maintainer's own README].

### Hybrid pattern in literature

- MaxSAT-based LNS is the strongest published precedent for hybrid local search on XHSTT: "a local search algorithm is used to drive an initial solution into a local optimum and then more powerful large neighborhood search (LNS) techniques based on maxSAT are used to further improve the solution" — Demirović & Musliu, *Computers & Operations Research*, 2017 [source: https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | MaxSAT-based large neighborhood search for high school timetabling — ScienceDirect | independent].
- ALNS curriculum-based course timetabling: Bellio et al., "Adaptive large neighborhood search for the curriculum-based course timetabling problem," *Annals of OR* [source: https://link.springer.com/article/10.1007/s10479-016-2151-2 | Springer Annals of OR | independent].
- ASP-based LNS: "ASP-Based Large Neighborhood Prioritized Search for Course Timetabling," Springer 2024 [source: https://link.springer.com/chapter/10.1007/978-3-031-74209-5_5 | Springer LNCS | independent].
- Note: CP-SAT itself runs LNS internally — "CP-SAT schedules its LNS strategies using a simple round-robin method" [source: https://d-krupke.github.io/cpsat-primer/09_lns.html | CP-SAT Primer — Large Neighborhood Search | practitioner]. Adding an external LNS layer on top of LAHC is a different lever (custom destroy/repair on Klassenzeit's domain) than CP-SAT's internal generic LNS portfolio [author estimate from juxtaposing the two sources].

## Cross-cutting notes

### Python 3.14 readiness

- Klassenzeit is pinned to Python 3.14.2. Of the Python-side candidates surveyed:
  - ALNS: explicit support 3.9–3.13 [source: https://pypi.org/project/alns/ | alns · PyPI | independent]. **3.14 not yet listed.**
  - mealpy: explicit support up to 3.13 [source: https://pypi.org/project/mealpy/ | mealpy · PyPI | independent]. **3.14 not yet listed.**
  - jMetalPy: badges show 3.11–3.12 [source: https://github.com/jMetal/jMetalPy | GitHub — jMetal/jMetalPy | practitioner].
  - Hexaly: requires Python ≥ 3.6; **3.14 wheel availability not confirmed** [source: https://www.hexaly.com/docs/last/installation/pythonsetup.html | Hexaly — Python Setup | vendor].
  - Pure-Python libraries (ALNS, mealpy, jMetalPy) often work on newer Python ahead of classifier updates [author estimate — they have no compiled extensions in their core dependency tree per their PyPI metadata showing only numpy/matplotlib].

### Maintenance gate (commits in last 12 months from 2026-05-08, i.e., after 2025-05-08)

- **Pass**: SolverForge (v0.11.1 on 2026-05-05); jMetalPy (v1.9.0 on 2025-10-29); mealpy (v3.0.3 on 2025-08-16); argmin (v0.11.0 on 2025-09-28); FET (v7.8.5 on 2026-04-11).
- **Edge case**: ALNS (last release v7.0.0 on 2024-10-21 — over 12 months from search date; main-branch commits not verified above the release cadence) [source: https://github.com/N-Wouda/ALNS/releases | ALNS Releases on GitHub | practitioner].
- **Fail**: Tablix (last release ~2009), TimeFinder (project declares unmaintained), oxigen (2021), genevo (2022), netaheuristics (likely stale), copper (single 0.1.0 release).

### License gate (permissive preferred; AGPL yellow; commercial-only out)

- **Permissive**: ALNS (MIT), mealpy (MIT), jMetalPy (MIT), argmin (Apache/MIT), SolverForge (Apache-2.0), copper (MIT), genevo (Apache/MIT), metaheurustics-rs (MIT), netaheuristics (MIT).
- **MPL-2.0**: oxigen.
- **AGPL-3.0 (yellow)**: FET — only viable as subprocess if the "mere aggregation" framing holds [source: https://www.gnu.org/licenses/agpl-3.0.html | GNU Affero General Public License v3 | independent].
- **Commercial-only**: Hexaly / LocalSolver — out per Klassenzeit's brief.

### Integration shape vs `BenchBackend` contract

- `BenchBackend` enum lives in `solver/solver-bench/src/main.rs` (per `brief.md`). Candidates expressed as Rust crates plug in directly; Python candidates need a Python-side peer module pattern modeled on `solver/solver-py/python/klassenzeit_solver/cpsat.py` [source: brief.md Reference State (2026-05-08) | independent — internal repo memory].
- Subprocess candidates (FET via `fet-cl`) require a Python or Rust shim that writes the FET XML, invokes `fet-cl --inputfile=... --outputdir=...`, then parses `activities_timetable.xml` and `result.txt`, and translates back to Klassenzeit's `Solution` JSON for the canonical Rust scorer [source: https://www.timetabling.de/manual/FET-manual.en.html | FET Manual — timetabling.de | practitioner], [source: https://manpages.ubuntu.com/manpages/bionic/man1/fet-cl.1.html | fet-cl(1) — Ubuntu Manpages | independent].

## Candidate summary table

| Candidate | Language | License | Last release | Solver class | School-TT fit | Klassenzeit gate |
|---|---|---|---|---|---|---|
| FET | C++/Qt | AGPL-3.0 | 2026-04-11 (v7.8.5) | Heuristic (recursive swapping) | Native (years/groups/rooms/teachers/daily caps) | Yellow on license; subprocess-only integration; no library API |
| Tablix | C | GPL | ~2009 (v0.3.5) | Parallel GA | Timetabling-specialized | **Out** (maintenance) |
| TimeFinder | Java | GNU | 2010 (v4) | Manual + heuristic GUI | Timetabling-specialized | **Out** (Java + maintenance + self-declared abandoned) |
| Hexaly / LocalSolver | C++ | Commercial | Hexaly 14.x (2025) | Local search + CP/MIP hybrid | Vendor case studies on YDUQS | **Out** (commercial-only) |
| ALNS | Python | MIT | 2024-10-21 (v7.0.0) | ALNS (destroy/repair) | RCPSP example; firefighter timetable in literature | Edge on maintenance; no Py 3.14 classifier |
| MEALPY | Python | MIT | 2025-08-16 (v3.0.3) | 233 algorithms (mostly continuous) | Indirect (railway TT 2026 paper) | Pass; combinatorial fit needs custom encoding |
| jMetalPy | Python | MIT | 2025-10-29 (v1.9.0) | NSGA-II/III, MOEA/D, SA, GA | Permutation encoding only | Pass; no school-TT class |
| argmin | Rust | Apache/MIT | 2025-09-28 (v0.11.0) | SA, PSO, CMA-ES, line searches | No combinatorial scheduling examples | Pass; weak combinatorial story |
| SolverForge | Rust | Apache-2.0 | 2026-05-05 (v0.11.1) | LAHC, SA, Tabu, Great Deluge, SCHC, DLA, Hill Climbing | Employee/meeting scheduling; no school-TT example shipped | Pass; pure-Rust BenchBackend fit; sub-1.0 maturity |
| oxigen | Rust | MPL-2.0 | 2021 | GA | N/A | **Out** (maintenance) |
| genevo | Rust | Apache/MIT | 2022 | GA | Knapsack/N-queens | **Out** (maintenance) |
| metaheurustics-rs | Rust | MIT | 2025 (early) | PSO/DE/GA/SA/GWO (continuous) | None | Pass; continuous-only catalogue |
| netaheuristics | Rust | MIT | 2022 | VNS/SA/LNS (planned) | None | Likely **out** (maintenance, scope incomplete) |
| copper | Rust | MIT | 0.1.0 | Early-stage CP | None | Pass on license; very early |

## Additional pointers surfaced

- **PyJobShop** (Python, MIT, arXiv 2502.13483, Lan & Berkhout 2025) — wraps both OR-Tools CP-SAT and IBM CP Optimizer behind a unified Python scheduling DSL [source: https://github.com/PyJobShop/PyJobShop | GitHub — PyJobShop/PyJobShop | practitioner], [source: https://arxiv.org/abs/2502.13483 | arXiv 2502.13483 — PyJobShop | independent]. Job-shop framing rather than school-TT, but a precedent for a Python-side scheduling DSL fronting multiple CP backends.
- **CPMpy** (Python, Apache-2.0) — "solver-independent by transparently translating to CP, MIP, SMT and SAT solvers"; default solver OR-Tools CP-SAT, also Gurobi, PySAT, Z3, MiniZinc-mediated CP solvers; participated and won medals at XCSP3 2024 and 2025 [source: https://github.com/CPMpy/cpmpy | GitHub — CPMpy/cpmpy | practitioner]. Could serve as a portable layer to test multiple backends without writing per-solver glue.
- **pychoco** (Python, BSD; see cluster `cp-sat-smt-backends`) — "pychoco library uses a native-build of the original Java Choco-solver library, in the form of a shared library, which can be used without any JVM—created with GraalVM native-image tool" [source: https://pypi.org/project/pychoco/ | pychoco · PyPI | practitioner]. Item 56 names Choco as out, but pychoco's GraalVM native build dodges the JVM; brief reading depends on whether "no Java" means "no JVM at runtime" (pychoco passes) or "no Java codebase" (pychoco fails).
- **PATAT 2024 / IHTC 2024** stats among open-source CP/MILP submissions: "CP-SAT (OR Tools) was used by four teams, and MiniZinc by one. In contrast, among teams employing MILP, 18 used Gurobi, one used CPLEX and one relied on the open-source solver Soplex" [source: https://patatconference.org/patat2024/proceedings/papers/52.pdf | IHTC 2024 paper | independent]. Signal that the OSS-CP field today is dominated by CP-SAT, with MiniZinc a distant second.
