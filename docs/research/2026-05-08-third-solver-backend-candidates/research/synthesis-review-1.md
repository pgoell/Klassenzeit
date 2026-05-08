# Synthesis Review (Iteration 1)

VERDICT: ISSUES

ISSUES:
  - id: minizinc-challenge-2024-categories-misstated
    severity: minor
    category: evidence-gap
    description: Synthesis SQ2 claims CP-SAT swept "Fixed, Free, Parallel, and Local Search" categories in 2023/2024/2025, but `comparative-evidence-school-timetabling.md` shows 2024 categories were Fixed/Free/Open/Local Search (no Parallel) and 2025 has Fixed/Free/Parallel/Local Search (no Open). Cross-year category list is not uniform.
    location: synthesis.md SQ2 third claim ("CP-SAT (the current Klassenzeit production candidate) sweeps gold at the MiniZinc Challenge in 2023, 2024, and 2025 across Fixed, Free, Parallel, and Local Search categories")
  - id: pumpkin-demirovic-coauthorship-source-not-in-inventory
    severity: minor
    category: evidence-gap
    description: A load-bearing thesis claim is that Pumpkin's lead author Demirović co-authored the MaxSAT-LNS-on-XHSTT papers, supplying "direct school-timetabling pedigree." The cp-sat-smt-backends cluster cites the dbai.tuwien.ac.at PhD thesis as the source for this connection, but that URL is absent from the synthesis Source Inventory; the connection rests on the reader inferring authorship overlap between two unlinked entries (Pumpkin README contributor list and the Demirović-Musliu paper).
    location: synthesis.md Thesis paragraph and SQ1 first claim Notes ("Lead author (Demirović) co-authored the foundational MaxSAT-LNS-on-XHSTT papers")
  - id: mit-vs-apache-permissiveness-claim-overstated
    severity: minor
    category: logic
    description: Reconciled Contradiction #6 asserts "MIT license is strictly more permissive than SCIP's Apache-2.0," then immediately concedes "within Klassenzeit's policy band, neither is a problem." The strictly-more-permissive framing is not standard (Apache-2.0 adds patent grants but is widely treated as equivalently permissive) and the conclusion does not depend on it; the reasoning would be cleaner without the assertion.
    location: synthesis.md Reconciled Contradictions #6 (HiGHS vs SCIP)
  - id: minizinc-version-date-drift-not-adjudicated
    severity: minor
    category: evidence-gap
    description: Cluster files conflict on MiniZinc 2.9.7's release date — `mip-lp-backends.md` and synthesis state April 2026, while `cp-sat-smt-backends.md` says "2.9.7 on April 30, 2024" (alongside 2.8.7 as current in a 2024 OR-Tools issue, which contradicts 2.9.7 existing in 2024). Synthesis adopts the 2026 date without flagging the cluster-internal disagreement.
    location: synthesis.md SQ1 MiniZinc claim ("MiniZinc 2.9.7 (April 2026)")
  - id: alns-main-branch-commit-recency-unverified
    severity: minor
    category: evidence-gap
    description: ALNS gate-failure decision is acknowledged as conservative ("if Pascal wants ALNS specifically, a fresh git log check is the cheapest way to flip the verdict") but the synthesis still excludes ALNS based on release-date alone without performing the cheap check. This affects the completeness of the excluded-candidates list rather than the recommendation, but leaves a known gap in the gate evaluation methodology.
    location: synthesis.md Reconciled Contradictions #5 and SQ1 excluded candidates list
  - id: cluster-sq-coverage-not-explicit
    severity: minor
    category: coverage
    description: The synthesis reorganizes around 5 synthesis-level SQs that do not map 1:1 onto the plan.md cluster sub-questions. Specific plan SQs (HiGHS roadmap details, MiniZinc as portability layer in depth, FET architecture-and-license interaction, ITC2007 results, Rust-crate-by-crate breakdown) are addressed implicitly via the gate exclusions and pick rationale, but no explicit coverage map ties synthesis claims back to plan SQs. Reader must trust that nothing was dropped.
    location: synthesis.md "Claims by Sub-Question" section vs plan.md cluster SQ structure

SUMMARY:
The synthesis is logically coherent, evidence-dense, and arrives at a defensible 1-2-3 ranking with explicit gates, distinct pillars, and a thorough excluded-candidates list. All issues identified are minor: factual drift in the MiniZinc Challenge category list, an unsourced co-authorship claim load-bearing for the Pumpkin pillar, an overstated permissiveness comparison, an un-adjudicated date conflict, an unverified ALNS edge case, and a missing plan-SQ coverage map. None invalidate the thesis or the recommended bake-off ordering.
