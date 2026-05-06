# Bench observability columns implementation plan (item 30)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `peak_memory_kb`, `time_to_first_feasible_ms`, `time_to_optimal_ms` columns to `solver/solver-core/benches/BENCH_RESULTS.md` so cross-backend RAM and timing trade-offs become legible per cell.

**Architecture:** New internal `solve_with_config_stats(problem, config) -> (Solution, SolveStats)` in `solver-core` carries timing probes; the existing `solve_with_config` becomes a one-line wrapper that discards stats. `solver-bench` is restructured into supervisor + cell-child via recursive self-spawn, so each cell runs in a fresh process and `getrusage(RUSAGE_SELF).ru_maxrss` is honest. The cpsat python module self-reports its own peak via `resource.getrusage` and reports ttf/tto via a `CpSolverSolutionCallback`.

**Tech Stack:** Rust 2021, PyO3 0.28, `libc = "0.2"` (new), `serde_json` (existing), proptest, pytest, OR-Tools `cp_model.CpSolverSolutionCallback`.

Spec: `docs/superpowers/specs/2026-05-06-bench-observability-columns-design.md`. Brainstorm: `/tmp/kz-brainstorm/brainstorm.md`.

---

## File map

**Create:**

- `solver/solver-bench/tests/end_to_end.rs`: supervisor smoke test (spawns the binary, parses markdown).
- `docs/adr/0034-bench-cell-subprocess-and-observability.md`: ADR.

**Modify:**

- `solver/solver-core/src/types.rs`: add `SolveStats`.
- `solver/solver-core/src/solve.rs`: split into `solve_with_config_stats` + `solve_with_config` wrapper; thread stats through to LAHC.
- `solver/solver-core/src/lahc.rs`: accept `&mut SolveStats` + `solve_start: Instant`; record ttf and tto in the loop.
- `solver/solver-core/src/lib.rs`: re-export `SolveStats` and `solve_with_config_stats`.
- `solver/solver-core/tests/lahc_property.rs`: proptest `lahc_stats_ttf_le_tto_le_total`.
- `solver/solver-core/src/solve.rs#tests`: three inline unit tests for FFD-already-feasible, LAHC-improves, never-feasible.
- `solver/solver-py/python/klassenzeit_solver/cpsat.py`: `_FirstSolutionCallback`, `_read_peak_rss_kb`, three new output JSON fields.
- `solver/solver-py/tests/test_cpsat.py`: two new tests for the OPTIMAL and INFEASIBLE branches.
- `solver/solver-bench/Cargo.toml`: add `libc = "0.2"` and `serde = { workspace = true, features = ["derive"] }`.
- `Cargo.toml` (workspace): add `libc = "0.2"` to `[workspace.dependencies]` if not already; same for `serde` with `derive` feature.
- `solver/solver-bench/src/main.rs`: split into supervisor + cell-child; `CellResult` derives serde + gains 3 fields; markdown header + row formatter add 3 columns.
- `solver/solver-core/benches/BENCH_RESULTS.md`: regenerated (low-budget shape demo) with the 12-column header + footer addendum.
- `docs/adr/README.md`: index entry for ADR 0034.
- `docs/superpowers/OPEN_THINGS.md`: delete item 30; advance next-pickup; refine item 42.
- `solver/CLAUDE.md`: bench supervisor architecture bullet.
- `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`: refresh body + description frontmatter.

---

## Task 1: `SolveStats` type, `solve_with_config_stats`, LAHC probes

Bottom-up: introduces the public Rust API surface and the LAHC probe so subsequent tasks (cpsat + bench) have something to call.

**Files:**

- Modify: `solver/solver-core/src/types.rs`
- Modify: `solver/solver-core/src/solve.rs`
- Modify: `solver/solver-core/src/lahc.rs`
- Modify: `solver/solver-core/src/lib.rs`
- Modify: `solver/solver-core/tests/lahc_property.rs`

- [ ] **Step 1.1: Write the failing inline unit test for FFD-already-feasible.**

Append to `solver/solver-core/src/solve.rs` inside the existing `#[cfg(test)] mod tests { ... }` block:

```rust
#[test]
fn solve_with_config_stats_returns_zero_ttf_when_greedy_is_feasible() {
    use crate::solve_with_config_stats;
    use crate::test_fixtures::grundschule_fixture;
    use crate::types::ConstraintWeights;
    let problem = grundschule_fixture();
    let cfg = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 1,
            teacher_gap: 1,
            ..ConstraintWeights::default()
        },
        deadline: None,
        ..SolveConfig::default()
    };
    let (sol, stats) = solve_with_config_stats(&problem, &cfg).expect("solve");
    assert!(sol.violations.is_empty());
    assert_eq!(stats.time_to_first_feasible_ms, Some(0.0));
    assert_eq!(stats.time_to_optimal_ms, Some(0.0));
}
```

- [ ] **Step 1.2: Run it; verify failure.**

```
cargo test -p solver-core --lib solve_with_config_stats_returns_zero_ttf_when_greedy_is_feasible
```

Expected: compile error `cannot find function 'solve_with_config_stats'`.

- [ ] **Step 1.3: Add `SolveStats` to `types.rs`.**

Append after the existing public types in `solver/solver-core/src/types.rs` (after `Violation` and before module-end):

```rust
/// Optional timing probes produced by [`crate::solve_with_config_stats`].
/// Populated by the LAHC loop and the FFD-greedy entry-check; consumers
/// (today: `solver-bench`) median or aggregate across seed runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SolveStats {
    /// Wall-clock from `solve_with_config_stats` entry to first feasible
    /// incumbent (`violations.is_empty() && placements.len() ==
    /// placements_expected`). `Some(0.0)` when FFD greedy is already
    /// feasible at LAHC entry. `None` when the run never reaches
    /// feasibility.
    pub time_to_first_feasible_ms: Option<f64>,
    /// Wall-clock from `solve_with_config_stats` entry to the last
    /// running-best improvement of `state.soft_score`. `Some(0.0)` when
    /// FFD greedy already produced `state.soft_score == 0` and feasible.
    /// `None` when no LAHC iteration improved the running-best (or LAHC
    /// did not run because deadline is `None`). LAHC has no proof of
    /// optimality; this is a lower bound on the iteration count to the
    /// final reported soft score.
    pub time_to_optimal_ms: Option<f64>,
}
```

- [ ] **Step 1.4: Add `solve_with_config_stats` to `solve.rs`.**

In `solver/solver-core/src/solve.rs`, change `solve_with_config` to delegate. Insert immediately above the existing `pub fn solve_with_config` definition:

```rust
/// Same as [`solve_with_config`] but additionally returns timing probes
/// (`time_to_first_feasible_ms`, `time_to_optimal_ms`). Used by
/// `solver-bench`; production callers (`solve`, `solve_json_with_config`,
/// the solver-py binding, the backend) call [`solve_with_config`] which
/// discards the stats.
pub fn solve_with_config_stats(
    problem: &Problem,
    config: &SolveConfig,
) -> Result<(Solution, crate::types::SolveStats), Error> {
    use crate::types::SolveStats;
    let solve_start = std::time::Instant::now();
    let mut stats = SolveStats::default();
    validate_structural(problem)?;

    let (seed_placements, pinned, mut pin_violations) = validate_pins(problem);

    let idx = Indexed::new(problem);
    let mut solution = Solution {
        placements: seed_placements,
        violations: {
            let mut v = pre_solve_violations(problem);
            v.append(&mut pin_violations);
            v
        },
        soft_score: 0,
    };

    let mut state = GreedyState::new();
    use crate::ids::LessonGroupId;
    let mut placed_groups: HashSet<LessonGroupId> = HashSet::new();
    let mut group_members: HashMap<LessonGroupId, Vec<usize>> = HashMap::new();
    for (i, lesson) in problem.lessons.iter().enumerate() {
        if let Some(group_id) = lesson.lesson_group_id {
            group_members.entry(group_id).or_default().push(i);
        }
    }
    let teacher_max: HashMap<TeacherId, u8> = problem
        .teachers
        .iter()
        .map(|t| (t.id, t.max_hours_per_week))
        .collect();
    let class_max_lessons_per_day: HashMap<SchoolClassId, u8> = problem
        .school_classes
        .iter()
        .filter_map(|c| c.max_lessons_per_day.map(|cap| (c.id, cap)))
        .collect();

    seed_greedy_state_from_pins(problem, &solution.placements, &mut state);

    let mut tb_order: Vec<usize> = (0..problem.time_blocks.len()).collect();
    tb_order.sort_unstable_by_key(|&i| {
        let tb = &problem.time_blocks[i];
        (tb.day_of_week, tb.position, tb.id.0)
    });
    let mut room_order: Vec<usize> = (0..problem.rooms.len()).collect();
    room_order.sort_unstable_by_key(|&i| problem.rooms[i].id.0);
    let max_position_per_day: HashMap<u8, u8> =
        problem
            .time_blocks
            .iter()
            .fold(HashMap::new(), |mut acc, tb| {
                acc.entry(tb.day_of_week)
                    .and_modify(|m| *m = (*m).max(tb.position))
                    .or_insert(tb.position);
                acc
            });

    let order = crate::ordering::ffd_order(problem, &idx);
    for &lesson_idx in &order {
        let lesson = &problem.lessons[lesson_idx];
        if pinned.contains(&lesson.id) {
            continue;
        }
        if !idx.teacher_qualified(lesson.teacher_id, lesson.subject_id) {
            continue;
        }

        if let Some(group_id) = lesson.lesson_group_id {
            if !placed_groups.insert(group_id) {
                continue;
            }
            let member_indices = group_members.get(&group_id).cloned().unwrap_or_default();
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
                            &class_max_lessons_per_day,
                            &config.weights,
                            &mut state,
                            &mut solution.placements,
                            &tb_order,
                            &room_order,
                            &max_position_per_day,
                        )
                    };
                    if !placed {
                        for &mi in &member_indices {
                            let member = &problem.lessons[mi];
                            if !idx.teacher_qualified(member.teacher_id, member.subject_id) {
                                continue;
                            }
                            solution.violations.push(Violation {
                                kind: ViolationKind::LessonGroupSplit,
                                lesson_id: member.id,
                                hour_index: block_index * n,
                                reason: None,
                            });
                        }
                    }
                }
                continue;
            }
        }

        let n = lesson.preferred_block_size;
        let block_count = lesson.hours_per_week / n;
        for block_index in 0..block_count {
            let placed = try_place_block(
                problem,
                lesson,
                n,
                &idx,
                &teacher_max,
                &class_max_lessons_per_day,
                &config.weights,
                &mut state,
                &mut solution.placements,
                &tb_order,
                &room_order,
                &max_position_per_day,
            );
            if !placed {
                solution.violations.push(Violation {
                    kind: unplaced_kind(
                        problem,
                        lesson,
                        &idx,
                        &teacher_max,
                        &state.used_teacher,
                        &state.used_class,
                        &state.hours_by_teacher,
                    ),
                    lesson_id: lesson.id,
                    hour_index: block_index * n,
                    reason: None,
                });
            }
        }
    }

    let placements_expected: usize = problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as usize)
        .sum();
    if solution.violations.is_empty() && solution.placements.len() == placements_expected {
        stats.time_to_first_feasible_ms = Some(0.0);
        if state.soft_score == 0 {
            stats.time_to_optimal_ms = Some(0.0);
        }
    }

    crate::lahc::run(
        problem,
        &idx,
        config,
        &mut solution.placements,
        &mut state,
        &pinned,
        &class_max_lessons_per_day,
        &mut stats,
        solve_start,
    );

    validate_no_room_hopping(problem, &solution.placements)?;
    validate_no_double_booking(problem, &solution.placements)?;

    #[cfg(debug_assertions)]
    if let Err(e) = validate_daily_caps(problem, &solution.placements) {
        panic!("daily-cap post-condition violated: {e}");
    }
    #[cfg(debug_assertions)]
    if let Err(e) = validate_no_double_booking(problem, &solution.placements) {
        panic!("no-double-booking post-condition violated: {e}");
    }

    solution.soft_score =
        crate::score::score_solution(problem, &solution.placements, &config.weights);
    Ok((solution, stats))
}
```

Then collapse the existing `pub fn solve_with_config` body to a one-line delegate:

```rust
pub fn solve_with_config(problem: &Problem, config: &SolveConfig) -> Result<Solution, Error> {
    solve_with_config_stats(problem, config).map(|(sol, _)| sol)
}
```

(Keep its existing rustdoc unchanged.)

- [ ] **Step 1.5: Update `lahc::run` signature and probes.**

In `solver/solver-core/src/lahc.rs`, change the `pub(crate) fn run` signature to add two parameters at the end. The function body changes to use `solve_start` instead of its own `let start = Instant::now()`, plus the two probe blocks:

```rust
#[allow(clippy::too_many_arguments)] // Reason: orchestrator helper, all args needed
pub(crate) fn run(
    problem: &Problem,
    idx: &Indexed,
    config: &SolveConfig,
    placements: &mut Vec<Placement>,
    state: &mut crate::solve::GreedyState,
    pinned: &HashSet<LessonId>,
    class_max_lessons_per_day: &HashMap<SchoolClassId, u8>,
    stats: &mut crate::types::SolveStats,
    solve_start: std::time::Instant,
) {
    let Some(deadline) = config.deadline else {
        return;
    };
    if placements.is_empty() {
        return;
    }
    let mut change_rng = SmallRng::seed_from_u64(config.seed);
    let mut rr_rng = SmallRng::seed_from_u64(config.seed.wrapping_add(1));
    let mut kempe_rng = SmallRng::seed_from_u64(config.seed.wrapping_add(2));
    // ... existing init lines until placements_expected ...
    let placements_expected: usize = problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as usize)
        .sum();

    let mut running_best = state.soft_score;

    let mut iter: u64 = 0;
    while iter < max_iter && solve_start.elapsed() < deadline {
        // ... existing branch dispatch on is_rr_iter / is_kempe_iter / Change ...

        iter += 1;
        lahc_list[(iter as usize - 1) % LAHC_LIST_LEN] = state.soft_score;

        if stats.time_to_first_feasible_ms.is_none()
            && state.soft_score == 0
            && placements.len() == placements_expected
        {
            stats.time_to_first_feasible_ms =
                Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }
        if state.soft_score < running_best {
            running_best = state.soft_score;
            stats.time_to_optimal_ms = Some(solve_start.elapsed().as_secs_f64() * 1000.0);
        }

        if state.soft_score == 0 && placements.len() == placements_expected {
            break;
        }
    }
}
```

Replace any remaining `start.elapsed()` references inside the function with `solve_start.elapsed()`. Remove the now-unused `let start = Instant::now()` line. Keep `use std::time::Instant` if `Instant` is used elsewhere in the file; otherwise drop it.

Update the inner loop's `try_place_block` time-of-day expectations: those helpers do not know about `solve_start`; this rename is purely the local variable swap.

- [ ] **Step 1.6: Re-export from `lib.rs`.**

In `solver/solver-core/src/lib.rs`:

```rust
pub use solve::{solve, solve_with_config, solve_with_config_stats};
pub use types::{
    ConstraintWeights, Lesson, Placement, Problem, Room, RoomBlockedTime, RoomSubjectSuitability,
    SchoolClass, Solution, SolveConfig, SolveStats, Subject, Teacher, TeacherBlockedTime,
    TeacherQualification, TimeBlock, Violation, ViolationKind, PRODUCTION_ACTIVE_WEIGHTS,
};
```

- [ ] **Step 1.7: Run the FFD test; verify pass.**

```
cargo test -p solver-core --lib solve_with_config_stats_returns_zero_ttf_when_greedy_is_feasible
```

Expected: PASS.

- [ ] **Step 1.8: Add the LAHC-improves and never-feasible inline tests.**

Append to the same `#[cfg(test)] mod tests { ... }` block in `solver/solver-core/src/solve.rs`:

```rust
#[test]
fn solve_with_config_stats_records_running_best_improvement() {
    use crate::solve_with_config_stats;
    use crate::test_fixtures::grundschule_fixture;
    use crate::types::ConstraintWeights;
    let problem = grundschule_fixture();
    let cfg = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 1,
            teacher_gap: 1,
            ..ConstraintWeights::default()
        },
        deadline: Some(std::time::Duration::from_millis(20)),
        max_iterations: Some(2000),
        seed: 1,
        lahc_rr_period: None,
        lahc_kempe_period: None,
    };
    let (_sol, stats) = solve_with_config_stats(&problem, &cfg).expect("solve");
    assert!(stats.time_to_first_feasible_ms.is_some());
    assert!(stats.time_to_optimal_ms.is_some());
    let ttf = stats.time_to_first_feasible_ms.unwrap();
    let tto = stats.time_to_optimal_ms.unwrap();
    assert!(ttf <= tto + 1e-6, "ttf {} > tto {}", ttf, tto);
}

#[test]
fn solve_with_config_stats_returns_none_when_unfeasible() {
    use crate::solve_with_config_stats;
    use crate::types::{ConstraintWeights, Lesson, Problem, SchoolClass, Subject, Teacher,
        TimeBlock, TeacherQualification};
    use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
    use uuid::Uuid;
    fn id<F: FnOnce(Uuid) -> T, T>(f: F) -> T { f(Uuid::from_bytes([1; 16])) }
    let cls = SchoolClassId(Uuid::from_bytes([1; 16]));
    let subj = SubjectId(Uuid::from_bytes([2; 16]));
    let teacher = TeacherId(Uuid::from_bytes([3; 16]));
    let tb = TimeBlockId(Uuid::from_bytes([4; 16]));
    let lesson = LessonId(Uuid::from_bytes([5; 16]));
    let problem = Problem {
        time_blocks: vec![TimeBlock { id: tb, day_of_week: 0, position: 0 }],
        teachers: vec![Teacher { id: teacher, max_hours_per_week: 5 }],
        rooms: vec![],
        subjects: vec![Subject { id: subj, ..Default::default() }],
        school_classes: vec![SchoolClass { id: cls, ..Default::default() }],
        lessons: vec![Lesson {
            id: lesson,
            school_class_ids: vec![cls],
            subject_id: subj,
            teacher_id: teacher,
            hours_per_week: 5,
            preferred_block_size: 1,
            ..Default::default()
        }],
        teacher_qualifications: vec![TeacherQualification { teacher_id: teacher, subject_id: subj }],
        teacher_blocked_times: vec![],
        room_blocked_times: vec![],
        room_subject_suitabilities: vec![],
        pinned_placements: vec![],
    };
    let cfg = SolveConfig::default();
    let (_sol, stats) = solve_with_config_stats(&problem, &cfg).expect("solve");
    assert_eq!(stats.time_to_first_feasible_ms, None);
    assert_eq!(stats.time_to_optimal_ms, None);
}
```

If `Lesson` / `Subject` / `SchoolClass` / `TimeBlock` types do not implement `Default` for the optional fields used above, fall back to constructing them with explicit fields per the existing test patterns in `solve.rs#tests` (search for `fn make_minimal_problem` or similar inside the file). The intent is: 1 lesson asks 5 hours, only 1 TB exists, no room, so FFD records 5 violations and stats stay `None`.

- [ ] **Step 1.9: Run all three inline tests; verify pass.**

```
cargo test -p solver-core --lib solve_with_config_stats
```

Expected: 3 tests PASS.

- [ ] **Step 1.10: Add the proptest invariant.**

Append to `solver/solver-core/tests/lahc_property.rs` (inside the existing `proptest! { ... }` block):

```rust
#[test]
fn lahc_stats_ttf_le_tto_le_total(p in lahc_small_problem(), seed in 0u64..1024) {
    use solver_core::solve_with_config_stats;
    let cfg = SolveConfig {
        weights: ConstraintWeights {
            class_gap: 1,
            teacher_gap: 1,
            ..ConstraintWeights::default()
        },
        seed,
        deadline: Some(std::time::Duration::from_millis(50)),
        max_iterations: Some(2000),
        lahc_rr_period: None,
        lahc_kempe_period: None,
    };
    let outer_start = std::time::Instant::now();
    let (_sol, stats) = solve_with_config_stats(&p, &cfg).expect("solve");
    let total_ms = outer_start.elapsed().as_secs_f64() * 1000.0;
    if let (Some(ttf), Some(tto)) = (stats.time_to_first_feasible_ms, stats.time_to_optimal_ms) {
        prop_assert!(ttf <= tto + 1e-6, "ttf {} > tto {}", ttf, tto);
        prop_assert!(tto <= total_ms + 50.0, "tto {} > total+50ms {}", tto, total_ms + 50.0);
    }
}
```

Note: imports `solve_with_config_stats` from the public crate root; if the existing test file uses `use solver_core::*;`, no extra import is needed.

- [ ] **Step 1.11: Run the property test plus the existing LAHC property tests.**

```
cargo nextest run -p solver-core --test lahc_property
```

Expected: all green.

- [ ] **Step 1.12: 5x128 local proptest sweep on the new property.**

Per solver/CLAUDE.md, a generator-touching change requires a sweep before commit. The new test only adds a fresh predicate, but its dep on `lahc_small_problem()` makes the sweep applicable as a defensive check.

```
for s in 1 2 3 4 5; do
  PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property
done
```

Expected: all 5 invocations green; no new entries in `solver-core/tests/lahc_property.proptest-regressions`.

- [ ] **Step 1.13: Run the full Rust test suite.**

```
mise run test:rust
```

Expected: green. If a downstream test fails because of the LAHC signature change, the failure will be a compile error pointing to a mismatched call site; fix the call site (likely none outside `solve.rs`).

- [ ] **Step 1.14: Run lint.**

```
mise run lint
```

Expected: green.

- [ ] **Step 1.15: Commit.**

```bash
git add solver/solver-core/src/types.rs solver/solver-core/src/solve.rs \
        solver/solver-core/src/lahc.rs solver/solver-core/src/lib.rs \
        solver/solver-core/tests/lahc_property.rs
git commit -m "feat(solver-core): SolveStats with ttf/tto probes via solve_with_config_stats (item 30)"
```

---

## Task 2: cpsat python observability fields

**Files:**

- Modify: `solver/solver-py/python/klassenzeit_solver/cpsat.py`
- Modify: `solver/solver-py/tests/test_cpsat.py`

- [ ] **Step 2.1: Write the failing OPTIMAL test.**

Append to `solver/solver-py/tests/test_cpsat.py` (after the existing tests, before EOF):

```python
def test_solve_cpsat_json_emits_observability_fields_when_optimal() -> None:
    out = solve_cpsat_json(_cpsat_trivial_one_lesson_problem(), deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["violations"] == []
    assert "peak_rss_kb" in sol
    assert isinstance(sol["peak_rss_kb"], int)
    assert sol["peak_rss_kb"] > 0
    assert isinstance(sol["time_to_first_feasible_ms"], float)
    assert sol["time_to_first_feasible_ms"] >= 0.0
    assert isinstance(sol["time_to_optimal_ms"], float)
    assert sol["time_to_optimal_ms"] >= 0.0
    assert sol["time_to_first_feasible_ms"] <= sol["time_to_optimal_ms"] + 1e-6


def test_solve_cpsat_json_omits_tto_when_not_optimal() -> None:
    out = solve_cpsat_json(_cpsat_infeasible_problem(), deadline_ms=2_000)
    sol = json.loads(out)
    assert sol["placements"] == []
    assert "peak_rss_kb" in sol
    assert isinstance(sol["peak_rss_kb"], int)
    assert sol["peak_rss_kb"] > 0
    assert sol["time_to_first_feasible_ms"] is None
    assert sol["time_to_optimal_ms"] is None
```

- [ ] **Step 2.2: Run the new tests; verify they fail.**

```
mise run solver:rebuild
uv run pytest solver/solver-py/tests/test_cpsat.py::test_solve_cpsat_json_emits_observability_fields_when_optimal solver/solver-py/tests/test_cpsat.py::test_solve_cpsat_json_omits_tto_when_not_optimal -v
```

Expected: both fail with `KeyError: 'peak_rss_kb'` or similar.

- [ ] **Step 2.3: Modify `cpsat.py` to add the callback, peak helper, and three new JSON fields.**

Replace the relevant sections of `solver/solver-py/python/klassenzeit_solver/cpsat.py`:

Add at the top with the other imports:

```python
import resource
import sys
```

(Keep existing `import sys` if already present; the existing file already imports `sys`.)

Insert a new class definition above `solve_cpsat_json`:

```python
class _FirstSolutionCallback(cp_model.CpSolverSolutionCallback):
    """Records ``solver.WallTime() * 1000`` on the first feasible solution."""

    def __init__(self) -> None:
        super().__init__()
        self.first_ms: float | None = None

    def on_solution_callback(self) -> None:
        if self.first_ms is None:
            self.first_ms = self.WallTime() * 1000.0


def _read_peak_rss_kb() -> int:
    raw = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return raw // 1024 if sys.platform == "darwin" else raw
```

Modify `solve_cpsat_json` to use the callback and emit the new fields. Replace the body's `status = solver.solve(model)` plus the OPTIMAL/FEASIBLE branch and the INFEASIBLE/UNKNOWN branch:

```python
    callback = _FirstSolutionCallback()
    status = solver.solve(model, callback)
    peak_rss_kb = _read_peak_rss_kb()

    if status in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        placements = _extract_placements(solver, anchor_vars, meta)
        soft_score = score_solution_json(problem_json, json.dumps(placements))
        ttf = callback.first_ms
        tto = solver.WallTime() * 1000.0 if status == cp_model.OPTIMAL else None
        return json.dumps(
            {
                "placements": placements,
                "violations": [],
                "soft_score": int(soft_score),
                "peak_rss_kb": peak_rss_kb,
                "time_to_first_feasible_ms": ttf,
                "time_to_optimal_ms": tto,
            }
        )
    if status in (cp_model.INFEASIBLE, cp_model.UNKNOWN):
        status_name = solver.status_name(status).lower()
        reason = f"cpsat: {status_name}"
        violations = []
        for lesson in problem["lessons"]:
            for hour in range(lesson["hours_per_week"]):
                violations.append(
                    {
                        "kind": "no_free_time_block",
                        "lesson_id": lesson["id"],
                        "hour_index": hour,
                        "reason": reason,
                    }
                )
        return json.dumps(
            {
                "placements": [],
                "violations": violations,
                "soft_score": 0,
                "peak_rss_kb": peak_rss_kb,
                "time_to_first_feasible_ms": None,
                "time_to_optimal_ms": None,
            }
        )
```

Leave the MODEL_INVALID and unexpected-status branches unchanged.

- [ ] **Step 2.4: Rebuild and run the new tests.**

```
mise run solver:rebuild
uv run pytest solver/solver-py/tests/test_cpsat.py::test_solve_cpsat_json_emits_observability_fields_when_optimal solver/solver-py/tests/test_cpsat.py::test_solve_cpsat_json_omits_tto_when_not_optimal -v
```

Expected: both PASS.

- [ ] **Step 2.5: Run the full cpsat test module to confirm no regression.**

```
uv run pytest solver/solver-py/tests/test_cpsat.py -v
```

Expected: all PASS.

- [ ] **Step 2.6: Run lint.**

```
mise run lint
```

Expected: green.

- [ ] **Step 2.7: Commit.**

```bash
git add solver/solver-py/python/klassenzeit_solver/cpsat.py solver/solver-py/tests/test_cpsat.py
git commit -m "feat(klassenzeit_solver): cpsat reports peak_rss_kb, ttf, tto in output JSON (item 30)"
```

---

## Task 3: solver-bench supervisor + cell-child mode + observability columns

**Files:**

- Modify: `Cargo.toml` (workspace root): add `libc` and `serde` derive feature to `[workspace.dependencies]` if missing.
- Modify: `solver/solver-bench/Cargo.toml`
- Modify: `solver/solver-bench/src/main.rs`
- Create: `solver/solver-bench/tests/end_to_end.rs`

- [ ] **Step 3.1: Inspect workspace Cargo.toml to confirm dep state.**

```
grep -n 'libc\|^serde \|^serde =\|^serde\b' Cargo.toml
```

If `libc` is missing under `[workspace.dependencies]`, add `libc = "0.2"`. If `serde` is present without `derive`, add the feature; or add a fresh entry: `serde = { version = "1", features = ["derive"] }`. If `serde` is unused at the workspace level today, add the workspace entry as `serde = { version = "1", features = ["derive"] }`.

- [ ] **Step 3.2: Add deps to solver-bench Cargo.toml.**

Modify `solver/solver-bench/Cargo.toml` `[dependencies]`:

```toml
[dependencies]
solver-core = { path = "../solver-core", version = "0.1.0" }
serde       = { workspace = true, features = ["derive"] }
serde_json  = { workspace = true }
libc        = { workspace = true }
```

- [ ] **Step 3.3: Write the failing supervisor smoke test.**

Create `solver/solver-bench/tests/end_to_end.rs`:

```rust
//! End-to-end smoke for the solver-bench supervisor + cell-child split.
//! Spawns the supervisor binary at a tiny budget/seeds count and asserts the
//! markdown output includes the three observability columns.

use std::path::PathBuf;
use std::process::Command;

fn unique_outfile(label: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("kz-bench-end-to-end-{label}-{nanos}.md"))
}

#[test]
fn supervisor_emits_three_new_columns_in_markdown() {
    let out = unique_outfile("columns");
    let status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "200ms",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor");
    assert!(status.success(), "supervisor exited non-zero");
    let body = std::fs::read_to_string(&out).expect("read markdown output");
    assert!(body.contains("Peak RSS (kB)"), "missing peak column header: {body}");
    assert!(
        body.contains("Time to first feasible"),
        "missing ttf column header: {body}"
    );
    assert!(body.contains("Time to optimal"), "missing tto column header: {body}");
    let _ = std::fs::remove_file(&out);
}
```

- [ ] **Step 3.4: Run it; verify failure.**

```
cargo nextest run -p solver-bench --test end_to_end
```

Expected: build fails or test fails because the markdown header still has 9 columns.

- [ ] **Step 3.5: Rewrite `solver/solver-bench/src/main.rs` (full file).**

Replace the file with the supervisor + cell-child split. The new shape:

```rust
//! Solver feasibility bake-off bench harness.
//!
//! Two-mode binary:
//! - Supervisor (default): parses CLI, spawns one `solver-bench --cell ...`
//!   child per `(fixture, backend)` pair, collects each cell's CellResult JSON,
//!   writes a markdown table to BENCH_RESULTS.md.
//! - Cell-child (`--cell ...`): runs the seed loop for one (fixture, backend)
//!   pair, reads its own peak RSS via `getrusage(RUSAGE_SELF)`, prints one
//!   CellResult JSON object on stdout, exits.
//!
//! ADR: docs/adr/0034-bench-cell-subprocess-and-observability.md

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use solver_core::solve_with_config_stats;
use solver_core::test_fixtures::{
    dreizuegig_fixture, ffd_lock_in_grundschule, grundschule_fixture, zweizuegig_fixture,
};
use solver_core::types::{Problem, SolveConfig, SolveStats};
use solver_core::PRODUCTION_ACTIVE_WEIGHTS;

fn placements_expected_for_problem(problem: &Problem) -> u64 {
    problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as u64)
        .sum()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchBackend {
    Lahc,
    LahcRr,
    LahcRrKempe,
    CpSat,
}

impl BenchBackend {
    fn label(self) -> &'static str {
        match self {
            BenchBackend::Lahc => "lahc",
            BenchBackend::LahcRr => "lahc_rr",
            BenchBackend::LahcRrKempe => "lahc_rr_kempe",
            BenchBackend::CpSat => "cpsat",
        }
    }
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "lahc" => Ok(Self::Lahc),
            "lahc_rr" => Ok(Self::LahcRr),
            "lahc_rr_kempe" => Ok(Self::LahcRrKempe),
            "cpsat" => Ok(Self::CpSat),
            other => Err(format!("unknown backend '{other}'")),
        }
    }
    const ALL: [Self; 4] = [Self::Lahc, Self::LahcRr, Self::LahcRrKempe, Self::CpSat];
}

type FixtureEntry = (&'static str, fn() -> Problem);

const FIXTURES: &[FixtureEntry] = &[
    ("grundschule", grundschule_fixture),
    ("zweizuegig", zweizuegig_fixture),
    ("dreizuegig", dreizuegig_fixture),
    ("lock_in", ffd_lock_in_grundschule),
];

fn fixture_by_name(name: &str) -> Option<fn() -> Problem> {
    FIXTURES.iter().find(|(n, _)| *n == name).map(|(_, f)| *f)
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CellResult {
    seeds: u64,
    feasibility_count: u64,
    hard_violations_median: u32,
    placements_total_median: u64,
    placements_expected: u64,
    soft_score_median: Option<u32>,
    ffd_ms_median: f64,
    total_ms_median: f64,
    peak_kb: u64,
    time_to_first_feasible_ms_median: Option<f64>,
    time_to_optimal_ms_median: Option<f64>,
}

struct SupervisorArgs {
    budget: Duration,
    seeds: u64,
    fixtures: Vec<String>,
    out: PathBuf,
}

fn default_supervisor_args() -> SupervisorArgs {
    SupervisorArgs {
        budget: Duration::from_secs(60),
        seeds: 20,
        fixtures: FIXTURES.iter().map(|(n, _)| (*n).to_string()).collect(),
        out: PathBuf::from("solver/solver-core/benches/BENCH_RESULTS.md"),
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    if let Some(rest) = s.strip_suffix("ms") {
        rest.parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|e| format!("invalid duration '{s}': {e}"))
    } else if let Some(rest) = s.strip_suffix('s') {
        rest.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|e| format!("invalid duration '{s}': {e}"))
    } else {
        Err(format!("invalid duration '{s}': expect '<n>s' or '<n>ms'"))
    }
}

fn parse_supervisor_args(raw: Vec<String>) -> Result<SupervisorArgs, String> {
    let mut args = default_supervisor_args();
    let mut iter = raw.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--budget" => {
                let v = iter.next().ok_or("--budget needs a value")?;
                args.budget = parse_duration(&v)?;
            }
            "--seeds" => {
                let v = iter.next().ok_or("--seeds needs a value")?;
                args.seeds = v
                    .parse::<u64>()
                    .map_err(|e| format!("--seeds must be a positive integer: {e}"))?;
            }
            "--fixtures" => {
                let v = iter.next().ok_or("--fixtures needs a value")?;
                args.fixtures = v.split(',').map(str::to_string).collect();
            }
            "--out" => {
                let v = iter.next().ok_or("--out needs a value")?;
                args.out = PathBuf::from(v);
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    Ok(args)
}

struct CellArgs {
    fixture: String,
    backend: BenchBackend,
    budget: Duration,
    seeds: u64,
}

fn parse_cell_args(raw: Vec<String>) -> Result<CellArgs, String> {
    let mut fixture: Option<String> = None;
    let mut backend: Option<BenchBackend> = None;
    let mut budget: Option<Duration> = None;
    let mut seeds: Option<u64> = None;
    let mut iter = raw.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--cell" => {
                fixture = Some(iter.next().ok_or("--cell needs a fixture name")?);
            }
            "--backend" => {
                backend = Some(BenchBackend::parse(
                    &iter.next().ok_or("--backend needs a value")?,
                )?);
            }
            "--budget" => {
                budget = Some(parse_duration(&iter.next().ok_or("--budget needs a value")?)?);
            }
            "--seeds" => {
                seeds = Some(
                    iter.next()
                        .ok_or("--seeds needs a value")?
                        .parse::<u64>()
                        .map_err(|e| format!("--seeds must be a positive integer: {e}"))?,
                );
            }
            other => return Err(format!("unknown cell flag '{other}'")),
        }
    }
    Ok(CellArgs {
        fixture: fixture.ok_or("--cell <fixture> required")?,
        backend: backend.ok_or("--backend <name> required")?,
        budget: budget.ok_or("--budget <dur> required")?,
        seeds: seeds.ok_or("--seeds <n> required")?,
    })
}

fn main() -> ExitCode {
    let raw: Vec<String> = env::args().skip(1).collect();
    if matches!(raw.first().map(|s| s.as_str()), Some("--cell")) {
        return run_cell_child(raw);
    }
    run_supervisor(raw)
}

fn run_supervisor(raw: Vec<String>) -> ExitCode {
    let args = match parse_supervisor_args(raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("solver-bench: {e}");
            return ExitCode::from(2);
        }
    };
    let mut markdown = String::new();
    write_header(&mut markdown);

    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("solver-bench: cannot resolve current exe: {e}");
            return ExitCode::FAILURE;
        }
    };

    for (name, _build) in FIXTURES {
        if !args.fixtures.iter().any(|f| f == name) {
            continue;
        }
        for backend in &BenchBackend::ALL {
            eprintln!("cell start: {} / {}", name, backend.label());
            let cell = match spawn_cell(&exe, name, *backend, args.budget, args.seeds) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("cell error: {} / {}: {e}", name, backend.label());
                    return ExitCode::FAILURE;
                }
            };
            eprintln!(
                "cell done: {} / {} feasibility {}/{} hard_med={} placements_med={}/{} \
                 soft_med={} total_ms_med={:.0} peak_kb={} ttf_med={} tto_med={}",
                name,
                backend.label(),
                cell.feasibility_count,
                cell.seeds,
                cell.hard_violations_median,
                cell.placements_total_median,
                cell.placements_expected,
                cell.soft_score_median
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                cell.total_ms_median,
                cell.peak_kb,
                cell.time_to_first_feasible_ms_median
                    .map(|v| format!("{:.0}", v))
                    .unwrap_or_else(|| "-".to_string()),
                cell.time_to_optimal_ms_median
                    .map(|v| format!("{:.0}", v))
                    .unwrap_or_else(|| "-".to_string()),
            );
            write_row(&mut markdown, name, *backend, &cell);
        }
    }

    write_footer(&mut markdown);

    if let Err(e) = fs::write(&args.out, &markdown) {
        eprintln!("solver-bench: failed to write {:?}: {e}", args.out);
        return ExitCode::FAILURE;
    }
    eprintln!("wrote {:?}", args.out);
    ExitCode::SUCCESS
}

fn spawn_cell(
    exe: &Path,
    fixture: &str,
    backend: BenchBackend,
    budget: Duration,
    seeds: u64,
) -> Result<CellResult, String> {
    let budget_str = if budget < Duration::from_secs(1) {
        format!("{}ms", budget.as_millis())
    } else {
        format!("{}s", budget.as_secs())
    };
    let mut cmd = Command::new(exe);
    cmd.arg("--cell")
        .arg(fixture)
        .arg("--backend")
        .arg(backend.label())
        .arg("--budget")
        .arg(&budget_str)
        .arg("--seeds")
        .arg(seeds.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let child = cmd.spawn().map_err(|e| format!("spawn cell: {e}"))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait cell: {e}"))?;
    if !output.status.success() {
        return Err(format!("cell exited with {:?}", output.status));
    }
    let stdout =
        std::str::from_utf8(&output.stdout).map_err(|e| format!("cell stdout utf-8: {e}"))?;
    serde_json::from_str(stdout.trim()).map_err(|e| format!("cell JSON: {e}; raw: {stdout}"))
}

fn run_cell_child(raw: Vec<String>) -> ExitCode {
    let args = match parse_cell_args(raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("solver-bench --cell: {e}");
            return ExitCode::from(2);
        }
    };
    let build = match fixture_by_name(&args.fixture) {
        Some(b) => b,
        None => {
            eprintln!("solver-bench --cell: unknown fixture '{}'", args.fixture);
            return ExitCode::from(2);
        }
    };
    let problem = build();
    let expected = placements_expected_for_problem(&problem);
    let cell = match args.backend {
        BenchBackend::CpSat => run_cpsat_cell(&problem, expected, args.budget, args.seeds),
        _ => run_lahc_cell(args.backend, &problem, expected, args.budget, args.seeds),
    };
    let json = serde_json::to_string(&cell).expect("serialise CellResult");
    let mut stdout = std::io::stdout().lock();
    if let Err(e) = stdout.write_all(json.as_bytes()) {
        eprintln!("solver-bench --cell: write stdout: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn read_self_peak_kb() -> u64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if rc != 0 {
        return 0;
    }
    let raw = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        raw / 1024
    } else {
        raw
    }
}

fn run_lahc_cell(
    backend: BenchBackend,
    problem: &Problem,
    expected: u64,
    budget: Duration,
    seeds: u64,
) -> CellResult {
    let weights = PRODUCTION_ACTIVE_WEIGHTS.clone();
    let greedy_cfg = SolveConfig {
        weights: weights.clone(),
        deadline: None,
        ..SolveConfig::default()
    };

    let ffd_start = Instant::now();
    let _greedy = solve_with_config_stats(problem, &greedy_cfg).expect("greedy solve");
    let ffd_ms = ffd_start.elapsed().as_secs_f64() * 1_000.0;

    let mut total_ms_samples: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut hard_violations_samples: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut placements_total_samples: Vec<u64> = Vec::with_capacity(seeds as usize);
    let mut soft_score_feasible: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut ttf_feasible: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut tto_feasible: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut feasibility_count: u64 = 0;

    let (lahc_rr_period, lahc_kempe_period) = match backend {
        BenchBackend::Lahc => (None, None),
        BenchBackend::LahcRr => (Some(25u32), None),
        BenchBackend::LahcRrKempe => (Some(25u32), Some(23u32)),
        BenchBackend::CpSat => unreachable!("cpsat dispatched above"),
    };

    for seed in 1..=seeds {
        let cfg = SolveConfig {
            weights: weights.clone(),
            deadline: Some(budget),
            seed,
            lahc_rr_period,
            lahc_kempe_period,
            ..SolveConfig::default()
        };
        let start = Instant::now();
        let (solution, stats) = solve_with_config_stats(problem, &cfg).expect("solve");
        let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
        let hard = solution.violations.len() as u32;
        let placements_total = solution.placements.len() as u64;
        debug_assert!(placements_total <= expected);
        let feasible = hard == 0 && placements_total == expected;
        if feasible {
            feasibility_count += 1;
            soft_score_feasible.push(solution.soft_score);
            if let Some(t) = stats.time_to_first_feasible_ms {
                ttf_feasible.push(t);
            }
            if let Some(t) = stats.time_to_optimal_ms {
                tto_feasible.push(t);
            }
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(placements_total);
    }

    CellResult {
        seeds,
        feasibility_count,
        hard_violations_median: median_u32(&mut hard_violations_samples),
        placements_total_median: median_u64(&mut placements_total_samples),
        placements_expected: expected,
        soft_score_median: if soft_score_feasible.is_empty() {
            None
        } else {
            Some(median_u32(&mut soft_score_feasible))
        },
        ffd_ms_median: ffd_ms,
        total_ms_median: median_f64(&mut total_ms_samples),
        peak_kb: read_self_peak_kb(),
        time_to_first_feasible_ms_median: if ttf_feasible.is_empty() {
            None
        } else {
            Some(median_f64(&mut ttf_feasible))
        },
        time_to_optimal_ms_median: if tto_feasible.is_empty() {
            None
        } else {
            Some(median_f64(&mut tto_feasible))
        },
    }
}

fn build_cpsat_command(problem_path: &Path, budget: Duration, seed: u64) -> Command {
    let mut cmd = Command::new("python3");
    cmd.arg("-m")
        .arg("klassenzeit_solver.cpsat")
        .arg("--problem-file")
        .arg(problem_path)
        .arg("--deadline-ms")
        .arg(budget.as_millis().to_string())
        .arg("--seed")
        .arg(seed.to_string());
    cmd
}

fn tempfile_path(prefix: &str, suffix: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}{nanos}{suffix}"))
}

fn run_cpsat_cell(problem: &Problem, expected: u64, budget: Duration, seeds: u64) -> CellResult {
    let problem_json =
        serde_json::to_string(problem).expect("serialise problem for cpsat subprocess");
    let tmpfile = tempfile_path("kz-bench-problem-", ".json");
    std::fs::write(&tmpfile, problem_json.as_bytes()).expect("write problem tempfile");

    let mut total_ms_samples: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut hard_violations_samples: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut placements_total_samples: Vec<u64> = Vec::with_capacity(seeds as usize);
    let mut soft_score_feasible: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut ttf_feasible: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut tto_feasible: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut feasibility_count: u64 = 0;
    let mut peak_kb_max: u64 = 0;

    #[derive(Deserialize)]
    struct CpSatJson {
        placements: serde_json::Value,
        violations: Vec<serde_json::Value>,
        soft_score: u32,
        peak_rss_kb: Option<u64>,
        time_to_first_feasible_ms: Option<f64>,
        time_to_optimal_ms: Option<f64>,
    }

    for seed in 1..=seeds {
        let start = Instant::now();
        let result = build_cpsat_command(&tmpfile, budget, seed).output();
        let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
        let solution_json = match result {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            Ok(o) => {
                eprintln!(
                    "cpsat subprocess non-zero exit (seed={seed}): {}",
                    String::from_utf8_lossy(&o.stderr)
                );
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                placements_total_samples.push(0);
                continue;
            }
            Err(e) => {
                eprintln!("cpsat subprocess error (seed={seed}): {e}");
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                placements_total_samples.push(0);
                continue;
            }
        };
        let parsed: CpSatJson = match serde_json::from_str(&solution_json) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cpsat parse error (seed={seed}): {e}");
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                placements_total_samples.push(0);
                continue;
            }
        };
        let placements_total = parsed
            .placements
            .as_array()
            .map(|a| a.len() as u64)
            .unwrap_or(0);
        let hard = parsed.violations.len() as u32;
        debug_assert!(placements_total <= expected);
        let feasible = hard == 0 && placements_total == expected;
        if feasible {
            feasibility_count += 1;
            soft_score_feasible.push(parsed.soft_score);
            if let Some(t) = parsed.time_to_first_feasible_ms {
                ttf_feasible.push(t);
            }
            if let Some(t) = parsed.time_to_optimal_ms {
                tto_feasible.push(t);
            }
        }
        if let Some(p) = parsed.peak_rss_kb {
            peak_kb_max = peak_kb_max.max(p);
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(placements_total);
    }

    let _ = std::fs::remove_file(&tmpfile);

    CellResult {
        seeds,
        feasibility_count,
        hard_violations_median: median_u32(&mut hard_violations_samples),
        placements_total_median: median_u64(&mut placements_total_samples),
        placements_expected: expected,
        soft_score_median: if soft_score_feasible.is_empty() {
            None
        } else {
            Some(median_u32(&mut soft_score_feasible))
        },
        ffd_ms_median: 0.0,
        total_ms_median: median_f64(&mut total_ms_samples),
        peak_kb: peak_kb_max,
        time_to_first_feasible_ms_median: if ttf_feasible.is_empty() {
            None
        } else {
            Some(median_f64(&mut ttf_feasible))
        },
        time_to_optimal_ms_median: if tto_feasible.is_empty() {
            None
        } else {
            Some(median_f64(&mut tto_feasible))
        },
    }
}

fn median_u32(values: &mut [u32]) -> u32 {
    values.sort_unstable();
    let mid = values.len() / 2;
    values[mid]
}

fn median_u64(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    let mid = values.len() / 2;
    values[mid]
}

fn median_f64(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    values[mid]
}

fn write_header(out: &mut String) {
    out.push_str("# Solver bake-off feasibility bench\n\n");
    out.push_str("<!-- Regenerated by `mise run bench:bakeoff`. Do not hand-edit. -->\n\n");
    out.push_str(
        "| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Placements (median / expected) | Soft score (median, feasible) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) | Peak RSS (kB) | Time to first feasible (ms, median) | Time to optimal (ms, median) |\n",
    );
    out.push_str(
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
}

fn write_row(out: &mut String, fixture: &str, backend: BenchBackend, cell: &CellResult) {
    let soft = match cell.soft_score_median {
        Some(s) => s.to_string(),
        None => "-".to_string(),
    };
    let ttf = match cell.time_to_first_feasible_ms_median {
        Some(v) => format!("{v:.0}"),
        None => "-".to_string(),
    };
    let tto = match cell.time_to_optimal_ms_median {
        Some(v) => format!("{v:.0}"),
        None => "-".to_string(),
    };
    out.push_str(&format!(
        "| {fixture} | {backend} | {seeds} | {n}/{seeds} | {hard} | {placed}/{expected} | {soft} | {ffd:.2} | {total:.0} | {peak} | {ttf} | {tto} |\n",
        backend = backend.label(),
        seeds = cell.seeds,
        n = cell.feasibility_count,
        hard = cell.hard_violations_median,
        placed = cell.placements_total_median,
        expected = cell.placements_expected,
        ffd = cell.ffd_ms_median,
        total = cell.total_ms_median,
        peak = cell.peak_kb,
    ));
}

fn write_footer(out: &mut String) {
    let cpu = read_cpu().unwrap_or_else(|| "unknown".to_string());
    let kernel = read_kernel().unwrap_or_else(|| "unknown".to_string());
    let rustc = read_rustc().unwrap_or_else(|| "unknown".to_string());
    let date = chrono_today();
    out.push('\n');
    out.push_str(&format!(
        "Refreshed {date} on {cpu}, Linux {kernel}, {rustc}.\n\n"
    ));
    out.push_str(
        "Refresh with `mise run bench:bakeoff` when a backend changes or a fixture is added. The\n",
    );
    out.push_str(
        "bench is host-sensitive on wall-clock and Peak RSS columns and host-stable on feasibility / hard-violation\n",
    );
    out.push_str(
        "columns. Each cell runs in its own subprocess so Peak RSS reflects only that cell. Linux\n",
    );
    out.push_str(
        "`ru_maxrss` is kilobytes; macOS is bytes (the bench normalises to kilobytes). Time to first\n",
    );
    out.push_str(
        "feasible and Time to optimal are medians over feasible seeds; '-' marks no feasible seed.\n\n",
    );
    out.push_str("See `docs/adr/0029-solver-feasibility-bake-off.md` for methodology and `docs/adr/0034-bench-cell-subprocess-and-observability.md` for the cell-subprocess architecture.\n");
}

fn read_cpu() -> Option<String> {
    fs::read_to_string("/proc/cpuinfo").ok().and_then(|c| {
        c.lines()
            .find_map(|l| l.strip_prefix("model name").and_then(|s| s.split_once(':')))
            .map(|(_, v)| v.trim().to_string())
    })
}

fn read_kernel() -> Option<String> {
    Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn read_rustc() -> Option<String> {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn chrono_today() -> String {
    Command::new("date")
        .args(["-Idate"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_accepts_seconds_and_milliseconds() {
        assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("250ms").unwrap(), Duration::from_millis(250));
        assert!(parse_duration("60").is_err());
    }

    #[test]
    fn parse_supervisor_args_reads_all_flags() {
        let raw = vec![
            "--budget".to_string(),
            "5s".to_string(),
            "--seeds".to_string(),
            "4".to_string(),
            "--fixtures".to_string(),
            "grundschule,lock_in".to_string(),
            "--out".to_string(),
            "/tmp/out.md".to_string(),
        ];
        let args = parse_supervisor_args(raw).unwrap();
        assert_eq!(args.budget, Duration::from_secs(5));
        assert_eq!(args.seeds, 4);
        assert_eq!(
            args.fixtures,
            vec!["grundschule".to_string(), "lock_in".to_string()]
        );
        assert_eq!(args.out, PathBuf::from("/tmp/out.md"));
    }

    #[test]
    fn parse_supervisor_args_rejects_unknown_flag() {
        let raw = vec!["--unknown".to_string()];
        assert!(parse_supervisor_args(raw).is_err());
    }

    #[test]
    fn parse_cell_args_reads_fixture_backend_budget_seeds() {
        let raw = vec![
            "--cell".to_string(),
            "grundschule".to_string(),
            "--backend".to_string(),
            "lahc_rr".to_string(),
            "--budget".to_string(),
            "200ms".to_string(),
            "--seeds".to_string(),
            "3".to_string(),
        ];
        let args = parse_cell_args(raw).unwrap();
        assert_eq!(args.fixture, "grundschule");
        assert_eq!(args.backend, BenchBackend::LahcRr);
        assert_eq!(args.budget, Duration::from_millis(200));
        assert_eq!(args.seeds, 3);
    }

    #[test]
    fn median_u32_returns_middle_value() {
        let mut v = vec![5, 1, 3];
        assert_eq!(median_u32(&mut v), 3);
    }

    #[test]
    fn write_header_includes_three_new_columns() {
        let mut out = String::new();
        write_header(&mut out);
        assert!(out.contains("Peak RSS (kB)"), "missing peak header: {out}");
        assert!(
            out.contains("Time to first feasible"),
            "missing ttf header: {out}"
        );
        assert!(out.contains("Time to optimal"), "missing tto header: {out}");
    }

    #[test]
    fn write_row_renders_observability_columns() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 20,
            hard_violations_median: 0,
            placements_total_median: 45,
            placements_expected: 45,
            soft_score_median: Some(0),
            ffd_ms_median: 0.13,
            total_ms_median: 60000.0,
            peak_kb: 49152,
            time_to_first_feasible_ms_median: Some(0.4),
            time_to_optimal_ms_median: Some(1500.0),
        };
        let mut out = String::new();
        write_row(&mut out, "grundschule", BenchBackend::LahcRrKempe, &cell);
        assert!(out.contains("| 49152 |"), "missing peak: {out}");
        assert!(out.contains("| 0 |"), "missing ttf rounded to 0 ms: {out}");
        assert!(out.contains("| 1500 |"), "missing tto: {out}");
    }

    #[test]
    fn write_row_renders_dash_when_no_feasible_seed() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 0,
            hard_violations_median: 1,
            placements_total_median: 0,
            placements_expected: 0,
            soft_score_median: None,
            ffd_ms_median: 0.05,
            total_ms_median: 60050.0,
            peak_kb: 49152,
            time_to_first_feasible_ms_median: None,
            time_to_optimal_ms_median: None,
        };
        let mut out = String::new();
        write_row(&mut out, "grundschule", BenchBackend::Lahc, &cell);
        assert!(out.contains("| 0/20 |"));
        assert!(out.contains("| 49152 |"));
        // Three dash-cells in a row: soft_score, ttf, tto.
        assert!(out.contains("| - |"));
    }

    #[test]
    fn cell_result_round_trips_through_json() {
        let cell = CellResult {
            seeds: 4,
            feasibility_count: 4,
            hard_violations_median: 0,
            placements_total_median: 45,
            placements_expected: 45,
            soft_score_median: Some(15),
            ffd_ms_median: 0.5,
            total_ms_median: 60000.0,
            peak_kb: 12345,
            time_to_first_feasible_ms_median: Some(2.5),
            time_to_optimal_ms_median: Some(40.0),
        };
        let s = serde_json::to_string(&cell).unwrap();
        let back: CellResult = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cell);
    }

    #[test]
    fn placements_expected_for_problem_sums_hours_per_week() {
        let problem = grundschule_fixture();
        let manual_sum: u64 = problem
            .lessons
            .iter()
            .map(|l| l.hours_per_week as u64)
            .sum();
        assert_eq!(placements_expected_for_problem(&problem), manual_sum);
        assert_eq!(placements_expected_for_problem(&problem), 45);
    }

    #[test]
    fn cpsat_subprocess_command_args_match_module_invocation() {
        let cmd = build_cpsat_command(
            std::path::Path::new("/tmp/p.json"),
            Duration::from_secs(60),
            7,
        );
        let argv: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            argv,
            vec![
                "-m".to_string(),
                "klassenzeit_solver.cpsat".to_string(),
                "--problem-file".to_string(),
                "/tmp/p.json".to_string(),
                "--deadline-ms".to_string(),
                "60000".to_string(),
                "--seed".to_string(),
                "7".to_string(),
            ]
        );
        assert_eq!(cmd.get_program(), "python3");
    }
}
```

- [ ] **Step 3.6: Build to surface any compile errors.**

```
cargo build -p solver-bench --release
```

Fix any compile errors surfaced (most likely candidates: `libc` import path differences across Linux/macOS, `serde` derive feature missing).

- [ ] **Step 3.7: Run the unit tests.**

```
cargo nextest run -p solver-bench --lib
```

Expected: all unit tests PASS.

- [ ] **Step 3.8: Run the end-to-end smoke.**

```
cargo nextest run -p solver-bench --test end_to_end
```

Expected: PASS. The smoke test runs grundschule/lahc, lahc_rr, lahc_rr_kempe, cpsat at 200ms × 1 seed each, ~3 s total wall-clock.

- [ ] **Step 3.9: Run the full Rust test suite + lint.**

```
mise run test:rust && mise run lint
```

Expected: green.

- [ ] **Step 3.10: Commit.**

```bash
git add Cargo.toml solver/solver-bench/Cargo.toml solver/solver-bench/src/main.rs solver/solver-bench/tests/end_to_end.rs
git commit -m "feat(solver-bench): subprocess-per-cell mode + observability columns (item 30)"
```

If the workspace `Cargo.lock` updates, include it: `git add Cargo.lock`.

---

## Task 4: Low-budget BENCH_RESULTS.md shape demo

**Files:**

- Modify: `solver/solver-core/benches/BENCH_RESULTS.md`

- [ ] **Step 4.1: Run the bake-off bench at low budget.**

```
mise run bench:bakeoff -- --budget 5s --seeds 4
```

This runs all 4 fixtures × 4 backends with 4 seeds each at 5 s budget for LAHC and 5 s deadline for cpsat. Wall-clock estimate: 16 cells × ~5 s LAHC mean (or up to 5 s cpsat mean) = ~5 to 10 minutes.

- [ ] **Step 4.2: Append the shape-demo footer addendum.**

The bench writes `solver/solver-core/benches/BENCH_RESULTS.md` directly. Open the file and add one line at the bottom:

```
_Shape demo at low budget/seeds (`--budget 5s --seeds 4`); production refresh queued as OPEN_THINGS item 42._
```

- [ ] **Step 4.3: Sanity-check the file has 12 columns and the new headers.**

```
head -10 solver/solver-core/benches/BENCH_RESULTS.md
grep -c '| Peak RSS (kB) |' solver/solver-core/benches/BENCH_RESULTS.md
```

Expected: `1` (one header line carrying the new column).

- [ ] **Step 4.4: Commit.**

```bash
git add solver/solver-core/benches/BENCH_RESULTS.md
git commit -m "chore(solver): low-budget BENCH_RESULTS.md shape demo with new columns (item 30)"
```

---

## Task 5: ADR 0034

**Files:**

- Create: `docs/adr/0034-bench-cell-subprocess-and-observability.md`
- Modify: `docs/adr/README.md`

- [ ] **Step 5.1: Write the ADR.**

Create `docs/adr/0034-bench-cell-subprocess-and-observability.md` from `docs/adr/template.md`. Use a colon (not em-dash) in the title per the project's prose rule for new ADRs.

```markdown
# 0034: Bench cell-subprocess architecture and observability columns

Date: 2026-05-06

## Status

Accepted.

## Context

The solver bake-off bench (`mise run bench:bakeoff`, `solver-bench`) was a single Rust process that ran every `(fixture, backend)` cell in sequence and emitted one markdown table to `BENCH_RESULTS.md`. Production-default decisions on `Settings.solver_backend` need three additional cells per row: peak resident-set size during the cell, wall-clock to first feasible incumbent, and wall-clock to the run's final soft score. The naive shape "one bench process, `getrusage(RUSAGE_SELF).ru_maxrss` after each in-process LAHC cell" produces monotonic-cumulative numbers across cells: cell N's reported peak is `max(over cells 1..=N)`, not cell N's actual peak. Cells run in size order grundschule, zweizuegig, dreizuegig, lock_in; under monotonic-max, dreizuegig's peak hides lock_in's peak. Cross-backend RAM trade-offs become illegible.

## Decision

Reorganise `solver-bench` into a supervisor and per-cell child process via recursive self-spawn:

- Supervisor (default mode): parses CLI, spawns one `solver-bench --cell <fixture> --backend <name> --budget <d> --seeds <n>` child per cell via `std::env::current_exe()`, captures stdout, parses a `CellResult` JSON object, formats one markdown row per cell, writes the file.
- Cell-child mode (`--cell ...`): runs the seed loop for one (fixture, backend) pair, reads its own peak via `libc::getrusage(libc::RUSAGE_SELF)`, prints one `CellResult` JSON object on stdout, exits.

LAHC stats (`time_to_first_feasible_ms`, `time_to_optimal_ms`) come from a new `solver_core::solve_with_config_stats` that returns `(Solution, SolveStats)`. The existing `solve_with_config` becomes a one-line wrapper that discards stats; production callers (the no-config `solve()` entry, `solve_json_with_config`, the solver-py binding, the backend `solver_io.py`) stay byte-identical.

CP-SAT stats come from a `cp_model.CpSolverSolutionCallback` (first feasible) and `solver.WallTime()` (final at OPTIMAL); the python module reports its own peak RSS via `resource.getrusage(resource.RUSAGE_SELF)`. The output JSON gains three additive fields: `peak_rss_kb`, `time_to_first_feasible_ms`, `time_to_optimal_ms`.

The bench cell-child reads the python child's `peak_rss_kb` from its stdout JSON and takes the max across the seed loop for the cpsat row's `Peak RSS (kB)` column.

## Consequences

Positive:

- Per-cell peak RSS is honest and cross-backend comparable. cpsat's ~50 MB python footprint vs LAHC's sub-MB working set is now legible per fixture.
- LAHC's time-to-first-feasible and time-to-optimal are visible per cell. Together with the existing `Soft score (median, feasible)` column they make ADR 0032 production-default revisits and ADR 0033 deadline tunings fact-based.
- Production callers are unchanged. `solve_with_config` keeps its signature; `SolveStats` is opt-in via `solve_with_config_stats`.

Negative:

- One additional process spawn per cell (~5 ms); invisible against multi-second cell budgets.
- New `libc` dep in `solver-bench`. Deviates from solver/CLAUDE.md "no external runtime deps for solver-bench". Accepted because `libc` is foundational (Rust org-maintained) and the alternatives (`/proc/self/status` parse) are Linux-only and string-fragile.
- `ru_maxrss` units differ across OS (Linux: kilobytes; macOS: bytes). Bench normalises to kilobytes with `cfg!(target_os = "macos")` division; documented in the markdown footer.
- LAHC `time_to_optimal_ms` is the wall-clock of the last running-best improvement, not a proof-of-optimality timestamp. LAHC has no proof of optimality; the field is a lower bound on the iteration count to the final soft score. Documented on the field rustdoc.

## Alternatives considered

1. Read `/proc/self/status:VmHWM` instead of `getrusage`. Linux-only, string-fragile. Rejected.
2. Read `RUSAGE_CHILDREN` deltas around python subprocess invocations. Subtle delta semantics, harder to test than the python-self-report path. Rejected.
3. Fork the cell from inside the supervisor process. Avoids re-exec but introduces unsafe `libc::fork` and pipe-based stats transfer. Rejected.
4. Per-seed records in the cell-child JSON. Forces the supervisor to re-implement the median helpers. Rejected; aggregated `CellResult` is sufficient.

## References

- OPEN_THINGS item 30 (`docs/superpowers/OPEN_THINGS.md`).
- Spec: `docs/superpowers/specs/2026-05-06-bench-observability-columns-design.md`.
- Plan: `docs/superpowers/plans/2026-05-06-bench-observability-columns.md`.
- ADR 0029 (bake-off methodology), ADR 0030 (cpsat layout), ADR 0031 (production default), ADR 0032 (default revisit), ADR 0033 (daily caps + deadline raise).
```

- [ ] **Step 5.2: Index in `docs/adr/README.md`.**

Add one line to the table-of-ADRs in `docs/adr/README.md`, in numeric order:

```markdown
| 0034 | [Bench cell-subprocess architecture and observability columns](0034-bench-cell-subprocess-and-observability.md) |
```

(Match the existing column shape; check the file for the actual table format and copy it.)

- [ ] **Step 5.3: Lint.**

```
mise run lint
```

Expected: green.

- [ ] **Step 5.4: Commit.**

```bash
git add docs/adr/0034-bench-cell-subprocess-and-observability.md docs/adr/README.md
git commit -m "docs(adr): 0034 bench cell-subprocess and observability"
```

---

## Task 6: OPEN_THINGS + auto-memory updates

**Files:**

- Modify: `docs/superpowers/OPEN_THINGS.md`
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/MEMORY.md` (description line for the roadmap entry)

- [ ] **Step 6.1: Delete OPEN_THINGS item 30; advance next-pickup; refine item 42.**

Open `docs/superpowers/OPEN_THINGS.md`. Three edits:

1. In the active-sprint preamble (line 9 today), update the next-pickup sentence. Current text starts with "Next pickup: P0 item 30 ..."; rewrite to point at item 31 (P0, schedule-quality metrics) or, if 31 has secondary blockers, the next P0 in the observability/correctness phases. Keep the rest of the preamble's history intact.
2. Delete item 30's body (the full paragraph numbered "30." in the Observability phase).
3. Refine item 42 in the sprint-tidy phase: append one sentence noting that the new columns are in place after item 30 shipped, and the production refresh now produces 12 columns.

- [ ] **Step 6.2: Update auto-memory roadmap.**

Open `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`. Two edits:

1. Frontmatter `description:` field: replace the "Next pickup is item 30" sentence with the next pickup (item 31, schedule-quality metrics, or whatever pickup the OPEN_THINGS rewrite chose). The description is what the memory loader uses for relevance ranking; do NOT leave it stale.
2. Body: append a paragraph dated 2026-05-06 describing item 30 shipping (subprocess-per-cell + 12-column BENCH_RESULTS.md + ADR 0034) and the next pickup. Mirror the wording style of the existing item-41 paragraph (line ~30 of the file).

- [ ] **Step 6.3: Update MEMORY.md index pointer.**

Open `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/MEMORY.md`. The first line is the roadmap status entry. Update the one-line hook to reflect the new next-pickup. Keep under 150 chars.

- [ ] **Step 6.4: Spot-check no other roadmap memory references item 30 as open.**

```
grep -rn 'item 30\|peak_memory_kb\|time_to_first_feasible\|time_to_optimal' /home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/ docs/superpowers/OPEN_THINGS.md docs/superpowers/specs/ docs/superpowers/plans/ 2>/dev/null | head -20
```

Expected: only this PR's spec/plan/ADR/OPEN_THINGS-historical-text references remain. The auto-memory and the active-sprint preamble should no longer call item 30 the "next pickup".

- [ ] **Step 6.5: Commit.**

```bash
git add docs/superpowers/OPEN_THINGS.md
git commit -m "docs: shrink OPEN_THINGS active sprint after item 30 ships"
```

The auto-memory edits are not under git; they live under `/home/pascal/.claude/...`. Apply them via `Edit`/`Write` directly; no commit needed (the memory dir is outside the repo).

---

## Task 7: solver/CLAUDE.md addendum + repo-level docs sweep

**Files:**

- Modify: `solver/CLAUDE.md`
- Modify (if architecture doc references the bench): `docs/architecture/overview.md`

- [ ] **Step 7.1: Read solver/CLAUDE.md "Bench workflow" section to find the right insertion point.**

```
grep -n 'Bench workflow\|bench:bakeoff\|solver-bench' solver/CLAUDE.md | head -10
```

Insertion point: under "## Bench workflow", after the bullet that explains `mise run bench:bakeoff`'s `cpsat` column behaviour.

- [ ] **Step 7.2: Add the supervisor architecture bullet.**

Append (text shape; adapt indentation to match neighbours):

```markdown
- **`solver-bench` is supervisor + cell-child via recursive self-spawn.** Default mode parses CLI, then spawns one `solver-bench --cell <fixture> --backend <name> --budget <d> --seeds <n>` child per `(fixture, backend)` pair. The cell-child runs the seed loop in-process for LAHC, spawns python per seed for cpsat, reads its own peak RSS via `libc::getrusage(libc::RUSAGE_SELF)` at exit, and emits a single `CellResult` JSON object on stdout. Each cell's peak RSS is honest because each cell runs in its own process; a single-process bench would report monotonic-cumulative `ru_maxrss` instead. ADR 0034. The bench therefore has one runtime dep deviation from the "no external runtime deps" rule above: `libc = "0.2"` for the syscall binding. Documented in ADR 0034.
```

- [ ] **Step 7.3: Skim `docs/architecture/overview.md` for bench references.**

```
grep -n 'solver-bench\|bake-off bench\|BENCH_RESULTS\|peak_memory\|time_to_first\|time_to_optimal' docs/architecture/overview.md
```

If a section already documents the bench harness, add one paragraph noting the supervisor/cell-child split. If not, leave the file alone (per spec out-of-scope).

- [ ] **Step 7.4: Lint.**

```
mise run lint
```

Expected: green.

- [ ] **Step 7.5: Commit.**

```bash
git add solver/CLAUDE.md
[ -f docs/architecture/overview.md ] && git diff --cached --quiet docs/architecture/overview.md || git add docs/architecture/overview.md
git commit -m "docs(claude): document bench supervisor + cell-child architecture"
```

(The `[ -f ... ] || git add` clause is bash-style. If your shell doesn't support it, just run `git add docs/architecture/overview.md` directly when applicable.)

---

## Self-review (filled by the planning author)

**Spec coverage.** Each spec section maps to a task:

- "New public Rust API in `solver-core`" → Task 1.
- "New Python module surface in `klassenzeit_solver.cpsat`" → Task 2.
- "New `solver-bench` architecture" → Task 3.
- "Markdown table refresh" → Task 3 (header / row / footer changes).
- "Tests" → Tasks 1, 2, 3 (each task ships its own tests TDD-first).
- "Low-budget bench refresh" → Task 4.
- "ADR" → Task 5.
- "OPEN_THINGS, auto-memory, solver/CLAUDE.md" → Tasks 6 and 7.

**Placeholder scan.** No "TBD", "TODO", "implement later". Each step has either a code block or an exact command + expected output.

**Type consistency.** `SolveStats` field names match across spec, types.rs change, lahc.rs probes, the `CellResult` JSON shape (`time_to_first_feasible_ms_median`, `time_to_optimal_ms_median` adds the `_median` suffix at the bench layer to mirror the existing `_median` columns), and the python module's `time_to_first_feasible_ms` / `time_to_optimal_ms` (no `_median` suffix because the python module reports per-seed). The bench's cpsat parser uses anonymous-Deserialize on the python output, mapping `time_to_first_feasible_ms` to a per-seed `Option<f64>`; aggregation into `time_to_first_feasible_ms_median` happens inside the cell-child.

**Granularity.** Steps are 2 to 5 minutes each (test, run, code, run, commit). Tasks 1 and 3 are larger because of the wire-shape work; their substeps stay bite-sized.
