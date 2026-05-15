# 0041: Supervision rota for Hofpausen

- **Status:** Accepted
- **Date:** 2026-05-14

## Context

ADR 0040 added `TimeBlock.kind` and seeded `break` rows so the period grid is honest about Hofpausen. The remaining customer-facing mechanism in OPEN_THINGS item 3 is the Aufsichtspflichten rota: for each Hofpause, exactly one teacher supervises, the load is distributed fairly across the week, and a teacher only supervises when on-site (free at the slot, has a lesson on either side). The solver must produce the rota in the same artifact as the schedule because lesson placements determine who is on-site at any given break.

This ADR pins the load-bearing decisions across the solver, persistence, and reporting layers so a future change touching any one of them sees the original intent.

## Decision

**Algorithm.** Solver-core post-processes lesson placements with a deterministic min-load greedy in `solver/solver-core/src/supervision.rs`. For each `kind == Break` TimeBlock iterated in `(day, position)` order: eligibility is "free at the slot AND has a placement at `pos - 1` or `pos + 1`". Among eligible teachers, pick the one with the lowest running supervision count; ties broken by smallest `TeacherId`. No eligible teacher emits `ViolationKind::SupervisionGap` with `reason = "day=<d> position=<p> candidates=0"`.

**Fairness metric.** The soft cost is `weights.supervision_spread * (max_count - min_count)` over teachers with at least one supervision. The Production weight is 5. The metric is one-dimensional, cheap, and easy to test at the 10-Hofpause / week scale every Grundschule seed has today.

**LAHC integration.** Two entry points in `supervision.rs`: `compute_supervision_spread` (count-only, no allocation) called from `score_solution`; `compute_supervision_full` (returns assignments + violations + spread) called once at solution finalization. No per-move LAHC delta is added; instead, the Change-move accept site in `lahc.rs::try_change_move` recomputes `state.canonical_score = score_solution(problem, placements, weights)`, which already routes through `compute_supervision_spread`. R&R and Kempe accept paths already use `score_solution`. The per-iteration `debug_assert_eq!(state.canonical_score, score_solution(...))` invariant is preserved.

**Persistence shape.** Supervision assignments live in a new `supervision_assignments` table (`id` UUID PK, `time_block_id` UUID UNIQUE NOT NULL FK to `time_blocks(id) ON DELETE CASCADE`, `teacher_id` UUID NOT NULL FK to `teachers(id) ON DELETE CASCADE`, `created_at`). The POST /api/classes/{id}/schedule route deletes every row whose `time_block_id` belongs to the class's WeekScheme and inserts the fresh rota in a single transaction. Scoping is WeekScheme-wide, not class-wide, because Hofpause slots are shared across classes that share a WeekScheme; a per-class solve produces a school-wide rota. Reads filter the table by key (`read_supervision_assignments_for_teacher` selects `WHERE teacher_id = :teacher_id`) and return `[]` rather than 404 on empty.

**API surface.** `ScheduleResponse` (POST) gains a `supervision_assignments` field, populated via `filter_solution_for_class`'s passthrough. `ScheduleReadResponse` (GET) gains the same field, populated at the manual-construction site in `read_schedule_for_teacher_route`. `ViolationResponse.kind` widens to include `"supervision_gap"`. The teacher-week view renders an "Aufsicht" / "Supervision" badge on break cells where the current teacher is the assigned supervisor.

**Quality predicates.** `quality_checks.build_lesson_ordinal_map(time_blocks)` is the canonical helper for projecting raw `TimeBlock.position` onto lesson ordinals. The integration test is its first caller; any new admin-facing quality endpoint must fold the projection in or surface phantom interior gaps at every break slot.

## Alternatives considered

- **LAHC-driven supervisor assignment.** Add a new move kind that swaps supervisors and let LAHC optimize the spread directly. Rejected on RNG-budget grounds: per OPEN_THINGS item 9, every new LAHC move shifts the determinism RNG-draw invariant. The deterministic greedy gets the spread to zero or near-zero on every Grundschule seed without touching the search loop.
- **Backend round-robin.** Have the backend assign supervisors post-solve. Rejected because it requires re-implementing the eligibility logic in Python, which would drift from solver-core's. The solver already maintains the placement bookkeeping needed for eligibility.
- **Variance / sum-of-squared-deviations as the fairness metric.** Rejected on YAGNI: at the 10-Hofpause scale, max-minus-min produces the same ordering as variance for all but pathological inputs, with cheaper arithmetic.
- **Per-move LAHC delta for supervision.** Rejected on cost (every Change move would need an O(eligible) recompute on adjacency-affected break slots) and on RNG-budget grounds. The Change-accept-site full recompute pays the cost once per accepted move instead of every proposal.
- **Class-scoped `supervision_assignments`.** Rejected because Hofpause slots belong to the WeekScheme; one supervisor per slot is a school-wide property. Class-scoped rows would invite two classes solving the same WeekScheme to produce conflicting rotas.

## Consequences

Easier: admins see the rota inside the teacher-week view; the legal Aufsichtspflicht is auditable inside the same artifact as the schedule. Future supervision UX (e.g. a printable rota PDF, drag-and-drop swaps) hangs off a single canonical table. The `compute_supervision_*` split is a template for the next soft-cost axis that needs both a score-time scalar and a finalise-time artifact.

Harder: `try_change_move` recomputes the full `score_solution` at every accept, which is slightly more expensive than the previous delta-only path. The full crate stayed within the BASELINE.md budget on smoke; the 2026-05-15 production refresh ratified the canonical-score invariant under ADR 0037 (see ADR 0037's ratification section). New soft costs landing in `score_solution` must thread either a delta or a recompute through `try_change_move` to keep the canonical-score invariant green; the rule is captured in `solver/CLAUDE.md`.

Revisit if a customer school requires a different fairness metric (variance or Gini), if Teilzeit teachers need explicit working-days support (OPEN_THINGS item 4), or if a refresh shows LAHC chronically failing to converge on supervision-heavy schedules (promote to a per-move delta).
