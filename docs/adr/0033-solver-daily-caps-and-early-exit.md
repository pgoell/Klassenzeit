# 0033: Solver daily caps + optimum-aware deadline

- **Status:** Accepted
- **Date:** 2026-05-05

## Context

Production solves on the Grundschule class 1a schedule surfaced three quality issues at once: two unplaced lessons, three consecutive German lessons on Thursday, and a long Monday gap. Investigation found that the bake-off bench did not exercise the first two: the solver had no `max-hours-per-subject-per-day` constraint at all (`ViolationKind` listed six variants, none of them subject-run-related), and the production `solve_deadline_ms` defaulted to 200 ms while bench cells ran for 5 s. The bench tied `lahc_rr_kempe` to soft-score 0 on every fixture, yet production frequently exited the LAHC loop before escaping the FFD-induced local minimum. ADR 0031 + ADR 0032 picked `lahc_rr_kempe` as production default on a bench that did not stress these axes.

## Decision

Two new hard constraints, enforced via legality pruning in `try_place_block` (and inherited by every LAHC / R&R / Kempe move that reuses its candidate gate):

- `Subject.max_hours_per_day`: non-null, default 2. Per-class-per-day cap on hours of one subject. Counts period span (a 2-hour Doppelstunde contributes 2).
- `SchoolClass.max_lessons_per_day`: nullable, default null (no cap). Per-class-per-day cap on total lessons. Counts placements (a 2-hour Doppelstunde contributes 1).

Two new diagnostic-only `ViolationKind` variants (`SubjectDailyHourCapExceeded`, `ClassDailyLessonCapExceeded`) widen the closed enum exposed by `ViolationResponse.kind`. The runtime never constructs them; they exist for the closed-enum surface and for future telemetry.

The LAHC outer loop terminates as soon as `placements.len() == placements_expected && state.soft_score == 0`. `solve_deadline_ms` production default raised from 200 ms to 5000 ms. `.env.test` keeps `KZ_SOLVE_DEADLINE_MS=0` (greedy-only test-mode path).

## Alternatives considered

- **Soft caps with a tunable weight.** Rejected because the user's framing ("I can't think of a longer subject than 2 hours") is structural, not preferential. A soft cap requires weight tuning to balance against gap and home-room penalties, with no obvious correct value.
- **Per-class-per-day caps only (skip the per-subject one).** Rejected because the reported "3 German on Thursday" symptom needs a per-subject limit; a class-total cap of 6 would still allow 3 of one subject if other subjects fill the remaining 3 slots.
- **Two ADRs (caps + deadline split).** Rejected: both decisions land in the same PR, the deadline change is small, and the ADR rule of thumb ("one decision per ADR") is satisfied by viewing this as one decision about "how do production solves stop emitting low-quality schedules." Splitting forced synthetic narrative separation when the rationale is shared.
- **CP-SAT + Kempe parallel ensemble; SSE / polling streaming of incumbents.** Rejected as premature. Step-1 evidence (raised deadline + early exit) likely closes both production gaps; the parallel-solver architecture is real engineering with no production data demanding it.

## Consequences

Production wall-clock per solve grows from ≤200 ms to ≤5000 ms in the worst case. Easy problems still complete in <100 ms thanks to the early exit. Schools with a subject they want to schedule above 2 hours/day must set `Subject.max_hours_per_day` explicitly via the existing edit dialog; classes that need a daily lesson cap below their time-block count likewise. Existing persisted schedules are not retroactively validated; they stay valid until regenerated.

Bake-off `BENCH_RESULTS.md` does not get refreshed in this PR. Spot-check at `--budget 5s --seeds 4 --fixtures grundschule` confirmed `lahc_rr_kempe` retains soft-score 0 with the new defaults; the canonical bench numbers stay valid. ADRs 0030, 0031, 0032 stay in force; this ADR layers structural constraints on top.

Future revisits land naturally if (a) a school subject genuinely needs > 2 hours/day enough that overriding per-Subject becomes friction worth removing the default for; (b) production solves consistently hit the 5000 ms ceiling without finding optimum, which would justify item 30's observability work and possibly the deferred CP-SAT ensemble.
