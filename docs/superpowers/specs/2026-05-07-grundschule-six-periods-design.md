# Item 13: Reduce demo Grundschule Wochenschema from 7 periods to 6

## Problem

The einzügige and zweizügige Grundschule demo seeds ship a 7-period Wochenschema (08:00 to 14:05, period 7 at 13:20-14:05). The 7th period was added during the MVP greedy phase as Solver-Slack so the FFD greedy could pack 23h (Klasse 1+2) and 26h (Klasse 3+4) Stundentafeln without lock-in. Hessen reality is a 6-period Halbtag for Klasse 1-4 (≈ 5 Zeitstunden plus Hofpausen) with Ganztag patterns extending to 8 periods; the seed's extra slack period misrepresents the schedule shape admins see in production demos and inflates the search space the quality bar (item 14) measures against.

OPEN_THINGS item 13 (P0): drop the 13:20-14:05 row from `_PERIODS` in `demo_grundschule.py`, update `test_demo_grundschule_shape.py` (35→30) and `test_demo_grundschule_zweizuegig_shape.py` (5×7→5×6) assertions, keep `_PERIODS_DREIZUEGIG` at 8 periods (Ganztagsschule pattern).

This is a prereq for item 14 (tightening the Grundschule quality bar): with 30 slots/week per class instead of 35, the position-spread and gap-count thresholds in `test_grundschule_schedule_quality_meets_quality_bar` measure something closer to a real Grundschule timetable.

## Scope

In scope:
- `demo_grundschule.py`: drop period 7 from `_PERIODS`; refresh `WEEK_SCHEME_DESCRIPTION` to drop the "Stunde 7 dient als Ganztags-..." sentence and change "7 Stunden" to "6 Stunden".
- `demo_grundschule_zweizuegig.py`: refresh `WEEK_SCHEME_DESCRIPTION` (same wording change). The zweizügig seed reuses `_PERIODS` directly, so the period count drops automatically.
- `demo_grundschule_dreizuegig.py`: keep 8 periods. `_PERIODS_DREIZUEGIG` currently spreads `*_PERIODS` and appends period 8; with `_PERIODS` shrinking to 6, dreizügig must add period 7 explicitly to retain its 8-period shape. Refresh the surrounding comment.
- `test_demo_grundschule_shape.py`: TimeBlock count assertion 35 → 30; positions-set assertion `{1..7}` → `{1..6}`; explicit-period-times assertion drops the period-7 row; rename test `test_time_blocks_span_five_days_seven_periods_forty_five_minutes` → `..._six_periods_...`.
- `test_demo_grundschule_zweizuegig_shape.py`: `5 * 7` → `5 * 6` (with comment update); `len(blocks) == 35` → `30`; `range(1, 8)` → `range(1, 7)`.
- `docs/superpowers/OPEN_THINGS.md`: delete item 13 outright (no `✅ Shipped` marker per autopilot rules); delete the acknowledged-deferral entry "Reduce demo Grundschule Wochenschema from 7 to 6 periods"; update the Hessen reference-data line that currently says "the shipped seed uses a 7-period grid" to reflect the new 6-period Halbtag shape.

Out of scope:
- Rust bench fixture (`solver/solver-core/src/test_fixtures.rs::einzuegig_fixture`, `zweizuegig_fixture`) stays at 35 time-blocks. The bench fixture is a hand-authored coordinate system that anchors `BASELINE.md` and `BENCH_RESULTS.md`; updating it requires a paired bench refresh (item 15 broken bench, ~5 h `mise run bench:bakeoff`) that has nothing to do with the seed-quality intent of this PR. Comments in the Rust fixture become technically out-of-date relative to the Python seed; tracked, not bundled.
- Backend / frontend code beyond seeds and seed-shape tests. Routes, Pydantic schemas, OpenAPI types, frontend components stay unchanged: TimeBlock counts are data, not contracts.
- Item 14 (quality-bar threshold revisit). This PR is its prereq; item 14 will revisit `max_position=6` and friends once the seed shrink lands.
- Item 12 (`Subject.prefer_late_period=5` for FÖ). Independent prereq for item 14, on a separate branch.
- ADR additions. The seed-shape reduction does not introduce a new architectural axis; the existing `Settings.solve_deadline_ms`, `Settings.solver_backend`, and ADR 0033 cover the production-readiness story.

## Approach

Two-commit PR on `feat/grundschule-six-periods`. Behavioural change is one commit; the dreizügig structural refactor lands separately to keep `git bisect` legible per the CLAUDE.md "structural and behavioral never in the same commit" rule.

### Commit 1: `refactor(seed): retain dreizuegig 8-period shape independently of _PERIODS`

`backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py`:
- Inline period 7 into `_PERIODS_DREIZUEGIG`:

  ```python
  _PERIODS_DREIZUEGIG: tuple[_PeriodTimes, ...] = (
      *_PERIODS,
      _PeriodTimes(7, time(13, 20), time(14, 5)),
      _PeriodTimes(8, time(14, 5), time(14, 50)),
  )
  ```

- Refresh the comment cluster above (`# Dreizuegig extends the einzuegig 7-period grid ...`) to describe dreizügig's now-self-contained 8-period Ganztag shape, since `_PERIODS` no longer carries period 7.

Behaviour preserved: dreizügig still emits 8 TimeBlocks per day at positions 1..8 with the same time-windows as today. Verification gate for this commit:

```bash
mise run test:py -- backend/tests/seed/test_demo_grundschule_dreizuegig.py backend/tests/seed/test_demo_grundschule_dreizuegig_shape.py backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py backend/tests/seed/test_demo_grundschule_dreizuegig_whole_school_schedule.py
```

All four dreizügig-touching tests must stay green; no behavioural drift.

### Commit 2: `feat(seed): grundschule wochenschema shrinks from 7 to 6 periods (item 13)`

Test edits (TDD red side, written first inside the agent's working tree):
- `backend/tests/seed/test_demo_grundschule_shape.py`:
  - Line 46: `TimeBlock == 35` → `30`.
  - Line 56: rename `test_time_blocks_span_five_days_seven_periods_forty_five_minutes` → `test_time_blocks_span_five_days_six_periods_forty_five_minutes`.
  - Line 61: `len(blocks) == 35` → `30`.
  - Line 70: `positions == {1, 2, 3, 4, 5, 6, 7}` → `{1, 2, 3, 4, 5, 6}`.
  - Lines 185-193: drop the `(7, time(13, 20), time(14, 5))` row from the explicit period-times assertion list.
- `backend/tests/seed/test_demo_grundschule_zweizuegig_shape.py`:
  - Line 49: `5 * 7  # 5 days x 7 periods` → `5 * 6  # 5 days x 6 periods`.
  - Line 142: `len(blocks) == 35` → `30`.
  - Line 145: `list(range(1, 8))` → `list(range(1, 7))`.

Run the targeted shape tests; expect red on each updated assertion (today's seed still emits 35 blocks).

```bash
mise run test:py -- backend/tests/seed/test_demo_grundschule_shape.py backend/tests/seed/test_demo_grundschule_zweizuegig_shape.py
```

Source edits (green side):
- `backend/src/klassenzeit_backend/seed/demo_grundschule.py`:
  - Drop `_PeriodTimes(7, time(13, 20), time(14, 5))` from `_PERIODS`.
  - Rewrite `WEEK_SCHEME_DESCRIPTION`: "Hessen Grundschule: 5 Tage, 6 Stunden a 45 Minuten, Hofpausen nach der 2. und 4. Stunde."
- `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py`:
  - Rewrite `WEEK_SCHEME_DESCRIPTION` to match the new einzügig wording with the "zwei Zuege pro Jahrgang" qualifier preserved.

Re-run the shape tests; expect green.

OPEN_THINGS edits:
- Delete the item-13 bullet entirely.
- Delete the acknowledged-deferral entry "Reduce demo Grundschule Wochenschema from 7 to 6 periods" (its blocker, FFD same-room lock-in, has been progressively addressed across items 21, 22, 48, 52, 54).
- Refresh the Hessen reference-data parenthetical: change "The shipped seed uses a 7-period grid (08:00 to 14:05, Periode 7 ab 13:20) to give the MVP greedy solver enough slack; revisit to a 6-period Halbtag once FFD or LAHC ship" to describe the new 6-period Halbtag shape and the dreizügig 8-period Ganztag extension as the canonical reference.

Verification gate for the commit:

```bash
mise run test:py -- backend/tests/seed/   # all seed tests, including pinned solvability
mise run lint
```

Full backend pytest as final gate before push:

```bash
mise run test:py
```

xfail-marked tests (`test_seeded_grundschule_solves_with_auto_assigned_teachers`, `test_grundschule_schedule_meets_quality_bar`, `test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers`) may flip XPASS ↔ XFAIL under the tighter grid; either direction is acceptable per the existing `strict=False` contract. A hard `failed` row on the canonical-pin zweizügig solvability test (`test_seeded_grundschule_zweizuegig_solves_with_zero_violations`) would mean the 196-placement assertion no longer holds under the tighter grid; mitigation is to inspect the FFD/LAHC trace and either re-pin `_TEACHER_ASSIGNMENTS_ZWEIZUEGIG` or expand scope. Likelihood is low (196/240 slot density, 1h preferred_block_size on most subjects, FFD has historically packed this fixture cleanly).

## Risks

- **Zweizügig solvability test pinned to 196 placements may flip red.** Strict `total_placements == 196` assertion. If the canonical teacher distribution can no longer feasibly pack into 30 slots/week, the test fails; response is to re-pin or expand scope, not to relax the assertion.
- **Auto-assign xfail tests may flip XPASS → XFAIL or vice versa under tighter grid.** Both `strict=False`; either direction is acceptable. Note in PR body so reviewers don't read a flipped status as a regression.
- **Backend pytest duration budget (`.test-duration-budget`) may shift slightly.** Shape tests are fast; solvability tests at 5000 ms LAHC dominate and are unchanged in deadline. No expected drift outside the noise floor.

## Success criteria

- `mise run test:py` passes (xfail flips are acceptable, hard fails are not).
- `mise run lint` passes.
- The two einzügig + zweizügig shape tests assert 30 blocks (`5 × 6`) and pass.
- Dreizügig still emits 8 TimeBlocks per day (positions 1..8) with no skip in the position sequence.
- `WEEK_SCHEME_DESCRIPTION` no longer mentions "Stunde 7" or "7 Stunden" in the einzügig and zweizügig seeds.
- Item 13 is deleted from OPEN_THINGS.md; the acknowledged-deferral entry is pulled; the Hessen reference-data line is refreshed.
