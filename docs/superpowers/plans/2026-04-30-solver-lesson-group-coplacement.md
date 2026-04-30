# Lesson-group co-placement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `solve_with_config` honour `Lesson.lesson_group_id` by co-placing all members of a non-null group at one `time_block` with pairwise-distinct rooms, emitting `ViolationKind::LessonGroupSplit` per member per failed block when atomic placement fails.

**Architecture:** Atomic group placement in the greedy phase (a `placed_groups: HashSet<LessonGroupId>` gates a per-group `try_place_group` helper that mirrors `try_place_block`'s shape but iterates members for hard-feasibility, dedupes class deltas, and assigns rooms greedy lowest-id-first). LAHC's Change move skips group placements via a one-line guard mirroring the existing Doppelstunden pattern. Validation lives in `validate_structural` in solver-core; the field stays non-user-editable in this PR so Pydantic mirroring is deferred.

**Tech Stack:** Rust 2021 (`solver-core`, `cargo nextest`), TypeScript (frontend i18n), criterion bench refresh, ADR 0022.

---

## File Structure

**Create:**
- `docs/adr/0022-lesson-group-coplacement.md`: load-bearing decisions for atomic placement, violation kind, LAHC skip, score dedup, room assignment.

**Modify:**
- `solver/solver-core/src/types.rs`: add `ViolationKind::LessonGroupSplit`; doc-comment update on the variant.
- `solver/solver-core/src/validate.rs`: add group-invariant checks (`hours_per_week` agreement, `preferred_block_size` agreement, pairwise-distinct teacher per group); validation tests.
- `solver/solver-core/src/solve.rs`: `placed_groups` set, `try_place_group` helper (hard-feasibility plus greedy lowest-id-first room assignment plus deduped class score deltas), atomic placement plus per-member `LessonGroupSplit` emission; unit tests.
- `solver/solver-core/src/lahc.rs`: one-line guard `if lesson.lesson_group_id.is_some() { return false; }` after the existing Doppel guard; mirror unit test.
- `solver/solver-core/src/ordering.rs`: module docstring note about the FFD interaction (no code change).
- `solver/solver-core/benches/BASELINE.md`: regenerate via `mise run bench:record` after impl lands.
- `frontend/src/i18n/violation-keys.ts`: new `lesson_group_split` arm in the typed switch.
- `frontend/src/i18n/locales/en/translation.json` and `frontend/src/i18n/locales/de/translation.json`: add `schedule.violations.lessonGroupSplit` keys.
- `backend/src/klassenzeit_backend/api/schemas.py`: add `"lesson_group_split"` to `ViolationResponse.kind` `Literal[...]` union.
- `docs/superpowers/OPEN_THINGS.md`: close sprint item 6, mark FFD-eligibility-for-cross-class deferral as "closed by side effect" if dreizuegige stays solvable, file the LAHC group-swap follow-up.

**Files to verify (no expected change):**
- `backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py`: expected to still pass; soft score should improve.
- `solver/solver-core/tests/lahc_property.rs`: RNG-budget invariant; expected to still pass.

---

## Task 1: Add `ViolationKind::LessonGroupSplit` (failing test, then variant)

**Files:**
- Modify: `solver/solver-core/src/types.rs`
- Test: inline `#[cfg(test)] mod tests` in the same file.

- [ ] **Step 1: Write the failing test**

Add inside `mod tests` in `solver/solver-core/src/types.rs`, next to the existing `violation_kind_serialises_in_snake_case` test:

```rust
#[test]
fn violation_kind_serialises_lesson_group_split() {
    assert_eq!(
        serde_json::to_string(&ViolationKind::LessonGroupSplit).unwrap(),
        "\"lesson_group_split\""
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p solver-core types::tests::violation_kind_serialises_lesson_group_split`
Expected: FAIL with "no variant or associated item named `LessonGroupSplit` found for enum `ViolationKind`".

- [ ] **Step 3: Add the variant**

Update `pub enum ViolationKind` in `solver/solver-core/src/types.rs` to:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// The lesson's assigned teacher lacks the subject qualification.
    NoQualifiedTeacher,
    /// Placing this hour would push the teacher past `max_hours_per_week`.
    TeacherOverCapacity,
    /// No time block has both the (teacher, class) pair free.
    NoFreeTimeBlock,
    /// No room is suitable for the subject and free in any free time block.
    NoSuitableRoom,
    /// Atomic lesson-group co-placement failed for this block: no `time_block`
    /// admits all group members with pairwise-distinct rooms and free
    /// teachers / classes. One entry per member per failed block.
    LessonGroupSplit,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -p solver-core types::tests::violation_kind_serialises_lesson_group_split`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add solver/solver-core/src/types.rs
git commit -m "feat(solver-core): add LessonGroupSplit violation kind"
```

---

## Task 2: Validate group invariants in `validate_structural`

**Files:**
- Modify: `solver/solver-core/src/validate.rs`
- Test: same file's `#[cfg(test)] mod tests`.

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `solver/solver-core/src/validate.rs` (these helpers; copy the pattern of the existing `minimal_problem` builder):

```rust
fn two_member_group_problem() -> Problem {
    use crate::ids::LessonGroupId;
    let group_id = LessonGroupId(uuid(99));
    let mut p = minimal_problem();
    p.school_classes.push(SchoolClass {
        id: SchoolClassId(uuid(7)),
    });
    p.subjects.push(Subject {
        id: SubjectId(uuid(8)),
        prefer_early_periods: false,
        avoid_first_period: false,
    });
    p.teachers.push(Teacher {
        id: TeacherId(uuid(9)),
        max_hours_per_week: 10,
    });
    p.teacher_qualifications.push(TeacherQualification {
        teacher_id: TeacherId(uuid(9)),
        subject_id: SubjectId(uuid(8)),
    });
    p.lessons[0].lesson_group_id = Some(group_id);
    p.lessons.push(Lesson {
        id: LessonId(uuid(10)),
        school_class_ids: vec![SchoolClassId(uuid(7))],
        subject_id: SubjectId(uuid(8)),
        teacher_id: TeacherId(uuid(9)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: Some(group_id),
    });
    p
}

#[test]
fn validate_structural_accepts_group_with_consistent_invariants() {
    validate_structural(&two_member_group_problem()).unwrap();
}

#[test]
fn validate_structural_accepts_single_member_group() {
    use crate::ids::LessonGroupId;
    let mut p = minimal_problem();
    p.lessons[0].lesson_group_id = Some(LessonGroupId(uuid(99)));
    validate_structural(&p).unwrap();
}

#[test]
fn validate_structural_rejects_group_members_with_different_hours_per_week() {
    let mut p = two_member_group_problem();
    p.lessons[1].hours_per_week = 2;
    let err = validate_structural(&p).unwrap_err();
    assert!(matches!(err, Error::Input(msg) if msg.contains("hours_per_week")));
}

#[test]
fn validate_structural_rejects_group_members_with_different_block_size() {
    let mut p = two_member_group_problem();
    p.lessons[0].hours_per_week = 2;
    p.lessons[0].preferred_block_size = 2;
    p.lessons[1].hours_per_week = 2;
    let err = validate_structural(&p).unwrap_err();
    assert!(matches!(err, Error::Input(msg) if msg.contains("preferred_block_size")));
}

#[test]
fn validate_structural_rejects_group_with_duplicate_teacher() {
    let mut p = two_member_group_problem();
    p.lessons[1].teacher_id = p.lessons[0].teacher_id;
    let err = validate_structural(&p).unwrap_err();
    assert!(matches!(err, Error::Input(msg) if msg.contains("duplicate teacher")));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p solver-core validate::tests --filter-expr 'test(group)'`
Expected: the three "rejects" tests FAIL with "expected `Err`, got `Ok(())`"; the two "accepts" tests PASS (no group enforcement yet).

- [ ] **Step 3: Add the group-invariant check after the per-lesson loop**

Insert before `Ok(())` at the end of `validate_structural` in `solver/solver-core/src/validate.rs`:

```rust
    use crate::ids::LessonGroupId;
    let mut groups: std::collections::HashMap<LessonGroupId, Vec<&crate::types::Lesson>> =
        std::collections::HashMap::new();
    for lesson in &problem.lessons {
        if let Some(group_id) = lesson.lesson_group_id {
            groups.entry(group_id).or_default().push(lesson);
        }
    }
    for (group_id, members) in &groups {
        if members.len() < 2 {
            continue;
        }
        let first = &members[0];
        for member in &members[1..] {
            if member.hours_per_week != first.hours_per_week {
                return Err(Error::Input(format!(
                    "lesson group {} members disagree on hours_per_week: {} vs {}",
                    group_id.0, first.hours_per_week, member.hours_per_week
                )));
            }
            if member.preferred_block_size != first.preferred_block_size {
                return Err(Error::Input(format!(
                    "lesson group {} members disagree on preferred_block_size: {} vs {}",
                    group_id.0, first.preferred_block_size, member.preferred_block_size
                )));
            }
        }
        let mut seen_teachers: HashSet<TeacherId> = HashSet::new();
        for member in members {
            if !seen_teachers.insert(member.teacher_id) {
                return Err(Error::Input(format!(
                    "lesson group {} has duplicate teacher {}",
                    group_id.0, member.teacher_id.0
                )));
            }
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p solver-core validate::tests`
Expected: all `validate::tests` pass.

- [ ] **Step 5: Run the wider workspace tests to confirm no regression**

Run: `mise run test:rust`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add solver/solver-core/src/validate.rs
git commit -m "feat(solver-core): validate lesson-group invariants in validate_structural"
```

---

## Task 3: Atomic group placement helper and FFD-loop integration

**Files:**
- Modify: `solver/solver-core/src/solve.rs`
- Test: inline `#[cfg(test)] mod tests` in the same file.

This is the central task. Tests come first; we red-green each behaviour.

- [ ] **Step 1: Write the failing happy-path test**

Append to `mod tests` in `solver/solver-core/src/solve.rs`:

```rust
fn two_member_group_base_problem() -> Problem {
    use crate::ids::LessonGroupId;
    let mut p = base_problem();
    p.time_blocks = vec![TimeBlock {
        id: TimeBlockId(solve_uuid(10)),
        day_of_week: 0,
        position: 0,
    }];
    p.school_classes.push(SchoolClass {
        id: SchoolClassId(solve_uuid(51)),
    });
    p.teachers.push(Teacher {
        id: TeacherId(solve_uuid(21)),
        max_hours_per_week: 10,
    });
    p.teacher_qualifications.push(TeacherQualification {
        teacher_id: TeacherId(solve_uuid(21)),
        subject_id: SubjectId(solve_uuid(40)),
    });
    p.rooms.push(Room {
        id: RoomId(solve_uuid(31)),
    });
    let group_id = LessonGroupId(solve_uuid(70));
    p.lessons[0].lesson_group_id = Some(group_id);
    p.lessons[0].school_class_ids = vec![
        SchoolClassId(solve_uuid(50)),
        SchoolClassId(solve_uuid(51)),
    ];
    p.lessons.push(Lesson {
        id: LessonId(solve_uuid(61)),
        school_class_ids: vec![
            SchoolClassId(solve_uuid(50)),
            SchoolClassId(solve_uuid(51)),
        ],
        subject_id: SubjectId(solve_uuid(40)),
        teacher_id: TeacherId(solve_uuid(21)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: Some(group_id),
    });
    p
}

#[test]
fn lesson_group_atomic_places_two_members_at_one_tb_with_distinct_rooms() {
    let p = two_member_group_base_problem();
    let s = greedy_solve(&p).unwrap();
    assert_eq!(s.placements.len(), 2, "both members place");
    assert_eq!(
        s.placements[0].time_block_id, s.placements[1].time_block_id,
        "members co-place at the same TB"
    );
    assert_ne!(
        s.placements[0].room_id, s.placements[1].room_id,
        "members occupy distinct rooms"
    );
    assert!(s.violations.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p solver-core tests::lesson_group_atomic_places_two_members_at_one_tb_with_distinct_rooms`
Expected: FAIL because today's greedy treats group members as ordinary multi-class lessons; the second one fails because the first booked all member classes.

- [ ] **Step 3: Add the placed_groups gate plus `try_place_group` helper**

Update `solve_with_config` in `solver/solver-core/src/solve.rs`. Inside the function, just after `let mut state = GreedyState::new();`, add:

```rust
    use crate::ids::LessonGroupId;
    let mut placed_groups: HashSet<LessonGroupId> = HashSet::new();
    let mut group_members: HashMap<LessonGroupId, Vec<usize>> = HashMap::new();
    for (i, lesson) in problem.lessons.iter().enumerate() {
        if let Some(group_id) = lesson.lesson_group_id {
            group_members.entry(group_id).or_default().push(i);
        }
    }
```

Then inside the `for &lesson_idx in &order` loop, add the group dispatch immediately after the `if !idx.teacher_qualified(...) { continue; }` check:

```rust
        if let Some(group_id) = lesson.lesson_group_id {
            if !placed_groups.insert(group_id) {
                continue;
            }
            let member_indices = group_members.get(&group_id).cloned().unwrap_or_default();
            // Single-member groups fall through to the normal placement path.
            if member_indices.len() < 2 {
                placed_groups.remove(&group_id);
            } else {
                let unqualified_member = member_indices.iter().any(|&mi| {
                    let m = &problem.lessons[mi];
                    !idx.teacher_qualified(m.teacher_id, m.subject_id)
                });
                let n = lesson.preferred_block_size;
                let block_count = lesson.hours_per_week / n;
                for block_index in 0..block_count {
                    let placed = if unqualified_member {
                        false
                    } else {
                        try_place_group(
                            problem,
                            &member_indices,
                            n,
                            &idx,
                            &teacher_max,
                            &config.weights,
                            &mut state,
                            &mut solution.placements,
                            &tb_order,
                            &room_order,
                        )
                    };
                    if !placed {
                        for &mi in &member_indices {
                            let member = &problem.lessons[mi];
                            // Skip pre-solve-violation members: they already
                            // have NoQualifiedTeacher entries; do not duplicate.
                            if !idx.teacher_qualified(member.teacher_id, member.subject_id) {
                                continue;
                            }
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
        }
```

Append the helper function below `try_place_block`. It mirrors `try_place_block` but iterates members at every probe and tracks per-member room assignment:

```rust
#[allow(clippy::too_many_arguments)] // Reason: internal helper; refactoring to a struct hurts clarity more than it helps
fn try_place_group(
    problem: &Problem,
    member_indices: &[usize],
    n: u8,
    idx: &Indexed,
    teacher_max: &HashMap<TeacherId, u8>,
    weights: &ConstraintWeights,
    state: &mut GreedyState,
    placements: &mut Vec<Placement>,
    tb_order: &[usize],
    room_order: &[usize],
) -> bool {
    let n_usize = n as usize;
    let members: Vec<&Lesson> = member_indices.iter().map(|&i| &problem.lessons[i]).collect();
    let class_set: std::collections::BTreeSet<SchoolClassId> = members
        .iter()
        .flat_map(|m| m.school_class_ids.iter().copied())
        .collect();

    #[derive(Debug, Clone)]
    struct GroupCandidate {
        outer_pos: usize,
        day: u8,
        start_pos: u8,
        end_pos: u8,
        rooms: Vec<RoomId>,
        score: u32,
    }
    let mut best: Option<GroupCandidate> = None;

    'outer: for outer_pos in 0..tb_order.len() {
        if outer_pos + n_usize > tb_order.len() {
            break;
        }
        let first_tb = &problem.time_blocks[tb_order[outer_pos]];

        for k in 1..n_usize {
            let nb = &problem.time_blocks[tb_order[outer_pos + k]];
            if nb.day_of_week != first_tb.day_of_week
                || nb.position != first_tb.position + (k as u8)
            {
                continue 'outer;
            }
        }

        for k in 0..n_usize {
            let tb = &problem.time_blocks[tb_order[outer_pos + k]];
            for member in &members {
                if state.used_teacher.contains(&(member.teacher_id, tb.id))
                    || idx.teacher_blocked(member.teacher_id, tb.id)
                {
                    continue 'outer;
                }
            }
            for class in &class_set {
                if state.used_class.contains(&(*class, tb.id)) {
                    continue 'outer;
                }
            }
        }
        for member in &members {
            let current = state
                .hours_by_teacher
                .get(&member.teacher_id)
                .copied()
                .unwrap_or(0);
            let max = teacher_max.get(&member.teacher_id).copied().unwrap_or(0);
            if current.saturating_add(n) > max {
                continue 'outer;
            }
        }

        // Greedy lowest-id-first room assignment per member, with both the
        // window-wide used_room set AND a within-group "rooms already taken
        // by earlier members" set.
        let mut chosen: Vec<RoomId> = Vec::with_capacity(members.len());
        let mut taken: HashSet<RoomId> = HashSet::new();
        let mut all_assigned = true;
        for member in &members {
            let mut picked: Option<RoomId> = None;
            'rooms: for &room_idx in room_order {
                let room = &problem.rooms[room_idx];
                if taken.contains(&room.id) {
                    continue;
                }
                if !idx.room_suits_subject(room.id, member.subject_id) {
                    continue;
                }
                for k in 0..n_usize {
                    let tb = &problem.time_blocks[tb_order[outer_pos + k]];
                    if state.used_room.contains(&(room.id, tb.id))
                        || idx.room_blocked(room.id, tb.id)
                    {
                        continue 'rooms;
                    }
                }
                picked = Some(room.id);
                break;
            }
            match picked {
                Some(r) => {
                    taken.insert(r);
                    chosen.push(r);
                }
                None => {
                    all_assigned = false;
                    break;
                }
            }
        }
        if !all_assigned {
            continue;
        }

        let start_pos = first_tb.position;
        let end_pos = start_pos + n - 1;
        let mut class_delta_sum: i64 = 0;
        for class in &class_set {
            let class_partition = state.class_positions.get(&(*class, first_tb.day_of_week));
            let class_old = match class_partition {
                Some(p) => crate::score::gap_count(p),
                None => 0,
            };
            let class_new = gap_count_after_window_insert(class_partition, start_pos, end_pos);
            class_delta_sum += i64::from(class_new) - i64::from(class_old);
        }
        let mut teacher_delta_sum: i64 = 0;
        for member in &members {
            let teacher_partition = state
                .teacher_positions
                .get(&(member.teacher_id, first_tb.day_of_week));
            let teacher_old = match teacher_partition {
                Some(p) => crate::score::gap_count(p),
                None => 0,
            };
            let teacher_new =
                gap_count_after_window_insert(teacher_partition, start_pos, end_pos);
            teacher_delta_sum += i64::from(teacher_new) - i64::from(teacher_old);
        }
        let mut subject_pref = 0u32;
        for member in &members {
            let subject = problem
                .subjects
                .iter()
                .find(|s| s.id == member.subject_id)
                .expect("validate_structural ensures member subject_id resolves");
            for k in 0..n_usize {
                let tb = &problem.time_blocks[tb_order[outer_pos + k]];
                subject_pref = subject_pref
                    .saturating_add(crate::score::subject_preference_score(subject, tb, weights));
            }
        }
        let class_delta_w = class_delta_sum.saturating_mul(i64::from(weights.class_gap));
        let teacher_delta_w =
            teacher_delta_sum.saturating_mul(i64::from(weights.teacher_gap));
        let new_signed = i64::from(state.soft_score)
            .saturating_add(class_delta_w)
            .saturating_add(teacher_delta_w)
            .saturating_add(i64::from(subject_pref));
        let score = u32::try_from(new_signed.max(0)).unwrap_or(u32::MAX);

        if let Some(b) = &best {
            if score >= b.score {
                continue;
            }
        }

        best = Some(GroupCandidate {
            outer_pos,
            day: first_tb.day_of_week,
            start_pos,
            end_pos,
            rooms: chosen,
            score,
        });

        if score == state.soft_score {
            break;
        }
    }

    let Some(c) = best else {
        return false;
    };

    for (member_pos, member) in members.iter().enumerate() {
        let room_id = c.rooms[member_pos];
        for k in 0..n_usize {
            let tb = &problem.time_blocks[tb_order[c.outer_pos + k]];
            placements.push(Placement {
                lesson_id: member.id,
                time_block_id: tb.id,
                room_id,
            });
            state.used_teacher.insert((member.teacher_id, tb.id));
            state.used_room.insert((room_id, tb.id));
        }
        *state
            .hours_by_teacher
            .entry(member.teacher_id)
            .or_insert(0) += n;
    }
    for k in 0..n_usize {
        let tb = &problem.time_blocks[tb_order[c.outer_pos + k]];
        for class in &class_set {
            state.used_class.insert((*class, tb.id));
        }
    }
    for class in &class_set {
        let part = state.class_positions.entry((*class, c.day)).or_default();
        for pos in c.start_pos..=c.end_pos {
            let ins = part.binary_search(&pos).unwrap_or_else(|i| i);
            part.insert(ins, pos);
        }
    }
    for member in &members {
        let part = state
            .teacher_positions
            .entry((member.teacher_id, c.day))
            .or_default();
        for pos in c.start_pos..=c.end_pos {
            let ins = part.binary_search(&pos).unwrap_or_else(|i| i);
            part.insert(ins, pos);
        }
    }
    state.soft_score = c.score;
    true
}
```

- [ ] **Step 4: Run the happy-path test to verify it passes**

Run: `cargo nextest run -p solver-core tests::lesson_group_atomic_places_two_members_at_one_tb_with_distinct_rooms`
Expected: PASS.

- [ ] **Step 5: Add the failure-mode test**

Append to `mod tests`:

```rust
#[test]
fn lesson_group_emits_violation_per_member_when_no_slot_fits() {
    use crate::ids::LessonGroupId;
    let mut p = two_member_group_base_problem();
    // Remove the second room; only one room exists, so two members cannot
    // claim distinct rooms at the only TB.
    p.rooms.truncate(1);
    let s = greedy_solve(&p).unwrap();
    assert!(
        s.placements.is_empty(),
        "no placements when group cannot atomically place"
    );
    let split: Vec<_> = s
        .violations
        .iter()
        .filter(|v| v.kind == ViolationKind::LessonGroupSplit)
        .collect();
    assert_eq!(split.len(), 2, "one LessonGroupSplit per member");
    assert_eq!(split[0].hour_index, 0);
    let lesson_ids: HashSet<LessonId> = split.iter().map(|v| v.lesson_id).collect();
    assert_eq!(lesson_ids.len(), 2);
    let _ = LessonGroupId(solve_uuid(70));
}
```

- [ ] **Step 6: Run failure-mode test**

Run: `cargo nextest run -p solver-core tests::lesson_group_emits_violation_per_member_when_no_slot_fits`
Expected: PASS (the placement helper already returns false when room assignment fails, and the loop emits one violation per member).

- [ ] **Step 7: Add the multi-block test**

Append to `mod tests`:

```rust
#[test]
fn lesson_group_with_two_hours_places_into_two_distinct_tbs() {
    let mut p = two_member_group_base_problem();
    p.time_blocks = vec![
        TimeBlock {
            id: TimeBlockId(solve_uuid(10)),
            day_of_week: 0,
            position: 0,
        },
        TimeBlock {
            id: TimeBlockId(solve_uuid(11)),
            day_of_week: 0,
            position: 1,
        },
    ];
    p.lessons[0].hours_per_week = 2;
    p.lessons[1].hours_per_week = 2;
    let s = greedy_solve(&p).unwrap();
    assert_eq!(s.placements.len(), 4);
    let tbs: HashSet<TimeBlockId> = s.placements.iter().map(|pl| pl.time_block_id).collect();
    assert_eq!(tbs.len(), 2, "group occupies two distinct TBs");
}
```

- [ ] **Step 8: Run multi-block test**

Run: `cargo nextest run -p solver-core tests::lesson_group_with_two_hours_places_into_two_distinct_tbs`
Expected: PASS.

- [ ] **Step 9: Add the non-group blocking test**

Append to `mod tests`:

```rust
#[test]
fn lesson_group_blocked_by_non_group_class_use() {
    use crate::ids::LessonGroupId;
    let mut p = two_member_group_base_problem();
    // Add a TB so we have two slots; non-group lesson takes TB10, group must
    // place at TB11.
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(solve_uuid(11)),
        day_of_week: 0,
        position: 1,
    });
    // Non-group single-class lesson on class 50.
    p.subjects.push(Subject {
        id: SubjectId(solve_uuid(41)),
        prefer_early_periods: false,
        avoid_first_period: false,
    });
    p.teachers.push(Teacher {
        id: TeacherId(solve_uuid(22)),
        max_hours_per_week: 10,
    });
    p.teacher_qualifications.push(TeacherQualification {
        teacher_id: TeacherId(solve_uuid(22)),
        subject_id: SubjectId(solve_uuid(41)),
    });
    p.lessons.push(Lesson {
        id: LessonId(solve_uuid(62)),
        school_class_ids: vec![SchoolClassId(solve_uuid(50))],
        subject_id: SubjectId(solve_uuid(41)),
        teacher_id: TeacherId(solve_uuid(22)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    });
    let s = greedy_solve(&p).unwrap();
    assert_eq!(s.placements.len(), 3, "all three lessons place");
    let group_tb = s.placements.iter().find(|pl| {
        pl.lesson_id == LessonId(solve_uuid(60)) || pl.lesson_id == LessonId(solve_uuid(61))
    });
    let non_group_tb = s
        .placements
        .iter()
        .find(|pl| pl.lesson_id == LessonId(solve_uuid(62)))
        .unwrap();
    assert_ne!(
        group_tb.unwrap().time_block_id,
        non_group_tb.time_block_id,
        "group does not collide with non-group class booking"
    );
    let _ = LessonGroupId(solve_uuid(70));
}
```

- [ ] **Step 10: Run non-group blocking test**

Run: `cargo nextest run -p solver-core tests::lesson_group_blocked_by_non_group_class_use`
Expected: PASS.

- [ ] **Step 11: Add the unqualified-member edge-case test**

Append to `mod tests`:

```rust
#[test]
fn lesson_group_with_unqualified_member_does_not_place() {
    let mut p = two_member_group_base_problem();
    // Remove the qualification for member at index 1.
    p.teacher_qualifications
        .retain(|q| q.teacher_id != TeacherId(solve_uuid(21)));
    let s = greedy_solve(&p).unwrap();
    let split: Vec<_> = s
        .violations
        .iter()
        .filter(|v| v.kind == ViolationKind::LessonGroupSplit)
        .collect();
    let unqual: Vec<_> = s
        .violations
        .iter()
        .filter(|v| v.kind == ViolationKind::NoQualifiedTeacher)
        .collect();
    assert_eq!(split.len(), 1, "qualified member gets LessonGroupSplit");
    assert_eq!(unqual.len(), 1, "unqualified member keeps NoQualifiedTeacher");
    assert_eq!(split[0].lesson_id, LessonId(solve_uuid(60)));
    assert_eq!(unqual[0].lesson_id, LessonId(solve_uuid(61)));
    assert!(s.placements.is_empty());
}
```

- [ ] **Step 12: Run unqualified-member test**

Run: `cargo nextest run -p solver-core tests::lesson_group_with_unqualified_member_does_not_place`
Expected: PASS.

- [ ] **Step 13: Run the entire solver-core suite to confirm no regression**

Run: `cargo nextest run -p solver-core`
Expected: all tests pass.

- [ ] **Step 14: Run lint**

Run: `mise run lint:rust`
Expected: PASS.

- [ ] **Step 15: Commit**

```bash
git add solver/solver-core/src/solve.rs
git commit -m "feat(solver-core): atomic lesson-group co-placement in greedy"
```

---

## Task 4: LAHC skips lesson-group placements

**Files:**
- Modify: `solver/solver-core/src/lahc.rs`
- Test: same file's `#[cfg(test)] mod tests`.

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `solver/solver-core/src/lahc.rs`. Mirrors `lahc_does_not_move_block_placements`:

```rust
#[test]
fn lahc_does_not_move_grouped_placements() {
    use crate::ids::LessonGroupId;
    use crate::types::{
        Lesson, Problem, Room, SchoolClass, Subject, Teacher, TeacherQualification,
    };

    let class_a = SchoolClassId(lahc_uuid(50));
    let class_b = SchoolClassId(lahc_uuid(51));
    let teacher_a = TeacherId(lahc_uuid(20));
    let teacher_b = TeacherId(lahc_uuid(21));
    let subject = SubjectId(lahc_uuid(40));
    let room_a = RoomId(lahc_uuid(30));
    let room_b = RoomId(lahc_uuid(31));
    let lesson_a = LessonId(lahc_uuid(60));
    let lesson_b = LessonId(lahc_uuid(61));
    let group_id = LessonGroupId(lahc_uuid(70));
    let tb_zero = TimeBlockId(lahc_uuid(10));
    let tb_one = TimeBlockId(lahc_uuid(11));

    let problem = Problem {
        time_blocks: vec![
            TimeBlock {
                id: tb_zero,
                day_of_week: 0,
                position: 0,
            },
            TimeBlock {
                id: tb_one,
                day_of_week: 0,
                position: 1,
            },
        ],
        teachers: vec![
            Teacher {
                id: teacher_a,
                max_hours_per_week: 10,
            },
            Teacher {
                id: teacher_b,
                max_hours_per_week: 10,
            },
        ],
        rooms: vec![Room { id: room_a }, Room { id: room_b }],
        subjects: vec![Subject {
            id: subject,
            prefer_early_periods: false,
            avoid_first_period: true,
        }],
        school_classes: vec![SchoolClass { id: class_a }, SchoolClass { id: class_b }],
        lessons: vec![
            Lesson {
                id: lesson_a,
                school_class_ids: vec![class_a, class_b],
                subject_id: subject,
                teacher_id: teacher_a,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: Some(group_id),
            },
            Lesson {
                id: lesson_b,
                school_class_ids: vec![class_a, class_b],
                subject_id: subject,
                teacher_id: teacher_b,
                hours_per_week: 1,
                preferred_block_size: 1,
                lesson_group_id: Some(group_id),
            },
        ],
        teacher_qualifications: vec![
            TeacherQualification {
                teacher_id: teacher_a,
                subject_id: subject,
            },
            TeacherQualification {
                teacher_id: teacher_b,
                subject_id: subject,
            },
        ],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
    };
    let idx = crate::index::Indexed::new(&problem);

    // Seed the group at position 0 (the avoid-first slot LAHC would normally
    // want to escape).
    let mut placements = vec![
        Placement {
            lesson_id: lesson_a,
            time_block_id: tb_zero,
            room_id: room_a,
        },
        Placement {
            lesson_id: lesson_b,
            time_block_id: tb_zero,
            room_id: room_b,
        },
    ];
    let mut class_positions: HashMap<(SchoolClassId, u8), Vec<u8>> = HashMap::new();
    class_positions.insert((class_a, 0), vec_part(&[0]));
    class_positions.insert((class_b, 0), vec_part(&[0]));
    let mut teacher_positions: HashMap<(TeacherId, u8), Vec<u8>> = HashMap::new();
    teacher_positions.insert((teacher_a, 0), vec_part(&[0]));
    teacher_positions.insert((teacher_b, 0), vec_part(&[0]));
    let mut used_teacher: HashSet<(TeacherId, TimeBlockId)> = HashSet::new();
    used_teacher.insert((teacher_a, tb_zero));
    used_teacher.insert((teacher_b, tb_zero));
    let mut used_class: HashSet<(SchoolClassId, TimeBlockId)> = HashSet::new();
    used_class.insert((class_a, tb_zero));
    used_class.insert((class_b, tb_zero));
    let mut used_room: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
    used_room.insert((room_a, tb_zero));
    used_room.insert((room_b, tb_zero));
    let mut current_score: u32 = 2;

    let config = SolveConfig {
        weights: ConstraintWeights {
            avoid_first_period: 1,
            ..ConstraintWeights::default()
        },
        seed: 0,
        deadline: Some(std::time::Duration::from_millis(50)),
        max_iterations: Some(2000),
    };

    run(
        &problem,
        &idx,
        &config,
        &mut placements,
        &mut class_positions,
        &mut teacher_positions,
        &mut used_teacher,
        &mut used_class,
        &mut used_room,
        &mut current_score,
    );

    let tb_ids: HashSet<TimeBlockId> = placements.iter().map(|p| p.time_block_id).collect();
    assert!(
        tb_ids.contains(&tb_zero) && !tb_ids.contains(&tb_one),
        "group placement must not be moved by LAHC; got {:?}",
        tb_ids
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p solver-core lahc::tests::lahc_does_not_move_grouped_placements`
Expected: FAIL because LAHC will move one of the placements off TB0 to escape the avoid_first penalty.

- [ ] **Step 3: Add the guard**

In `try_change_move` in `solver/solver-core/src/lahc.rs`, immediately after the existing Doppelstunden guard:

```rust
    if lesson.preferred_block_size > 1 {
        return false;
    }
    if lesson.lesson_group_id.is_some() {
        return false;
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -p solver-core lahc::tests::lahc_does_not_move_grouped_placements`
Expected: PASS.

- [ ] **Step 5: Run the LAHC determinism property test**

Run: `cargo nextest run -p solver-core --test lahc_property`
Expected: PASS (the new guard adds no `random_range` draws).

- [ ] **Step 6: Run lint**

Run: `mise run lint:rust`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add solver/solver-core/src/lahc.rs
git commit -m "feat(solver-core): LAHC skips lesson-group placements"
```

---

## Task 5: Frontend i18n entry for the new violation kind

**Files:**
- Modify: `frontend/src/i18n/violation-keys.ts`
- Modify: `frontend/src/i18n/locales/en/translation.json`
- Modify: `frontend/src/i18n/locales/de/translation.json`
- Modify: `backend/src/klassenzeit_backend/api/schemas.py` (Literal[...] union widening)

The backend Pydantic schema must widen first; otherwise `mise run fe:types` regenerates an api-types.ts that does not contain `"lesson_group_split"`, and the new switch arm fails to type-check.

- [ ] **Step 1: Widen the Pydantic Literal**

Edit `backend/src/klassenzeit_backend/api/schemas.py` to add `"lesson_group_split"` to the `Literal[...]` union of `ViolationResponse.kind`. Find the existing union (currently `Literal["no_qualified_teacher", "teacher_over_capacity", "no_free_time_block", "no_suitable_room"]`) and append `, "lesson_group_split"`. Run `rg "no_qualified_teacher" backend/src/klassenzeit_backend/api/schemas.py` to locate the exact line.

- [ ] **Step 2: Regenerate api-types.ts**

Run: `mise run fe:types`
Expected: `frontend/src/lib/api-types.ts` updates the `ViolationResponse.kind` union.

- [ ] **Step 3: Add the switch arm**

Edit `frontend/src/i18n/violation-keys.ts`:

```ts
import type { components } from "@/lib/api-types";

type ViolationKind = components["schemas"]["ViolationResponse"]["kind"];

export function violationItemKey(
  kind: ViolationKind,
):
  | "schedule.violations.noQualifiedTeacher"
  | "schedule.violations.teacherOverCapacity"
  | "schedule.violations.noFreeTimeBlock"
  | "schedule.violations.noSuitableRoom"
  | "schedule.violations.lessonGroupSplit" {
  switch (kind) {
    case "no_qualified_teacher":
      return "schedule.violations.noQualifiedTeacher";
    case "teacher_over_capacity":
      return "schedule.violations.teacherOverCapacity";
    case "no_free_time_block":
      return "schedule.violations.noFreeTimeBlock";
    case "no_suitable_room":
      return "schedule.violations.noSuitableRoom";
    case "lesson_group_split":
      return "schedule.violations.lessonGroupSplit";
  }
}
```

- [ ] **Step 4: Add the i18n entries**

Run `rg '"noQualifiedTeacher"' frontend/src/i18n/locales/` to locate the existing keys; add a sibling `lessonGroupSplit` entry to both files inside the same `schedule.violations` block.

`frontend/src/i18n/locales/en/translation.json`:
```json
"lessonGroupSplit": "Lesson group could not be placed in a single time block."
```

`frontend/src/i18n/locales/de/translation.json`:
```json
"lessonGroupSplit": "Gruppenstunde konnte nicht in einem gemeinsamen Zeitblock platziert werden."
```

- [ ] **Step 5: Typecheck and lint frontend**

Run: `cd frontend && mise exec -- pnpm exec tsc --noEmit`
Expected: PASS.

Run: `mise run fe:lint`
Expected: PASS.

- [ ] **Step 6: Run frontend tests**

Run: `mise run fe:test`
Expected: PASS.

- [ ] **Step 7: Run backend tests touching the schema**

Run: `mise run test:py -- backend/tests -k violations`
Expected: PASS (no test asserts the closed enum membership today).

- [ ] **Step 8: Commit**

```bash
git add backend/src/klassenzeit_backend/api/schemas.py frontend/src/lib/api-types.ts frontend/src/i18n/violation-keys.ts frontend/src/i18n/locales/en/translation.json frontend/src/i18n/locales/de/translation.json
git commit -m "feat(frontend): i18n for lesson_group_split violation kind"
```

---

## Task 6: Bench refresh

**Files:**
- Modify: `solver/solver-core/benches/BASELINE.md`

The dreizuegige fixture already exercises the lesson-group shape. With the constraint live, soft scores should improve.

- [ ] **Step 1: Run the bench**

Run: `mise run bench:record`
Expected: `solver/solver-core/benches/BASELINE.md` updates with new dreizuegige numbers; grundschule and zweizuegige should be within noise of the prior values.

- [ ] **Step 2: Inspect the diff**

Run: `git diff solver/solver-core/benches/BASELINE.md`
Expected: dreizuegige `Soft score` column drops (from 8 toward 0); placements stay 294; greedy p50 µs same or better.

- [ ] **Step 3: Confirm the 20% budget is honoured**

If any fixture's p50 µs jumps more than 20% over the prior committed value, stop and investigate before committing. The expected outcome is improvement on dreizuegige; non-group fixtures should be flat.

- [ ] **Step 4: Commit**

```bash
git add solver/solver-core/benches/BASELINE.md
git commit -m "perf(solver-core): refresh BASELINE.md with lesson-group co-placement"
```

---

## Task 7: ADR 0022

**Files:**
- Create: `docs/adr/0022-lesson-group-coplacement.md`
- Modify: `docs/adr/README.md`

- [ ] **Step 1: Verify the ADR number is free**

Run: `ls docs/adr/*.md | sort | tail -3`
Expected: highest existing number is 0021. If 0022 already exists, pick the next free number.

- [ ] **Step 2: Write the ADR**

Use the project's "NNNN: Title" colon format (no em-dash) per the project rule. Content:

```markdown
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
   `solve_with_config` tracks `placed_groups: HashSet<LessonGroupId>`; the
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
   class sets. The atomic-placement scorer dedupes the class set via a
   `BTreeSet<SchoolClassId>` over the union of member class sets so the
   class-gap delta is counted once per (class, day, position) tuple. Teacher
   and subject-preference deltas iterate members directly because teachers
   are pairwise-distinct (validation rule) and subjects are independent.

## Alternatives considered

- **Independent placement with a "is the booker in my group?" probe** in
  `state.used_class`. Rejected: complicates the hot probe and leaks
  group-awareness into single-class lessons.
- **Aggregate the group into one FFD entity.** Rejected: option (a) above
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
  daily partition. Soft score on the dreizuegige bench fixture drops
  accordingly.
- Greedy work decreases on the dreizuegige fixture (one decision per group
  instead of one per member). p50 wall-clock should hold or improve;
  `BASELINE.md` is refreshed in the same PR.
- The `ViolationKind` wire format gains one variant (additive). Frontend
  i18n adds the matching `schedule.violations.lessonGroupSplit` entry in
  en + de.
- The "FFD eligibility weighting for cross-class lessons" deferral may
  graduate to "closed by side effect" once the dreizuegige solvability
  test passes without the eligibility tweak.
```

- [ ] **Step 3: Update the ADR index**

Open `docs/adr/README.md` and add the new ADR row in numeric order.

- [ ] **Step 4: Verify no em-dashes / en-dashes in the new ADR**

Run: `grep -c '—' docs/adr/0022-lesson-group-coplacement.md && grep -c '–' docs/adr/0022-lesson-group-coplacement.md`
Expected: both counts are `0`.

- [ ] **Step 5: Commit**

```bash
git add docs/adr/0022-lesson-group-coplacement.md docs/adr/README.md
git commit -m "docs(adr): record 0022 lesson-group co-placement constraint"
```

---

## Task 8: Backend dreizuegige solvability sanity check

**Files:**
- Verify: `backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py`

- [ ] **Step 1: Run the dreizuegige solvability test**

Run: `mise run test:py -- backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py -v`
Expected: PASS. The test asserts the schedule is solvable; with the constraint live, the soft score should improve and the schedule should remain feasible.

- [ ] **Step 2: If the test now expects a tighter assertion (e.g. soft_score == 0), update the assertion**

Open the test, identify any soft-score assertion, and tighten if the new value is consistent across two runs locally. If the test only asserts feasibility, no edit is needed.

- [ ] **Step 3: Run the wider backend suite**

Run: `mise run test:py`
Expected: PASS.

- [ ] **Step 4: Commit (only if a test assertion changed)**

If the test was edited:

```bash
git add backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py
git commit -m "test(backend): tighten dreizuegige solvability soft-score assertion"
```

If no edit was needed, no commit; proceed.

---

## Task 9: OPEN_THINGS update

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Mark sprint item 6 as shipped**

Open `docs/superpowers/OPEN_THINGS.md`. Find the algorithm-phase section item 6 ("Lesson-group co-placement constraint"). Add a `✅ Shipped 2026-04-30 in PR ...` clause matching the format used by items 2 to 4 of the data + schema phase. Cite the PR slug `feat/solver-lesson-group-coplacement`. The `(#NNN)` PR number lands at PR-merge time; for now, use the slug.

- [ ] **Step 2: Reassess the FFD-eligibility-for-cross-class-lessons deferral**

If Task 8's solvability test passed without changes, edit the "Acknowledged deferrals" entry "FFD eligibility weighting for cross-class lessons" to add a closing note: "Closed by side effect 2026-04-30 in `feat/solver-lesson-group-coplacement`: the lesson-group constraint collapses each Religion trio into one time-block, removing the 5x7-grid pressure that motivated the original deferral." If the solvability test required a workaround, leave the deferral open with a refreshed framing.

- [ ] **Step 3: File the LAHC group-swap follow-up**

Append to "Acknowledged deferrals":

```markdown
- **Atomic group-swap LAHC move.** Today's LAHC skips lesson-group placements via a one-line guard in `try_change_move` (mirrors the Doppelstunden pattern). A richer move shape would pick a group, move every member to a new TB, and reassign rooms greedily; the determinism property test in `tests/lahc_property.rs` would need a third `random_range` draw per iteration. Out of scope until benches surface a real soft-score gap on group-heavy fixtures. Surfaced during the lesson-group co-placement PR.
```

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/OPEN_THINGS.md
git commit -m "docs: close sprint item 6 (lesson-group co-placement) in OPEN_THINGS"
```

---

## Task 10: Run the full local pre-push gate

- [ ] **Step 1: Run the entire test suite**

Run: `mise run test`
Expected: PASS across Rust, Python, frontend.

- [ ] **Step 2: Run lint**

Run: `mise run lint`
Expected: PASS.

- [ ] **Step 3: Status check**

Run: `git status && git log --oneline origin/master..HEAD`
Expected: clean working tree; commits are the spec, the seven feature commits, the bench refresh, the ADR, the OPEN_THINGS update.

---

## Self-review checklist

1. **Spec coverage.**
   - Add `LessonGroupSplit` variant: Task 1.
   - Group invariants in `validate_structural`: Task 2.
   - Atomic placement in greedy: Task 3 (every test described in the spec is named in Task 3 steps).
   - Class-delta dedup across members: Task 3 step 3 (`class_set: BTreeSet<SchoolClassId>`).
   - Greedy lowest-id-first room assignment: Task 3 step 3 (`taken: HashSet<RoomId>` + `for &room_idx in room_order` walk).
   - LAHC skip: Task 4.
   - Frontend i18n: Task 5.
   - Bench refresh: Task 6.
   - ADR 0022: Task 7.
   - Backend solvability check: Task 8.
   - OPEN_THINGS update + LAHC group-swap follow-up: Task 9.

2. **Placeholder scan.** No TBDs, no "implement later", no "similar to Task N" placeholders. Every code-touching step shows the code.

3. **Type consistency.** `LessonGroupId` matches the existing newtype in `solver/solver-core/src/ids.rs`. `ViolationKind::LessonGroupSplit` lands in Task 1, used in Task 3, and surfaces in Task 5's frontend switch and Task 5's backend `Literal[...]`. `try_place_group` signature matches its caller in Task 3 step 3. `placed_groups: HashSet<LessonGroupId>` and `group_members: HashMap<LessonGroupId, Vec<usize>>` are introduced together and consumed together.
