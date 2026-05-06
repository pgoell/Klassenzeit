# Mix `preferred_block_size` in `lahc_small_problem` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Widen `lahc_small_problem` in `solver/solver-core/tests/lahc_property.rs` to draw `preferred_block_size` from `[1u8, 2u8]` per problem and constrain `hours_per_week` to a multiple of it, so the Kempe chain code's multi-position window walk gets coverage in CI.

**Architecture:** Pure proptest-generator widening in one test file. No production solver-core changes (Path A). If the widened generator fires `validate_no_double_booking` (Path B), the spec calls for a fix-first commit followed by the test widening; this plan covers Path A and stops at the verification step before the test commit if Path B kicks in (a separate plan / decision is required for the fix).

**Tech Stack:** Rust 1.85, proptest 1.x, `cargo nextest`, mise tasks (`mise run test:rust`, `mise run lint`).

---

### Task 1: Widen the proptest input clause and lesson construction

**Files:**
- Modify: `solver/solver-core/tests/lahc_property.rs:91-174`

- [ ] **Step 1: Read the current generator to confirm baseline**

Read `solver/solver-core/tests/lahc_property.rs` lines 91-174. Confirm:
- The `prop_compose!` block has five inputs (`n_classes`, `n_teachers`, `n_rooms`, `n_days`, `slots_per_day`).
- The `Lesson` literal sets `hours_per_week: 2 + ((i as u8) % 3)` and `preferred_block_size: 1`.
- The comment block at lines 151-153 references the item 37 R&R rollback bug.

- [ ] **Step 2: Edit the generator**

Apply this exact edit. Old block (lines 91-158, full `prop_compose!`):

```rust
prop_compose! {
    fn lahc_small_problem()(
        n_classes in 1usize..=3,
        n_teachers in 1usize..=4,
        n_rooms in 1usize..=3,
        n_days in 1u8..=3,
        slots_per_day in 2u8..=5,
    ) -> Problem {
        let subject_a = SubjectId(lahc_id_from(1));
        let subjects = vec![Subject { id: subject_a, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0, prefer_late_period: 0, max_hours_per_day: 8 }];

        let teachers: Vec<Teacher> = (0..n_teachers)
            .map(|i| Teacher {
                id: TeacherId(lahc_id_from(1000 + i as u32)),
                max_hours_per_week: 40,
            })
            .collect();
        let teacher_qualifications: Vec<TeacherQualification> = teachers
            .iter()
            .map(|t| TeacherQualification {
                teacher_id: t.id,
                subject_id: subject_a,
            })
            .collect();

        let school_classes: Vec<SchoolClass> = (0..n_classes)
            .map(|i| SchoolClass {
                id: SchoolClassId(lahc_id_from(2000 + i as u32)),
                home_room_id: None,
                max_lessons_per_day: None,
            })
            .collect();

        let rooms: Vec<Room> = (0..n_rooms)
            .map(|i| Room {
                id: RoomId(lahc_id_from(3000 + i as u32)),
            })
            .collect();

        let mut time_blocks: Vec<TimeBlock> = Vec::new();
        let mut tb_idx = 0u32;
        for d in 0..n_days {
            for p in 0..slots_per_day {
                time_blocks.push(TimeBlock {
                    id: TimeBlockId(lahc_id_from(4000 + tb_idx)),
                    day_of_week: d,
                    position: p,
                });
                tb_idx += 1;
            }
        }

        let lessons: Vec<Lesson> = school_classes
            .iter()
            .enumerate()
            .map(|(i, sc)| Lesson {
                id: LessonId(lahc_id_from(5000 + i as u32)),
                school_class_ids: vec![sc.id],
                subject_id: subject_a,
                teacher_id: teachers[i % teachers.len()].id,
                // Vary hours so FFD spreads multi-block lessons across days; sprint item 37
                // rollback bug only fires on multi-block-across-days lessons (preferred_block_size=1
                // and hours_per_week>=3), the constant 2 hid it.
                hours_per_week: 2 + ((i as u8) % 3),
                preferred_block_size: 1,
                lesson_group_id: None,
            })
            .collect();
```

New block (replaces old block, same indentation, comments updated, lessons map adjusted):

```rust
prop_compose! {
    fn lahc_small_problem()(
        n_classes in 1usize..=3,
        n_teachers in 1usize..=4,
        n_rooms in 1usize..=3,
        n_days in 1u8..=3,
        slots_per_day in 2u8..=5,
        preferred_block_size in 1u8..=2u8,
    ) -> Problem {
        let subject_a = SubjectId(lahc_id_from(1));
        let subjects = vec![Subject { id: subject_a, prefer_early_period: 0, avoid_first_period: 0, avoid_last_period: 0, prefer_late_period: 0, max_hours_per_day: 8 }];

        let teachers: Vec<Teacher> = (0..n_teachers)
            .map(|i| Teacher {
                id: TeacherId(lahc_id_from(1000 + i as u32)),
                max_hours_per_week: 40,
            })
            .collect();
        let teacher_qualifications: Vec<TeacherQualification> = teachers
            .iter()
            .map(|t| TeacherQualification {
                teacher_id: t.id,
                subject_id: subject_a,
            })
            .collect();

        let school_classes: Vec<SchoolClass> = (0..n_classes)
            .map(|i| SchoolClass {
                id: SchoolClassId(lahc_id_from(2000 + i as u32)),
                home_room_id: None,
                max_lessons_per_day: None,
            })
            .collect();

        let rooms: Vec<Room> = (0..n_rooms)
            .map(|i| Room {
                id: RoomId(lahc_id_from(3000 + i as u32)),
            })
            .collect();

        let mut time_blocks: Vec<TimeBlock> = Vec::new();
        let mut tb_idx = 0u32;
        for d in 0..n_days {
            for p in 0..slots_per_day {
                time_blocks.push(TimeBlock {
                    id: TimeBlockId(lahc_id_from(4000 + tb_idx)),
                    day_of_week: d,
                    position: p,
                });
                tb_idx += 1;
            }
        }

        let lessons: Vec<Lesson> = school_classes
            .iter()
            .enumerate()
            .map(|(i, sc)| {
                // Vary hours so FFD spreads multi-block lessons across days; sprint item 37
                // rollback bug only fires on multi-block-across-days lessons
                // (preferred_block_size=1 and hours_per_week>=3), the constant 2 hid it.
                // Sprint item 40 widens the generator to draw preferred_block_size from
                // {1, 2} per problem so the Kempe chain code's multi-position window walk
                // gets coverage; hours_per_week stays a multiple of the drawn block size
                // so validate_structural never rejects the generated Problem.
                let hours = if preferred_block_size == 2 {
                    2u8 + 2 * ((i as u8) % 2)
                } else {
                    2u8 + ((i as u8) % 3)
                };
                Lesson {
                    id: LessonId(lahc_id_from(5000 + i as u32)),
                    school_class_ids: vec![sc.id],
                    subject_id: subject_a,
                    teacher_id: teachers[i % teachers.len()].id,
                    hours_per_week: hours,
                    preferred_block_size,
                    lesson_group_id: None,
                }
            })
            .collect();
```

The closing of the `prop_compose!` (lines 160-173, the `Problem { ... }` literal and braces) stays untouched.

- [ ] **Step 3: Confirm the file still parses and the existing baseline tests still pass with `preferred_block_size: 1`-coverage byte-identical**

Run:

```bash
cargo nextest run -p solver-core --test lahc_property
```

Expected: all 14 tests pass at the default `cases: 32`. If a test fails at this step, do NOT commit; investigate before continuing (likely a transcription error in Step 2).

---

### Task 2: Local 5-seed sweep at `PROPTEST_CASES=128`

**Files:** None modified.

- [ ] **Step 1: Run the lahc_property file 5 times with varied seeds**

Run:

```bash
for s in 1 2 3 4 5; do
  echo "=== seed=$s ==="
  PROPTEST_CASES=128 PROPTEST_SEED=$s \
    cargo nextest run -p solver-core --test lahc_property
done
```

Expected for Path A: every iteration ends with `Summary [ X.YZs] 14 tests run: 14 passed`.

Expected for Path B: at least one iteration fails with one of:
- `panicked at .../validate.rs ... validate_no_double_booking`
- `panicked at .../validate.rs ... validate_no_room_hopping` or `validate_daily_caps`
- `prop_assert!(lahc_rr_kempe.placements.len() >= greedy.placements.len())` failure
- `prop_assert_eq!(lahc.soft_score, recomputed)` failure

If any iteration fails, STOP this plan and decide whether to switch to Path B (see the spec's "Validation" section). The remaining tasks below assume Path A succeeded.

---

### Task 3: Confirm the wider workspace stays green

**Files:** None modified.

- [ ] **Step 1: Run the full Rust suite**

Run:

```bash
mise run test:rust
```

Expected: `Summary [ ... ] N tests run: N passed`. The lahc_property changes touch only that test file; if any other crate's tests fail, investigate (most likely a generator-shared helper drift; unlikely given the change scope).

- [ ] **Step 2: Run all linters**

Run:

```bash
mise run lint
```

Expected: every linter exits 0. The change adds one line to a `prop_compose!` block and adjusts the lesson-construction closure; `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo machete` all stay clean.

If `cargo fmt --check` complains, run `mise run fmt` and re-run `mise run lint`.

---

### Task 4: Commit Task 1's edit

**Files:** None additional.

- [ ] **Step 1: Stage and commit**

Run:

```bash
git add solver/solver-core/tests/lahc_property.rs
git commit -m "test(solver-core): mix preferred_block_size in lahc_small_problem (item 40)"
```

Expected: pre-commit hook (`lefthook` running `mise run lint`) passes; commit-msg hook (`cog`) accepts the Conventional Commits message.

---

### Task 5: Delete OPEN_THINGS item 40 and advance the next-pickup line

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md` (delete item 40 stanza; update active-sprint preamble)

- [ ] **Step 1: Read the active-sprint preamble and the item 40 block**

Read `docs/superpowers/OPEN_THINGS.md` lines 7-17 (preamble + item 40 paragraph). Confirm the current "Next pickup" line names item 40 and the item 40 stanza is the first numbered item under `### Correctness phase`.

- [ ] **Step 2: Delete the item 40 stanza**

Edit `docs/superpowers/OPEN_THINGS.md` to remove the entire numbered `40.` paragraph (one paragraph block from `40. **Mix ...` through the end of that paragraph, preserving the blank line above the next item `41.`).

- [ ] **Step 3: Update the active-sprint preamble**

In the same file, update the "Next pickup" sentence in the active-sprint preamble to read:

> Next pickup: P0 item 41 (reconcile `solution.soft_score` with the full weighted cost; bake-off cells will compare on the same objective once this lands). Item 40 (mix `preferred_block_size` in `lahc_small_problem`) shipped this PR; the Kempe chain code's multi-position window walk now has CI coverage via the widened proptest generator, with `validate_no_double_booking`'s `cfg(debug_assertions)` panic catching any future regression on every property-test invocation.

Keep the rest of the preamble paragraph intact (the bit about item 39 / the religion-trio exemption).

- [ ] **Step 4: Verify the file still has the `### Observability phase`, `### Test realism phase`, `### Backend tidy phase` structure intact**

Run:

```bash
grep -n "^### \|^## " docs/superpowers/OPEN_THINGS.md | head -20
```

Expected: the `## Active sprint program: ...`, `### Correctness phase`, `### Observability phase`, `### Test realism phase`, `### Backend tidy phase` headers are all present and in this order.

- [ ] **Step 5: Commit the OPEN_THINGS update**

Run:

```bash
git add docs/superpowers/OPEN_THINGS.md
git commit -m "docs: close OPEN_THINGS item 40 (lahc_property block-size mix)"
```

Expected: lefthook + cog hooks pass.

---

### Task 6: Refresh auto-memory roadmap status

**Files:**
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`

- [ ] **Step 1: Read the current roadmap-status memory**

Read `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`. The current body says: "Active sprint 'Solver feasibility correctness + observability'. Items 26-29, 37, 33, 39 shipped + ADR 0032/0033. Next pickup is item 40 (mix preferred_block_size in lahc_small_problem). Beyond-Grundschule Sprint 1 paused until correctness sprint closes."

- [ ] **Step 2: Update the body**

Edit the body to read: "Active sprint 'Solver feasibility correctness + observability'. Items 26-29, 37, 33, 39, 40 shipped + ADR 0032/0033. Next pickup is item 41 (reconcile `solution.soft_score` with the full weighted cost). Beyond-Grundschule Sprint 1 paused until correctness sprint closes."

(The frontmatter block at the top stays unchanged. The `MEMORY.md` index entry's hook says "Next pickup is item 40"; refresh that one-line hook in `MEMORY.md` too so the index stays in sync with the memory body.)

- [ ] **Step 3: Read the MEMORY.md index entry**

Read `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/MEMORY.md`. Find the line: `- [Roadmap status](project_roadmap_status.md): ... Next pickup is item 40 ... `.

- [ ] **Step 4: Update the index hook**

Edit the same one-line entry in `MEMORY.md` to swap "item 40" for "item 41" and append "40" to the shipped-items list. Keep the line under ~150 characters.

Auto-memory files are not under git; no commit step.

---

### Task 7: Pre-push verification

**Files:** None modified.

- [ ] **Step 1: Confirm the branch state matches expectations**

Run:

```bash
git log --oneline master..HEAD
```

Expected (Path A, in this order from oldest to newest):

```
<sha> docs: close OPEN_THINGS item 40 (lahc_property block-size mix)
<sha> test(solver-core): mix preferred_block_size in lahc_small_problem (item 40)
<sha> docs: add lahc_property block-size mix design spec (item 40)
```

(Three commits total: the spec from Step 3 of the autopilot run, the test edit, the OPEN_THINGS update. Plan commit lands in step 4 of the autopilot run before this plan executes; the implementation plan itself is committed separately by the autopilot orchestrator.)

- [ ] **Step 2: Re-run the lahc_property file at default cases as a smoke test**

Run:

```bash
cargo nextest run -p solver-core --test lahc_property
```

Expected: 14 tests pass.

This step does NOT replace the 5-seed sweep from Task 2; it confirms HEAD is still green after the OPEN_THINGS commit.

---

## Notes for the executor

- **If Task 2 fires Path B:** STOP. Surface the failure mode (panic message, test name, seed) to the orchestrator. Do not attempt the Path B fix from this plan; the fix needs its own brainstorm + plan (per CLAUDE.md "structural change and behavioral change never ship in the same commit", and the spec's commit-order decision in Q6).
- **If Task 3's `mise run lint` flags a clippy regression:** the only added code is a constant arithmetic expression and a single-field struct change. Run `mise run fmt` first; if a real clippy warning remains, paste it into the orchestrator transcript before fixing.
- **`mise run bench` is intentionally NOT in this plan.** No production code changes; criterion delta is structurally zero.
