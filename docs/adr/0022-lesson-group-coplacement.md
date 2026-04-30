# 0022: Lesson-group co-placement constraint

- **Status:** Accepted
- **Date:** 2026-04-30

## Context

Sprint item 6 (algorithm phase, P0) of the "Realer Schulalltag" sprint. PR
#156 shipped `Lesson.lesson_group_id` end-to-end without solver semantics; the
dreizuegige Grundschule seed has each Jahrgang's RK / RE / ETH trio sharing
one `lesson_group_id`. Until the solver respects the field, members of one
group block each other on the per-class hard constraint instead of co-placing
into one time-block. This ADR locks in the algorithm-phase decisions.

## Decision

1. **Atomic group placement in the greedy phase.** The FFD-ordered loop in
   `solve_with_config` tracks `placed_groups: HashSet<LessonGroupId>`. The
   first reachable member of an unplaced group triggers `try_place_group`,
   which co-places every member at one TB with pairwise-distinct rooms
   (greedy lowest-id-first per member) and pairwise-distinct teachers
   (validated up front). Subsequent members in the FFD order short-circuit.
2. **`ViolationKind::LessonGroupSplit`.** New variant. One entry per
   qualified group member per failed block (mirrors the per-block violation
   shape used elsewhere). Pre-solve `NoQualifiedTeacher` continues to cover
   unqualified members; the qualified members get `LessonGroupSplit` when
   the group cannot atomically place.
3. **Validation in `solver-core::validate_structural`.** Group members must
   share `hours_per_week` and `preferred_block_size`, and the teacher set is
   pairwise distinct. Pydantic mirroring is deferred until the field becomes
   user-editable.
4. **LAHC skips group placements** via a one-line guard in `try_change_move`,
   placed after the two `random_range` draws so the determinism RNG-budget
   invariant in `tests/lahc_property.rs` holds. Mirrors the existing
   Doppelstunden pattern.
5. **Class-delta dedup in atomic placement.** Group members typically share
   class sets. The atomic-placement scorer dedupes the class set via an
   insertion-ordered `Vec<SchoolClassId>` plus a `HashSet` over the union of
   member class sets so the class-gap delta is counted once per
   (class, day, position) tuple. Teacher and subject-preference deltas
   iterate members directly because teachers are pairwise-distinct
   (validation rule) and subjects are independent.

## Alternatives considered

- **Independent placement with a "is the booker in my group?" probe** in
  `state.used_class`. Rejected: complicates the hot probe and leaks
  group-awareness into single-class lessons.
- **Aggregate the group into one FFD entity.** Rejected: option (1) above
  handles ordering implicitly because the most-constrained member sorts to
  the front under FFD anyway.
- **Atomic group-swap LAHC move.** Filed as a follow-up under
  "Acknowledged deferrals". Today's skip preserves group co-placement at
  the cost of one neighbourhood; a richer move shape is justified once
  benches show a soft-score gap.
- **Pydantic mirror of group invariants.** Filed as a follow-up. The field
  is non-user-editable; the Rust gate is sufficient for the seed and the
  bench fixture.

## Consequences

- Religion trios in the dreizuegige seed collapse from six time-block uses
  per Jahrgang to two, removing four "extra positions" from each class's
  daily partition. Soft score on the dreizuegige bench fixture drops from
  8 to 0.
- Greedy work decreases on the dreizuegige fixture (one decision per group
  instead of one per member). p50 wall-clock holds within 1 percent of the
  prior committed value; `BASELINE.md` was refreshed in the same PR.
- The `ViolationKind` wire format gains one variant (additive). Frontend
  i18n adds the matching `schedule.violations.lessonGroupSplit` entry in
  en plus de.
- The "FFD eligibility weighting for cross-class lessons" deferral may
  graduate to "closed by side effect" once the dreizuegige solvability
  test passes without the eligibility tweak.
