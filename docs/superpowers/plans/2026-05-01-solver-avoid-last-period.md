# Avoid-last-period Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a sixth soft-constraint axis (`avoid_last_period`) to the solver, end-to-end across solver-core, backend, frontend, and demo seeds. Symmetric to existing `avoid_first_period`.

**Architecture:** New `Subject.avoid_last_period: bool` column (additive Alembic migration, default false) and new `ConstraintWeights.avoid_last_period: u32` field (active default 1) drive a per-placement score that fires when `tb.position == max_position_for_day` for the placement's `day_of_week`. `subject_preference_score` gains a `max_position_for_day: u8` parameter; callers (`score_solution`, `solve_with_config`'s greedy, `lahc::run`) build a `HashMap<u8, u8>` once per solve and pass the per-placement value. Demo seeds mark Mathematik and Deutsch as avoid-last. Frontend adds a third checkbox to the subject edit dialog.

**Tech Stack:** Rust (solver-core, solver-py via maturin), Python (FastAPI + SQLAlchemy + Alembic + Pydantic), TypeScript (React 19 + TanStack Query + RHF + Zod + react-i18next).

**Spec:** `docs/superpowers/specs/2026-05-01-solver-avoid-last-period-design.md`.

---

## File Structure

**Created:**

- `backend/alembic/versions/<rev>_add_subject_avoid_last_period.py` — additive boolean column.
- `docs/adr/0024-avoid-last-period.md` — decision record.

**Modified, solver-core (Rust):**

- `solver/solver-core/src/types.rs` — `Subject.avoid_last_period: bool`, `ConstraintWeights.avoid_last_period: u32` (with rustdoc).
- `solver/solver-core/src/score.rs` — `subject_preference_score` signature, `score_solution` builds `max_position_per_day`, all literal `Subject { ... }` and `ConstraintWeights { ... }` test fixtures threaded.
- `solver/solver-core/src/solve.rs` — greedy callers at lines 338 and 617 plus surrounding scope thread `max_position_per_day`; active default `avoid_last_period: 1` at line 31; literal test fixtures (5 sites: lines 748, 812, 855, 886, 986) threaded.
- `solver/solver-core/src/json.rs` — active default `avoid_last_period: 1` at line 37; literal Subject test fixture at line 107 threaded.
- `solver/solver-core/src/lahc.rs` — `run()` builds `max_position_per_day` once after the existing lookups; threads through `try_change_move` to the `subject_preference_score` calls at lines 163-164; all literal test fixtures threaded.
- `solver/solver-core/src/validate.rs` — literal Subject fixtures threaded (lines 258-259, 439-440).
- `solver/solver-core/src/ordering.rs` — literal Subject fixtures threaded (lines 101-102, 106-107).
- `solver/solver-core/tests/ffd_solver_outcome.rs` — fixtures threaded (lines 52-53, 57-58); ConstraintWeights at line 136 gets `avoid_last_period: 0`.
- `solver/solver-core/tests/grundschule_smoke.rs` — fixture threaded (lines 56-57).
- `solver/solver-core/tests/score_property.rs` — fixtures threaded (lines 46-47, 144-145, 192-193).
- `solver/solver-core/tests/properties.rs` — fixture threaded (lines 186-187).
- `solver/solver-core/tests/lahc_property.rs` — single-line fixture threaded (line 38).
- `solver/solver-core/benches/solver_fixtures.rs` — three fixture rows; Mathematik (index 1) and Deutsch (index 0) get `avoid_last_period: true`, others `false`. Lines 89-90, 196-197, 363-364 are the existing analogous lines.

**Modified, solver-py (Python tests):**

- `solver/solver-py/tests/test_bindings.py` — JSON Subject dict at line 39 gets `"avoid_last_period": False`. Same for line 514 (or wherever similar dicts live).
- `solver/solver-py/tests/test_multi_class.py` — line 33 dict gets the field.

**Modified, backend (Python):**

- `backend/src/klassenzeit_backend/db/models/subject.py` — new `avoid_last_period: Mapped[bool]` column.
- `backend/src/klassenzeit_backend/scheduling/schemas/subject.py` — three new fields on `SubjectCreate`, `SubjectUpdate`, `SubjectResponse`.
- `backend/src/klassenzeit_backend/scheduling/routes/subjects.py` — six sites mirroring `avoid_first_period`.
- `backend/src/klassenzeit_backend/scheduling/solver_io.py` — emit `avoid_last_period` per Subject.
- `backend/tests/scheduling/test_subjects.py` — extend POST + PATCH cases.
- `backend/tests/scheduling/test_solver_io.py` — extend emission test.

**Modified, demo seeds (commit 4):**

- `backend/src/klassenzeit_backend/seed/demo_grundschule.py` — `_SubjectSpec.avoid_last_period: bool = False`; flip Deutsch + Mathematik to `True`; thread into `_create_subjects`.
- `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py` — thread `spec.avoid_last_period` into Subject construction (line 387 area).

**Modified, frontend (TypeScript, commit 3):**

- `frontend/src/features/subjects/schema.ts` — Zod field.
- `frontend/src/features/subjects/subjects-dialogs.tsx` — defaultValues + new FormField checkbox block.
- `frontend/src/i18n/locales/en.json` — `subjects.fields.avoidLastPeriod` with `label` + `help`.
- `frontend/src/i18n/locales/de.json` — same German keys.
- `frontend/src/lib/api-types.ts` — regenerated by `mise run fe:types`, committed verbatim.
- `frontend/tests/subjects-page.test.tsx` — extend the existing form spec to round-trip the new flag.

---

## Task 1: solver-core Rust algorithm change

**Owning subagent prompt summary:** Add `Subject.avoid_last_period: bool` and `ConstraintWeights.avoid_last_period: u32`; thread `max_position_for_day: u8` through `subject_preference_score` and every caller; refresh test/bench fixtures. Land helper plus first caller atomically per the solver-core rule. **Acceptance:** `mise run test:rust` passes, `mise run lint` passes (clippy warnings = errors), `mise run bench` runs without compile failure (numbers go in commit 4 if Q9 says so).

**Files:**

- Create: none in this task.
- Modify: `solver/solver-core/src/{types.rs, score.rs, solve.rs, json.rs, lahc.rs, validate.rs, ordering.rs}`, `solver/solver-core/tests/{ffd_solver_outcome.rs, grundschule_smoke.rs, score_property.rs, properties.rs, lahc_property.rs}`, `solver/solver-core/benches/solver_fixtures.rs`.
- Test: same files (inline `#[cfg(test)] mod tests`), plus `solver-core/tests/*.rs` integration tests.

### Step 1.1: Write a failing test for the new behavior in score.rs

Add this test to `solver/solver-core/src/score.rs` inside the existing `#[cfg(test)] mod tests` block, near `subject_preference_score_constant_at_position_zero_when_avoid_first_set`:

```rust
#[test]
fn subject_preference_score_constant_at_max_position_when_avoid_last_set() {
    let subject = Subject {
        id: SubjectId(score_uuid(40)),
        prefer_early_periods: false,
        avoid_first_period: false,
        avoid_last_period: true,
    };
    let weights = ConstraintWeights {
        avoid_last_period: 11,
        ..ConstraintWeights::default()
    };
    let tb_max = TimeBlock {
        id: TimeBlockId(score_uuid(11)),
        day_of_week: 0,
        position: 4,
    };
    let tb_non_max = TimeBlock {
        id: TimeBlockId(score_uuid(10)),
        day_of_week: 0,
        position: 3,
    };
    // max_position_for_day = 4
    assert_eq!(
        subject_preference_score(&subject, &tb_max, 4, &weights),
        11
    );
    assert_eq!(
        subject_preference_score(&subject, &tb_non_max, 4, &weights),
        0
    );
}
```

- [ ] **Step 1.1.1:** Add the test as written above.

- [ ] **Step 1.1.2: Run the test to verify a compile failure**

Run: `cargo nextest run -p solver-core subject_preference_score_constant_at_max_position`
Expected: compile error, `Subject` has no field `avoid_last_period`. The compile failure is the red.

### Step 1.2: Extend `Subject` and `ConstraintWeights` in types.rs

Modify `solver/solver-core/src/types.rs`:

```rust
pub struct Subject {
    pub id: SubjectId,
    pub prefer_early_periods: bool,
    pub avoid_first_period: bool,
    /// When true, scoring adds `weights.avoid_last_period` per placement of any
    /// lesson teaching this subject at `tb.position == max_position_for_day`,
    /// where the max is taken over all time-blocks sharing `tb.day_of_week`.
    /// Mirror of `avoid_first_period` at the other end of the day.
    #[serde(default)]
    pub avoid_last_period: bool,
}
```

```rust
pub struct ConstraintWeights {
    pub class_gap: u32,
    pub teacher_gap: u32,
    pub prefer_early_period: u32,
    pub avoid_first_period: u32,
    pub prefer_home_room: u32,
    /// Constant penalty per placement of an `avoid_last_period` subject at
    /// `tb.position == max_position_for_day` for that placement's
    /// `day_of_week`. Zero when the subject's flag is false, the weight is
    /// zero, or the placement is not at the day's max position.
    pub avoid_last_period: u32,
}
```

- [ ] **Step 1.2.1:** Add the new field on `Subject` with `#[serde(default)]` and the rustdoc comment.

- [ ] **Step 1.2.2:** Add the new field on `ConstraintWeights` with the rustdoc comment. `Default::default()` will produce `0`.

- [ ] **Step 1.2.3: Run cargo build to see the compiler enumerate every literal-construction site needing the new field**

Run: `cargo build -p solver-core --all-targets`
Expected: a list of E0063 errors, one per `Subject { ... }` and (if any leftover) `ConstraintWeights { ... }` literal in source. Use this list as the worklist for steps 1.4-1.6 below.

### Step 1.3: Update `subject_preference_score` signature and body

Modify the helper in `solver/solver-core/src/score.rs`:

```rust
/// Per-placement subject-preference score. Returns
/// `tb.position * weights.prefer_early_period` (linear) when the subject's
/// `prefer_early_periods` flag is set, plus `weights.avoid_first_period`
/// (binary) when the `avoid_first_period` flag is set and `tb.position == 0`,
/// plus `weights.avoid_last_period` (binary) when the `avoid_last_period`
/// flag is set and `tb.position == max_position_for_day`. Pure: depends only
/// on `subject`, `tb`, `max_position_for_day`, `weights`. Allocation-free.
pub(crate) fn subject_preference_score(
    subject: &crate::types::Subject,
    tb: &TimeBlock,
    max_position_for_day: u8,
    weights: &ConstraintWeights,
) -> u32 {
    let mut score = 0u32;
    if subject.prefer_early_periods {
        score = score
            .saturating_add(u32::from(tb.position).saturating_mul(weights.prefer_early_period));
    }
    if subject.avoid_first_period && tb.position == 0 {
        score = score.saturating_add(weights.avoid_first_period);
    }
    if subject.avoid_last_period && tb.position == max_position_for_day {
        score = score.saturating_add(weights.avoid_last_period);
    }
    score
}
```

Also extend the early-out in `score_solution` to include the new weight:

```rust
if weights.class_gap == 0
    && weights.teacher_gap == 0
    && weights.prefer_early_period == 0
    && weights.avoid_first_period == 0
    && weights.prefer_home_room == 0
    && weights.avoid_last_period == 0
{
    return 0;
}
```

And update `score_solution` to build `max_position_per_day` once and thread it:

```rust
let max_position_per_day: HashMap<u8, u8> = problem
    .time_blocks
    .iter()
    .fold(HashMap::new(), |mut acc, tb| {
        acc.entry(tb.day_of_week)
            .and_modify(|m| *m = (*m).max(tb.position))
            .or_insert(tb.position);
        acc
    });
```

Then change the per-placement subject-preference loop:

```rust
let subject_preference: u32 = placements
    .iter()
    .map(|p| {
        let lesson = lesson_lookup[&p.lesson_id];
        let subject = subject_lookup[&lesson.subject_id];
        let tb = tb_lookup[&p.time_block_id];
        let max_pos = max_position_per_day
            .get(&tb.day_of_week)
            .copied()
            .unwrap_or(tb.position);
        subject_preference_score(subject, tb, max_pos, weights)
    })
    .sum();
```

(The `unwrap_or(tb.position)` defensive default cannot fire in production because `tb` came from `problem.time_blocks`, which the fold visited; the defensive value still produces correct scoring if it ever did fire because position equals itself.)

- [ ] **Step 1.3.1:** Apply the signature change and body update to `subject_preference_score`.

- [ ] **Step 1.3.2:** Update `score_solution`'s early-out check.

- [ ] **Step 1.3.3:** Add the `max_position_per_day` HashMap construction near the existing `tb_lookup` / `lesson_lookup` block in `score_solution`.

- [ ] **Step 1.3.4:** Update the `subject_preference` map closure to look up `max_pos` and pass it.

### Step 1.4: Update solve.rs greedy callers

Modify `solver/solver-core/src/solve.rs`:

- At line 31, inside the active-default `ConstraintWeights { ... }` block in `SolveConfig::default()`, add `avoid_last_period: 1,`.

- Update the rustdoc on `solve()` (around line 21) to mention the sixth axis: change `prefer_early_period = avoid_first_period = 1` to `prefer_early_period = avoid_first_period = avoid_last_period = 1` (existing `prefer_home_room = 1` should already be in place; if not, add it too).

- Build `max_position_per_day` near the top of `solve_with_config` (alongside the existing `tb_order` / `room_order` precomputes) and thread it through to wherever `subject_preference_score` is called inside the greedy. The call sites are at lines 338 and 617 (inside `try_place_block`-style scopes that already see `tb` and `weights`). Each call site changes from:

  ```rust
  subject_pref = subject_pref
      .saturating_add(crate::score::subject_preference_score(subject, tb, weights));
  ```

  to:

  ```rust
  let max_pos = max_position_per_day
      .get(&tb.day_of_week)
      .copied()
      .unwrap_or(tb.position);
  subject_pref = subject_pref
      .saturating_add(crate::score::subject_preference_score(subject, tb, max_pos, weights));
  ```

  If both call sites live inside the same hot loop, hoist the `max_pos` lookup out of the inner `for k in 0..n_usize` loop when `tb.day_of_week` is invariant across the n-window (it is — the greedy only places windows on a single day). Same-day windows let you compute `max_pos` once before entering the inner loop. Choose the cleanest scope per site without over-refactoring; correctness first, hoisting only if the inner loop count is large enough to matter.

- Update the 5 literal `Subject { ... }` test fixtures at lines 748, 812, 855, 886, 986 to add `avoid_last_period: false,`.

- [ ] **Step 1.4.1:** Active default at line 31.

- [ ] **Step 1.4.2:** Build `max_position_per_day` in `solve_with_config`.

- [ ] **Step 1.4.3:** Thread the value into both `subject_preference_score` call sites (lines 338 and 617).

- [ ] **Step 1.4.4:** Update the 5 literal Subject test fixtures (lines 748, 812, 855, 886, 986) with `avoid_last_period: false,`.

- [ ] **Step 1.4.5:** Update the rustdoc at line 21 to enumerate the sixth axis.

### Step 1.5: Update lahc.rs Change-move delta path

Modify `solver/solver-core/src/lahc.rs` `run()` function (line 29 onwards). After the existing `subject_lookup` block (line 54-55), add:

```rust
let max_position_per_day: HashMap<u8, u8> = problem
    .time_blocks
    .iter()
    .fold(HashMap::new(), |mut acc, tb| {
        acc.entry(tb.day_of_week)
            .and_modify(|m| *m = (*m).max(tb.position))
            .or_insert(tb.position);
        acc
    });
```

Pass `&max_position_per_day` into `try_change_move` (add the parameter to its signature; the function already has `#[allow(clippy::too_many_arguments)]` so adding one more is fine).

Inside `try_change_move`, update the two `subject_preference_score` calls at lines 163-164 from:

```rust
let subject_pref_old = crate::score::subject_preference_score(subject, &old_tb, weights);
let subject_pref_new = crate::score::subject_preference_score(subject, &new_tb, weights);
```

to:

```rust
let old_max = max_position_per_day
    .get(&old_tb.day_of_week)
    .copied()
    .unwrap_or(old_tb.position);
let new_max = max_position_per_day
    .get(&new_tb.day_of_week)
    .copied()
    .unwrap_or(new_tb.position);
let subject_pref_old = crate::score::subject_preference_score(subject, &old_tb, old_max, weights);
let subject_pref_new = crate::score::subject_preference_score(subject, &new_tb, new_max, weights);
```

Update every literal `Subject { ... }` test fixture in the file's `mod tests` block to include `avoid_last_period: false,`.

- [ ] **Step 1.5.1:** Add `max_position_per_day` HashMap construction in `run()`.

- [ ] **Step 1.5.2:** Add the parameter to `try_change_move` and thread the call.

- [ ] **Step 1.5.3:** Update the two `subject_preference_score` invocations.

- [ ] **Step 1.5.4:** Update every literal `Subject { ... }` in the `mod tests` block.

### Step 1.6: Update remaining test/bench/json fixtures

For every E0063 reported by `cargo build -p solver-core --all-targets`, add `avoid_last_period: false,` to the literal Subject construction. Fixtures known in advance:

- `solver/solver-core/src/json.rs` line 107 (Subject literal in unit test); line 37 (`avoid_last_period: 1` in active-default ConstraintWeights block); update the rustdoc at line 27 to enumerate the sixth axis.
- `solver/solver-core/src/validate.rs` lines 258-259 and 439-440 (literal Subject fixtures).
- `solver/solver-core/src/ordering.rs` lines 101-102 and 106-107 (literal Subject fixtures).
- `solver/solver-core/src/score.rs` literal Subject test fixtures inside `mod tests` (lines 256-260, 426-431, 446-451, 470-475, 518-522, 605-613-area, 626-635-area, 648-660-area, 670-684-area, 695-712-area, 766-771-area). The exact line numbers will shift as you edit; rely on `cargo build` errors as ground truth.

Integration tests under `solver/solver-core/tests/`:

- `ffd_solver_outcome.rs` lines 52-53 and 57-58 (Subject literals); line 136 (ConstraintWeights, add `avoid_last_period: 0,` since the test's intent is "weights = avoid_first_period only").
- `grundschule_smoke.rs` lines 56-57.
- `score_property.rs` lines 46-47, 144-145, 192-193 (three Subject literals); also any ConstraintWeights literals in this file should keep `avoid_last_period: 0` unless the property test specifically exercises the new axis (none should, since the axes the file covers are pre-existing).
- `properties.rs` lines 186-187.
- `lahc_property.rs` line 38 (single-line Subject literal; preserve the existing one-line shape: `let subjects = vec![Subject { id: subject_a, prefer_early_periods: false, avoid_first_period: false, avoid_last_period: false }];`).

Bench fixtures in `solver/solver-core/benches/solver_fixtures.rs`:

- Three Subject construction loops at the lines indicated by the existing `avoid_first_period: i == 7` (or similar) entries (lines 89-90, 196-197, 363-364). For each, add a sibling line on the same `Subject { ... }` block:

  ```rust
  avoid_last_period: matches!(i, 0 | 1), // index 0 = Deutsch, 1 = Mathematik
  ```

  This mirrors the existing `prefer_early_periods` line; both axes flag the same two academic Hauptfaecher.

  Note: the dreizuegige fixture starts at line 363 and currently has `avoid_first_period: i == 9` (different index for Sport because the dreizuegige subject list is longer). Inspect each of the three blocks before flipping; the academic-Hauptfaecher index stays at `0 | 1` (Deutsch and Mathematik come first in every list) but verify before committing.

- [ ] **Step 1.6.1:** Update json.rs (line 107 fixture, line 37 active default, line 27 rustdoc).

- [ ] **Step 1.6.2:** Update validate.rs and ordering.rs literal Subject fixtures.

- [ ] **Step 1.6.3:** Update score.rs literal Subject fixtures (rely on cargo errors for exact lines).

- [ ] **Step 1.6.4:** Update integration tests in `solver-core/tests/*.rs`.

- [ ] **Step 1.6.5:** Update bench fixtures in `solver_fixtures.rs` for all three demo seeds (grundschule, zweizuegig, dreizuegig).

- [ ] **Step 1.6.6:** Run `cargo build -p solver-core --all-targets` and verify zero E0063 errors remain.

### Step 1.7: Add new behavior tests beyond Step 1.1's anchor

Add to `solver/solver-core/src/score.rs` `mod tests`:

```rust
#[test]
fn score_solution_includes_avoid_last_only_at_max_day_position() {
    // Two-day fixture: day 0 maxes at position 1, day 1 maxes at position 2.
    // The avoid-last-flagged subject placed at (day 0, pos 1), (day 0, pos 0),
    // (day 1, pos 2), (day 1, pos 1) fires the penalty exactly twice.
    let weights = ConstraintWeights {
        avoid_last_period: 3,
        ..ConstraintWeights::default()
    };
    let subject_id = SubjectId(score_uuid(40));
    let class_id = SchoolClassId(score_uuid(50));
    let teacher_id = TeacherId(score_uuid(20));
    let lesson_id = LessonId(score_uuid(60));
    let room_id = RoomId(score_uuid(30));
    let problem = Problem {
        time_blocks: vec![
            TimeBlock { id: TimeBlockId(score_uuid(10)), day_of_week: 0, position: 0 },
            TimeBlock { id: TimeBlockId(score_uuid(11)), day_of_week: 0, position: 1 },
            TimeBlock { id: TimeBlockId(score_uuid(12)), day_of_week: 1, position: 0 },
            TimeBlock { id: TimeBlockId(score_uuid(13)), day_of_week: 1, position: 1 },
            TimeBlock { id: TimeBlockId(score_uuid(14)), day_of_week: 1, position: 2 },
        ],
        teachers: vec![Teacher { id: teacher_id, max_hours_per_week: 10 }],
        rooms: vec![Room { id: room_id }],
        subjects: vec![Subject {
            id: subject_id,
            prefer_early_periods: false,
            avoid_first_period: false,
            avoid_last_period: true,
        }],
        school_classes: vec![SchoolClass { id: class_id, home_room_id: None }],
        lessons: vec![Lesson {
            id: lesson_id,
            school_class_ids: vec![class_id],
            subject_id,
            teacher_id,
            hours_per_week: 4,
            preferred_block_size: 1,
            lesson_group_id: None,
        }],
        teacher_qualifications: vec![TeacherQualification { teacher_id, subject_id }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
    };
    let p = |tb: u8| Placement {
        lesson_id,
        time_block_id: TimeBlockId(score_uuid(tb)),
        room_id,
    };
    let placements = [p(10), p(11), p(12), p(14)]; // (d0,p0), (d0,p1), (d1,p0), (d1,p2)
    // p(11) is day 0 max (pos 1); p(14) is day 1 max (pos 2). Two hits at weight 3 = 6.
    assert_eq!(score_solution(&problem, &placements, &weights), 6);
}
```

Add a greedy integration test in `solver/solver-core/src/solve.rs` mirroring the existing `greedy_avoids_position_zero_for_avoid_first_subject_when_alternative_exists` (line 1105). Same two-day shape; flip the flag from avoid_first to avoid_last; assert the placement does not land on the day's max position when an alternative exists. Place this test next to its avoid_first sibling.

```rust
#[test]
fn greedy_avoids_max_position_for_avoid_last_subject_when_alternative_exists() {
    // Single class, single teacher, single subject flagged avoid_last_period.
    // Three time-blocks on day 0 (positions 0, 1, 2; max = 2). One hour to place.
    // Expect the greedy to choose pos 0 or pos 1, not pos 2.
    let mut p = three_block_one_class_problem(); // existing helper near line 1100
    p.subjects[0].avoid_last_period = true;
    let config = SolveConfig {
        weights: ConstraintWeights {
            avoid_last_period: 1,
            ..ConstraintWeights::default()
        },
        ..SolveConfig::default()
    };
    let solution = solve_with_config(&p, &config).unwrap();
    assert_eq!(solution.placements.len(), 2);
    // hours_per_week is 2 in three_block_one_class_problem; both placements
    // must avoid position 2 (the day's max). Positions 0 and 1 are fine.
    let tb_lookup: HashMap<TimeBlockId, &TimeBlock> =
        p.time_blocks.iter().map(|tb| (tb.id, tb)).collect();
    for placement in &solution.placements {
        let tb = tb_lookup[&placement.time_block_id];
        assert_ne!(tb.position, 2, "greedy should avoid max-position TB; got {:?}", placement);
    }
}
```

If `three_block_one_class_problem` does not exist in `solve.rs` (it lives in `score.rs`), copy the construction inline (the `score.rs` helper is not pub-crate visible in `solve.rs`); 30 lines of literal construction is fine for a test. Pin the lesson's `hours_per_week` to 2 so the assertion is meaningful.

- [ ] **Step 1.7.1:** Add `score_solution_includes_avoid_last_only_at_max_day_position` to `score.rs`.

- [ ] **Step 1.7.2:** Add `greedy_avoids_max_position_for_avoid_last_subject_when_alternative_exists` to `solve.rs` (inline the fixture if needed).

### Step 1.8: Verify solver-core green and commit

- [ ] **Step 1.8.1: Run all solver-core tests**

Run: `mise run test:rust`
Expected: all tests pass; no E0063, no clippy warnings.

- [ ] **Step 1.8.2: Run lint**

Run: `mise run lint`
Expected: green. If `cargo machete` complains about an unused dep, that's pre-existing and unrelated; no new dep was added.

- [ ] **Step 1.8.3: Verify bench compiles**

Run: `mise run bench`
Expected: bench runs to completion (numbers may shift; budget check is the criterion second-run delta, not absolute).

- [ ] **Step 1.8.4: Commit**

Run:

```bash
git add solver/solver-core
git commit -m "feat(solver-core): avoid-last-period soft-constraint axis"
```

Expected: pre-commit lint runs and passes; `cog verify` passes (Conventional Commits: `feat(solver-core):` is valid).

---

## Task 2: backend Python schema, model, route, solver IO

**Owning subagent prompt summary:** Add `avoid_last_period` to the Subject ORM model and Pydantic schemas; thread through CRUD routes and `solver_io.build_problem_json`; write Alembic migration; extend tests. **Acceptance:** `mise run test:py` passes, `mise run lint` passes, OpenAPI dump regenerates without manual fix-up.

**Files:**

- Create: `backend/alembic/versions/<rev>_add_subject_avoid_last_period.py` (revision hash auto-generated by Alembic).
- Modify: `backend/src/klassenzeit_backend/db/models/subject.py`, `backend/src/klassenzeit_backend/scheduling/schemas/subject.py`, `backend/src/klassenzeit_backend/scheduling/routes/subjects.py`, `backend/src/klassenzeit_backend/scheduling/solver_io.py`.
- Test: `backend/tests/scheduling/test_subjects.py`, `backend/tests/scheduling/test_solver_io.py`.

### Step 2.1: Drop schema-cached test DBs (one-time) before TDD

Per `backend/CLAUDE.md`: "Schema-changing PRs: drop the template + per-worker DBs before the first test run."

- [ ] **Step 2.1.1: Drop the template and worker DBs to force re-migration**

Run:

```bash
mise exec -- psql -h localhost -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test_template" \
  && mise exec -- psql -h localhost -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test_gw0" \
  && mise exec -- psql -h localhost -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test_gw1" \
  && mise exec -- psql -h localhost -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test_gw2" \
  && mise exec -- psql -h localhost -U postgres -c "DROP DATABASE IF EXISTS klassenzeit_test_gw3"
```

Expected: success or "database does not exist" (idempotent). The conftest will re-migrate the template on the next test run.

(If the local environment uses a non-default postgres host or auth, adapt the connection string. Postgres for this repo runs via `podman compose` from the root `compose.yaml`; consult `mise run db:up` for the standard host/port.)

### Step 2.2: Generate the Alembic migration

- [ ] **Step 2.2.1: Generate the migration from autogenerate**

Run: `mise exec -- uv run alembic revision --autogenerate -m "add subject avoid last period"` from the repo root (or from `backend/` if Alembic config is scoped there).
Expected: a new file under `backend/alembic/versions/<rev>_add_subject_avoid_last_period.py`.

- [ ] **Step 2.2.2: Tidy the generated migration**

Per `backend/CLAUDE.md`, autogenerate emits `typing.Sequence` and `typing.Union[X, Y]`; this repo uses `collections.abc.Sequence` + PEP 604 unions. Mirror the shape of `1064685e0d18_add_subject_preference_columns.py`. Final shape:

```python
"""add subject avoid last period

Revision ID: <auto>
Revises: 1064685e0d18  # or whichever revision is currently head; verify with `alembic heads`
Create Date: 2026-05-01 ...

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "<auto>"
down_revision: str | Sequence[str] | None = "<head>"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    """Upgrade schema."""
    op.add_column(
        "subjects",
        sa.Column(
            "avoid_last_period",
            sa.Boolean(),
            nullable=False,
            server_default=sa.text("false"),
        ),
    )


def downgrade() -> None:
    """Downgrade schema."""
    op.drop_column("subjects", "avoid_last_period")
```

Replace `<head>` with the current head (run `mise exec -- uv run alembic heads` and copy the revision ID; expected to be `1064685e0d18` or its descendant).

### Step 2.3: Extend the ORM model

Modify `backend/src/klassenzeit_backend/db/models/subject.py`:

```python
class Subject(Base):
    """A school subject (e.g. Mathematik, Deutsch, Sport)."""

    __tablename__ = "subjects"

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, server_default=func.gen_random_uuid())
    name: Mapped[str] = mapped_column(String(100), unique=True)
    short_name: Mapped[str] = mapped_column(String(10), unique=True)
    color: Mapped[str] = mapped_column(String(16))
    prefer_early_periods: Mapped[bool] = mapped_column(
        Boolean, default=False, server_default=text("false")
    )
    avoid_first_period: Mapped[bool] = mapped_column(
        Boolean, default=False, server_default=text("false")
    )
    avoid_last_period: Mapped[bool] = mapped_column(
        Boolean, default=False, server_default=text("false")
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )
```

- [ ] **Step 2.3.1:** Add the column.

### Step 2.4: Extend Pydantic schemas

Modify `backend/src/klassenzeit_backend/scheduling/schemas/subject.py`:

```python
class SubjectCreate(BaseModel):
    """Request body for creating a subject."""

    name: str
    short_name: str
    color: str = Field(pattern=COLOR_PATTERN)
    prefer_early_periods: bool = False
    avoid_first_period: bool = False
    avoid_last_period: bool = False


class SubjectUpdate(BaseModel):
    """Request body for patching a subject."""

    name: str | None = None
    short_name: str | None = None
    color: str | None = Field(default=None, pattern=COLOR_PATTERN)
    prefer_early_periods: bool | None = None
    avoid_first_period: bool | None = None
    avoid_last_period: bool | None = None


class SubjectResponse(BaseModel):
    """Response body for a subject."""

    id: uuid.UUID
    name: str
    short_name: str
    color: str
    prefer_early_periods: bool
    avoid_first_period: bool
    avoid_last_period: bool
    created_at: datetime
    updated_at: datetime
```

- [ ] **Step 2.4.1:** Apply the three field additions.

### Step 2.5: Thread through routes

Modify `backend/src/klassenzeit_backend/scheduling/routes/subjects.py`:

- In `create_subject_route`: add `avoid_last_period=body.avoid_last_period,` to the `Subject(...)` constructor; add `avoid_last_period=subject.avoid_last_period,` to the returned `SubjectResponse(...)` (line 84 area).
- In `list_subjects`: add `avoid_last_period=s.avoid_last_period,` to the per-row `SubjectResponse(...)` (line 112 area).
- In `get_subject`: add `avoid_last_period=subject.avoid_last_period,` to the `SubjectResponse(...)` (line 146 area).
- In `update_subject`: add the conditional after the existing `avoid_first_period` block (line 184 area):

  ```python
  if body.avoid_last_period is not None:
      subject.avoid_last_period = body.avoid_last_period
  ```

  (The `is not None` guard is correct here because `avoid_last_period` is NOT NULL; per `backend/CLAUDE.md`, the `model_fields_set` shape is reserved for nullable columns.) Then add `avoid_last_period=subject.avoid_last_period,` to the response constructor (line 199 area).

- [ ] **Step 2.5.1:** Apply all six edits.

### Step 2.6: Update solver_io to emit the new field

Modify `backend/src/klassenzeit_backend/scheduling/solver_io.py`. Find the existing emission loop (around line 260, where `prefer_early_periods` and `avoid_first_period` are written into the JSON dict). Add the new key:

```python
"avoid_last_period": s.avoid_last_period,
```

next to the existing two.

- [ ] **Step 2.6.1:** Add the emission.

### Step 2.7: Extend backend tests

Modify `backend/tests/scheduling/test_subjects.py`. The existing avoid_first_period tests at lines 272-296 and 304-318 are the templates. Mirror them:

- In the existing POST-cases test (whichever asserts `body["avoid_first_period"] is False`), add an assertion for `body["avoid_last_period"] is False`. Then add a sibling test that POSTs `{"avoid_last_period": True}` and asserts the response carries `avoid_last_period: True`.
- Add a sibling PATCH test mirroring `test_update_subject_can_toggle_avoid_first_without_touching_prefer_early_periods` (or whatever the canonical name is at line 304):

  ```python
  async def test_update_subject_can_toggle_avoid_last_period_without_touching_other_flags(
      authed_client: AsyncClient,
      seeded_subject: Subject,
  ) -> None:
      """PATCH /subjects/{id} can toggle avoid_last_period without touching prefer_early_periods or avoid_first_period."""
      res = await authed_client.patch(
          f"/api/subjects/{seeded_subject.id}",
          json={"avoid_last_period": True},
      )
      assert res.status_code == 200
      body = res.json()
      assert body["avoid_last_period"] is True
      assert body["prefer_early_periods"] == seeded_subject.prefer_early_periods
      assert body["avoid_first_period"] == seeded_subject.avoid_first_period
  ```

  The exact fixture names (`authed_client`, `seeded_subject`) match what the existing avoid_first PATCH test uses; verify and copy.

Modify `backend/tests/scheduling/test_solver_io.py`. The existing emission test (around line 542) asserts `prefer_early_periods` and `avoid_first_period` survive into the JSON; extend the assertion list to include `avoid_last_period`:

```python
assert matched["avoid_last_period"] is False
```

next to the existing assertion. Also add `avoid_last_period: False` to the fixtures at lines 514 and 555 (the ones that currently set `prefer_early_periods` and `avoid_first_period` explicitly).

- [ ] **Step 2.7.1:** Extend the existing POST round-trip test.

- [ ] **Step 2.7.2:** Add the new PATCH-toggle test.

- [ ] **Step 2.7.3:** Extend the solver_io emission test and update its fixtures.

### Step 2.8: Update solver-py contract test fixtures

Modify `solver/solver-py/tests/test_bindings.py` line 39 area and line 514 (or whichever spots construct the minimal Subject JSON dict): add `"avoid_last_period": False` to each.

Modify `solver/solver-py/tests/test_multi_class.py` line 33 area: same edit.

- [ ] **Step 2.8.1:** Update both files.

### Step 2.9: Verify backend green and commit

- [ ] **Step 2.9.1: Run backend pytest**

Run: `mise run test:py`
Expected: all tests pass. The first run re-migrates the template DB (slower); subsequent runs use the cached template.

- [ ] **Step 2.9.2: Run solver-py contract tests**

Run: `cd /home/pascal/Code/Klassenzeit && mise exec -- uv run pytest solver/solver-py/tests`
Expected: all tests pass.

- [ ] **Step 2.9.3: Run lint**

Run: `mise run lint`
Expected: green. `ty` should be happy with the new ORM column (it traces through SQLAlchemy `Mapped[bool]`).

- [ ] **Step 2.9.4: Commit**

Run:

```bash
git add backend solver/solver-py/tests
git commit -m "feat(backend): avoid-last-period subject column + API"
```

---

## Task 3: frontend Zod schema + form + i18n + regenerated types

**Owning subagent prompt summary:** Add `avoid_last_period` Zod field, third checkbox in subject edit dialog, en+de i18n keys, regenerate `lib/api-types.ts`, extend the existing subject-form Vitest spec. **Acceptance:** `mise run fe:test` passes, `mise run fe:build` succeeds, `cd frontend && mise exec -- pnpm exec tsc --noEmit` is green, `mise run lint` passes.

**Files:**

- Modify: `frontend/src/features/subjects/schema.ts`, `frontend/src/features/subjects/subjects-dialogs.tsx`, `frontend/src/i18n/locales/en.json`, `frontend/src/i18n/locales/de.json`, `frontend/src/lib/api-types.ts` (regenerated), `frontend/tests/subjects-page.test.tsx`.

### Step 3.1: Regenerate OpenAPI types

- [ ] **Step 3.1.1: Regenerate the type file**

Run: `mise run fe:types`
Expected: `frontend/src/lib/api-types.ts` is rewritten; `git diff frontend/src/lib/api-types.ts` shows three new `avoid_last_period: boolean` (or `boolean | null`) entries near the existing `avoid_first_period` ones (lines 1908-1960 area).

### Step 3.2: Extend the Zod schema

Modify `frontend/src/features/subjects/schema.ts`:

```typescript
import { z } from "zod";
import { COLOR_PATTERN } from "./color";

export const SubjectFormSchema = z.object({
  name: z.string().trim().min(1, "Name is required").max(100),
  short_name: z.string().trim().min(1, "Short name is required").max(10),
  color: z.string().regex(COLOR_PATTERN, "Invalid color"),
  prefer_early_periods: z.boolean(),
  avoid_first_period: z.boolean(),
  avoid_last_period: z.boolean(),
});

export type SubjectFormValues = z.infer<typeof SubjectFormSchema>;
```

- [ ] **Step 3.2.1:** Add the new boolean field.

### Step 3.3: Extend the subject edit dialog

Modify `frontend/src/features/subjects/subjects-dialogs.tsx`:

- In `defaultValues` (line 46-52 area), add:

  ```typescript
  avoid_last_period: subject?.avoid_last_period ?? false,
  ```

- After the existing `avoid_first_period` `<FormField>` block (line 148-168 area), add a sibling block:

  ```tsx
  <FormField
    control={form.control}
    name="avoid_last_period"
    render={({ field }) => (
      <FormItem className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <FormControl>
            <Checkbox
              id="subject-avoid-last"
              checked={field.value}
              onCheckedChange={field.onChange}
            />
          </FormControl>
          <FormLabel htmlFor="subject-avoid-last">
            {t("subjects.fields.avoidLastPeriod.label")}
          </FormLabel>
        </div>
        <FormDescription>{t("subjects.fields.avoidLastPeriod.help")}</FormDescription>
      </FormItem>
    )}
  />
  ```

- [ ] **Step 3.3.1:** Apply both edits.

### Step 3.4: Add i18n keys

Modify `frontend/src/i18n/locales/en.json`. Inside `subjects.fields` (line 143-152 area), add:

```json
"avoidLastPeriod": {
  "label": "Avoid the last period",
  "help": "Avoid scheduling lessons of this subject in the very last period of the day (e.g. Hauptfächer)."
}
```

Modify `frontend/src/i18n/locales/de.json`. Inside the matching `subjects.fields` block, add:

```json
"avoidLastPeriod": {
  "label": "Letzte Stunde meiden",
  "help": "Vermeide es, Stunden dieses Fachs in der letzten Stunde des Tages zu legen (z. B. Hauptfächer)."
}
```

(Verify the existing `avoidFirstPeriod` key shape in `de.json` for tone and capitalisation parity before writing.)

- [ ] **Step 3.4.1:** Add both i18n entries.

### Step 3.5: Extend the existing subject-form Vitest spec

Modify `frontend/tests/subjects-page.test.tsx`. Find the existing test that exercises the avoid_first_period checkbox (search for `avoidFirstPeriod` or `Avoid the first period`); copy that test shape and add a sibling that:

1. Renders the dialog with a subject whose `avoid_last_period` is false.
2. Asserts the new checkbox renders by its accessible label ("Avoid the last period" in en).
3. Toggles it and submits; asserts the mutation payload includes `avoid_last_period: true`.

The exact fixture and helper names match what the existing avoid_first test uses; mirror them. Pin the locale to `en` per `frontend/CLAUDE.md` ("Component tests that query English labels must pin the locale.") if the existing test does so.

- [ ] **Step 3.5.1:** Add the new test next to the existing avoid_first sibling.

### Step 3.6: Verify frontend green and commit

- [ ] **Step 3.6.1: Run Vitest**

Run: `mise run fe:test`
Expected: green; the new test passes.

- [ ] **Step 3.6.2: Build (regenerates routeTree.gen.ts) and typecheck**

Run: `cd frontend && mise exec -- pnpm exec tsc --noEmit`
Expected: green. The build itself does not need a fresh run unless route files changed (none here).

- [ ] **Step 3.6.3: Run lint**

Run: `mise run lint`
Expected: green; biome accepts the new tsx block.

- [ ] **Step 3.6.4: Commit**

Run:

```bash
git add frontend
git commit -m "feat(frontend): avoid-last-period checkbox in subject edit dialog"
```

---

## Task 4: demo seeds + ADR + bench refresh + OPEN_THINGS update

**Owning subagent prompt summary:** Mark Mathematik and Deutsch as `avoid_last_period=True` in both demo seed files; write ADR 0024; refresh `BASELINE.md` if `mise run bench` shows movement past noise (~3%); mark sprint item 8 shipped in OPEN_THINGS. **Acceptance:** seed solvability tests pass, ADR 0024 lands, OPEN_THINGS updated.

**Files:**

- Create: `docs/adr/0024-avoid-last-period.md`.
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule.py`, `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py`, `docs/superpowers/OPEN_THINGS.md`, possibly `solver/solver-core/benches/BASELINE.md`.

### Step 4.1: Extend `_SubjectSpec` and seed flags

Modify `backend/src/klassenzeit_backend/seed/demo_grundschule.py` (around lines 56-71):

```python
class _SubjectSpec(NamedTuple):
    name: str
    short_name: str
    color: str
    prefer_early_periods: bool = False
    avoid_first_period: bool = False
    avoid_last_period: bool = False


_SUBJECTS: Sequence[_SubjectSpec] = (
    _SubjectSpec("Deutsch", "D", "chart-1", prefer_early_periods=True, avoid_last_period=True),
    _SubjectSpec("Mathematik", "M", "chart-2", prefer_early_periods=True, avoid_last_period=True),
    # ... rest unchanged
    _SubjectSpec("Sport", "SP", "chart-4", avoid_first_period=True),
    # ...
)
```

(Match the existing tuple shape; only the first two entries flip and only `avoid_last_period=True` is added.)

Then modify the `_create_subjects` (or equivalent) helper at line 200 area to thread the flag:

```python
for spec in _SUBJECTS:
    subject = Subject(
        name=spec.name,
        short_name=spec.short_name,
        color=spec.color,
        prefer_early_periods=spec.prefer_early_periods,
        avoid_first_period=spec.avoid_first_period,
        avoid_last_period=spec.avoid_last_period,
    )
    # ... existing append/commit logic
```

(Adapt to the actual function shape; the change is a single new `avoid_last_period=spec.avoid_last_period,` line in the Subject constructor.)

- [ ] **Step 4.1.1:** Update `_SubjectSpec` with the new optional field.

- [ ] **Step 4.1.2:** Flip Deutsch and Mathematik in the `_SUBJECTS` tuple.

- [ ] **Step 4.1.3:** Thread `spec.avoid_last_period` into the Subject construction.

### Step 4.2: Mirror the change in the dreizuegige seed

Modify `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py` (line 387 area):

```python
subject = Subject(
    name=spec.name,
    short_name=spec.short_name,
    color=spec.color,
    prefer_early_periods=spec.prefer_early_periods,
    avoid_first_period=spec.avoid_first_period,
    avoid_last_period=spec.avoid_last_period,
)
```

The dreizuegige seed imports `_SUBJECTS` and `_SubjectSpec` from `demo_grundschule.py` (per `backend/CLAUDE.md`'s seed cross-import rule), so the tuple update in 4.1 propagates automatically. The only change here is the new `avoid_last_period=spec.avoid_last_period,` line in the Subject construction.

The zweizuegige seed (`demo_grundschule_zweizuegig.py`) likewise imports the shared spec; verify by `grep`.

- [ ] **Step 4.2.1:** Add the new constructor argument in the dreizuegige seed.

- [ ] **Step 4.2.2:** Verify the zweizuegige seed picks up the change automatically (or apply the same edit if the seed copy-pastes the construction).

### Step 4.3: Refresh bench numbers if needed

- [ ] **Step 4.3.1: Run the bench**

Run: `mise run bench`
Expected: criterion runs three fixtures (grundschule, zweizuegig, dreizuegig); the second run output shows deltas against the first.

- [ ] **Step 4.3.2: Decide whether to refresh BASELINE.md**

If any p50 drift exceeds ~3% on any fixture, run `mise run bench:record` to overwrite `solver/solver-core/benches/BASELINE.md`; cite the diff in the PR body. Otherwise, leave the file alone (per the spec's Q9 decision: "no refresh under 3%").

If a refresh is needed, the new BASELINE.md gets staged in this commit. If not, skip.

### Step 4.4: Write ADR 0024

Create `docs/adr/0024-avoid-last-period.md`. Mirror ADR 0023's shape:

```markdown
# 0024: Avoid-last-period soft constraint

- **Status:** Accepted
- **Date:** 2026-05-01

## Context

Sprint item 8 (algorithm phase, P1) of the "Realer Schulalltag" sprint. The previous algorithm-phase work (PR-9c, ADR 0017) introduced two subject-level pedagogy axes: `prefer_early_periods` (linear in `tb.position`) and `avoid_first_period` (binary at `tb.position == 0`). The mirror at the other end of the day was deliberately deferred. The dreizuegige seed and the Hessen Grundschule pedagogy reference both call for "Hauptfächer früh, also nicht in der letzten Stunde": Mathematik and Deutsch should avoid the last period of the day, where end-of-day fatigue is highest. The existing avoid-first axis covers the wakeup-cold edge but not this one.

## Decision

1. **`avoid_last_period: bool` on `Subject`.** Additive Alembic migration, server-side default `FALSE`, NOT NULL. Wire format additive end-to-end (Pydantic, Zod, OpenAPI, solver JSON via `#[serde(default)]`).
2. **`avoid_last_period: u32` axis on `ConstraintWeights`.** Per-placement binary penalty: a placement at `tb.position == max_position_for_day` for that placement's `day_of_week` contributes `weights.avoid_last_period` once. The active-default `solve()` weight is `1`, alongside the existing five axes.
3. **Per-day max-position lookup.** `score_solution`, the lowest-delta greedy in `solve_with_config`, and the LAHC Change-move delta path each build a `HashMap<u8, u8>` from `problem.time_blocks` once per call (folding `max(position)` per `day_of_week`) and pass the per-placement value into `subject_preference_score`. The function gains a `max_position_for_day: u8` parameter; allocation-free.
4. **Demo seeds mark Mathematik and Deutsch as avoid-last.** Same two Hauptfächer the seed already marks `prefer_early_periods=True`. The flag carries through all three demo fixtures (Grundschule, zweizuegig, dreizuegig) via the shared `_SubjectSpec` table.

## Alternatives considered

- **Single global max position.** Simpler, but wrong for asymmetric Hessen schedules where Halbtag days end earlier than Ganztag days. Per-day max captures the actual user-meaningful "last period of *this* day" semantics.
- **Inline avoid-last logic in `score_solution` only.** Splits the per-placement axis logic across two sites (avoid_first inside `subject_preference_score`, avoid_last in the score loop), making the LAHC delta path's symmetric old-vs-new score call awkward. Threading `max_position_for_day` through the helper keeps all three axes unified.
- **Bundle with sprint item 9 (configurable per-subject weights, P2).** Risks dragging a P1 over a sprint boundary; the OPEN_THINGS rule "structural and behavioural changes never ship in the same commit" reinforces keeping each axis its own additive change.

## Consequences

Easier: parity with `avoid_first_period`, no new public-API reshape required, BASELINE.md refresh is optional (per-placement scoring is `O(placements)` with hoisted lookups, well inside the 20% sprint budget; the home-room PR confirmed the same shape needs no refresh). Harder: every literal `Subject { ... }` and `ConstraintWeights { ... }` test fixture across solver-core, solver-py tests, and benches gains one new field; future axis additions compound the maintenance cost. Revisit once sprint item 9 (configurable weights) lands and per-axis weights become a real user knob.
```

- [ ] **Step 4.4.1:** Write the file.

- [ ] **Step 4.4.2:** Update `docs/adr/README.md` index to list ADR 0024 next to ADR 0023.

### Step 4.5: Update OPEN_THINGS

Modify `docs/superpowers/OPEN_THINGS.md`. Find the "Algorithm phase" sprint section, sprint item 8 ("Avoid-last-period axis. `[P1]`"). Mark it shipped in the same shape as items 6 and 7 (the lesson-group co-placement and home-room preference items):

```markdown
8. **Avoid-last-period axis.** `[P1]` ✅ Shipped 2026-05-01 in PR `feat/solver-avoid-last-period`. Adds `Subject.avoid_last_period: bool` (additive migration, default false) and `avoid_last_period` axis on `ConstraintWeights` (active default 1). Per-placement binary penalty at `tb.position == max_position_for_day` for that placement's `day_of_week`. `subject_preference_score` gains a `max_position_for_day: u8` parameter; `score_solution`, `solve_with_config`'s greedy, and `lahc::run` each build a `HashMap<u8, u8>` once per call and thread the per-placement value. Demo seeds (Grundschule, zweizuegige, dreizuegige) mark Mathematik and Deutsch as avoid-last via the shared `_SubjectSpec`. Bench: <fill in based on Q9 outcome — either "p50 wall-clock within 1 percent of baseline per fixture, no refresh" or the actual diff if BASELINE.md was refreshed>. ADR 0024 records the decision.
```

- [ ] **Step 4.5.1:** Update sprint item 8 in OPEN_THINGS.

### Step 4.6: Run end-to-end verification

- [ ] **Step 4.6.1: Run all tests**

Run: `mise run test`
Expected: green across Rust + Python + frontend.

- [ ] **Step 4.6.2: Run seed solvability tests**

Run: `mise run test:py -- backend/tests/seed -v`
Expected: green; all three seed solvability tests pass with the new flag.

- [ ] **Step 4.6.3: Run lint**

Run: `mise run lint`
Expected: green.

### Step 4.7: Commit

- [ ] **Step 4.7.1: Stage and commit**

Run:

```bash
git add backend/src/klassenzeit_backend/seed docs/adr/0024-avoid-last-period.md docs/adr/README.md docs/superpowers/OPEN_THINGS.md
git add solver/solver-core/benches/BASELINE.md  # only if Step 4.3.2 said to refresh
git commit -m "feat: mark Mathe/Deutsch avoid-last in demo seeds + ADR 0024"
```

---

## Self-review

**1. Spec coverage:**

- ✅ Goal: avoid-last-period axis end-to-end → Tasks 1, 2, 3, 4.
- ✅ Database: additive Alembic migration → Step 2.2.
- ✅ ORM model column → Step 2.3.
- ✅ Pydantic schemas (Create, Update, Response) → Step 2.4.
- ✅ Routes (six sites) → Step 2.5.
- ✅ solver_io emission → Step 2.6.
- ✅ Demo seeds (Grundschule, zweizuegige, dreizuegige) → Steps 4.1, 4.2.
- ✅ solver-core types (`Subject`, `ConstraintWeights`) → Step 1.2.
- ✅ `subject_preference_score` signature change → Step 1.3.
- ✅ score_solution `max_position_per_day` build + thread → Step 1.3.
- ✅ Active defaults in `solve.rs` and `json.rs` → Steps 1.4.1, 1.6.1.
- ✅ LAHC delta path → Step 1.5.
- ✅ solver-py contract test fixtures → Step 2.8.
- ✅ Frontend (Zod, form, i18n, regenerated types, Vitest) → Task 3.
- ✅ ADR 0024 → Step 4.4.
- ✅ Bench refresh policy (Q9: only if past noise) → Step 4.3.
- ✅ OPEN_THINGS sprint item 8 marked shipped → Step 4.5.
- ✅ Test plan from spec (subject_preference_score new test, score_solution new test, greedy integration test, backend POST/PATCH, solver_io emission, frontend Vitest) → distributed across Steps 1.1, 1.7, 2.7, 3.5.

**2. Placeholder scan:**

- "<rev>" in Step 2.2 is a real Alembic auto-generated revision hash, not a placeholder; the autogenerate command produces the value. Acceptable.
- "<head>" in Step 2.2's down_revision is "look up the actual revision via `alembic heads` and paste it"; the step shows the command. Acceptable.
- Bench refresh outcome in Step 4.5's OPEN_THINGS update has a `<fill in based on Q9 outcome>` token; resolve at commit time based on the actual Step 4.3.2 outcome. Acceptable.
- No "TODO", "TBD", "implement later" anywhere.

**3. Type consistency:**

- `subject_preference_score(subject, tb, max_position_for_day, weights)` — same signature in score.rs (Step 1.3), solve.rs (Step 1.4), lahc.rs (Step 1.5).
- `ConstraintWeights.avoid_last_period: u32` — used as `weights.avoid_last_period` in the score helper and as `avoid_last_period: 1` (active default) and `avoid_last_period: 0` (test fixtures).
- `Subject.avoid_last_period: bool` — used as `subject.avoid_last_period`, `spec.avoid_last_period`, `body.avoid_last_period`, and `field.value` (frontend) consistently.
- Pydantic field name matches Rust JSON key matches ORM column matches Zod field: `avoid_last_period` (snake_case) everywhere, including the i18n key path `subjects.fields.avoidLastPeriod` (camelCase only inside the i18n catalogue per the existing convention; `avoidFirstPeriod` follows the same shape, so this is consistent with prior art).

No drift; plan is internally consistent.
