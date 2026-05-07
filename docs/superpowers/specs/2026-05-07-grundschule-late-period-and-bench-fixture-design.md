# Grundschule late-period preference, bench-fixture repair, and conditional xfail removal

OPEN_THINGS items addressed: 12 (P0), 15 (P0), 11 (P0, conditional), 14 (P0, conditional).

## Problem

Four open items in the active sprint program ("Solver feasibility correctness + observability") cluster around the demo Grundschule seed and the criterion bench fixture. Three are blocked on each other and one is independent but adjacent:

1. **Item 12.** `Subject.prefer_late_period=5` for FÖ in the demo Grundschule seed was reverted to a no-op in PR #171 because activating it intermittently flaked the einzügig solvability test. The field still exists; the value is `0` everywhere. Item 14 is gated on this signal: the quality-bar test wants to assert "FÖ runs late" and the bench's `late_period_ratio_median` column reads `-` because no fixture has the proxy enabled.
2. **Item 15.** `mise run bench` is currently broken on master. The criterion bench fixture's `solver_greedy/zweizuegig` cell trips `assert!(solution.violations.is_empty())` at `solver-core/benches/solver_fixtures.rs:133` and aborts criterion before `dreizuegig` runs. The OPEN_THINGS bullet attributes this to the same-room hard constraint plus the bench's specific `class_gap=1, teacher_gap=1` weights pushing FFD into a lock-in. `mise run bench:record` fails the same way, so `BASELINE.md` cannot be refreshed end-to-end.
3. **Item 11.** `test_seeded_grundschule_solves_with_auto_assigned_teachers` is `pytest.mark.xfail(strict=False)`. Last measured (2026-05-06) at 9 XPASS / 11 XFAIL on the auto-assign teacher distribution at the production 5000 ms LAHC budget. Failure mode: `validate_no_double_booking` post-condition rejection. OPEN_THINGS instructed re-measurement after items 21+22 ship; those have not shipped, but items 48, 52, 53, 54 have, and the LAHC accept criterion now uses canonical-score (item 52) so the stability picture has shifted.
4. **Item 14.** `test_grundschule_schedule_meets_quality_bar` is `pytest.mark.xfail(strict=False)`. Thresholds are `max_spread=2`, `min_ratio=0.6`, `max_gaps_per_class=2`, `max_position=7` (the OPEN_THINGS spec says `6`; code is currently `7`). Item 13 has shipped (6-period wochenschema), so the `max_position=7` threshold is now a no-op (`max_position=6` matches the spec but is also nearly a no-op on a 6-period grid). Real signal lives in spread / ratio / gaps. Depends on item 12 lighting up the late-period axis and on the LAHC tunings shipped via items 48/52/54.

## Scope

In scope:
- Activate `Subject.prefer_late_period=5` for FÖ in the shared `_SUBJECTS` tuple (`backend/src/klassenzeit_backend/seed/demo_grundschule.py`); cascades to all three Python demo seeds via the existing import contract.
- Mirror the activation in `solver/solver-core/src/test_fixtures.rs` for `zweizuegig_fixture()` and `dreizuegig_fixture()` (FÖ subject prefer_late_period field). The smaller `grundschule_fixture()` has 8 subjects without FÖ and is unaffected.
- Add a fast unit test asserting median FÖ position is `>= 3` on the einzügig demo seed under the production 200 ms LAHC budget.
- Repair the `solver_greedy/zweizuegig` panic by tuning the bench fixture's teacher allocation, room-subject suitabilities, or both, so `mise run bench` runs end-to-end; mirror the change to the Python seed if the fix originates Python-side. Refresh `solver/solver-core/benches/BASELINE.md` via `mise run bench:record`.
- Run flake-loop measurements (per CLAUDE.md's documented pattern) on each xfailed test:
  - Item 11: 20 iterations at 5000 ms LAHC.
  - Item 14: 5 iterations at 200 ms LAHC.
  - Per-test gate: `0 FAIL` across the loop. XPASS counts as PASS (`strict=False` semantics).
- For each test that clears its gate, remove the `pytest.mark.xfail` marker in this PR. For each test that does not, leave the marker and update its body's stability annotation in OPEN_THINGS.
- For item 14 specifically: when removing the xfail (if the gate clears), tighten `max_position` from `7` to `6` per the OPEN_THINGS bullet's intent (item 13 shrunk wochenschema 7→6 periods; the current threshold predates the shrink).

Out of scope:
- Items 21 + 22 (LAHC R&R K + period tuning, standalone `lahc_kempe` backend). Independent of this PR; tracked separately in OPEN_THINGS.
- ADR 0035 production-default revisit (item 47). Requires the post-fix bench data items 21+22 produce.
- Production-shape `mise run bench:bakeoff` refresh (5 hours wall-clock). Smoke shape only in this PR; the production refresh is a post-merge follow-up under item 47's umbrella.
- Item 4 (root-cause fix for the suspected subject-UUID-order leak in `auto_assign_teachers_for_lessons`'s scarcity-first tiebreak). Item 11 measures whether the symptom has gone away; if not, item 4 stays open and item 11's xfail stays.
- Auditing the dreizügig and zweizügig auto-assigned solvability tests (`test_demo_grundschule_dreizuegig_solvability.py`, `test_demo_grundschule_zweizuegig_solvability.py`). Both have their own `xfail` markers; they share the upstream root cause (item 4) but their stability picture differs from einzügig. Item 11 is einzügig-specific.
- Backend / frontend / API surface changes. The Pydantic schemas, route handlers, and frontend already accept `prefer_late_period`; only the seed value moves.

## Approach

Four-commit PR on `feat/grundschule-quality-bar-and-bench-fixture`. Conventional Commits scopes match the touched module per `solver/CLAUDE.md` and root `.claude/CLAUDE.md`.

### Commit 1: `feat(seed): activate prefer_late_period=5 for FÖ across Grundschule fixtures (item 12)`

Behavioural change. The seed value flips from a no-op to a real soft-cost signal.

Files:
- `backend/src/klassenzeit_backend/seed/demo_grundschule.py`: line 75 (FÖ row of `_SUBJECTS`) flips `prefer_late_period=5`. The other three Python seeds (`demo_grundschule_zweizuegig.py`, `demo_grundschule_dreizuegig.py`) inherit via the imported `_SUBJECTS` tuple.
- `solver/solver-core/src/test_fixtures.rs`: in `zweizuegig_fixture`, change the per-subject closure to set `prefer_late_period: u32::from(i == 8) * 5` so subject index 8 (FOE) gets 5 and the other eight get 0. Same shape in `dreizuegig_fixture` (FOE is the relevant index there too; verify against the file's existing subject ordering comment).
- `backend/tests/seed/test_demo_grundschule_fö_late.py`: new test file. Drives the production HTTP route flow on the einzügig seed once at 200 ms LAHC, projects the FÖ placements, asserts the median position is `>= 3` on the 6-period grid (positions 0..5; latter half is 3..5).
- `mise run solver:rebuild` after the Rust-side change so the maturin wheel picks up the new fixture data; required for any subsequent backend test that touches the cpsat / Rust solver path.

Verification commands:
- `mise run solver:rebuild`
- `mise run test:py -- backend/tests/seed/test_demo_grundschule_fö_late.py -v`
- `mise run test:rust` (must stay green)
- `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig --out /tmp/late-period-smoke.md` and inspect the `Late period ratio` column for `>= 0.0` (not `-`) on the zweizuegig row. (The grundschule bench fixture has no FÖ; its column will stay `-` even post-item-12. This is correct and matches the bench renderer's "no proxy subject" semantics.)

### Commit 2: `fix(solver-bench): repair zweizuegig criterion bench fixture (item 15)`

Structural change. The fixture's teacher allocation or room-subject suitabilities shift to remove the FFD lock-in under the bench's `class_gap=1, teacher_gap=1` weights.

Method:
1. Reproduce locally: `cargo bench -p solver-core --bench solver_fixtures -- 'zweizuegig'`. Capture the violation kind and offending `(class, day, room, subject)` tuple from criterion's panic output. The fixture printer already asserts `solution.violations.is_empty()`; the panic message names the offending placements.
2. Branch on violation kind:
   - **`RoomHopping`**: tighten `room_subject_suitabilities` so the offending subject is locked to one room per class. Mirror to the Python seed if it has the same drift.
   - **`TeacherOverCapacity`**: rebalance teacher load (e.g., re-assign a 4b FÖ from one teacher to another). Mirror to Python seed.
   - **`DoubleBooking`** (unlikely but possible): rebalance period-grid pressure by moving an oversubscribed subject to a different teacher.
3. Re-run `cargo bench -p solver-core --bench solver_fixtures` to confirm all three cells produce zero violations.
4. `mise run bench:record` to overwrite `solver/solver-core/benches/BASELINE.md`. The new numbers replace the 2026-04-30 floor; the commit body explicitly cites the change as intentional perf-data refresh, not a regression.

Files:
- `solver/solver-core/src/test_fixtures.rs`: `zweizuegig_fixture` body changes (teacher allocation table, suitabilities, or both).
- `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py`: mirror change if the fix originated Python-side, or if `_TEACHER_ASSIGNMENTS_ZWEIZUEGIG` documents the wrong allocation.
- `solver/solver-core/benches/BASELINE.md`: regenerated; commit verbatim.

Verification commands:
- `cargo bench -p solver-core --bench solver_fixtures` (must run all three fixtures end-to-end)
- `mise run test:rust` (must stay green)
- `mise run test:py` (the Python seed mirror must keep all existing solvability tests green; if it doesn't, the mirror is wrong)

### Commit 3 (conditional): `test(seed): remove xfail from test_seeded_grundschule_solves_with_auto_assigned_teachers (item 11)`

Test-only change. Conditional on a 20-of-20 stability gate.

Method:
1. With commits 1 and 2 in place locally, run:

   ```bash
   for s in $(seq 1 20); do
     mise run test:py -- backend/tests/seed/test_demo_grundschule_solvability.py::test_seeded_grundschule_solves_with_auto_assigned_teachers -v 2>&1 \
       | tail -3 \
       || true
   done | tee /tmp/item-11-flake.log
   ```

2. Count outcomes from `/tmp/item-11-flake.log`:
   - `xpassed` and `passed` count as PASS.
   - `xfailed` counts as XFAIL (the test asserted but the xfail marker swallowed it).
   - `failed` is FAIL — the gate is `0 FAIL`.
3. If `0 FAIL` across the 20 runs, drop the `@pytest.mark.xfail(...)` decorator from `test_seeded_grundschule_solves_with_auto_assigned_teachers` (lines 30-42 of `test_demo_grundschule_solvability.py`). Confirm one final clean `mise run test:py -- backend/tests/seed/test_demo_grundschule_solvability.py -v` passes.
4. If any FAIL, skip this commit. Update OPEN_THINGS item 11's body in commit 5 (docs) with "Re-measured 2026-05-07: X XPASS / Y XFAIL / Z FAIL out of 20 runs."

Acceptance: `mise run test:py -- backend/tests/seed/test_demo_grundschule_solvability.py` passes after the marker is removed; the flake-log file is referenced in the PR body.

### Commit 4 (conditional): `test(scheduling): remove xfail from test_grundschule_schedule_meets_quality_bar + tighten max_position (item 14)`

Test-only change. Conditional on a 5-of-5 stability gate. Tightens `max_position` 7→6 in lockstep.

Method:
1. With commits 1 and 2 in place, run:

   ```bash
   for s in $(seq 1 5); do
     mise run test:py -- backend/tests/scheduling/test_grundschule_schedule_quality.py -v 2>&1 \
       | tail -3 \
       || true
   done | tee /tmp/item-14-flake.log
   ```

2. Same counting rule as commit 3.
3. If `0 FAIL` across 5 runs, drop the `@pytest.mark.xfail(...)` decorator (lines 104-118 of `test_grundschule_schedule_quality.py`) AND change `max_position=7` to `max_position=6` on line 188.
4. If any FAIL, skip. Update OPEN_THINGS item 14's body with the measurement.

Acceptance: same as commit 3.

### Commit 5: `docs(open-things): update items 11, 12, 14, 15 with shipped status and re-measurements`

Bookkeeping. Updates the OPEN_THINGS entries: items 12 and 15 get deleted (per the "delete shipped items" rule); items 11 and 14 either get deleted (if their xfail came off) or get a 2026-05-07 stability annotation in their body. Item 44 (BENCH_RESULTS late-period column refresh) stays unchanged: its acceptance is a production-shape `mise run bench:bakeoff`, which is post-merge.

## Test plan

- Unit: new `test_demo_grundschule_fö_late.py` asserts median FÖ position `>= 3`.
- Integration: existing `test_demo_grundschule_solvability.py` (auto-assign + canonical) and `test_grundschule_schedule_quality.py` must remain green per their flake-loop measurements.
- Rust: full `mise run test:rust` after each commit. The bench fixture change (commit 2) must keep `solver-core/tests/grundschule_smoke.rs` and the property tests green.
- Lint: `mise run lint` covers ruff, ty, vulture, clippy, machete, biome, actionlint, the unique-fn check, and the commit-types drift check.
- Bench: smoke `mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig` after commit 1 (verify late-period column lights up); `cargo bench -p solver-core --bench solver_fixtures` after commit 2 (verify all three fixtures pass); `mise run bench:record` regenerates `BASELINE.md` in commit 2.
- Pre-push: lefthook's `pre-push` runs the full workspace test suite (cargo nextest + pytest + vitest), so any regression surfaces before origin sees the push.

## Risks

- **Items 11 and 14 may not pass their gates.** Mitigation: the PR ships items 12 and 15 unconditionally and the conditional xfail removals only when the measurement supports them. Honest beats clean.
- **The bench fixture fix may cascade unexpected drift to the matching Python seed's solvability test.** Mitigation: after commit 2, `mise run test:py -- backend/tests/seed/test_demo_grundschule_zweizuegig_solvability.py -v` is a forced verification step.
- **Activating `prefer_late_period=5` may regress the einzügig auto-assigned solvability flake (the original PR #171 reason for the no-op).** Mitigation: item 11's flake-loop measurement detects this; if commit 1 makes the test flake harder, commit 3 is skipped and OPEN_THINGS records the regression. The right response would be either to roll back commit 1 (loses item 12 too) or to land item 12 with a documented "expected to flake item 11 harder" note. Decision deferred to the measurement.
- **`mise run bench:record` requires a quiet host.** The dev host runs the bench at recording time; criterion sample noise could land a value 5-10% off the long-run mean. Mitigation: the `BASELINE.md` footer documents the host; reviewers judge plausibility. The 20% regression budget for downstream PRs absorbs sample noise.

## Verification cadence

- After commit 1: `mise run test:py` (full backend suite must pass; the new FÖ-late unit test plus existing tests with the new soft-cost signal); `mise run test:rust`; smoke bake-off.
- After commit 2: `cargo bench` end-to-end; full `mise run test:rust`; full `mise run test:py`.
- After commit 3 (if it lands): `mise run test:py -- backend/tests/seed/test_demo_grundschule_solvability.py`.
- After commit 4 (if it lands): `mise run test:py -- backend/tests/scheduling/`.
- Before push: `mise run lint`, `mise run test`.

## Pointers

- ADR 0033 (`docs/adr/0033-canonical-objective-and-deadline.md`): canonical-objective LAHC, 5000 ms production deadline.
- `solver/CLAUDE.md`: "Adding a placement-time hard constraint can flake FFD greedy without LAHC" (item 12 risk); "Run `mise run solver:rebuild` before any production-budget `bench:bakeoff`" (commit 1 prerequisite); "Schedule-quality predicates live in `solver-bench/src/quality.rs`" (item 12 verification path).
- `backend/CLAUDE.md`: flake-loop pattern (used by commits 3 and 4); seed-module shared-constants rule (commit 1's cascade through `_SUBJECTS`).
- `docs/superpowers/OPEN_THINGS.md`: items 11, 12, 14, 15, 44 (BENCH_RESULTS refresh follow-up).
