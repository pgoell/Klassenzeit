# Solver `validate_no_double_booking` post-condition validator implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land a post-condition validator (`validate_no_double_booking`) that catches silent class / teacher / room double-bookings, lesson-cardinality mismatches, and malformed blocks in the final placements vector returned by `solve_with_config`. Wire it after `validate_no_room_hopping` (release-mode `Result`) and alongside `validate_daily_caps` (`#[cfg(debug_assertions)]` panic). Bundle the working-tree in-flight work into the same PR per user direction: daily-caps validator + smoke test (commit 1), lahc.rs `rr_attempt` slice-only score recompute (commit 2), lahc.rs Kempe `removed_subject_pref` caller-computes refactor (commit 3), the validator itself (commit 4), backend `collect_pinned_placements` cross-class pin scope rewrite (commit 5), CSS `@import` reorder (commit 6).

**Architecture:** One function in `solver-core/src/validate.rs`, one walk over `placements`, five assertions sharing the lesson and time-block lookups. Returns `Err(Error::Input(...))` on the first violation found, with messages prefixed by `double-booking:` / `lesson cardinality:` / `block shape:` so the panic message discriminates without a `match`. Wiring at `solver-core/src/solve.rs` after the existing `validate_no_room_hopping` call (release `Result`) and as a `#[cfg(debug_assertions)]` panic alongside `validate_daily_caps`. Ten inline unit tests cover all failure shapes plus the happy path. Bundled commits land in declared order so each commit compiles and tests pass standalone, and so commit 4's TDD step has the lahc.rs slice fix already in place when the validator's cfg-panic runs.

**Tech Stack:** Rust 1.85, `solver-core` library crate (no PyO3, no I/O). Tests via `cargo nextest` through `mise run test:rust`. Lint via `mise run lint`.

---

## File structure

| File | Purpose | Touched by |
| --- | --- | --- |
| `solver/solver-core/src/validate.rs` | Hosts `validate_no_double_booking` plus 10 inline unit tests. Already hosts `validate_daily_caps` and the existing tests in the working tree. | Tasks 1, 4 |
| `solver/solver-core/src/solve.rs` | Wires both validators after `validate_no_room_hopping`. Already wires `validate_daily_caps` cfg-panic in working tree. | Tasks 1, 4 |
| `solver/solver-core/src/lahc.rs` | Slice-only score recompute in `rr_attempt` (commit 2); caller-side `removed_subject_pref` refactor in Kempe (commit 3). Already in working tree. | Tasks 2, 3 |
| `solver/solver-core/tests/daily_caps.rs` | New `caps_kempe_solve_under_production_caps_smoke` smoke test. Already in working tree. | Task 1 |
| `backend/src/klassenzeit_backend/scheduling/solver_io.py` | `collect_pinned_placements` rewritten: pin cross-class lessons whenever any sibling sees drift. Already in working tree. | Task 5 |
| `frontend/src/styles/app.css` | Quicksand `@import` reorder before tailwindcss. Already in working tree. | Task 6 |
| `docs/superpowers/OPEN_THINGS.md` | Adds correctness phase header + items 39, 40, 41 (commit 1). Strips item 39 + item 33 entries in step 6 of the autopilot run (NOT in this plan; covered by autopilot's documentation pass). | Task 1 |

The conditional Q4-branch-2 commit (Kempe `used_*` contains-check fix) lands as Task 4b only if the cfg-panic fires on `caps_kempe_solve_under_production_caps_smoke` or any other existing test. Task 4b includes its own regression test in `solver-core/tests/lahc_property.rs`.

---

## Task 1: Commit working-tree daily-caps validator + cfg-panic wiring + smoke test + OPEN_THINGS sprint header

**Files:**
- Modify (already in working tree): `solver/solver-core/src/validate.rs` (+90 src / +93 tests)
- Modify (already in working tree): `solver/solver-core/src/solve.rs` (+10 lines wiring at the existing post-condition zone)
- Modify (already in working tree): `solver/solver-core/tests/daily_caps.rs` (+28 lines smoke test)
- Modify (already in working tree): `docs/superpowers/OPEN_THINGS.md` (+10 lines correctness phase header + items 39, 40, 41)

This commit picks up code that already lives in the working tree from the prior session. No new code is written; the work is to verify the tests pass on the existing diff and commit.

- [ ] **Step 1: Verify the daily-caps validator and smoke test pass on the current working tree**

Run:
```bash
mise run test:rust -- -p solver-core --test daily_caps
mise run test:rust -- -p solver-core --lib validate
```

Expected: both pass green. The 4 inline unit tests for `validate_daily_caps` + 4 existing daily-caps integration tests + the new `caps_kempe_solve_under_production_caps_smoke` all pass.

- [ ] **Step 2: Verify the OPEN_THINGS edit only adds the correctness-phase sprint header and items 39, 40, 41**

Run:
```bash
git diff docs/superpowers/OPEN_THINGS.md | head -25
```

Expected: only `+`-lines visible (no `-` deletions), introducing `### Correctness phase` and items 39, 40, 41 plus the sprint-header goal sentence about closing the silent-hard-violation gap.

- [ ] **Step 3: Stage the four files**

Run:
```bash
git add solver/solver-core/src/validate.rs solver/solver-core/src/solve.rs solver/solver-core/tests/daily_caps.rs docs/superpowers/OPEN_THINGS.md
git status
```

Expected: four files marked staged, three files (`solver/solver-core/src/lahc.rs`, `backend/src/klassenzeit_backend/scheduling/solver_io.py`, `frontend/src/styles/app.css`) still showing as unstaged.

- [ ] **Step 4: Commit**

Run:
```bash
git commit -m "$(cat <<'EOF'
feat(solver-core): validate_daily_caps post-condition + cfg-panic wiring (sprint 5 item 39 prep)

Adds `validate_daily_caps(problem, placements) -> Result<(), Error>` next to
`validate_no_room_hopping` in `solver-core/src/validate.rs`: walks the
final placements once, attributes each row to every member class, and
asserts no `(class, day, subject)` exceeds `Subject.max_hours_per_day`
and no `(class, day)` exceeds `SchoolClass.max_lessons_per_day`. Failure
returns `Err(Error::Input)` so production paths surface as runtime errors.

Wires the validator into `solve_with_config`:

  - Release-mode call right after `validate_no_room_hopping` (covers
    production runs).
  - `#[cfg(debug_assertions)]` panic so property and integration tests
    fail loudly on a silent cap violation.

Adds four inline unit tests in `validate.rs::tests` (within-caps happy
path, subject-hours-per-day exceeded, class-lessons-per-day exceeded,
two-period block counted as one lesson) plus the smoke test
`caps_kempe_solve_under_production_caps_smoke` in `tests/daily_caps.rs`
that runs the dreizuegig fixture (with `Subject.max_hours_per_day` clipped
to 2 everywhere) through 10 seeds of the production move config and
asserts the cfg-panic does not fire.

Bumps `docs/superpowers/OPEN_THINGS.md` with the correctness-phase sprint
header and items 39, 40, 41 (validator + property generator + soft-score
reconciliation). Item 39 ships in this PR as commit 4; items 40 and 41
stay queued.
EOF
)"
```

Expected: lefthook pre-commit runs and passes (lint sweep), commit-msg hook passes (`cog verify`), commit lands. Output ends with `[feat/solver-double-booking-validator <sha>] feat(solver-core): ...`.

---

## Task 2: Commit lahc.rs `rr_attempt` slice-only score recompute fix

**Files:**
- Modify (already in working tree): `solver/solver-core/src/lahc.rs` (one of two changes carried by the diff: `rr_attempt`'s post-recreate score recompute now uses `running_slice_from_placements` instead of `score::score_solution`)

This commit cherry-picks ONE of the two lahc.rs changes already in the working tree. The other (Kempe `removed_subject_pref` caller-computes) ships as Task 3. We split via `git add -p` because both changes live in the same file.

- [ ] **Step 1: Verify the slice fix lives in the working tree**

Run:
```bash
git diff solver/solver-core/src/lahc.rs | grep -A 6 "running_slice_from_placements"
```

Expected: shows the diff hunk where `rr_attempt`'s score recompute switched from `crate::score::score_solution(problem, placements, weights)` to `running_slice_from_placements(problem, placements, weights, max_position_per_day)` with a 6-line comment explaining the slice-vs-full-score asymmetry.

- [ ] **Step 2: Stage only the slice-fix hunk via `git add -p`**

Run:
```bash
git add -p solver/solver-core/src/lahc.rs
```

Interactively respond to the prompts:
- Hunk in `rr_attempt` (around line 942) introducing `running_slice_from_placements`: `y` (stage).
- Hunk in `kempe_snapshot_pre_score` (around line 1369) changing the function signature: `n` (skip; goes to Task 3).
- Hunk in `kempe_attempt` (around line 1591) changing `_class_max_lessons_per_day` to `class_max_lessons_per_day`: `n` (skip; part of Task 3 because the unused-prefix removal is paired with a new use of the param).
- Any other hunks in `kempe_attempt` (around line 1868 ff.) where `kempe_snapshot_pre_score` is called: `n` (skip).

Expected: `git diff --cached solver/solver-core/src/lahc.rs` shows only the `rr_attempt` recompute change; `git diff solver/solver-core/src/lahc.rs` shows the Kempe-side hunks unstaged.

- [ ] **Step 3: Verify the staged change compiles standalone**

Run:
```bash
git stash --keep-index --include-untracked --message "task2-keep-staged"
mise run lint
mise run test:rust
git stash pop
```

Expected: lint green, all rust tests green. (The `git stash --keep-index` trick stashes ONLY the unstaged changes and leaves the staged slice-fix in place, so we test exactly what's about to be committed without the Task-3 Kempe refactor or any other parallel work interfering.)

If `git stash pop` reports a conflict (it shouldn't because we didn't modify the same hunks in the index and the stash), inspect with `git status`, resolve manually, and re-verify.

- [ ] **Step 4: Commit**

Run:
```bash
git commit -m "$(cat <<'EOF'
fix(solver-core): drop class_day_balance and home_room from rr_attempt slice score

`rr_attempt` recomputes `state.soft_score` after a successful R&R recreate
so the LAHC gate decides on correct numbers and downstream Change moves
operate on a consistent baseline. The recompute used
`score::score_solution`, which sums class_gap + teacher_gap + subject_pref
+ class_day_balance + home_room. The Change-move and Kempe deltas
maintain only the slice (class_gap + teacher_gap + subject_pref); pinning
the post-R&R score against the full weighted total contaminates
`state.soft_score` by the class_day_balance + home_room contribution and
later Change moves drive `state.soft_score` negative because their deltas
do not match the inflated baseline.

Switches the recompute to a slice-only helper
(`running_slice_from_placements`) so `state.soft_score` stays inside the
Change/Kempe maintenance contract. The full weighted score still appears
in `solution.soft_score` via the existing `state.soft_score` assignment
in `solve_with_config`; that assignment continues to omit
class_day_balance + home_room (item 41), which is a separate concern.
EOF
)"
```

Expected: lefthook pre-commit runs and passes, commit lands.

---

## Task 3: Commit lahc.rs Kempe `removed_subject_pref` caller-computes refactor

**Files:**
- Modify (already in working tree): `solver/solver-core/src/lahc.rs` (`kempe_snapshot_pre_score` signature change; caller computes `removed_subject_pref` from actual ruined rows; `_class_max_lessons_per_day` prefix removal in `kempe_attempt`)

This commit picks up the SECOND lahc.rs change that we skipped in Task 2.

- [ ] **Step 1: Verify only the Kempe-side hunks remain unstaged in lahc.rs**

Run:
```bash
git diff solver/solver-core/src/lahc.rs | head -10
git diff --cached solver/solver-core/src/lahc.rs
```

Expected: the unstaged diff shows the `kempe_snapshot_pre_score` signature change (`subject_lookup`, `weights`, `max_position_per_day` parameters dropped; return type changes from `(KempePartitionSnapshot, u32)` to `KempePartitionSnapshot`) and the `kempe_attempt` `class_max_lessons_per_day` prefix removal. The cached diff is empty (Task 2's commit landed).

- [ ] **Step 2: Stage the remaining lahc.rs hunks**

Run:
```bash
git add solver/solver-core/src/lahc.rs
git diff --cached solver/solver-core/src/lahc.rs | wc -l
```

Expected: cached diff has lines in the 80-150 range (the Kempe signature change hunks).

- [ ] **Step 3: Verify the change compiles and tests pass**

Run:
```bash
git stash --keep-index --include-untracked --message "task3-keep-staged"
mise run lint
mise run test:rust
git stash pop
```

Expected: lint green, all rust tests green.

- [ ] **Step 4: Commit**

Run:
```bash
git commit -m "$(cat <<'EOF'
refactor(solver-core): caller computes kempe removed_subject_pref from ruined rows

`kempe_snapshot_pre_score` previously returned `(snapshot, removed_subject_pref)`.
The `removed_subject_pref` total was computed by summing
`subject_preference_score` over every placement of every chain member,
which double-counts when a chain member has another untouched block on a
different day (those placements stay put, so they should contribute zero
to the delta).

Splits the snapshot into snapshot-only and lets the caller compute
`removed_subject_pref` from the actually-ruined rows captured in the
recreate loop. The function signature drops `subject_lookup`, `weights`,
and `max_position_per_day` parameters that the caller already has access
to. Pure restructure: identical observable behaviour for the single-block
chain shapes that the existing property tests cover; correct delta for
multi-block-on-other-day chain members (relevant once item 40 widens the
property generator to mix preferred_block_size).

Also unprefixes `class_max_lessons_per_day` in `kempe_attempt`'s
parameter list since the parameter is now used (legality check during
chain construction added in this refactor).
EOF
)"
```

Expected: lefthook pre-commit runs and passes, commit lands.

---

## Task 4: TDD `validate_no_double_booking` validator + wire it

**Files:**
- Modify: `solver/solver-core/src/validate.rs` (add `validate_no_double_booking` function + 10 inline unit tests in the existing `#[cfg(test)] mod tests` block)
- Modify: `solver/solver-core/src/solve.rs` (add Result-form call after `validate_no_room_hopping`; add cfg-panic block alongside `validate_daily_caps`'s)

This is the headline change for the PR. Strict TDD discipline: write tests first, watch them fail, implement minimum, watch them pass, refactor.

- [ ] **Step 1: Write the function signature and the happy-path test only**

Add to `solver/solver-core/src/validate.rs`, immediately after `validate_daily_caps`'s function body (around line 340):

```rust
/// Hard-constraint sanity check: the final placements vector contains no
/// class / teacher / room double-booking, every lesson appears exactly
/// `hours_per_week` times, and every block is `preferred_block_size`
/// contiguous positions on one day in one room. A failure here indicates
/// a solver bug (a move applied without contains-checks) rather than
/// malformed input; production callers surface it as a runtime error.
/// Failure messages are prefixed with `double-booking:`,
/// `lesson cardinality:`, or `block shape:` so debug-mode panic messages
/// discriminate which check fired without parsing.
pub fn validate_no_double_booking(
    problem: &Problem,
    placements: &[Placement],
) -> Result<(), Error> {
    let _ = (problem, placements);
    Ok(())
}
```

Then add to the `#[cfg(test)] mod tests` block at the bottom of `validate.rs`:

```rust
#[test]
fn validate_no_double_booking_accepts_well_formed_schedule() {
    let mut p = minimal_problem();
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(uuid(11)),
        day_of_week: 0,
        position: 1,
    });
    p.lessons[0].hours_per_week = 2;
    p.lessons[0].preferred_block_size = 2;
    let placements = vec![
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[1].id,
            room_id: p.rooms[0].id,
        },
    ];
    validate_no_double_booking(&p, &placements).unwrap();
}
```

- [ ] **Step 2: Verify the happy-path test passes against the stub**

Run:
```bash
mise run test:rust -- -p solver-core --lib validate::tests::validate_no_double_booking_accepts_well_formed_schedule
```

Expected: PASS (the stub returns `Ok(())` unconditionally so the happy path passes trivially).

- [ ] **Step 3: Write the three double-booking failure tests**

Append to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn validate_no_double_booking_rejects_class_double_booking() {
    let mut p = minimal_problem();
    let class_id = p.school_classes[0].id;
    p.lessons.push(Lesson {
        id: LessonId(uuid(20)),
        school_class_ids: vec![class_id],
        subject_id: p.subjects[0].id,
        teacher_id: p.teachers[0].id,
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    });
    p.lessons[0].hours_per_week = 1;
    let placements = vec![
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[1].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
    ];
    let err = validate_no_double_booking(&p, &placements).unwrap_err();
    assert!(matches!(err, Error::Input(msg) if msg.contains("double-booking: class")));
}

#[test]
fn validate_no_double_booking_rejects_teacher_double_booking() {
    let mut p = minimal_problem();
    let class2 = SchoolClass {
        id: SchoolClassId(uuid(30)),
        home_room_id: None,
        max_lessons_per_day: None,
    };
    p.school_classes.push(class2.clone());
    p.lessons.push(Lesson {
        id: LessonId(uuid(31)),
        school_class_ids: vec![class2.id],
        subject_id: p.subjects[0].id,
        teacher_id: p.teachers[0].id,
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    });
    p.lessons[0].hours_per_week = 1;
    let placements = vec![
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[1].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
    ];
    let err = validate_no_double_booking(&p, &placements).unwrap_err();
    assert!(matches!(err, Error::Input(msg) if msg.contains("double-booking: teacher")));
}

#[test]
fn validate_no_double_booking_rejects_room_double_booking() {
    let mut p = minimal_problem();
    let class2 = SchoolClass {
        id: SchoolClassId(uuid(40)),
        home_room_id: None,
        max_lessons_per_day: None,
    };
    p.school_classes.push(class2.clone());
    p.teachers.push(Teacher {
        id: TeacherId(uuid(41)),
        max_hours_per_week: 10,
    });
    p.teacher_qualifications.push(TeacherQualification {
        teacher_id: TeacherId(uuid(41)),
        subject_id: p.subjects[0].id,
    });
    p.lessons.push(Lesson {
        id: LessonId(uuid(42)),
        school_class_ids: vec![class2.id],
        subject_id: p.subjects[0].id,
        teacher_id: TeacherId(uuid(41)),
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    });
    p.lessons[0].hours_per_week = 1;
    let placements = vec![
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[1].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
    ];
    let err = validate_no_double_booking(&p, &placements).unwrap_err();
    assert!(matches!(err, Error::Input(msg) if msg.contains("double-booking: room")));
}
```

- [ ] **Step 4: Run the three tests to verify they fail (RED)**

Run:
```bash
mise run test:rust -- -p solver-core --lib validate::tests::validate_no_double_booking_rejects_class_double_booking validate::tests::validate_no_double_booking_rejects_teacher_double_booking validate::tests::validate_no_double_booking_rejects_room_double_booking
```

Expected: all three FAIL with `unwrap_err()` panicking on `Ok(())` (because the stub still returns `Ok(())`).

- [ ] **Step 5: Implement double-booking detection (replace the stub body)**

Replace the body of `validate_no_double_booking` in `validate.rs`:

```rust
pub fn validate_no_double_booking(
    problem: &Problem,
    placements: &[Placement],
) -> Result<(), Error> {
    use std::collections::hash_map::Entry;
    use std::collections::HashMap;

    let lesson_by_id: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let tb_by_id: HashMap<TimeBlockId, &TimeBlock> =
        problem.time_blocks.iter().map(|t| (t.id, t)).collect();

    let mut class_used: HashMap<(SchoolClassId, TimeBlockId), LessonId> = HashMap::new();
    let mut teacher_used: HashMap<(TeacherId, TimeBlockId), LessonId> = HashMap::new();
    let mut room_used: HashMap<(RoomId, TimeBlockId), LessonId> = HashMap::new();
    let mut rows_by_lesson: HashMap<LessonId, Vec<(u8, u8, RoomId)>> = HashMap::new();

    for p in placements {
        let lesson = lesson_by_id
            .get(&p.lesson_id)
            .ok_or_else(|| Error::Input(format!("unknown lesson {:?}", p.lesson_id)))?;
        let tb = tb_by_id
            .get(&p.time_block_id)
            .ok_or_else(|| Error::Input(format!("unknown time block {:?}", p.time_block_id)))?;

        for class_id in &lesson.school_class_ids {
            match class_used.entry((*class_id, p.time_block_id)) {
                Entry::Vacant(v) => {
                    v.insert(p.lesson_id);
                }
                Entry::Occupied(o) if *o.get() == p.lesson_id => {
                    // Same lesson, same row: caught by the cardinality check below.
                }
                Entry::Occupied(o) => {
                    return Err(Error::Input(format!(
                        "double-booking: class {:?} at time-block {:?}: lessons {:?} and {:?}",
                        class_id,
                        p.time_block_id,
                        o.get(),
                        p.lesson_id
                    )));
                }
            }
        }
        match teacher_used.entry((lesson.teacher_id, p.time_block_id)) {
            Entry::Vacant(v) => {
                v.insert(p.lesson_id);
            }
            Entry::Occupied(o) if *o.get() == p.lesson_id => {}
            Entry::Occupied(o) => {
                return Err(Error::Input(format!(
                    "double-booking: teacher {:?} at time-block {:?}: lessons {:?} and {:?}",
                    lesson.teacher_id,
                    p.time_block_id,
                    o.get(),
                    p.lesson_id
                )));
            }
        }
        match room_used.entry((p.room_id, p.time_block_id)) {
            Entry::Vacant(v) => {
                v.insert(p.lesson_id);
            }
            Entry::Occupied(o) if *o.get() == p.lesson_id => {}
            Entry::Occupied(o) => {
                return Err(Error::Input(format!(
                    "double-booking: room {:?} at time-block {:?}: lessons {:?} and {:?}",
                    p.room_id,
                    p.time_block_id,
                    o.get(),
                    p.lesson_id
                )));
            }
        }
        rows_by_lesson
            .entry(p.lesson_id)
            .or_default()
            .push((tb.day_of_week, tb.position, p.room_id));
    }

    Ok(())
}
```

- [ ] **Step 6: Verify the three double-booking tests now pass (GREEN)**

Run:
```bash
mise run test:rust -- -p solver-core --lib validate::tests::validate_no_double_booking
```

Expected: 4 tests pass (happy path + 3 double-booking failures). The cardinality and block-shape tests do not exist yet.

- [ ] **Step 7: Write the cardinality test for the cross-class shape**

Append to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn validate_no_double_booking_rejects_class_double_booking_via_cross_class_lesson() {
    let mut p = minimal_problem();
    let class1 = p.school_classes[0].id;
    let class2 = SchoolClass {
        id: SchoolClassId(uuid(50)),
        home_room_id: None,
        max_lessons_per_day: None,
    };
    p.school_classes.push(class2.clone());
    p.lessons.push(Lesson {
        id: LessonId(uuid(51)),
        school_class_ids: vec![class1, class2.id],
        subject_id: p.subjects[0].id,
        teacher_id: p.teachers[0].id,
        hours_per_week: 1,
        preferred_block_size: 1,
        lesson_group_id: None,
    });
    p.lessons[0].hours_per_week = 1;
    let placements = vec![
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[1].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
    ];
    let err = validate_no_double_booking(&p, &placements).unwrap_err();
    let msg = match err {
        Error::Input(m) => m,
        _ => panic!("expected Error::Input"),
    };
    assert!(msg.contains("double-booking: class"), "msg: {msg}");
    assert!(msg.contains(&format!("{:?}", class1)), "msg: {msg}");
}
```

- [ ] **Step 8: Verify the cross-class test passes against the existing implementation (no new code needed)**

Run:
```bash
mise run test:rust -- -p solver-core --lib validate::tests::validate_no_double_booking_rejects_class_double_booking_via_cross_class_lesson
```

Expected: PASS. The cross-class lesson's iteration over `lesson.school_class_ids` already exercises the per-class entry insert; the second placement (single-class on `class1`) collides with the cross-class lesson's `class1` entry and triggers the existing `double-booking: class` error.

- [ ] **Step 9: Write the cardinality tests (too few + too many)**

Append:

```rust
#[test]
fn validate_no_double_booking_rejects_lesson_cardinality_too_few() {
    let mut p = minimal_problem();
    p.lessons[0].hours_per_week = 2;
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(uuid(60)),
        day_of_week: 0,
        position: 1,
    });
    let placements = vec![Placement {
        lesson_id: p.lessons[0].id,
        time_block_id: p.time_blocks[0].id,
        room_id: p.rooms[0].id,
    }];
    let err = validate_no_double_booking(&p, &placements).unwrap_err();
    let msg = match err {
        Error::Input(m) => m,
        _ => panic!("expected Error::Input"),
    };
    assert!(msg.contains("lesson cardinality"), "msg: {msg}");
    assert!(msg.contains("expected 2"), "msg: {msg}");
}

#[test]
fn validate_no_double_booking_rejects_lesson_cardinality_too_many() {
    let mut p = minimal_problem();
    p.lessons[0].hours_per_week = 2;
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(uuid(70)),
        day_of_week: 0,
        position: 1,
    });
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(uuid(71)),
        day_of_week: 1,
        position: 0,
    });
    let placements = vec![
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[1].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[2].id,
            room_id: p.rooms[0].id,
        },
    ];
    let err = validate_no_double_booking(&p, &placements).unwrap_err();
    let msg = match err {
        Error::Input(m) => m,
        _ => panic!("expected Error::Input"),
    };
    assert!(msg.contains("lesson cardinality"), "msg: {msg}");
    assert!(msg.contains("expected 2"), "msg: {msg}");
}
```

- [ ] **Step 10: Verify both tests fail (RED)**

Run:
```bash
mise run test:rust -- -p solver-core --lib validate::tests::validate_no_double_booking_rejects_lesson_cardinality_too_few validate::tests::validate_no_double_booking_rejects_lesson_cardinality_too_many
```

Expected: both FAIL because the validator's body returns `Ok(())` after the first walk; no cardinality check yet.

- [ ] **Step 11: Add the cardinality check (extend the validator body)**

In `validate_no_double_booking`, replace the trailing `Ok(())` with:

```rust
    for (lesson_id, rows) in &rows_by_lesson {
        let lesson = lesson_by_id[lesson_id];
        if rows.len() != lesson.hours_per_week as usize {
            return Err(Error::Input(format!(
                "lesson cardinality: lesson {:?} has {} placements, expected {}",
                lesson_id,
                rows.len(),
                lesson.hours_per_week
            )));
        }
    }

    Ok(())
}
```

- [ ] **Step 12: Verify all 6 tests now pass (GREEN)**

Run:
```bash
mise run test:rust -- -p solver-core --lib validate::tests::validate_no_double_booking
```

Expected: 6 tests pass (happy + 4 dup variants + 2 cardinality).

- [ ] **Step 13: Write the three block-shape tests**

Append:

```rust
#[test]
fn validate_no_double_booking_rejects_block_shape_non_contiguous() {
    let mut p = minimal_problem();
    p.lessons[0].hours_per_week = 2;
    p.lessons[0].preferred_block_size = 2;
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(uuid(80)),
        day_of_week: 0,
        position: 2,
    });
    let placements = vec![
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[1].id,
            room_id: p.rooms[0].id,
        },
    ];
    let err = validate_no_double_booking(&p, &placements).unwrap_err();
    let msg = match err {
        Error::Input(m) => m,
        _ => panic!("expected Error::Input"),
    };
    assert!(msg.contains("block shape"), "msg: {msg}");
    assert!(msg.contains("contiguous run of length 2"), "msg: {msg}");
}

#[test]
fn validate_no_double_booking_rejects_block_shape_split_across_rooms() {
    let mut p = minimal_problem();
    p.lessons[0].hours_per_week = 2;
    p.lessons[0].preferred_block_size = 2;
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(uuid(90)),
        day_of_week: 0,
        position: 1,
    });
    p.rooms.push(Room {
        id: RoomId(uuid(91)),
    });
    let placements = vec![
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[1].id,
            room_id: p.rooms[1].id,
        },
    ];
    let err = validate_no_double_booking(&p, &placements).unwrap_err();
    let msg = match err {
        Error::Input(m) => m,
        _ => panic!("expected Error::Input"),
    };
    assert!(msg.contains("block shape"), "msg: {msg}");
    assert!(msg.contains("one room per block"), "msg: {msg}");
}

#[test]
fn validate_no_double_booking_rejects_block_shape_orphan_row() {
    let mut p = minimal_problem();
    p.lessons[0].hours_per_week = 2;
    p.lessons[0].preferred_block_size = 2;
    p.time_blocks.push(TimeBlock {
        id: TimeBlockId(uuid(100)),
        day_of_week: 1,
        position: 0,
    });
    let placements = vec![
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
        },
        Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[1].id,
            room_id: p.rooms[0].id,
        },
    ];
    let err = validate_no_double_booking(&p, &placements).unwrap_err();
    let msg = match err {
        Error::Input(m) => m,
        _ => panic!("expected Error::Input"),
    };
    assert!(msg.contains("block shape"), "msg: {msg}");
    assert!(msg.contains("multiple of 2"), "msg: {msg}");
}
```

- [ ] **Step 14: Verify all three block-shape tests fail (RED)**

Run:
```bash
mise run test:rust -- -p solver-core --lib validate::tests::validate_no_double_booking_rejects_block_shape
```

Expected: all three FAIL because the cardinality check passes (each lesson has exactly 2 placements) but no block-shape check exists yet.

- [ ] **Step 15: Add the block-shape check**

In `validate_no_double_booking`, replace the cardinality loop body with:

```rust
    for (lesson_id, mut rows) in rows_by_lesson {
        let lesson = lesson_by_id[&lesson_id];
        if rows.len() != lesson.hours_per_week as usize {
            return Err(Error::Input(format!(
                "lesson cardinality: lesson {:?} has {} placements, expected {}",
                lesson_id,
                rows.len(),
                lesson.hours_per_week
            )));
        }
        rows.sort_unstable_by_key(|(day, pos, _)| (*day, *pos));
        let n = lesson.preferred_block_size as usize;
        let mut day_groups: HashMap<u8, Vec<(u8, RoomId)>> = HashMap::new();
        for (day, pos, room) in rows {
            day_groups.entry(day).or_default().push((pos, room));
        }
        for (day, day_rows) in day_groups {
            if day_rows.len() % n != 0 {
                return Err(Error::Input(format!(
                    "block shape: lesson {:?} on day {} has {} rows, expected multiple of {}",
                    lesson_id,
                    day,
                    day_rows.len(),
                    n
                )));
            }
            for chunk in day_rows.chunks(n) {
                let first_pos = chunk[0].0;
                let first_room = chunk[0].1;
                for (i, (pos, _)) in chunk.iter().enumerate() {
                    if *pos != first_pos + i as u8 {
                        return Err(Error::Input(format!(
                            "block shape: lesson {:?} on day {} has positions {:?}, expected contiguous run of length {}",
                            lesson_id,
                            day,
                            chunk.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
                            n
                        )));
                    }
                }
                for (_, room) in chunk.iter() {
                    if *room != first_room {
                        return Err(Error::Input(format!(
                            "block shape: lesson {:?} on day {} has rooms {:?}, expected one room per block",
                            lesson_id,
                            day,
                            chunk.iter().map(|(_, r)| *r).collect::<Vec<_>>()
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 16: Verify all 10 tests pass (GREEN)**

Run:
```bash
mise run test:rust -- -p solver-core --lib validate::tests::validate_no_double_booking
```

Expected: 10 tests pass. If any fail, debug the validator body using the failure message; common pitfalls: HashMap iteration order means the orphan-row test may report `multiple of 2` for either day depending on iteration order (the test should pass regardless because both days have 1 row).

- [ ] **Step 17: Wire the Result-form call after `validate_no_room_hopping` in `solve.rs`**

Modify `solver/solver-core/src/solve.rs` around line 233. Locate the existing block:

```rust
    // Post-solve hard-constraint sanity check. A failure here is a solver bug.
    validate_no_room_hopping(problem, &solution.placements)?;

    // Debug-only post-condition: daily caps (ADR 0033) are enforced as
    // legality pruning, so a violation here means the pruning has a hole.
    // Loud in dev/tests, free in release.
    #[cfg(debug_assertions)]
    if let Err(e) = validate_daily_caps(problem, &solution.placements) {
        panic!("daily-cap post-condition violated: {e}");
    }
```

Insert the new validator call between `validate_no_room_hopping` and the cfg-panic:

```rust
    // Post-solve hard-constraint sanity check. A failure here is a solver bug.
    validate_no_room_hopping(problem, &solution.placements)?;
    validate_no_double_booking(problem, &solution.placements)?;

    // Debug-only post-condition: daily caps (ADR 0033) are enforced as
    // legality pruning, so a violation here means the pruning has a hole.
    // Loud in dev/tests, free in release.
    #[cfg(debug_assertions)]
    if let Err(e) = validate_daily_caps(problem, &solution.placements) {
        panic!("daily-cap post-condition violated: {e}");
    }
    #[cfg(debug_assertions)]
    if let Err(e) = validate_no_double_booking(problem, &solution.placements) {
        panic!("no-double-booking post-condition violated: {e}");
    }
```

Also extend the `use` statement at the top of `solve.rs`:

```rust
use crate::validate::{
    pre_solve_violations, validate_daily_caps, validate_no_double_booking,
    validate_no_room_hopping, validate_structural,
};
```

- [ ] **Step 18: Run the full rust test suite to surface any existing-test failure**

Run:
```bash
mise run test:rust 2>&1 | tee /tmp/task4-test-output.log
```

Expected: all tests pass. If `caps_kempe_solve_under_production_caps_smoke` (or any other test) panics with `no-double-booking post-condition violated: ...`, that confirms Q4 branch 2 from the spec: the Kempe `used_*` insert bug fires on Doppelstunden.

Three branches:
1. **All green:** proceed to Step 19 (commit Task 4 cleanly). File the Kempe-fix as an OPEN_THINGS follow-up in step 6 of the autopilot.
2. **One or more tests panic on `no-double-booking post-condition violated:`:** stop here, do NOT commit Task 4 yet. Skip to Task 4b, land the Kempe contains-check fix as commit 4b, then return here for Step 19.
3. **A test panics on `no-double-booking post-condition violated:` from a property test or other unexpected location:** STOP autopilot. Surface to the user; the bug shape differs from item 39's prediction and needs investigation.

- [ ] **Step 19: Commit Task 4 (only if Step 18 reported all green or after Task 4b lands)**

Run:
```bash
git add solver/solver-core/src/validate.rs solver/solver-core/src/solve.rs
git commit -m "$(cat <<'EOF'
feat(solver-core): validate_no_double_booking post-condition (sprint 5 item 39)

Adds `validate_no_double_booking(problem, placements) -> Result<(), Error>`
in `solver-core/src/validate.rs`: walks `placements` once, and asserts:

  - No `(class, time_block_id)` pair has two different lessons.
  - No `(teacher, time_block_id)` pair has two different lessons.
  - No `(room, time_block_id)` pair has two different lessons.
  - Each lesson has exactly `hours_per_week` placements.
  - Each block is `preferred_block_size` contiguous positions on one day
    in one room.

Failure messages prefixed `double-booking:`, `lesson cardinality:`, or
`block shape:` so debug-mode panic messages discriminate which check
fired without parsing.

Wires the validator into `solve_with_config`:

  - Release-mode `Result`-form call right after `validate_no_room_hopping`
    so production paths surface failures as `Error::Input`.
  - `#[cfg(debug_assertions)]` panic alongside `validate_daily_caps` so
    property and integration tests fail loudly on a silent hard violation.

Adds 10 inline unit tests covering the happy path and every failure
shape: class double-booking (direct + via cross-class lesson), teacher
double-booking, room double-booking, lesson cardinality (too few + too
many), block shape (non-contiguous, split rooms, orphan row).

The validator catches silent hard violations regardless of which move
introduced them. Closes the diagnostic gap that lets `kempe_apply_block`
HashSet inserts double-book without a corresponding violation in the
soft score (`score::score_solution` deduplicates positions before
gap-counting). Foundation for trusting the bake-off numbers.
EOF
)"
```

Expected: lefthook pre-commit runs, lint green, commit lands.

---

## Task 4b (CONDITIONAL): Kempe `used_*` contains-check guards

**Run only if Task 4 Step 18 reported a panic on `no-double-booking post-condition violated:` from `caps_kempe_solve_under_production_caps_smoke` or another test.**

**Files:**
- Modify: `solver/solver-core/src/lahc.rs` (`kempe_attempt` chain construction or `kempe_apply_block` apply-time guard)
- Create: `solver/solver-core/tests/kempe_self_overlap.rs` (regression test)

The fix shape depends on whether the bug originates at chain construction or chain apply. Investigate first via the panic message and the trace; the chain construction in `kempe_attempt` is the canonical fix site because it can pre-check for window self-overlap and abort the chain before any apply.

- [ ] **Step 1: Capture the panic location**

Run the failing test under `RUST_BACKTRACE=1`:
```bash
RUST_BACKTRACE=1 mise run test:rust -- -p solver-core --test daily_caps caps_kempe_solve_under_production_caps_smoke 2>&1 | tail -40
```

Expected output: panic message includes the lesson and time-block of the double-booking; backtrace shows the call stack from `solve_with_config`.

- [ ] **Step 2: Write a focused regression test**

Create `solver/solver-core/tests/kempe_self_overlap.rs` with a hand-crafted problem (two lessons sharing a class, both with `preferred_block_size=2`) that deterministically triggers the bug under a specific seed when `lahc_kempe_period` is enabled. The test asserts `validate_no_double_booking(&problem, &solution.placements).is_ok()`.

- [ ] **Step 3: Run the test to confirm RED**

```bash
mise run test:rust -- -p solver-core --test kempe_self_overlap
```

Expected: panic on the cfg-debug-assertion at `solve.rs` (the validator's panic path).

- [ ] **Step 4: Add the contains-check guard in `kempe_apply_block` or chain construction**

Decide between two fix shapes based on Step 1's investigation:
- **Apply-time guard:** wrap each `state.used_*.insert(...)` in `kempe_apply_block` with a `if state.used_*.contains(&...) { return Err(KempeApplyError::Conflict); }` check, propagate through the call chain to `kempe_attempt` which rolls back.
- **Construction-time guard:** in `kempe_attempt`'s BFS expansion, track every `(time_block_id)` already claimed by an in-progress chain member and refuse to add a neighbour whose destination window collides.

Construction-time is preferred (the bug is a chain-shape bug, fixed where the chain shape is decided). Implement the relevant variant in `kempe_attempt`'s chain expansion loop.

- [ ] **Step 5: Run the regression test to confirm GREEN**

```bash
mise run test:rust -- -p solver-core --test kempe_self_overlap
mise run test:rust -- -p solver-core --test daily_caps
```

Expected: both pass.

- [ ] **Step 6: Run the full rust suite to confirm no regressions**

```bash
mise run test:rust
```

Expected: all green.

- [ ] **Step 7: Commit (place the commit between Task 4 Steps 18 and 19, so the validator commit captures the green state)**

```bash
git add solver/solver-core/src/lahc.rs solver/solver-core/tests/kempe_self_overlap.rs
git commit -m "$(cat <<'EOF'
fix(solver-core): kempe contains-check guards in apply_block

`kempe_apply_block` blindly inserts into `state.used_teacher`,
`state.used_class`, and `state.used_room` HashSets without checking for
collisions. A chain whose destination windows overlap (e.g., two
neighbours offset within the seed's window applied at the seed's
start position with `preferred_block_size >= 2`) silently double-books
the colliding `(class, time_block_id)`, `(teacher, time_block_id)`, or
`(room, time_block_id)` pair. The soft scorer's per-`(class, day)`
deduplication masks the violation in the score, so the bug never
surfaced through any quality metric.

Detected by `validate_no_double_booking` (item 39) firing during
[the affected test name].

Aborts the chain at construction time when a destination window would
collide with another in-progress member's window. The chain is
discarded and the move is rejected, mirroring the spirit of
`rr_collect_anchors`'s "one block per (lesson, day)" invariant on the
destination side.

Adds a targeted regression test at `solver-core/tests/kempe_self_overlap.rs`
that triggers the bug deterministically via a hand-crafted problem with
two `preferred_block_size=2` lessons sharing a class.
EOF
)"
```

Adjust the commit body to reflect the actual test name from Step 1.

---

## Task 5: Commit backend `collect_pinned_placements` cross-class pin scope rewrite (item 33)

**Files:**
- Modify (already in working tree): `backend/src/klassenzeit_backend/scheduling/solver_io.py` (`collect_pinned_placements` rewritten)

This commit picks up the working-tree solver_io.py change. Closes item 33 in OPEN_THINGS.

- [ ] **Step 1: Verify the python diff matches the spec's item 33 semantics**

Run:
```bash
git diff backend/src/klassenzeit_backend/scheduling/solver_io.py
```

Expected: the diff shows `collect_pinned_placements` rewritten so the SQL subquery now selects lessons whose membership lies OUTSIDE `exclude_class_ids` (using `notin_`), and the docstring is updated to describe the new contract: cross-class lessons are pinned whenever any sibling class would otherwise see drift; lessons whose membership lies entirely inside `exclude_class_ids` are dropped.

- [ ] **Step 2: Run the backend test suite**

Run:
```bash
mise run test:py -- backend/tests/scheduling/test_solver_io.py -v
```

Expected: all tests pass. If the test for `collect_pinned_placements` fails because it pinned the old semantics, update the test to reflect the new contract: a per-class re-solve should pin the cross-class lesson on every sibling class even when one of the lesson's classes is in `exclude_class_ids`. The test update lands in this commit, not as a follow-up.

- [ ] **Step 3: Stage and commit**

Run:
```bash
git add backend/src/klassenzeit_backend/scheduling/solver_io.py
# (and any test file modified in Step 2)
git commit -m "$(cat <<'EOF'
fix(backend): collect_pinned_placements pins cross-class lessons whenever any sibling unaffected (sprint 5 item 33)

`collect_pinned_placements` filtered out every ScheduledLesson belonging
to ANY class in `exclude_class_ids`, so cross-class lessons that touch
the focus class were dropped on a per-class re-solve. Sibling classes
then saw the cross-class lesson reappear at a different time-block,
producing `pinned_conflict` violations on the second per-class solve
(52 such violations observed on the dev zweizuegig fixture).

Rewrites the SQL subquery to select lessons whose membership lies OUTSIDE
`exclude_class_ids`. Cross-class lessons are now pinned whenever any
sibling class would otherwise see drift; lessons whose membership lies
entirely inside `exclude_class_ids` are dropped (the focus class re-places
them). Single-class lessons in excluded classes are dropped as before.

Closes the item 33 design ambiguity by codifying "schedule one class
without disturbing others" as: sibling schedules are immutable on
per-class re-solve, including cross-class lessons that touch the focus
class.
EOF
)"
```

Expected: lefthook pre-commit runs, lint green, commit lands.

---

## Task 6: Commit frontend `app.css` `@import` reorder

**Files:**
- Modify (already in working tree): `frontend/src/styles/app.css` (Quicksand `@import` precedes tailwindcss)

Trivial CSS reorder; ships standalone for clean review history.

- [ ] **Step 1: Verify the diff is one-line trivial**

Run:
```bash
git diff frontend/src/styles/app.css
```

Expected: one `+` line and one `-` line; the Quicksand Google Fonts `@import` moves above the Tailwind `@import "tailwindcss";` line.

- [ ] **Step 2: Run frontend lint**

Run:
```bash
mise run lint -- fe:lint
```

Expected: green.

- [ ] **Step 3: Stage and commit**

Run:
```bash
git add frontend/src/styles/app.css
git commit -m "style(frontend): order Google Fonts @import before tailwindcss"
```

Expected: lefthook pre-commit runs, lint green, commit lands.

---

## Task 7: Final cross-suite verification

After all six (or seven, including 4b) commits land, run the full lint + test sweep one more time to confirm the bundled change set is healthy as a whole.

- [ ] **Step 1: Full lint sweep**

Run:
```bash
mise run lint
```

Expected: green. If a lint fails (e.g., a unique-fn-name collision the per-commit hooks missed), fix in a `chore(...)` commit on the same branch, then re-run.

- [ ] **Step 2: Full Rust test sweep**

Run:
```bash
mise run test:rust
```

Expected: green. Includes the new validator's 10 unit tests + every existing test.

- [ ] **Step 3: Full Python test sweep**

Run:
```bash
mise run test:py
```

Expected: green. Includes the updated `test_solver_io.py` regression for the new `collect_pinned_placements` semantics.

- [ ] **Step 4: Full frontend test sweep**

Run:
```bash
mise run fe:test
```

Expected: green.

- [ ] **Step 5: Confirm working tree clean**

Run:
```bash
git status
git log --oneline -10
```

Expected: working tree clean, recent commits show the six (or seven) typed commits in order: `docs:` (spec, already landed), `feat(solver-core): validate_daily_caps`, `fix(solver-core): drop class_day_balance...`, `refactor(solver-core): caller computes kempe...`, [`fix(solver-core): kempe contains-check guards in apply_block` if Task 4b ran], `feat(solver-core): validate_no_double_booking`, `fix(backend): collect_pinned_placements pins cross-class...`, `style(frontend): order Google Fonts @import...`.

---

## Self-review

**Spec coverage:**
- `validate_no_double_booking` function with 5 assertions: Task 4 Steps 5, 11, 15.
- Result-form wiring at solve.rs after `validate_no_room_hopping`: Task 4 Step 17.
- `#[cfg(debug_assertions)]` panic alongside `validate_daily_caps`: Task 4 Step 17.
- 10 unit tests covering all failure shapes + happy path: Task 4 Steps 1, 3, 7, 9, 13.
- Bundled commit 1 (daily-caps validator): Task 1.
- Bundled commit 2 (lahc.rs slice fix): Task 2.
- Bundled commit 3 (lahc.rs Kempe refactor): Task 3.
- Bundled commit 5 (solver_io.py): Task 5.
- Bundled commit 6 (app.css): Task 6.
- Conditional Q4 branch 2 (Kempe contains-check fix): Task 4b.
- OPEN_THINGS strip + CLAUDE.md sentence: deferred to autopilot step 6 (documentation pass), not in this plan.

**Placeholder scan:** all step bodies contain executable commands or actual code. No "TBD", no "implement later". Task 4b's commit message has a `[the affected test name]` placeholder that the executor fills in from the panic output; this is intentional because the test name depends on which test panics.

**Type consistency:** the validator's parameter types (`&Problem`, `&[Placement]`) match the existing sibling validators. The `Vec<(u8, u8, RoomId)>` row tuple is internal-only. The function returns `Result<(), Error>` matching `validate_no_room_hopping` and `validate_daily_caps`. The `SchoolClass` struct literal in the cardinality tests includes `home_room_id: None` matching the type definition (verify against `solver-core/src/types.rs` if it errors during execution; the hand-crafted test problems only need fields the validator inspects).
