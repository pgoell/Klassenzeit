# Lesson-group co-placement constraint

**Sprint:** Realer Schulalltag + better scheduler (algorithm phase, P0).

**Closes (in `docs/superpowers/OPEN_THINGS.md`):** sprint item 6. Likely closes the "FFD eligibility weighting for cross-class lessons" deferral by side effect (verify post-impl); the existing "Block-aware LAHC for cross-class lessons" deferral and the new atomic group-swap LAHC follow-up stay open.

**ADR:** [0022: lesson-group co-placement constraint](../../adr/0022-lesson-group-coplacement.md), added in this PR.

## Goal

Make `solve_with_config` honour `Lesson.lesson_group_id` (already round-tripped end-to-end since PR #156). Lessons sharing a non-null `lesson_group_id` co-place into one `time_block` with pairwise-distinct rooms and pairwise-distinct teachers; failure to co-place emits a new `ViolationKind::LessonGroupSplit` per member per failed block. Refresh `solver/solver-core/benches/BASELINE.md` with the new dreizuegige numbers in the same PR.

## Non-goals

- Atomic group-swap LAHC move. LAHC skips group placements via a one-line guard, mirroring the existing Doppelstunden pattern; the richer move shape is filed under "Acknowledged deferrals".
- Pydantic mirror of the new group invariants (`hours_per_week`, `preferred_block_size`, pairwise-distinct teacher). The field is non-user-editable today (existing OPEN_THINGS deferral about surfacing it in the lesson edit dialog), so the Rust gate suffices.
- Frontend group editor. Out of scope this PR.
- Cross-Jahrgang Religion groups (1+2 share one Religion group). Per-Jahrgang groups only, matching the dreizuegige seed.
- Generalising LAHC's Change move over multi-class lessons that are *not* in a group. Already handled by the existing per-class probe; no change needed here.

## Architecture changes

### `solver-core::types`

Add a fifth variant to `ViolationKind`:

```rust
pub enum ViolationKind {
    NoQualifiedTeacher,
    TeacherOverCapacity,
    NoFreeTimeBlock,
    NoSuitableRoom,
    LessonGroupSplit,
}
```

Snake_case JSON serialisation gives `"lesson_group_split"`. The wire format addition is non-breaking (a new variant). Frontend i18n adds the matching entry.

`Lesson` already carries `lesson_group_id: Option<LessonGroupId>` from PR #156; no schema change.

### `solver-core::validate`

`validate_structural` gains group-invariant checks. After per-lesson validation, walk `problem.lessons` once to build `groups: HashMap<LessonGroupId, Vec<&Lesson>>`. For each group with two or more members:

1. All members share the same `hours_per_week`. Otherwise `Err(Error::Input("lesson group {} members disagree on hours_per_week ..."))`.
2. All members share the same `preferred_block_size`. Otherwise `Err(Error::Input("lesson group {} members disagree on preferred_block_size ..."))`.
3. Pairwise-distinct `teacher_id` across members. Otherwise `Err(Error::Input("lesson group {} has duplicate teacher ..."))`.

Single-member groups are silently allowed (degenerate case; no co-placement constraint to violate). The validator does not enforce identical `school_class_ids` (modelling choice; future seeds may have asymmetric class membership).

### `solver-core::solve`

The greedy loop in `solve_with_config` gains a `placed_groups: HashSet<LessonGroupId>` and a new helper `try_place_group`. The FFD-ordered traversal becomes:

```rust
for &lesson_idx in &order {
    let lesson = &problem.lessons[lesson_idx];
    if !idx.teacher_qualified(lesson.teacher_id, lesson.subject_id) {
        continue;
    }
    if let Some(group_id) = lesson.lesson_group_id {
        if !placed_groups.insert(group_id) {
            continue; // sibling already triggered atomic placement
        }
        let members: Vec<&Lesson> = group_members(problem, group_id);
        let n = lesson.preferred_block_size;
        let block_count = lesson.hours_per_week / n;
        for block_index in 0..block_count {
            let placed = try_place_group(
                problem,
                &members,
                n,
                /* idx, teacher_max, weights, state, placements, tb_order, room_order */
            );
            if !placed {
                for member in &members {
                    solution.violations.push(Violation {
                        kind: ViolationKind::LessonGroupSplit,
                        lesson_id: member.id,
                        hour_index: block_index * n,
                    });
                }
            }
        }
        continue;
    }
    // single-lesson path unchanged
}
```

`group_members` is a `O(lessons)` linear scan per first-reached group; cheap relative to the placement work it gates. Members are returned in input order (insertion order of the seed) so room assignment is deterministic.

`try_place_group` mirrors `try_place_block` but works across the whole group:

1. **Iterate `outer_pos in tb_order`** with the same window-contiguity guard for `n > 1`.
2. **Hard-feasibility per window position, per member**: every `(member.teacher_id, tb.id)` must be free; every `(class, tb.id)` for `class in union(members.school_class_ids)` must be free in `state.used_class`; every `member.teacher_id` must have remaining capacity for `n` more hours.
3. **Greedy room assignment per member**: walk `room_order`. For each member in input order, pick the lowest-id room that suits the member's subject, isn't blocked, isn't already in `used_room` for the window, and isn't already chosen by an earlier member of this group. If any member fails to find a room, the slot is infeasible.
4. **Score**: dedupe class deltas across the group via `BTreeSet<SchoolClassId>` over the union of member class sets; teacher deltas iterate members directly (teachers are pairwise-distinct by validation); subject preference iterates members directly.
5. **Pick the lowest-score window**; tiebreak `(day, start_pos, lowest member's room.id)`. Early-exit at `score == state.soft_score`.
6. **Commit**: write all member placement rows; insert teacher / room / class bookings (idempotent insert for shared classes); update partition maps once per (deduped class, day) tuple and once per (teacher, day) tuple; bump `state.soft_score`.

Returns `bool`: success places the entire group atomically; failure leaves state untouched.

The existing `unplaced_kind` helper is unchanged (it reports per-lesson violation kinds for non-group lessons). Group failure goes through the new `ViolationKind::LessonGroupSplit` path directly.

### `solver-core::lahc`

`try_change_move` gains one early-return guard immediately after the existing Doppelstunden guard:

```rust
if lesson.preferred_block_size > 1 {
    return false;
}
if lesson.lesson_group_id.is_some() {
    return false;
}
```

The two `random_range` draws in `run` happen before `try_change_move`, preserving the LAHC determinism RNG-budget invariant (`tests/lahc_property.rs`). The existing block-skip integration test gets a sibling `lahc_does_not_move_grouped_placements` test in `lahc.rs`.

### `solver-core::ordering`

No structural change. The eligibility metric stays per-lesson; the FFD loop's "first-reached group member triggers atomic placement" pattern in `solve.rs` makes ordering changes unnecessary. Module-level docstring gains a one-paragraph note describing the interaction.

### `solver-core::index`

No change. Group state is tracked locally in `solve.rs`'s greedy loop, not in `Indexed` (which is read-only for the duration of a solve).

### Frontend i18n

`frontend/src/i18n/violation-keys.ts`:

```ts
export const VIOLATION_KIND_KEYS = {
  no_qualified_teacher: "violations.kind.no_qualified_teacher",
  teacher_over_capacity: "violations.kind.teacher_over_capacity",
  no_free_time_block: "violations.kind.no_free_time_block",
  no_suitable_room: "violations.kind.no_suitable_room",
  lesson_group_split: "violations.kind.lesson_group_split",
} as const;
```

en: `"Lesson group could not be placed in a single time block."`
de: `"Gruppenstunde konnte nicht in einem gemeinsamen Zeitblock platziert werden."`

`tsconfig.json` and the OpenAPI types regenerator catch the new variant; `mise run fe:types` regenerates after the backend wire format updates.

### Backend

`backend/src/klassenzeit_backend/scheduling/build_problem_json.py` already passes `lesson_group_id` through (PR #156). No behaviour change.

`backend/src/klassenzeit_backend/api/schemas.py` `ViolationResponse.kind: Literal[...]` adds `"lesson_group_split"`.

The dreizuegige solvability test at `backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py` expects the schedule to be solvable; with co-placement the soft score should drop and the schedule should remain feasible. Audit and tighten if needed.

### ADR 0022

Records the load-bearing decisions:

1. Atomic placement model (option (a) of the brainstorm Q1, Q2).
2. Validation enforced in `solver-core` only this PR; Pydantic mirror filed as follow-up.
3. `LessonGroupSplit` per member per failed block (Q5).
4. LAHC skip mirroring Doppelstunden (Q6).
5. Class-delta dedup across group members for soft-score correctness (Q7).
6. Greedy lowest-id-first room assignment per member (Q8).

## Tests

### `solver-core/src/solve.rs`

Unit tests added inline. All use `greedy_solve` (greedy-only, no LAHC), so they stay fast.

- `lesson_group_atomic_places_two_members_at_one_tb_with_distinct_rooms`. 2 members, 2 classes, 1 TB, 2 rooms. Expects 2 placements at the same TB with different rooms.
- `lesson_group_emits_violation_per_member_per_block_when_no_slot_fits`. 3 members but only 2 free rooms at the only feasible TB. Expects 0 group placements and 3 `LessonGroupSplit` violations (1 per member, hour_index 0).
- `lesson_group_with_two_hours_places_into_two_distinct_tbs`. Validates the loop over `block_count`.
- `lesson_group_blocked_by_non_group_class_use`. Non-group lesson at TB1 booking class A; group must avoid TB1 because TB1 is busy for class A in non-group state.
- `lesson_group_room_assignment_picks_lowest_id_per_member`. 3 members, 5 rooms; expects rooms 30, 31, 32 in input order.
- `lesson_group_with_unqualified_member_does_not_place`. Group where one member's `(teacher_id, subject_id)` is missing from `teacher_qualifications`. The atomic placement helper checks every member's qualification before attempting any slot; if any member is unqualified, the group is treated as infeasible. `pre_solve_violations` still records `NoQualifiedTeacher` for the unqualified member's hours (existing path); the qualified members get `LessonGroupSplit` per block, no `Placement` rows.

### `solver-core/src/validate.rs`

- `validate_structural_rejects_group_members_with_different_hours_per_week`.
- `validate_structural_rejects_group_members_with_different_block_size`.
- `validate_structural_rejects_group_with_duplicate_teacher`.
- `validate_structural_accepts_single_member_group`.
- `validate_structural_accepts_group_with_consistent_invariants`.

### `solver-core/src/lahc.rs`

- `lahc_does_not_move_grouped_placements`. Mirrors the existing `lahc_does_not_move_block_placements`. Seeds a group placement at position 0 with `avoid_first_period=true` so the LAHC search would *want* to move it; asserts the placement stays put.

### `solver-core/tests/lahc_property.rs`

The existing determinism property test stays green: the new skip path adds no `random_range` calls.

### Backend

`backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py` continues to pass; the assertion for full feasibility tightens if soft_score now drops to 0 (verify after implementation).

### Bench

`solver/solver-core/benches/BASELINE.md` is regenerated via `mise run bench:record` and committed in the same PR. Expectation: dreizuegige greedy and LAHC rows show lower soft_score (currently 8 → likely under 4) and similar or improved p50 wall-clock. The 20% budget applies; if soft_score swings in the wrong direction, investigate before pinning a worse number.

## Risks and rollback

- **Soft-score regression on dreizuegige.** Atomic placement is constrained more tightly than independent placement; theoretically a group might fail to atomically place where independent placements would succeed. Mitigation: the existing dreizuegige seed has 12 Klassenräume × 5 days × 8 positions, ample slack for 4 Jahrgänge × 2 hours = 8 atomic group placements with 3 distinct rooms each. The bench refresh confirms.
- **Determinism RNG-budget breakage.** Mitigation: the LAHC skip fires after the two `random_range` draws; the existing property test in `tests/lahc_property.rs` catches regressions.
- **Score double-counting on shared class sets.** Mitigation: Q7's dedup; new unit test covers the 3-member identical-class case.
- **Backwards compatibility.** `lesson_group_id` is `Option<...>`; null defaults preserve all existing single-lesson behaviour. The new `ViolationKind` variant is additive; the frontend renders unknown kinds as a generic fallback today (existing pattern).

Rollback path: revert the feature commits. The `lesson_group_id` field stays in the schema (it landed in PR #156 and is independent of this PR's algorithmic logic).

## Out of scope follow-ups (filed in OPEN_THINGS)

- Atomic group-swap LAHC move.
- Pydantic mirror of the group invariants once `lesson_group_id` becomes user-editable.
- Frontend group editor (depends on the lesson edit dialog deferral that already exists).
- Cross-Jahrgang Religion groups (real-school deferral; today's per-Jahrgang scope covers the dreizuegige seed).
