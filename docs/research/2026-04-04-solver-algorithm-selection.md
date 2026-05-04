# Timetable Solver Algorithm Selection for Klassenzeit v2

*Date: 2026-04-04*

## Executive Summary

**Thesis: For Klassenzeit v2 at German school scale (10-30 classes, 30-80 teachers, 200-800 lessons), a construction-plus-local-search solver using Late Acceptance Hill-Climbing (LAHC) — implemented via SolverForge in Rust — offers the strongest trade-off between solution quality, implementation effort, parameterization simplicity, and long-term maintainability.**

The evidence favors LAHC over pure Simulated Annealing because LAHC matched or beat SA on 34 of 35 benchmark instances while requiring only a single parameter (list length) versus SA's three interdependent parameters [Late Acceptance Hill-Climbing Heuristic, 2017](https://www.sciencedirect.com/science/article/abs/pii/S0377221716305495) [independent]. SA achieves marginally better final quality given unlimited time [Large-scale Timetabling with Adaptive Tabu Search, 2022](https://www.degruyterbrill.com/document/doi/10.1515/jisys-2022-0003/html) [independent], but Klassenzeit's target is a usable timetable in under 60 seconds, not an academic competition entry. Tabu Search converges faster to acceptable solutions but plateaus earlier [Large-scale Timetabling with Adaptive Tabu Search, 2022](https://www.degruyterbrill.com/document/doi/10.1515/jisys-2022-0003/html) [independent], making it a strong secondary option to hybridize with LAHC — as Timefold's own architecture demonstrates [Timefold Local Search Documentation, 2025](https://docs.timefold.ai/timefold-solver/latest/optimization-algorithms/local-search) [vendor].

The critical implementation decision is not the metaheuristic but the scoring engine: incremental constraint evaluation dominates runtime at scale, and proportional penalty functions prevent score traps that render any algorithm ineffective [Timefold Score Performance Tips, 2025](https://docs.timefold.ai/timefold-solver/latest/constraints-and-score/performance) [vendor — note: Timefold has commercial interest in promoting its scoring architecture; however, the underlying principle of incremental evaluation is well-established in OR]; [Timefold Score Overview, 2025](https://docs.timefold.ai/timefold-solver/latest/constraints-and-score/overview) [vendor — same caveat]. SolverForge (v0.7.0) provides a Rust-native ConstraintStream API with incremental scoring, the same algorithm set as Timefold, and zero-allocation move evaluation [SolverForge GitHub, 2025](https://github.com/SolverForge/solverforge) [practitioner]. Despite its early lifecycle, its architecture directly mirrors Timefold's battle-tested design and eliminates the build-vs-buy problem.

Genetic Algorithms and pure ILP/MIP should be avoided. GAs converge slowly and lose population diversity on timetabling problems [GA Premature Convergence Issues](https://en.wikipedia.org/wiki/Premature_convergence) [independent]; [SA-Based Algorithm for Real-World High School Timetabling, 2010](https://ieeexplore.ieee.org/document/5632136) [independent]. Pure ILP hits scalability walls at the upper end of this problem range [CP-SAT Primer, 2025](https://d-krupke.github.io/cpsat-primer/) [practitioner], though hybrid fix-and-optimize approaches using ILP sub-solvers remain valuable for specific decomposition tasks [Fix-and-Optimize Heuristic for High School Timetabling, 2014](https://www.sciencedirect.com/science/article/pii/S0305054814001816) [independent].

## Table of Contents

1. [Introduction](#introduction)
2. [Methodology](#methodology)
3. [What Matters Most](#what-matters-most)
4. [Supporting Evidence](#supporting-evidence)
5. [Analysis & Insights](#analysis-and-insights)
6. [Limitations & Open Problems](#limitations-and-open-problems)
7. [Future Outlook](#future-outlook)
8. [Conclusions & Practical Starting Point](#conclusions-and-practical-starting-point)
9. [References](#references)

## Introduction

Klassenzeit v2 is a school timetabling application being rebuilt in pure Rust. The current solver is a greedy, single-pass construction heuristic (~200 lines) with no backtracking and no soft constraint optimization. This works for trivial instances but produces poor-quality timetables for real schools: teachers get unnecessary gaps, subjects cluster on single days, and preferred slots are ignored.

The goal is to select and implement a solver architecture that:
1. Always produces a feasible timetable (all 8 hard constraints satisfied) for typical German school instances
2. Optimizes soft constraints (teacher gaps, subject distribution, preferred slots, class teacher first period) to a quality level comparable with established tools like FET or Untis
3. Returns results in under 60 seconds for the target scale
4. Is implementable and maintainable by a single developer in Rust

This report evaluates five algorithmic families — Simulated Annealing, Tabu Search, Late Acceptance Hill-Climbing, Genetic Algorithms, and ILP/MIP/CP-SAT — against these requirements. It recommends an architecture, specific move types, a scoring model, and a concrete implementation path.

## Methodology

**Research approach:** Structured investigation across five sub-questions: algorithm selection, move neighborhood design, scoring architecture, Rust implementation patterns, and convergence/testing strategies. Sources were gathered from academic literature (ITC competition papers, OR journals, algorithm-specific studies), industry documentation (Timefold/OR-Tools), and practitioner resources (open-source solver codebases, implementation guides).

**Credibility tiers used throughout:**
- **[independent]** — Academic papers, peer-reviewed, no commercial interest. Highest weight.
- **[practitioner]** — Open-source project documentation, developer blogs, crates.io. Weighted by project maturity and adoption.
- **[vendor]** — Timefold, Google OR-Tools documentation. Valuable for architecture patterns but commercial interest noted. Timefold content is particularly relevant because SolverForge explicitly mirrors its architecture.

**Limitations of this research:** No hands-on benchmarking was performed. Algorithm comparisons come from different papers using different instances and hardware, making direct numerical comparison unreliable. The Klassenzeit-specific problem structure (German school conventions, specific constraint mix) may favor algorithms differently than the academic benchmarks studied.

## What Matters Most

### 1. LAHC beats SA with dramatically simpler parameterization

Late Acceptance Hill-Climbing produced better or equal solutions compared to SA on 34 of 35 benchmark instances tested, including exam timetabling problems structurally similar to school timetabling [Late Acceptance Hill-Climbing Heuristic, 2017](https://www.sciencedirect.com/science/article/abs/pii/S0377221716305495) [independent]. LAHC has one parameter: the history list length. SA requires tuning initial temperature, cooling rate, and iterations per temperature level — and these interact: "different initial temperature is required for each instance" [thin sourcing — this finding comes from general SA literature searches during adversarial review, not a single registered source]. For a single-developer project without a parameter tuning pipeline, this difference is decisive.

**The weakness:** LAHC is less studied than SA in the timetabling literature. The LAHC comparison in [Late Acceptance Hill-Climbing Heuristic, 2017](https://www.sciencedirect.com/science/article/abs/pii/S0377221716305495) [independent] used exam timetabling and TSP benchmarks, not school timetabling specifically. However, Timefold — which has years of production deployments — recommends combining Late Acceptance with a small Tabu component as a default configuration [Timefold Local Search Documentation, 2025](https://docs.timefold.ai/timefold-solver/latest/optimization-algorithms/local-search) [vendor], lending practitioner validation to this choice.

### 2. The scoring engine matters more than the metaheuristic

Incremental score calculation provides "a huge performance and scalability gain" [Timefold Score Performance Tips, 2025](https://docs.timefold.ai/timefold-solver/latest/constraints-and-score/performance) [vendor — Timefold has commercial interest in promoting its ConstraintStream architecture as superior to alternatives; however, the principle of incremental evaluation is standard in OR and independently validated by solver competition results]. Without it, every candidate move requires a full evaluation of all constraints against the entire timetable. With it, only the constraints affected by the changed assignments are recalculated. At 200-800 lessons, the difference is orders of magnitude in throughput.

Equally critical: **proportional penalty functions.** Penalizing a teacher conflict as "-1 hard" regardless of how many conflicts exist creates a score trap — the solver cannot distinguish between a timetable with 1 conflict and one with 50 conflicts, so it cannot hill-climb out of infeasibility [Timefold Score Performance Tips, 2025](https://docs.timefold.ai/timefold-solver/latest/constraints-and-score/performance) [vendor]; [Timefold Score Overview, 2025](https://docs.timefold.ai/timefold-solver/latest/constraints-and-score/overview) [vendor — Timefold's documentation promotes this as part of its scoring framework; the underlying principle is sound OR practice regardless of vendor].

### 3. Kempe chain neighborhoods dominate simpler moves

"The choice of neighbourhood is the most important decision" and "neighbourhoods based on Kempe chains are the most effective regardless of objectives or size" [Effect of Neighborhood Structures on TS for Timetabling, 2015](https://www.researchgate.net/publication/287192019) [independent]. A Kempe chain swap interchanges connected components between two timeslots in the conflict graph — effectively performing a coordinated multi-swap that maintains structural consistency.

For school timetabling, this means: when you want to move a lesson from slot A to slot B, you don't just move it (which may create a teacher conflict in B), you identify the chain of cascading swaps needed to accommodate it. This is more expensive per move but reaches solution regions that simple swap and change moves cannot access.

**Practical implication:** Implement three move types in order of priority: (1) Change move (reassign one lesson to a different slot), (2) Swap move (exchange two lessons' slots), (3) Kempe chain move. Start with 1+2, add 3 when the basic solver works. SolverForge provides Change and Swap out of the box [SolverForge GitHub, 2025](https://github.com/SolverForge/solverforge) [practitioner].

### 4. Genetic Algorithms are the wrong tool for this problem

GAs converge slowly on timetabling: SA achieved 100% improvement from initial solution in 8 minutes while GA achieved only 99% in 2 hours on the same real-world high school instance [SA-Based Algorithm for Real-World High School Timetabling, 2010](https://ieeexplore.ieee.org/document/5632136) [independent]. Beyond speed, GAs suffer from "premature convergence to local optima, loss of population diversity, and slow convergence speed" [GA Premature Convergence Issues](https://en.wikipedia.org/wiki/Premature_convergence) [independent]. The crossover operator is the fundamental problem — combining two valid timetables usually produces massively infeasible offspring because timetabling constraints are deeply interconnected.

FET, the most widely-used free school timetabling software, started as "Free Evolutionary Timetabling" and abandoned the evolutionary approach in 2007, switching to a recursive swapping heuristic — a "big breakthrough" by the developer's account [FET Free Timetabling Software, 2025](https://lalescu.ro/liviu/fet/) [practitioner]. This is the strongest practitioner signal in the dataset.

### 5. Pure ILP is viable at this scale but fragile above it

CP-SAT (Google OR-Tools) "overcomes many of the weaknesses of classical CP and provides a viable alternative to MIP-solvers" [CP-SAT Primer, 2025](https://d-krupke.github.io/cpsat-primer/) [practitioner]. A CP-SAT model has been demonstrated for school timetabling with 6 classes, 11 subjects, 13 teachers [School Timetabling with CP-SAT, 2026](https://medium.com/suboptimally-speaking/school-timetabling-with-constraint-programming-495f1126c28d) [practitioner] — the lower end of the Klassenzeit range. ILP has a 98% implementation rate in university timetabling practice [Review of University Timetabling, MDPI 2025](https://www.mdpi.com/2079-3197/13/1/10) [independent], suggesting it works.

**But:** "MIP-solvers are frequently able to optimize problems with hundreds of thousands of variables...classical CP-solvers often struggle with more than a few thousand variables" [CP-SAT Primer, 2025](https://d-krupke.github.io/cpsat-primer/) [practitioner]. At 30 classes x 80 teachers x 40 timeslots, the variable space grows rapidly. Hybrid approaches that decompose by class, teacher, or day make ILP tractable for larger instances [Fix-and-Optimize Heuristic for High School Timetabling, 2014](https://www.sciencedirect.com/science/article/pii/S0305054814001816) [independent], but they add architectural complexity.

**Position:** CP-SAT is a valid alternative approach but not the recommendation. It requires wrapping a C++ library (OR-Tools) via FFI, losing Rust's compile-time guarantees at the solver boundary. It's better suited as a verification tool — solve small instances exactly to validate the metaheuristic's output.

### 6. SolverForge eliminates the build-vs-buy dilemma for Rust

SolverForge (v0.7.0) provides: ConstraintStream API with incremental scoring ("10-100x speedups" claimed [SolverForge Overview Documentation, 2025](https://solverforge.org/docs/overview/) [practitioner — this is a vendor-like claim from the project's own documentation; independent benchmarks are not available]), all relevant metaheuristics (SA, TS, LAHC, Great Deluge, Step Counting HC), construction heuristics (First Fit, Best Fit, First Feasible), zero-allocation move evaluation with arena allocation, and derive macros for domain modeling [SolverForge GitHub, 2025](https://github.com/SolverForge/solverforge) [practitioner].

**The risk:** v0.7.0 is early. Limited community. One production testimonial (Ottawa Hospital). Timefold/OptaPlanner have years of battle-testing. If SolverForge has bugs or missing features, you're debugging someone else's solver framework rather than your own code.

**The mitigation:** SolverForge's architecture mirrors Timefold exactly — same score types, same algorithm names, same ConstraintStream operators. If it fails, the domain model and constraint definitions port to a hand-rolled solver with the same conceptual architecture. The abstraction is not wasted.

### 7. Two-level lexicographic scoring is sufficient

Use HardSoftScore with lexicographic comparison: any solution with 0 hard violations beats any solution with 1+ hard violations, regardless of soft score [Timefold Score Overview, 2025](https://docs.timefold.ai/timefold-solver/latest/constraints-and-score/overview) [vendor — Timefold promotes this as part of its framework, but lexicographic scoring is a standard OR technique independently described in [University Course Timetabling with Soft Constraints, 2003](https://www.unitime.org/papers/patat03.pdf) [independent]]. This eliminates the weight-balancing problem between hard and soft constraints.

Within the soft level, use weighted sums with integer penalties [author estimate — these specific weights are not sourced; they reflect a reasonable starting configuration based on constraint severity]:
- Teacher gap: -1 per gap period
- Subject distribution violation (duplicate subject on same day for a class): -2 per violation (more disruptive than a gap)
- Teacher preferred slot violation: -1 per slot
- Class teacher not in first period: -1 per day

Start with these weights and expose them as configuration. "Don't waste time with constraint weight discussions at the start" [Timefold Score Overview, 2025](https://docs.timefold.ai/timefold-solver/latest/constraints-and-score/overview) [vendor — Timefold has commercial interest in encouraging users to defer weight tuning to its UI-driven configuration workflow]. Avoid floating-point arithmetic; use scaled i64 values.

## Supporting Evidence

### Algorithm Comparison Detail

**SA performance profile:** SA with intensification outperformed published TS results on a real-world Brazilian high school [SA-Based Algorithm for Real-World High School Timetabling, 2010](https://ieeexplore.ieee.org/document/5632136) [independent]. On benchmark datasets, TS found 50 penalty points in 347 seconds while SA had 1050 at the same point but continued improving with more time [Large-scale Timetabling with Adaptive Tabu Search, 2022](https://www.degruyterbrill.com/document/doi/10.1515/jisys-2022-0003/html) [independent]. This confirms SA as a strong quality optimizer but slow to converge.

**Adaptive cooling is mandatory for SA if used:** Adaptive schedules that "detect critical temperatures and decelerate cooling when such a temperature is detected" outperform fixed geometric cooling [General Cooling Schedules for SA-Based Timetabling, 1996](https://link.springer.com/chapter/10.1007/3-540-61794-9_70) [independent]. Geometric cooling with alpha 0.8-0.99 is the practical range; below 0.8 is "excessively fast" [SA Cooling Schedules for School Timetabling, 2010](https://iaorifors.com/paper/30085) [independent]. Logarithmic cooling (T(t) = c/log(1+t)) guarantees global optimum convergence but is "very slow" — theoretical only [Cooling Schedules for Optimal Annealing, 1988](https://pubsonline.informs.org/doi/10.1287/moor.13.2.311) [independent].

**TS convergence advantage:** Adaptive TS with three phases (initialization, intensification, diversification) produced "excellent timetables" across benchmark datasets [Large-scale Timetabling with Adaptive Tabu Search, 2022](https://www.degruyterbrill.com/document/doi/10.1515/jisys-2022-0003/html) [independent]; [Adaptive Tabu Search for Course Timetabling, 2010](https://www.researchgate.net/publication/222418829) [independent]. Reactive TS adapts tabu tenure via an "internal online feedback loop" that detects revisited solutions [Reactive Tabu Search, 2024](https://algorithmafternoon.com/stochastic/reactive_tabu_search/) [practitioner]. This is more robust than fixed tenure.

**LAHC + Tabu hybrid:** Timefold's recommended configuration combines Late Acceptance with "a bit of Tabu — use a lower tabu size than pure Tabu Search" [Timefold Local Search Documentation, 2025](https://docs.timefold.ai/timefold-solver/latest/optimization-algorithms/local-search) [vendor]. This leverages LAHC's simplicity with TS's short-term memory to avoid cycling.

### Established Solver Architectures

**FET:** Switched from evolutionary to recursive swapping in 2007. Solves "complicated timetables in maximum 5-20 minutes, simpler ones under 5 minutes, extremely difficult ones hours" [FET Free Timetabling Software, 2025](https://lalescu.ro/liviu/fet/) [practitioner]. Constraint weights at 100% = mandatory; lower percentages = preferred, where 50% means "on average FET retries two times" [FET Free Timetabling Software, 2025](https://lalescu.ro/liviu/fet/) [practitioner].

**UniTime CPSolver:** Won 2 of 3 ITC-2007 tracks using "iterative forward search" — maintains feasible but possibly incomplete solutions, always satisfying hard constraints on assigned variables [UniTime CPSolver, 2024](https://github.com/UniTime/cpsolver) [practitioner]. This is the same construction-plus-improvement architecture recommended here.

**Timefold architecture:** Three-component local search: MoveSelector (generates candidate moves), Acceptor (accepts/rejects based on LAHC/Tabu/SA criteria), Forager (selects the best accepted move as the next step) [Timefold Local Search Documentation, 2025](https://docs.timefold.ai/timefold-solver/latest/optimization-algorithms/local-search) [vendor]. This clean separation is directly replicated in SolverForge.

**OR-Tools CP-SAT:** Requires boolean variables for every assignment combination (e.g., 2000 shifts x 100 employees = 200,000 variables) [Google OR-Tools vs Timefold, 2024](https://timefold.ai/blog/google-or-tools-versus-timefold-comparison) [vendor — this is from a Timefold comparison page with commercial interest in making OR-Tools look worse]. [CP-SAT Primer, 2025](https://d-krupke.github.io/cpsat-primer/) [practitioner] gives a more balanced view: CP-SAT is competitive with MIP for many problems. School timetabling with CP-SAT has been demonstrated [School Timetabling with CP-SAT, 2026](https://medium.com/suboptimally-speaking/school-timetabling-with-constraint-programming-495f1126c28d) [practitioner] but at small scale only.

### Rust Implementation Ecosystem

**SolverForge crate hierarchy [SolverForge GitHub, 2025](https://github.com/SolverForge/solverforge) [practitioner]:**
- `solverforge-core`: Score types, domain traits (`#[planning_solution]`, `#[planning_entity]`, `#[problem_fact]`)
- `solverforge-solver`: Algorithm phases, moves, termination conditions
- `solverforge-scoring`: ConstraintStream API, SERIO incremental engine
- `solverforge-config`: TOML-based configuration
- `solverforge-macros`: Derive macros

**Performance characteristics:** 445,000 steps/second on a sample scheduling problem [SolverForge GitHub, 2025](https://github.com/SolverForge/solverforge) [practitioner]. Zero-allocation moves via arena allocation. No trait objects, no runtime dispatch — all generics resolved at compile time [SolverForge GitHub, 2025](https://github.com/SolverForge/solverforge) [practitioner].

**Alternative: good_lp [good_lp: Linear Programming for Rust, 2025](https://github.com/rust-or/good_lp) [practitioner]** for ILP sub-problems. Abstracts over CBC and HiGHS (MIT-licensed, parallel MIP solver). HiGHS is statically linkable, no manual install needed. Useful if implementing a hybrid fix-and-optimize approach later.

**Data structures:** bitvec crate for one-bit-per-bool conflict matrices [bitvec, 2025](https://docs.rs/bitvec/latest/bitvec/) [practitioner]. Teacher-timeslot and room-timeslot conflict checking via bitset intersection is O(n/64) per check.

**Rust performance advantage:** Rust B&B solver was 6-7x faster than Python on 10 benchmark MIP problems, finding better solutions in all instances and better bounds in 8/10 [Python vs Rust for MILP Solvers, 2025](https://www.osiopt.com/blogs/python-or-rust-performance-comparison-in-optimization-model-environment) [practitioner].

## Analysis and Insights

### The Simplicity Principle

The most surprising finding is how consistently the evidence favors simpler approaches over complex ones. LAHC beats SA with fewer parameters [Late Acceptance Hill-Climbing Heuristic, 2017](https://www.sciencedirect.com/science/article/abs/pii/S0377221716305495) [independent]. FET abandoned GAs for recursive swapping [FET Free Timetabling Software, 2025](https://lalescu.ro/liviu/fet/) [practitioner]. Automated configuration found that "simpler configurations outperform state-of-the-art" for timetabling [When Simpler is Better, IEEE 2023](https://ieeexplore.ieee.org/document/10253986/) [independent]. CP-SAT with hot starts from a heuristic solution outperforms pure exact methods [CP for High School Timetabling with Hot Starts, 2018](https://link.springer.com/chapter/10.1007/978-3-319-93031-2_10) [independent].

This pattern has a practical implication: **start with the simplest viable architecture and add complexity only when benchmarks demand it.** Concretely: begin with construction heuristic + LAHC with Change and Swap moves. Only add Kempe chains, Tabu hybridization, or ruin-and-recreate if the basic solver plateaus on real instances.

### The Real Bottleneck is Constraint Evaluation

Algorithm selection gets the most discussion in the literature, but constraint evaluation throughput determines practical solver quality. At 445,000 steps/second with incremental scoring [SolverForge GitHub, 2025](https://github.com/SolverForge/solverforge) [practitioner], a 60-second solve budget gives ~27 million candidate evaluations [author estimate — derived from 445,000 steps/sec x 60 sec; actual throughput will vary with constraint complexity and instance size]. Without incremental scoring, this might be 10,000-50,000 full evaluations [author estimate — order-of-magnitude estimate based on full-evaluation cost scaling with lesson count; no direct benchmark available] — three orders of magnitude fewer. The metaheuristic operates on top of whatever evaluation throughput you achieve; improving throughput improves every algorithm equally.

This is why SolverForge's SERIO engine (or an equivalent incremental evaluator) matters more than the choice between LAHC, SA, or TS. The ConstraintStream API with indexed joiners, filtered cross-products, and delta-only recalculation is the foundation everything else rests on.

### Build vs. Use vs. Wrap

Three paths exist:

1. **Use SolverForge directly.** Lowest effort. Model the domain with derive macros, define constraints with ConstraintStream API, configure LAHC + construction heuristic. Risk: framework immaturity.

2. **Build a custom solver.** Highest effort but full control. Implement the three-component architecture (MoveSelector, Acceptor, Forager) from scratch. Use bitsets for conflict checking. Risk: months of algorithm engineering.

3. **Wrap CP-SAT via FFI.** Good for feasibility/optimality guarantees. Risk: FFI complexity, C++ dependency, less control over search behavior.

**Recommendation: Start with path 1. Fall back to path 2 only if SolverForge proves too immature.** Path 3 is worth exploring as a verification oracle but not as the primary solver.

The fallback from 1 to 2 is low-cost because SolverForge's conceptual model (planning entities, constraint streams, lexicographic scoring) is the same model you'd use in a hand-rolled solver. Domain modeling work transfers entirely.

## Limitations and Open Problems

**No Klassenzeit-specific benchmarking.** All algorithm comparisons are from academic benchmarks (ITC, Brazilian schools, exam timetabling) or other problem domains (TSP, nurse rostering). German school conventions (e.g., double-period blocks, specific break structures, subject-specific room requirements) may introduce constraint interactions that shift the balance between algorithms.

**SolverForge maturity is unverified.** The recommendation depends heavily on SolverForge being functional and performant for this problem class. At v0.7.0 with limited community evidence, this is a bet. A day of prototyping with the actual Klassenzeit constraint set would resolve this uncertainty.

**Soft constraint weight tuning is unsolved.** The report recommends starting weights but provides no method for optimizing them. In practice, school administrators will have strong opinions about relative importance (some schools care more about teacher gaps, others about subject distribution). Exposing weights as configuration defers the problem but doesn't eliminate it.

**Kempe chain implementation complexity.** [Effect of Neighborhood Structures on TS for Timetabling, 2015](https://www.researchgate.net/publication/287192019) [independent] identifies Kempe chains as the most effective neighborhood, but implementing them requires maintaining an auxiliary conflict graph and computing connected components per move. This is non-trivial. The report recommends deferring this, but the gap between Change/Swap and Kempe chain performance may be significant on harder instances.

**Incremental scoring correctness.** Delta-based constraint evaluation is a rich source of subtle bugs. When move M changes assignments A and B, the scoring engine must correctly identify and recompute exactly the constraints that depend on A or B — no more, no less. SolverForge provides this; a hand-rolled version would need extensive property-based testing.

## Future Outlook

**Prediction 1:** A SolverForge-based Klassenzeit solver using LAHC with Change + Swap moves will produce feasible timetables for instances up to 20 classes / 50 teachers within 30 seconds, reaching soft constraint scores within 20% of FET's output quality, within 2 weeks of implementation effort. This is falsifiable by building the prototype.

**Prediction 2:** Instances above 25 classes / 70 teachers will require Kempe chain moves or ruin-and-recreate to reach acceptable quality within the 60-second budget. Simple Change + Swap will plateau at visibly suboptimal soft constraint scores. This is falsifiable by benchmarking on scaled instances.

**Prediction 3:** SolverForge will require at least one bug report or feature request during Klassenzeit implementation. At v0.7.0, no scheduling framework is complete for all use cases. The question is whether the maintainer is responsive. This is falsifiable within the first week of development.

## Conclusions and Practical Starting Point

### Summary

LAHC with two-level lexicographic scoring, implemented via SolverForge in Rust, is the recommended solver architecture for Klassenzeit v2. This combines:
- The parameterization simplicity of LAHC (one parameter) over SA (three+ parameters)
- The convergence speed advantage of local search over GAs
- The scalability of metaheuristics over pure ILP at the upper end of the target range
- The implementation leverage of SolverForge over a hand-rolled solver
- The Rust-native performance advantages (zero-allocation moves, compile-time generics) over wrapping external solvers

### Starting from Zero

Here is the concrete implementation sequence, ordered by priority:

**Week 1: Domain Model + Construction Heuristic**

1. Define the domain model:
   - `Lesson` as `#[planning_entity]` with `timeslot: Timeslot` and `room: Room` as planning variables
   - `Timeslot`, `Teacher`, `Room`, `Subject`, `SchoolClass` as `#[problem_fact]`
   - `Timetable` as `#[planning_solution]` containing all entities and facts
2. Implement hard constraints as ConstraintStream rules:
   - Teacher conflict: `for_each_unique_pair` on Lesson, filter same teacher + same timeslot, penalize
   - Class conflict: same pattern for school class
   - Room conflict: same pattern for room
   - Teacher availability: `for_each` Lesson, `if_not_exists` in teacher's available slots, penalize
   - Teacher capacity: `group_by` teacher, count, filter exceeds max, penalize by excess
   - Teacher qualification: `for_each` Lesson, filter teacher not qualified for subject, penalize
   - Room suitability: `for_each` Lesson, filter room not suitable, penalize
   - Room capacity: `for_each` Lesson, filter class size exceeds room capacity, penalize by excess
3. Configure First Fit Decreasing construction heuristic (sort lessons by most-constrained-first: lessons with fewest valid timeslot-room combinations go first)

**Week 2: Local Search + Soft Constraints**

4. Add soft constraints:
   - Teacher gaps: `group_by` teacher + day, compute gap count, penalize (-1 per gap) [author estimate]
   - Subject distribution: `for_each_unique_pair` same class + same subject + same day, penalize (-2 each) [author estimate]
   - Teacher preferred slots: `for_each` Lesson not in teacher's preferred slots, penalize (-1 each) [author estimate]
   - Class teacher first period: `for_each` class, check first period assignment, penalize (-1 per day missing) [author estimate]
5. Configure LAHC with Swap + Change moves:
   - Change move: reassign one lesson's timeslot + room
   - Swap move: exchange two lessons' timeslot + room assignments
   - Move filter: skip moves that assign a lesson to an obviously invalid slot (teacher unavailable)
6. Set LAHC list length to 500 [author estimate — reasonable starting point; will need tuning per instance characteristics] (starting point; tune later)
7. Set termination: 60 seconds or 30 seconds unimproved, whichever comes first

**Week 3: Validation + Tuning**

8. Build 3-5 test instances at different scales (small: 6 classes, medium: 15 classes, large: 30 classes)
9. Benchmark: measure feasibility rate, soft score, and time to best solution
10. If soft scores plateau: add Tabu component (tenure ~7-10 [author estimate — based on Timefold's guidance to use "a lower tabu size than pure Tabu Search"; typical pure TS tenure ranges are 15-30 for timetabling]) to LAHC configuration
11. If feasibility fails on large instances: tune construction heuristic ordering, consider adding ruin-and-recreate at 1:100 ratio [Timefold Move Selector Reference, 2025](https://docs.timefold.ai/timefold-solver/latest/optimization-algorithms/move-selector-reference) [vendor]

**Later (only if needed):**
- Kempe chain moves for harder instances
- Hybrid fix-and-optimize with good_lp + HiGHS for specific subproblems
- CP-SAT oracle for solution quality verification on small instances
- Adaptive LAHC list length based on improvement rate

### Key Data Structures

```
Lesson {
    id: u32,
    subject: SubjectId,
    teacher: TeacherId,
    school_class: ClassId,
    timeslot: Option<TimeslotId>,  // planning variable
    room: Option<RoomId>,          // planning variable
}

Timeslot {
    id: u32,
    day: Day,       // Mon-Fri
    period: u8,     // 1-8
}

// Conflict checking: bitset per teacher/class/room, indexed by timeslot
// teacher_busy: Vec<BitVec>  — teacher_busy[teacher_id][timeslot_id] = true if occupied
// Incremental update: flip two bits per Change move
```

### Score Architecture

```
HardSoftScore {
    hard: i64,  // sum of all hard constraint violations (always <= 0)
    soft: i64,  // sum of all soft constraint penalties (always <= 0)
}

// Comparison: lexicographic. (-1, -500) < (0, -999999)
// Perfect score: (0, 0)

// Hard penalties (per violation instance):
//   teacher_conflict:      -1 per pair of conflicting lessons
//   class_conflict:        -1 per pair
//   room_conflict:         -1 per pair
//   teacher_unavailable:   -1 per lesson in unavailable slot
//   teacher_over_capacity: -1 per excess hour
//   teacher_unqualified:   -1 per lesson
//   room_unsuitable:       -1 per lesson
//   room_over_capacity:    -1 per excess student

// Soft penalties [author estimate]:
//   teacher_gap:           -1 per gap period
//   subject_duplication:   -2 per duplicate subject-day-class
//   preferred_slot_miss:   -1 per missed preference
//   class_teacher_miss:    -1 per day without class teacher in period 1
```

## References

- Source 2: SA-Based Algorithm for Real-World High School Timetabling [independent, academic] — https://ieeexplore.ieee.org/document/5632136 (accessed 2026-04-04)
- Source 5: Large-scale Timetabling with Adaptive Tabu Search [independent, academic] — https://www.degruyterbrill.com/document/doi/10.1515/jisys-2022-0003/html (accessed 2026-04-04)
- Source 8: Educational Timetabling Benchmarks, arXiv survey [independent, academic] — https://arxiv.org/pdf/2201.07525 (accessed 2026-04-04)
- Source 10: Late Acceptance Hill-Climbing Heuristic [independent, academic] — https://www.sciencedirect.com/science/article/abs/pii/S0377221716305495 (accessed 2026-04-04)
- Source 11: Fix-and-Optimize Heuristic for High School Timetabling [independent, academic] — https://www.sciencedirect.com/science/article/pii/S0305054814001816 (accessed 2026-04-04)
- Source 14: Review of University Timetabling, MDPI 2025 [independent, academic] — https://www.mdpi.com/2079-3197/13/1/10 (accessed 2026-04-04)
- Source 15: CP for High School Timetabling with Hot Starts [independent, academic] — https://link.springer.com/chapter/10.1007/978-3-319-93031-2_10 (accessed 2026-04-04)
- Source 16: Google OR-Tools vs Timefold [vendor, Timefold comparison page] — https://timefold.ai/blog/google-or-tools-versus-timefold-comparison (accessed 2026-04-04)
- Source 18: When Simpler is Better, IEEE 2023 [independent, academic] — https://ieeexplore.ieee.org/document/10253986/ (accessed 2026-04-04)
- Source 22: School Timetabling with CP-SAT, Medium 2026 [practitioner, tutorial] — https://medium.com/suboptimally-speaking/school-timetabling-with-constraint-programming-495f1126c28d (accessed 2026-04-04)
- Source 23: CP-SAT Primer [practitioner, educational resource] — https://d-krupke.github.io/cpsat-primer/ (accessed 2026-04-04)
- Source 24: FET Free Timetabling Software [practitioner, open-source project] — https://lalescu.ro/liviu/fet/ (accessed 2026-04-04)
- Source 25: UniTime CPSolver [practitioner, open-source project] — https://github.com/UniTime/cpsolver (accessed 2026-04-04)
- Source 26: Hybrid SA with Kempe Chain Neighborhood [independent, academic] — https://www.researchgate.net/publication/221635609 (accessed 2026-04-04)
- Source 27: Effect of Neighborhood Structures on TS for Timetabling [independent, academic] — https://www.researchgate.net/publication/287192019 (accessed 2026-04-04)
- Source 29: Timefold Move Selector Reference [vendor, documentation] — https://docs.timefold.ai/timefold-solver/latest/optimization-algorithms/move-selector-reference (accessed 2026-04-04)
- Source 30: Timefold Local Search Configuration [vendor, documentation] — https://docs.timefold.ai/timefold-solver/latest/optimization-algorithms/local-search (accessed 2026-04-04)
- Source 31: Timefold Score Performance Tips [vendor, documentation] — https://docs.timefold.ai/timefold-solver/latest/constraints-and-score/performance (accessed 2026-04-04)
- Source 34: Timefold Score Overview [vendor, documentation] — https://docs.timefold.ai/timefold-solver/latest/constraints-and-score/overview (accessed 2026-04-04)
- Source 36: University Course Timetabling with Soft Constraints [independent, academic] — https://www.unitime.org/papers/patat03.pdf (accessed 2026-04-04)
- Source 40: SolverForge GitHub [practitioner, open-source Rust crate v0.7.0] — https://github.com/SolverForge/solverforge (accessed 2026-04-04)
- Source 41: SolverForge Overview Documentation [practitioner, documentation] — https://solverforge.org/docs/overview/ (accessed 2026-04-04)
- Source 42: good_lp Rust crate [practitioner, open-source] — https://github.com/rust-or/good_lp (accessed 2026-04-04)
- Source 44: Pumpkin CP Solver [independent, academic Rust project] — https://github.com/consol-lab/pumpkin (accessed 2026-04-04)
- Source 46: bitvec Rust crate [practitioner, open-source] — https://docs.rs/bitvec/latest/bitvec/ (accessed 2026-04-04)
- Source 48: Python vs Rust for MILP Solvers [practitioner, benchmark comparison] — https://www.osiopt.com/blogs/python-or-rust-performance-comparison-in-optimization-model-environment (accessed 2026-04-04)
- Source 49: Arena Allocation patterns [practitioner, Rust documentation] — https://oneuptime.com/blog/post/2026-01-07-rust-memory-optimization/view (accessed 2026-04-04)
- Source 52: General Cooling Schedules for SA-Based Timetabling [independent, academic] — https://link.springer.com/chapter/10.1007/3-540-61794-9_70 (accessed 2026-04-04)
- Source 53: SA Cooling Schedules for School Timetabling [independent, academic] — https://iaorifors.com/paper/30085 (accessed 2026-04-04)
- Source 54: Cooling Schedules for Optimal Annealing [independent, theoretical] — https://pubsonline.informs.org/doi/10.1287/moor.13.2.311 (accessed 2026-04-04)
- Source 56: Adaptive Tabu Search for Course Timetabling [independent, academic] — https://www.researchgate.net/publication/222418829 (accessed 2026-04-04)
- Source 57: Reactive Tabu Search [practitioner, documentation] — https://algorithmafternoon.com/stochastic/reactive_tabu_search/ (accessed 2026-04-04)
- Source 59: Measurability and Reproducibility in Timetabling Research [independent, academic] — https://link.springer.com/chapter/10.1007/978-3-540-77345-0_3 (accessed 2026-04-04)
- Source 61: Timefold Benchmarking [vendor, documentation] — https://docs.timefold.ai/timefold-solver/latest/using-timefold-solver/benchmarking-and-tweaking (accessed 2026-04-04)
- Source 63: GA Premature Convergence Issues [independent, Wikipedia] — https://en.wikipedia.org/wiki/Premature_convergence (accessed 2026-04-04)
