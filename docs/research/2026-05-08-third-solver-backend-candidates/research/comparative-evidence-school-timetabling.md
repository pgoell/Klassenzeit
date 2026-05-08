# Comparative Evidence: School-Timetabling-Shaped Problems

Cluster: `comparative-evidence-school-timetabling`. Notes only, no synthesis.
Each fact carries an inline source citation in the format
`[source: <URL> | <Title> | <credibility-tag>]`.

---

## SQ1 — ITC 2007 / 2011 / XHSTT recent winners and their solver choices

### ITC 2011 (high school timetabling)

- The ITC2011 high school competition was won by the GOAL team (Fonseca, Santos, Toffolo, Brito, Souza), with a hybrid local-search solver. "The clear winner was the GOAL team (Fonseca et al. 2012), with an average rank of 1.18. Their solver produced the best solutions to most of the tests, and was never worse than equal second on any test." [source: https://www2.cs.sfu.ca/~mitchell/cmpt-827/2015-Fall/Projects/TT-ITC-2011-Report.pdf | The Third International Timetabling Competition | independent]
- GOAL is described as "a hybrid local search based solver for high school timetabling" [source: https://www.researchgate.net/publication/364582707_Systematic_Literature_Review_of_Metaheuristic_Methodologies_for_High_School_Timetabling_Problem | Systematic Literature Review of Metaheuristic Methodologies for High School Timetabling Problem | independent]
- "The ITC-2011 competition was dominated by metaheuristic methods, and in particular won by Fonseca, Santos, Toffolo, Brito, & Souza (2016b)" [source: https://www.researchgate.net/publication/364582707_Systematic_Literature_Review_of_Metaheuristic_Methodologies_for_High_School_Timetabling_Problem | Systematic Literature Review of Metaheuristic Methodologies for High School Timetabling Problem | independent]
- Other ITC2011 entries used VNS variants: "Fonseca and Santos (2014) proposed several VNS algorithms to solve ITC2011 high school timetabling problems. The variants of VNS tested are basic VNS, Reduced VNS (RVNS), Skewed VNS (SVNS) and Sequential Variable Neighbourhood Descent (SVND)." [source: https://www.sciencedirect.com/science/article/abs/pii/S0305054813003328 | Variable Neighborhood Search based algorithms for high school timetabling | independent]

### XHSTT — ongoing benchmark archive

- XHSTT is not a one-off competition; "the High School Timetabling Archive XHSTT-2011 contains 21 instances from 8 countries" and "currently 38 real-life instances from 11 different countries" [source: https://link.springer.com/article/10.1007/s10479-011-1012-2 | XHSTT: an XML archive for high school timetabling problems in different countries | independent]
- Recent (2022–2023) submissions of new best-known solutions are dominated by Wiesław Dudak (Wieslaw Dudek Timetables, Krakow), a commercial timetabling vendor (toolchain/solver class not publicly disclosed on the archive). "January 15, 2023: Wiesław Dudak (Wieslaw Dudek Timetables, Krakow) submitted improved solutions for artificial instances, reducing All11's infeasibility from 32 to 9 and All15 from 197 to 39." [source: https://www.utwente.nl/en/eemcs/dmmp/hstt/ | DMMP - High School Timetabling Project (HSTT) | independent]
- "November 16, 2022: Dudak achieved significant breakthroughs, bringing England StPaul down to 2 infeasibilities with objective value 1410, and producing the first feasible solution for Kottenpark2008." [source: https://www.utwente.nl/en/eemcs/dmmp/hstt/ | DMMP - High School Timetabling Project (HSTT) | independent]
- Notable academic submitters across the archive: George Fonseca (UFOP-GOAL team, "Reduced costs by over 50% on nine XHSTT-2014 instances; proved optimality for TES99"), Matias Sørensen (Brazil/Spain instances, MIP-based), and Martin Klemsa (Skolaris Software, Westside2009 with optimality proof, 2017) [source: https://www.utwente.nl/en/eemcs/dmmp/hstt/ | DMMP - High School Timetabling Project (HSTT) | independent]
- Kristiansen, Sørensen and Stidsen's MIP approach: "The approach found previously unknown optimal solutions for 2 instances of XHSTT and proved optimality of 4 known solutions. For the instances not solved to optimality, new non-trivial lower bounds were found in 11 cases, and new best known solutions were found in 9 cases." [source: https://link.springer.com/article/10.1007/s10951-014-0405-x | Integer programming for the generalized high school timetabling problem | independent]
- Same paper: "compared with the finalists of Round 2 of the International Timetabling Competition 2011 and was shown to be competitive with one of the finalists." [source: https://link.springer.com/article/10.1007/s10951-014-0405-x | Integer programming for the generalized high school timetabling problem | independent]

### ITC 2019 (university course timetabling)

- ITC2019 was won by team DSUM. Their algorithm "is based on a Mixed Integer Programming (MIP) model … For the commercial solver, DSUM used Gurobi 8.1.1 (later updated to Gurobi 9.0 post-competition) as their MIP solver." [source: https://dsumsoftware.com/itc2019/ | ITC 2019 – DSUM | practitioner]
- "5 of DSUM's solutions to the ITC 2019 instances are proven optimal. Additionally, DSUM improved 21 of their competition solutions since the competition deadline (November 18, 2019)." [source: https://dsumsoftware.com/itc2019/ | ITC 2019 – DSUM | practitioner]
- Holm et al. publish the graph-based MIP formulation: "A graph-based MIP formulation of the International Timetabling Competition 2019" in Journal of Scheduling 25(4), 2022. [source: https://ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00724-y.html | A graph-based MIP formulation of the International Timetabling Competition 2019 | independent]

### ITC 2021 (sports timetabling)

- "The first place winner was Team UoS consisting of Toni Martínez-Sykora, Chris Potts, and Carlos Lamas-Fernández with 596 points. Second place went to Team Udine consisting of Roberto Maria Rosati, Matteo Petris, Luca Di Gaspero, and Andrea Schaerf with 424 points." [source: https://robinxval.ugent.be/ITC2021/index.php | ITC2021 | Sport Scheduling Research Group | independent]
- Top finishers used metaheuristics: "Multi-neighborhood simulated annealing for the sports timetabling competition ITC2021" by Rosati et al. (Udine, second place). [source: https://link.springer.com/article/10.1007/s10951-022-00740-y | Multi-neighborhood simulated annealing for the sports timetabling competition ITC2021 | independent]
- An IP-and-fix-and-optimize matheuristic was also among approaches: "An integer programming formulation and a fix-and-optimize heuristic were proposed to address the problem, with the fix-and-optimize approach using the IP formulation to heuristically decompose the problem into sub-problems." [source: https://link.springer.com/article/10.1007/s10951-022-00738-6 | A fix-and-optimize heuristic for the ITC2021 sports timetabling problem | independent]

### IHTC 2024 (integrated healthcare timetabling, adjacent)

- 32 teams submitted; final podium at EURO 2025 (Leeds): "1. v777v (independent) — €1,100; 2. SDU-IMADA (University of Southern Denmark) — €700; 3. Twente (University of Twente) — €400. SDU-IMADA additionally won a €200 open-source software prize." [source: https://ihtc2024.github.io/ | IHTC 2024 - The Integrated Healthcare Timetabling Competition 2024 | independent]
- "v777v performs best in terms of the number of upper bounds (best solutions found), while SDU-IMADA achieves both a better average objective value and a smaller average gap to the upper bounds than any other finalist." [source: https://www.sciencedirect.com/science/article/pii/S3050784725000157 | The Integrated Healthcare Timetabling competition 2024 | independent]
- "v777v has a smaller mean rank for 14 instances, while SDU-IMADA does better for the other 16 instances. The performance of these two methods can be considered largely equivalent." [source: https://www.sciencedirect.com/science/article/pii/S3050784725000157 | The Integrated Healthcare Timetabling competition 2024 | independent]
- "CP-SAT (OR Tools) was used by four teams" in IHTC2024 [source: https://www.sciencedirect.com/science/article/pii/S3050784725000157 | The Integrated Healthcare Timetabling competition 2024 | independent]
- "SDU-IMADA was the only finalist who did not employ commercial solvers, ranking first in the open-source category." Their entry: "a local-search-based meta-heuristic algorithm implemented in Python and C++" [source: https://roar-net.eu/news/ihtc-2024-best-oss-prize/ | IHTC 2024 Best Open-Source Software Prize | practitioner]
- "Othman and Chiarandini developed a multi-neighborhood, lexicographic local search algorithm for the integrated healthcare timetabling competition 2024" [source: https://imada.sdu.dk/u/marco/Files/Chiarandini-CV.pdf | Marco Chiarandini Curriculum Vitæ as of October 2025 | practitioner]
- ORTEC (4th place) used MIP modeling with Gurobi [source: https://ortec.com/en/news/ortec-4th-place-healthcare-timetabling-competition | ORTEC Showcases Healthcare Optimization Leadership at IHTC 2024 | vendor]
- Team Twente (3rd place) "Our approach combines mixed-integer programming, constraint programming, and simulated annealing in a 3-phase solution approach based on decomposition into subproblems" [source: https://arxiv.org/abs/2511.04685 | A hybrid solution approach for the Integrated Healthcare Timetabling Competition 2024 | independent]
- Twente specifies tools: CP solver "OR-Tools" and MIP solver "Gurobi 12 … via an academic license"; "implemented the same model using CP, and solved it using OR-Tools, which was much faster in finding feasible solutions than Gurobi" [source: https://arxiv.org/html/2511.04685 | A hybrid solution approach for the Integrated Healthcare Timetabling Competition 2024 | independent]
- Twente self-report: "During the competition session at the EURO 2025 conference, it became apparent that other successful submissions were entirely heuristic-based approaches or used MIP only as a means to generate good solutions." [source: https://arxiv.org/html/2511.04685 | A hybrid solution approach for the Integrated Healthcare Timetabling Competition 2024 | independent]

---

## SQ2 — Direct head-to-head comparisons of CP-SAT vs HiGHS / SCIP / Gecode / metaheuristic on school-timetabling-shaped problems (2022–2026)

### The 2023 EJOR survey (Ceschia, Di Gaspero, Schaerf)

- Survey: "Educational timetabling: Problems, benchmarks, and state-of-the-art results", European Journal of Operational Research, vol. 308, no. 1, pp. 1–18, 2023. The arXiv preprint is 2201.07525. [source: https://www.sciencedirect.com/science/article/pii/S0377221722005641 | Educational timetabling: Problems, benchmarks, and state-of-the-art results | independent]
- Full PDF could not be retrieved directly via WebFetch (PDF binary content); abstract retrieved via arXiv: "the survey identifies six standard formulations and discusses their features, relevance, and usability. … reports main state-of-the-art results on the selected benchmarks, including solution quality (upper and lower bounds), search techniques, running times, statistical distributions, and other relevant settings." [source: https://arxiv.org/abs/2201.07525 | Educational Timetabling: Problems, Benchmarks, and State-of-the-Art Results | independent]
- Trend reported in 2022 SLR: "a shift of popularity from meta-heuristic to mathematical optimisation is observed in recent years. Recently exact methods based on integer programming, maxSAT and constraint programming have proven to be very effective for XHSTT." [source: https://www.researchgate.net/publication/364582707_Systematic_Literature_Review_of_Metaheuristic_Methodologies_for_High_School_Timetabling_Problem | Systematic Literature Review of Metaheuristic Methodologies for High School Timetabling Problem | independent]
- 2025 review of 95 IP-based university timetabling models, 1990–2023: "The implementation rate of models using integer programming is 98%, which is much higher than 34% implementation rates using meta-heuristics algorithms"; "CPLEX is the most frequently used integer programming solver for three types of timetabling problems including course timetabling, class timetabling, and exam timetabling." Reported solver counts: CPLEX (47), Gurobi (11), Google OR-Tools CP-SAT (1) [source: https://www.mdpi.com/2079-3197/13/1/10 | From Integer Programming to Machine Learning: A Technical Review on Solving University Timetabling Problems | independent]

### CP-SAT vs MIP head-to-head on scheduling

- Perron and Didier (Google) at CP 2023 invited talk on CP-SAT-LP: "improves upon the chuffed solver in two main directions. First, it uses a simplex alongside the SAT engine. Second, it implements and relies upon a portfolio of diverse workers for its search part." Claims "unsurpassed performance in the Constraint Programming community", "breakthrough results on Scheduling benchmarks (with the closure of many open problems)" including "Job-Shop and Resource Constraint Project Scheduling Problems", and "competitive results with the best MIP solvers (on purely integral problems)." [source: https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.CP.2023.3 | The CP-SAT-LP Solver (Invited Talk) | vendor]
- Perron presentation summary text quoted by independent search snapshot: "On constraint programming problems, CP-SAT beats Gurobi; on linear integer problems, CP-SAT beats SCIP and sometimes wins against Gurobi, but not often." [source: https://schedulingseminar.com/presentations/SchedulingSeminar_LaurentPerron.pdf | CP-SAT for scheduling | vendor]
- Same talk on CP Optimizer trade-off: "for scheduling problems with setup/transition times and costs, while the model is huge compared to CP Optimizer and presolve is much slower, there are cases where CP Optimizer proves optimality in 20 seconds while CP-SAT presolves the problem in 1 minute, then proves optimality in 5 seconds." [source: https://schedulingseminar.com/presentations/SchedulingSeminar_LaurentPerron.pdf | CP-SAT for scheduling | vendor]
- Same talk: "On academic scheduling problems (PSPLIB, JSSP), CP-SAT is better than CP Optimizer, thanks to recent work on scheduling LNS and scheduling cuts. On CP problems, CP-SAT beats Gurobi and SCIP, while on linear integer problems, CP-SAT beats SCIP and is not far from CPLEX." (snapshotted summary) [source: https://schedulingseminar.com/presentations/SchedulingSeminar_LaurentPerron.pdf | CP-SAT for scheduling | vendor]
- Vendor counter-benchmark on JSSP at very large scale (1000×1000 instances, tai_j1000_m1000_1): "Hexaly's results after 10 minutes of computation are, on average, 8.5% better than CP Optimizer's after 6 hours" and "OR-Tools, Gurobi, and Cplex fail to deliver feasible solutions within 6 hours of running time"; representative tai_j1000_m1000_1 numbers: Hexaly (10m) 877,062 vs CP Optimizer (6h) 959,945 (8.6% improvement) [source: https://www.hexaly.com/benchmarks/hexaly-vs-cp-optimizer-vs-or-tools-on-the-job-shop-scheduling-problem-jssp | Hexaly, CP Optimizer, OR-Tools, Gurobi, Cplex on very large-scale instances of the Job Shop Scheduling Problem (JSSP) | vendor]

### MiniZinc Challenge medal table 2023, 2024, 2025

CP-SAT (OR-Tools) holds gold across all four 2024 categories on which it competes; same in 2025; Pumpkin (Rust LCG-CP) entered 2024 and won bronze in 2025.

- 2023 medals (Fixed / Free / Parallel / Local Search): Fixed — Gold OR-Tools, Silver SICStus Prolog, Bronze Choco 4. Free — Gold OR-Tools, Silver PicatSAT, Bronze iZplus. Parallel — Gold OR-Tools, Silver PicatSAT, Bronze Choco 4. Local Search — Gold Yuck. [source: https://www.minizinc.org/challenge/2023/results/ | MiniZinc - Challenge 2023 Results | independent]
- 2024 medals: Fixed — Gold OR-Tools CP-SAT, Silver Choco-solver CP-SAT, Bronze SICStus Prolog. Free — Gold OR-Tools CP-SAT, Silver PicatSAT, Bronze iZplus. Open — Gold OR-Tools CP-SAT, Silver PicatSAT, Bronze Choco-solver CP. Local Search — Gold OR-Tools CP-SAT LS, Silver Yuck. 14 registered entrants including Pumpkin among others. [source: https://www.minizinc.org/challenge/2024/results/ | MiniZinc - Challenge 2024 Results | independent]
- 2025 medals: Fixed — Gold OR-Tools CP-SAT, Silver Choco-solver CP-SAT, Bronze SICStus Prolog / Pumpkin. Free — Gold OR-Tools CP-SAT, Silver PicatSAT, Bronze Choco-solver CP-SAT. Parallel — Gold OR-Tools CP-SAT, Silver PicatSAT, Bronze iZplus. Local Search — Gold OR-Tools CP-SAT LS, Silver Yuck, Bronze Atlantis. "There is no OPEN medal this year as there were no portfolio solver entrants at the competition deadline." [source: https://www.minizinc.org/challenge/2025/results/ | MiniZinc - Challenge 2025 Results | independent]
- "CP-SAT … has won multiple gold medals at the MiniZinc challenge since its debut in 2017." [source: https://research.google/blog/the-minizinc-challenge/ | The MiniZinc Challenge | vendor]

### Open-source MIP relative performance (HiGHS / SCIP / CBC)

- "HiGHS is the top-ranked open-source solver in MIP rankings according to Hans Mittelmann's benchmark." [source: https://github.com/ERGO-Code/HiGHS/discussions/1683 | Do we know why open source MIP solvers are significantly slower than commercial ones? | practitioner]
- "There is approximately one order of magnitude performance difference between HiGHS and Gurobi"; "CBC's performance is now significantly inferior to HiGHS"; "SCIP is currently one of the fastest non-commercial solvers for mixed integer programming (MIP) and mixed integer nonlinear programming (MINLP)." [source: https://github.com/ERGO-Code/HiGHS/discussions/1683 | Do we know why open source MIP solvers are significantly slower than commercial ones? | practitioner]
- License snapshot: "HiGHS is freely available under the MIT licence", "CBC … published under the Common Public License", "SCIP 10.0 is licensed under the Apache 2.0 license" [source: https://highs.dev/ | HiGHS - High-performance parallel linear optimization software | independent] [source: https://en.wikipedia.org/wiki/COIN-OR | COIN-OR - Wikipedia | independent] [source: https://arxiv.org/html/2511.18580 | The SCIP Optimization Suite 10.0 | independent]
- "In August 2024 Gurobi decided to withdraw from [Mittelmann's] benchmarks as well and their results have been removed." [source: https://plato.asu.edu/bench.html | Decison Tree for Optimization Software | independent]

### Pure CP-SAT failures noted on real-world school timetabling-shaped data

- Pure ILP on 18 real-world German high schools, Gurobi 10.0.1, 6h time limit, 64 GB RAM, AMD EPYC 7252: "Out of 18 instances, solutions were found for only 10 instances (55% success rate)"; "the integral solutions obtained from the experiment did not meet the criteria necessary for a viable school timetable"; "our ILP model as an exact method for finding solutions is not very effective, and even after 6 hours of runtime, it could only find solutions that are nowhere near satisfactory." [source: https://arxiv.org/html/2407.16898v1 | Introducing Individuality into Students' High School Timetables | independent]
- General CP-SAT scaling note: "MIP-solvers are frequently able to optimize problems with hundreds of thousands of variables and constraints, classical CP-solvers often struggle with problems with more than a few thousand variables and constraints. However, the relatively new CP-SAT of Google's OR-Tools suite shows to overcome many of the weaknesses and provides a viable alternative to MIP-solvers, being competitive for many problems and sometimes even superior." [source: https://d-krupke.github.io/cpsat-primer/ | Introduction - The CP-SAT Primer | practitioner]
- CP-SAT timetabling pain reported: "Performance becomes far poorer when some events (modelled as optional intervals) need to be left unscheduled, with problems taking >2 minutes to solve on 8 cores despite only trying to optimize 4 intervals." [source: https://github.com/google/or-tools/issues/1102 | CP-SAT: Performance drops from 60s to 700s - why? | practitioner]

---

## SQ3 — Where CP-SAT plateaus, what tends to break it in literature?

### Matheuristic / fix-and-optimize / LNS over a MIP model

- "Fix-and-optimize is a matheuristic that iteratively decomposes a problem into smaller subproblems. In each iteration of the algorithm, a decomposition process is applied aiming at fixing most of the decision variables at their value in the current solution. … each subproblem can be solved fairly quickly by a MIP solver, when compared with the full model." [source: https://www.sciencedirect.com/science/article/pii/S0305054814001816 | A fix-and-optimize heuristic for the high school timetabling problem | independent]
- Fonseca et al. on XHSTT-2014 with matheuristic over an alternative MIP formulation: "an alternative formulation provided four new best known lower bounds and, used in a matheuristic framework, improved eleven best known solutions." [source: https://www.sciencedirect.com/science/article/abs/pii/S0377221717302242 | Integer programming techniques for educational timetabling | independent]
- George Fonseca activity on XHSTT archive: "George Fonseca (2016-2017): Reduced costs by over 50% on nine XHSTT-2014 instances; proved optimality for TES99" [source: https://www.utwente.nl/en/eemcs/dmmp/hstt/ | DMMP - High School Timetabling Project (HSTT) | independent]
- "Matheuristic approach combines a Variable Neighbourhood Search algorithm with mathematical programming-based neighbourhoods for high school timetabling. This hybrid approach outperforms the standalone Variable Neighbourhood Search algorithm by far." [source: https://link.springer.com/article/10.1007/s10951-024-00817-w | Modelling and solving the university course timetabling problem with hybrid teaching considerations | independent]

### MaxSAT-based LNS

- Demirović and Musliu (2017) MaxSAT LNS: "the first time maxSAT was used within a LNS scheme. They proposed a destroy operator with two neighborhood vectors and a novel insertion approach, for which they modified the open-source maxSAT solver Open-WBO". Result: "managed to compute four new best known upper bounds for high school timetabling problems. Their approach outperformed the state-of-the-art solvers on many instances." [source: https://www.sciencedirect.com/science/article/abs/pii/S0305054816301927 | MaxSAT-based large neighborhood search for high school timetabling | independent]
- Same direction earlier on curriculum-based course timetabling: "researchers have applied SAT solvers and optimizers to the Curriculum-based Course Timetabling problem, yielding the best known solutions for 21 out of 32 standard benchmark instances, with 18 new lower bounds obtained using a Weighted Partial MaxSAT approach." [source: https://link.springer.com/article/10.1007/s10479-012-1081-x | Curriculum-based course timetabling with SAT and MaxSAT | independent]

### CP with hot-start / phase-saving (improving CP-SAT itself)

- "A drastic improvement in performance can be achieved by including solution-based phase saving, which directs the CP solver to first search in close proximity to the best solution found, and hot start approaches where existing heuristic methods produce a starting point for the CP solver." [source: https://link.springer.com/chapter/10.1007/978-3-319-93031-2_10 | Constraint Programming for High School Timetabling: A Scheduling-Based Model with Hot Starts | independent]
- ASP-based LNPS for course timetabling: "Large Neighborhood Prioritized Search (LNPS) is a metaheuristic that starts with an initial solution and then iteratively tries to obtain improved solutions by alternately destroying and prioritized searching for a current solution, and such approaches can significantly enhance the solving performance for course timetabling." [source: https://link.springer.com/chapter/10.1007/978-3-031-74209-5_5 | ASP-Based Large Neighborhood Prioritized Search for Course Timetabling | independent]

### CP-SAT internal LNS

- "Large neighborhood search (LNS) is a notable strategy in CP-SAT that tries to find a better solution by changing only a few variables. In practice, CP-SAT schedules its LNS strategies using a simple round-robin method" [source: https://d-krupke.github.io/cpsat-primer/09_lns.html | Large Neighborhood Search - The CP-SAT Primer | practitioner]

### Decomposition / parallel matheuristic on ITC2019

- "A parallelized matheuristic for the International Timetabling Competition 2019 … uses multiple methods based on a graph-based mixed-integer programming (MIP) model, includes two methods for producing initial solutions and uses a fix-and-optimize matheuristic to improve solutions, while also using the full MIP model to calculate lower bounds." [source: https://ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00728-8.html | A parallelized matheuristic for the International Timetabling Competition 2019 | independent]

---

## SQ4 — Concrete documented cases where switching to or adding HiGHS / SCIP / Gecode / Pumpkin gave measurable wins over CP-SAT or beat OptaPlanner / Timefold

### Direct evidence: thin

No 2022–2026 paper found in this cluster's searches that documents a measurable win for HiGHS, SCIP, Gecode, or Pumpkin over CP-SAT specifically on a school-timetabling-shaped problem. Counter-examples and tangents:

- HiGHS specifically on scheduling/timetabling: **no school-timetabling case study found** in this search round. "I did not find any research papers or case studies specifically about the HiGHS solver being applied to scheduling or timetabling problems." [source: https://highs.dev/ | HiGHS - High-performance parallel linear optimization software | independent] (negative finding via search summary)
- SCIP specifically on school timetabling: **no head-to-head case study found**. The 2025 IP-review across 95 university timetabling papers (1990–2023) reports CPLEX (47), Gurobi (11), Google OR-Tools CP-SAT (1) — SCIP not in the headline counts, indicating low representation. [source: https://www.mdpi.com/2079-3197/13/1/10 | From Integer Programming to Machine Learning: A Technical Review on Solving University Timetabling Problems | independent]
- Gecode on educational timetabling: "A CP model for examination timetabling was encoded in the MiniZinc modeling language and solved with Gecode, as well as two MIP models solved with Gurobi" — this is methodology mention, no head-to-head against CP-SAT. [source: https://docs.minizinc.dev/en/stable/solvers.html | Solving Technologies and Solver Backends — The MiniZinc Handbook | independent]
- Pumpkin on scheduling: Pumpkin examples include "disjunctive scheduling problems, alongside other example problems like BIBD and N-Queens"; published research at CP conference "including work on finding disjunctive cliques for scheduling problems". No XHSTT or course-timetabling benchmark numbers found. [source: https://github.com/consol-lab/pumpkin | GitHub - ConSol-Lab/Pumpkin | independent]
- Pumpkin maturity signal: "won bronze in the 2025 MiniZinc Challenge's fixed search track"; tied for bronze with SICStus Prolog [source: https://github.com/consol-lab/pumpkin | GitHub - ConSol-Lab/Pumpkin | independent] [source: https://www.minizinc.org/challenge/2025/results/ | MiniZinc - Challenge 2025 Results | independent]
- Pumpkin license, language, latest release: "Dual-licensed under Apache-2.0 and MIT", "primary language Rust (90.8% of codebase)", "Latest Release: pumpkin-solver-v0.3.0 (February 11, 2026)", "python bindings contained in pumpkin-solver-py" [source: https://github.com/consol-lab/pumpkin | GitHub - ConSol-Lab/Pumpkin | independent]

### Indirect evidence: where MIP-only beats CP-SAT-only

- ITC2019 winner DSUM: pure MIP via Gurobi, beat all CP-only and metaheuristic-only entries on university course timetabling (a related but different problem class). 5 instances proven optimal. [source: https://dsumsoftware.com/itc2019/ | ITC 2019 – DSUM | practitioner]
- IHTC2024: 1st (v777v) and 2nd (SDU-IMADA) were "entirely heuristic-based approaches or used MIP only as a means to generate good solutions"; the team using OR-Tools CP-SAT alongside Gurobi MIP and SA placed 3rd. [source: https://arxiv.org/html/2511.04685 | A hybrid solution approach for the Integrated Healthcare Timetabling Competition 2024 | independent]

### Indirect evidence: where heuristic / metaheuristic beats both

- Sports timetabling ITC2021 algorithm-selection paper analyzing 8 SOTA algorithms across 563 instances: "Metaheuristics (Udine, Goal, UoS): Dominated overall performance. Udine found feasible solutions for the most instances and demonstrated 'excellent results' in phased tournament regions." MIP solvers: "MIP Solvers (MODAL & Reprobate): Struggled considerably. The paper notes that 'complex sports scheduling problems are still beyond reach for IP solvers,' with MODAL finding feasible solutions for the fewest instances overall." CP/SAT hybrid (DITUoIArta): "Found feasible solutions on approximately 400+ instances". Coverage stat: "Udine's footprint covered 95.6% of the problem space with good solutions, while MODAL's covered 0%." [source: https://arxiv.org/html/2309.03229v2 | Which algorithm to select in sports timetabling? | independent]

### Timefold / OptaPlanner head-to-head

- Vendor-claimed performance: "Timefold is twice as fast as OptaPlanner out-of-the-box. More specifically, Timefold 1.0.0 delivers up to 15% better performance compared to OptaPlanner for problems modeled using Constraint Streams … teams moving from OptaPlanner to Timefold report around 5% better schedules on average." [source: https://timefold.ai/blog/optaplanner-fork | OptaPlanner continues as Timefold | vendor]
- No 2022–2026 paper found here showing Timefold or OptaPlanner achieving new XHSTT best-known results.

---

## SQ5 — Production-system precedents for the hybrid pattern (FFD + LAHC + external validator)

### Hybrid CP / MIP / metaheuristic in production-style competition settings

- Twente IHTC2024 paper articulates a hybrid pattern that mirrors a "validator" role for the exact engine: "implemented the same model using CP, and solved it using OR-Tools, which was much faster in finding feasible solutions than Gurobi"; later "MIP only as a means to generate good solutions" was the pattern used by higher-finishing teams. [source: https://arxiv.org/html/2511.04685 | A hybrid solution approach for the Integrated Healthcare Timetabling Competition 2024 | independent]
- IHTC2024 third-place paper itself: "combines mixed-integer programming, constraint programming, and simulated annealing in a 3-phase solution approach based on decomposition into subproblems." [source: https://arxiv.org/abs/2511.04685 | A hybrid solution approach for the Integrated Healthcare Timetabling Competition 2024 | independent]
- 2022 SLR on metaheuristics: "Hybrid systems, in which constraint programming is used to verify feasibility and metaheuristics are used to promote quality, are now recommended by many researchers." [source: https://www.researchgate.net/publication/364582707_Systematic_Literature_Review_of_Metaheuristic_Methodologies_for_High_School_Timetabling_Problem | Systematic Literature Review of Metaheuristic Methodologies for High School Timetabling Problem | independent]

### Production heuristic approach: FET (deployed school timetabling tool)

- FET algorithm "named 'recursive swapping', may be related to the algorithm known as 'ejection chain' or the more generalized 'ejection tree'." Activities sorted hardest-first; failed placements trigger recursive evictions of conflicting activities. [source: https://lalescu.ro/liviu/fet/doc/en/generation-algorithm-description.html | FET - Timetable Generation Algorithm - Description | practitioner]
- FET runtime envelope: "Usually, FET is able to solve a complicated timetable in maximum 5-20 minutes. For simpler timetables, it may take less time, under 5 minutes, while for extremely difficult timetables it may take a matter of hours." [source: https://lalescu.ro/liviu/fet/doc/en/generation-algorithm-description.html | FET - Timetable Generation Algorithm - Description | practitioner]

### OptaPlanner / Timefold: local-search-with-construction-heuristic as the dominant pattern

- "OptaPlanner is an AI constraint solver with advanced algorithms that deliver a near-optimal solution in a reasonable amount of time. Local Search variations (Tabu Search, Simulated Annealing, Late Acceptance, …) usually perform best for real-world problems given real-world time limitations." [source: https://www.optaplanner.org/docs/optaplanner/latest/optimization-algorithms/optimization-algorithms.html | Optimization algorithms :: Documentation | vendor]

### MIP-validator + LNS pattern callout

- ITC2019 parallelized matheuristic (Lemos et al., Journal of Scheduling 2022): "uses multiple methods based on a graph-based mixed-integer programming (MIP) model, includes two methods for producing initial solutions and uses a fix-and-optimize matheuristic to improve solutions, while also using the full MIP model to calculate lower bounds." [source: https://ideas.repec.org/a/spr/jsched/v25y2022i4d10.1007_s10951-022-00728-8.html | A parallelized matheuristic for the International Timetabling Competition 2019 | independent]

### IHTC2024 Best Open-Source Software prize submission as a production-shaped artifact

- SDU-IMADA: "a local-search-based meta-heuristic algorithm implemented in Python and C++ following the ROAR-NET API specification" (placed 2nd overall, won open-source prize) [source: https://roar-net.eu/news/ihtc-2024-best-oss-prize/ | IHTC 2024 Best Open-Source Software Prize | practitioner]
- "Othman and Chiarandini developed a multi-neighborhood, lexicographic local search algorithm" [source: https://imada.sdu.dk/u/marco/Files/Chiarandini-CV.pdf | Marco Chiarandini Curriculum Vitæ as of October 2025 | practitioner]
- Code: ihtc2024-imada-submission [source: https://github.com/Arthod/ihtc2024-imada-submission/blob/main/src/solution_data.h | ihtc2024-imada-submission/src/solution_data.h at main | practitioner]

---

## Cross-cutting reference points (cited but not bucketed under one SQ)

- Klassenzeit project context (LAHC + Rust solver + CP-SAT backend) is not directly referenced in any external publication; absence of "Klassenzeit" in the literature confirmed via search [source: https://medium.com/suboptimally-speaking/school-timetabling-with-constraint-programming-495f1126c28d | School Timetabling with Constraint Programming | practitioner] (negative finding).
- Survey access friction: arXiv 2201.07525 v2 returns 404; the only retrievable abstract is via arXiv abs page. ScienceDirect, Springer, MDPI all return 403/303 to WebFetch. Detailed best-known-result tables from Ceschia/Di Gaspero/Schaerf 2023 not directly extractable in this round; their summaries are surfaced via secondary sources cited above [source: https://arxiv.org/abs/2201.07525 | Educational Timetabling: Problems, Benchmarks, and State-of-the-Art Results | independent]

---

## Saturation note

Search rounds completed: breadth (5 queries), depth-by-SQ (12 queries), adversarial / negative-evidence (4 queries), iterative deepening on Pumpkin / IHTC2024 / MiniZinc Challenge (6 queries). Repeat searches on "HiGHS school timetabling case study" and "SCIP school timetabling head-to-head" yielded the same null results across distinct query phrasings, indicating saturation of public head-to-head MIP-vs-CP-SAT evidence on school-timetabling-specific benchmarks at this date (2026-05-08).
