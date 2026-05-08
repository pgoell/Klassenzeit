# 0035: Reject Timefold backend

- **Status:** Accepted
- **Date:** 2026-05-08

## Context

OPEN_THINGS item 55 (`[P0]`) called for a bounded spike comparing Rust LAHC variants, CP-SAT, and Timefold on the four canonical fixtures (`grundschule`, `zweizuegig`, `dreizuegig`, `ffd_lock_in_grundschule`), all evaluated against the canonical `QualityReport` (`solver-core/src/quality.rs`, ADR 0030 uniform-scorer contract). Item 55 listed three valid acceptance outcomes: keep Rust as default, make Timefold the preferred backend, or keep Timefold as a rejected experiment with concrete reasons. Pre-implementation research showed that two of the three outcomes are foreclosed by upstream state, and the third (Java integration) violates the item's "do not let this become a product rewrite" framing.

## Decision

Reject Timefold. Do not invest in either the archived Python binding or a from-scratch Java integration. Defer the "third-backend validator" question to a future spike that selects from non-archived candidates (Choco Solver, HiGHS, GLPK, or another maintained alternative) when Rust LAHC and CP-SAT both plateau on a measurable quality axis. See the new P2 follow-up under `## Open solver follow-ups` in `docs/OPEN_THINGS.md` for the trigger condition.

The four concrete reasons:

1. **Python binding archived.** `TimefoldAI/timefold-solver-python` was archived on GitHub on 2025-10-06. The latest PyPI release `timefold==1.24.0b0` (2025-07) is still beta and now unmaintained. Upstream maintainers no longer recommend the Python implementation for new projects.
2. **Python version conflict.** `timefold==1.24.0b0` supports Python 3.10-3.12. Klassenzeit pins `python = "3.14.2"` in `mise.toml`. Adopting the binding would require either downgrading the runtime (regression on every other module) or maintaining a parallel 3.12 venv solely for the Timefold subprocess.
3. **Java path violates the bounded-spike framing.** A from-scratch Java integration adds a JDK 17+ toolchain pin, a Maven (or Gradle) module under `solver/solver-timefold/`, ~1k-2k LOC of Java domain classes plus a `ConstraintProvider` covering all hard constraints (`NoSuitableRoom`, `RoomDoubleBooked`, `TeacherDoubleBooked`, `ClassDoubleBooked`, `LessonGroupSplit`, `validate_no_room_hopping`, `validate_no_double_booking`, `validate_daily_caps`) and all soft axes (`class_gap`, `teacher_gap`, `subject_preference`, `home_room`, `class_day_balance`), a fat-JAR build step in CI, and a new `BenchBackend::Timefold` dispatch path. Realistic effort is three or more PRs; item 55 explicitly says "Do not let this become a product rewrite."
4. **Supporting performance caveat.** PyPI states that the Python binding is "significantly slower than using Timefold Solver for Java or Kotlin." Even when usable, the Python path was never going to demonstrate Timefold's actual capability, so a Python-binding result would have undersold the alternative we were trying to evaluate.

The user gate condition (received during brainstorming): "if it's not possible to integrate using python or rust i don't want it." Both available paths fail this gate.

## Alternatives considered

- **From-scratch Java Timefold integration.** Rejected. Cost (3+ PRs, JDK toolchain pin, Maven module, ~1k-2k LOC of Java) violates item 55's bounded-spike framing. Opportunity cost: items 34, 47, 44, 14, 11, 4 in the active sprint plus the entire Beyond-Grundschule Sprint 1 queue behind a multi-week investment whose upside is "maybe Timefold beats LAHC on some quality axis we then have to retain a Java toolchain to keep using."
- **Archived Python binding (`timefold==1.24.0b0`).** Rejected. Upstream archived on 2025-10-06, Python version conflicts with `mise.toml`'s 3.14.2 pin, and building a production solver path on archived upstream is a long-term liability.
- **Pivot immediately to Choco Solver / HiGHS / GLPK.** Deferred, not rejected. Picking the right alternative needs its own brainstorm. Captured as a P2 follow-up in `## Open solver follow-ups`, with the trigger condition that Rust LAHC and CP-SAT both plateau on a quality axis post-tuning.

## Consequences

The repo retains zero Java tooling. CI cost stays at the current baseline. `Settings.solver_backend` keeps its existing `Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"]` shape; no new variant is added. The existing CP-SAT comparison axis (ADR 0030) remains the sole external-solver point of reference. The "validate against another solver before deeper LAHC tuning" intent of item 55 is partially served by CP-SAT today; the production-default revisit (item 47, planned ADR 0036) reasons from per-component vectors against Rust LAHC variants and CP-SAT, per `solver/CLAUDE.md`. We would revisit this decision and re-open the third-backend question if Rust LAHC tuning (post item 47) and CP-SAT both plateau on the same quality axis on a future BENCH_RESULTS refresh, in which case the new P2 OPEN_THINGS follow-up captures the trigger and a fresh brainstorm picks the alternative.
