# Grundschule Six Periods Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce the einzügige and zweizügige demo Grundschule Wochenschema from 7 to 6 periods (drop the 13:20-14:05 row), keep dreizügig at 8 periods (Ganztagsschule pattern), update the einzügig + zweizügig shape-test assertions and seed `WEEK_SCHEME_DESCRIPTION` strings, clean up OPEN_THINGS.md.

**Architecture:** Two-commit PR. Commit 1 is a structural refactor (`refactor(seed)`) that inlines period 7 into `_PERIODS_DREIZUEGIG` so dreizügig retains its 8-period shape independently of `_PERIODS`. Commit 2 is the behavioural change (`feat(seed)`) that drops period 7 from `_PERIODS`, refreshes the einzügig + zweizügig `WEEK_SCHEME_DESCRIPTION`, updates the shape-test assertions, and prunes OPEN_THINGS.md.

**Tech Stack:** Python 3.13, pytest async fixtures, SQLAlchemy ORM (TimeBlock, WeekScheme), uv, mise. No frontend / Rust changes.

---

## File Structure

**Modify:**
- `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py:76-84` — refactor `_PERIODS_DREIZUEGIG` to inline period 7 (Commit 1).
- `backend/src/klassenzeit_backend/seed/demo_grundschule.py:31-35,44-52` — drop period 7 from `_PERIODS`, refresh `WEEK_SCHEME_DESCRIPTION` (Commit 2).
- `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py:58-63` — refresh `WEEK_SCHEME_DESCRIPTION` (Commit 2; the seed reuses `_PERIODS` from einzügig so the period count drops automatically).
- `backend/tests/seed/test_demo_grundschule_shape.py:46,56,61,70,185-193` — update assertions and rename test (Commit 2).
- `backend/tests/seed/test_demo_grundschule_zweizuegig_shape.py:49,142,145` — update assertions (Commit 2).
- `docs/superpowers/OPEN_THINGS.md` — delete item 13, delete acknowledged-deferral entry, refresh Hessen reference-data parenthetical (Commit 2).

**Out of scope (do not modify):**
- `solver/solver-core/src/test_fixtures.rs` — Rust bench fixtures stay at 7 periods / 35 blocks.
- Any frontend file (no API surface change).
- Any backend route, schema, or migration (only seed data changes).

---

## Task 1: Refactor — retain dreizügig 8-period shape independently of `_PERIODS`

**Why this commit is structural-only:** dreizügig's emitted shape (8 TimeBlocks per day at positions 1..8 with the same time-windows) is preserved byte-for-byte. We're peeling period 7 out of `*_PERIODS` reuse and into the dreizügig-local definition, so a future shrink of `_PERIODS` does not collateral-damage dreizügig.

**Files:**
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py:76-84`
- Test (existing, must stay green): `backend/tests/seed/test_demo_grundschule_dreizuegig_shape.py`

- [ ] **Step 1: Read the current `_PERIODS_DREIZUEGIG` definition and surrounding comment**

Run:
```bash
sed -n '74,86p' backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py
```

Expected: lines 76-84 show:
```python
# Dreizuegig extends the einzuegig 7-period grid with an eighth ganztags
# period so the FFD greedy can place all 12 classes' Stundentafel-driven
# lessons plus the cross-class Religion trio (3 lessons per Jahrgang, each
# spanning 3 classes) without UUID-tiebreak-dependent flakiness. The 8th
# period (14:05 to 14:50) follows the existing 7-period pattern.
_PERIODS_DREIZUEGIG: tuple[_PeriodTimes, ...] = (
    *_PERIODS,
    _PeriodTimes(8, time(14, 5), time(14, 50)),
)
```

- [ ] **Step 2: Edit `demo_grundschule_dreizuegig.py` to inline period 7 and refresh comment**

Replace lines 76-84 with:
```python
# Dreizuegig is a Ganztagsschule pattern: 8 periods per day. Periods 1-6
# are the morning Halbtag grid shared with einzuegig (`_PERIODS`); periods
# 7 and 8 (13:20-14:05 and 14:05-14:50) are the dreizuegig-only Ganztags-
# Stundenfenster that give the FFD greedy enough slack to place all 12
# classes' Stundentafel-driven lessons plus the cross-class Religion trio
# (3 lessons per Jahrgang, each spanning 3 classes) without UUID-tiebreak-
# dependent flakiness.
_PERIODS_DREIZUEGIG: tuple[_PeriodTimes, ...] = (
    *_PERIODS,
    _PeriodTimes(7, time(13, 20), time(14, 5)),
    _PeriodTimes(8, time(14, 5), time(14, 50)),
)
```

- [ ] **Step 3: Run dreizügig tests to confirm no behavioural drift**

Run:
```bash
mise run test:py -- backend/tests/seed/test_demo_grundschule_dreizuegig_shape.py backend/tests/seed/test_demo_grundschule_dreizuegig.py backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py backend/tests/seed/test_demo_grundschule_dreizuegig_whole_school_schedule.py -v
```

Expected: all four files pass green. The dreizügig shape test must still see 8 periods per day, 12 × 8 × 5 = 480 placements... actually the dreizügig fixtures have their own counts; pytest reports them green as-is.

- [ ] **Step 4: Run lint to catch comment-formatting drift**

Run:
```bash
mise run lint
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add backend/src/klassenzeit_backend/seed/demo_grundschule_dreizuegig.py
git commit -m "refactor(seed): retain dreizuegig 8-period shape independently of _PERIODS"
```

---

## Task 2: Behavioural — write the failing einzügig + zweizügig shape-test edits (TDD red)

**Why test edits go first:** the assertions encode the new contract (5 × 6 = 30 TimeBlocks). Updating them against today's 35-block seed produces a clean red, then the source edits in Task 3 produce green.

**Files:**
- Modify: `backend/tests/seed/test_demo_grundschule_shape.py:46,56,61,70,185-193`
- Modify: `backend/tests/seed/test_demo_grundschule_zweizuegig_shape.py:49,142,145`

- [ ] **Step 1: Edit `test_demo_grundschule_shape.py` line 46 (TimeBlock count)**

Replace:
```python
    assert await _count(seeded_session, TimeBlock) == 35
```
with:
```python
    assert await _count(seeded_session, TimeBlock) == 30
```

- [ ] **Step 2: Rename the test on line 56 (period count in name)**

Replace:
```python
async def test_time_blocks_span_five_days_seven_periods_forty_five_minutes(
```
with:
```python
async def test_time_blocks_span_five_days_six_periods_forty_five_minutes(
```

- [ ] **Step 3: Edit `test_demo_grundschule_shape.py` line 61 (`len(blocks)`)**

Replace:
```python
    assert len(blocks) == 35
```
with:
```python
    assert len(blocks) == 30
```

- [ ] **Step 4: Edit `test_demo_grundschule_shape.py` line 70 (positions set)**

Replace:
```python
        assert positions == {1, 2, 3, 4, 5, 6, 7}, (day, positions)
```
with:
```python
        assert positions == {1, 2, 3, 4, 5, 6}, (day, positions)
```

- [ ] **Step 5: Edit `test_demo_grundschule_shape.py` lines 185-193 (explicit period-times)**

Replace:
```python
    assert [(r[0], r[1], r[2]) for r in rows] == [
        (1, time(8, 0), time(8, 45)),
        (2, time(8, 45), time(9, 30)),
        (3, time(9, 50), time(10, 35)),
        (4, time(10, 35), time(11, 20)),
        (5, time(11, 35), time(12, 20)),
        (6, time(12, 20), time(13, 5)),
        (7, time(13, 20), time(14, 5)),
    ]
```
with:
```python
    assert [(r[0], r[1], r[2]) for r in rows] == [
        (1, time(8, 0), time(8, 45)),
        (2, time(8, 45), time(9, 30)),
        (3, time(9, 50), time(10, 35)),
        (4, time(10, 35), time(11, 20)),
        (5, time(11, 35), time(12, 20)),
        (6, time(12, 20), time(13, 5)),
    ]
```

- [ ] **Step 6: Edit `test_demo_grundschule_zweizuegig_shape.py` line 49**

Replace:
```python
    assert await _count_zw(seeded_zweizuegig, TimeBlock) == 5 * 7  # 5 days x 7 periods
```
with:
```python
    assert await _count_zw(seeded_zweizuegig, TimeBlock) == 5 * 6  # 5 days x 6 periods
```

- [ ] **Step 7: Edit `test_demo_grundschule_zweizuegig_shape.py` line 142**

Replace:
```python
    assert len(blocks) == 35
```
with:
```python
    assert len(blocks) == 30
```

- [ ] **Step 8: Edit `test_demo_grundschule_zweizuegig_shape.py` line 145 (positions list)**

Replace:
```python
        assert [b.position for b in day_blocks] == list(range(1, 8))
```
with:
```python
        assert [b.position for b in day_blocks] == list(range(1, 7))
```

- [ ] **Step 9: Run the two shape tests; expect red**

Run:
```bash
mise run test:py -- backend/tests/seed/test_demo_grundschule_shape.py backend/tests/seed/test_demo_grundschule_zweizuegig_shape.py -v
```

Expected: failures on the updated assertions (today's seed still emits 35 blocks). Specific expected fail messages include `assert 35 == 30`, `assert {1, 2, 3, 4, 5, 6, 7} == {1, 2, 3, 4, 5, 6}`, and `assert [1, 2, 3, 4, 5, 6, 7] == [1, 2, 3, 4, 5, 6]`. Do NOT commit yet — these go in the green-side commit alongside the seed source edits.

---

## Task 3: Behavioural — drop period 7 from `_PERIODS` and refresh seed descriptions (TDD green)

**Files:**
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule.py:31-35,44-52`
- Modify: `backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py:59-63`

- [ ] **Step 1: Edit `demo_grundschule.py` lines 31-35 (`WEEK_SCHEME_DESCRIPTION`)**

Replace:
```python
WEEK_SCHEME_DESCRIPTION = (
    "Hessen Grundschule: 5 Tage, 7 Stunden a 45 Minuten, "
    "Hofpausen nach der 2. und 4. Stunde. Stunde 7 dient als Ganztags- / "
    "AG-Zeitfenster und gibt dem Solver Slack fuer volle Stundentafeln."
)
```
with:
```python
WEEK_SCHEME_DESCRIPTION = (
    "Hessen Grundschule: 5 Tage, 6 Stunden a 45 Minuten, "
    "Hofpausen nach der 2. und 4. Stunde."
)
```

- [ ] **Step 2: Edit `demo_grundschule.py` lines 44-52 (`_PERIODS`)**

Replace:
```python
_PERIODS: tuple[_PeriodTimes, ...] = (
    _PeriodTimes(1, time(8, 0), time(8, 45)),
    _PeriodTimes(2, time(8, 45), time(9, 30)),
    _PeriodTimes(3, time(9, 50), time(10, 35)),
    _PeriodTimes(4, time(10, 35), time(11, 20)),
    _PeriodTimes(5, time(11, 35), time(12, 20)),
    _PeriodTimes(6, time(12, 20), time(13, 5)),
    _PeriodTimes(7, time(13, 20), time(14, 5)),
)
```
with:
```python
_PERIODS: tuple[_PeriodTimes, ...] = (
    _PeriodTimes(1, time(8, 0), time(8, 45)),
    _PeriodTimes(2, time(8, 45), time(9, 30)),
    _PeriodTimes(3, time(9, 50), time(10, 35)),
    _PeriodTimes(4, time(10, 35), time(11, 20)),
    _PeriodTimes(5, time(11, 35), time(12, 20)),
    _PeriodTimes(6, time(12, 20), time(13, 5)),
)
```

- [ ] **Step 3: Edit `demo_grundschule_zweizuegig.py` lines 59-63 (`WEEK_SCHEME_DESCRIPTION`)**

Replace:
```python
WEEK_SCHEME_DESCRIPTION = (
    "Hessen Grundschule, zwei Zuege pro Jahrgang: 5 Tage, 7 Stunden a 45 Minuten, "
    "Hofpausen nach der 2. und 4. Stunde. Stunde 7 dient als Ganztags- / "
    "AG-Zeitfenster und gibt dem Solver Slack fuer volle Stundentafeln."
)
```
with:
```python
WEEK_SCHEME_DESCRIPTION = (
    "Hessen Grundschule, zwei Zuege pro Jahrgang: 5 Tage, 6 Stunden a 45 Minuten, "
    "Hofpausen nach der 2. und 4. Stunde."
)
```

- [ ] **Step 4: Run the two shape tests; expect green**

Run:
```bash
mise run test:py -- backend/tests/seed/test_demo_grundschule_shape.py backend/tests/seed/test_demo_grundschule_zweizuegig_shape.py -v
```

Expected: all tests pass.

- [ ] **Step 5: Run all seed tests to catch any other coupling**

Run:
```bash
mise run test:py -- backend/tests/seed/ -v
```

Expected: pass except for any pre-existing `xfail` markers on `test_seeded_grundschule_solves_with_auto_assigned_teachers` (einzügig) and `test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers` (zweizügig) — both are `strict=False` so XPASS or XFAIL are both acceptable. The canonical-pin `test_seeded_grundschule_zweizuegig_solves_with_zero_violations` (asserts `total_placements == 196`) MUST be green; if it's red, escalate (see Task 4).

- [ ] **Step 6: Run full backend pytest as final sanity gate**

Run:
```bash
mise run test:py
```

Expected: pass; xfail flips on item-4 / item-11 / item-14 trackers are acceptable (strict=False).

---

## Task 4: Update OPEN_THINGS.md — delete item 13, prune deferral, refresh reference data

**Why this lands in commit 2:** the OPEN_THINGS edits are documentation that captures the same semantic change as the source edit (the seed now ships at 6 periods); they belong in the same atomic commit so a `git revert` of the behavioural change pulls the documentation back in lockstep.

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md` (three regions)

- [ ] **Step 1: Delete item-13 bullet entirely**

Locate the line starting `13. **Reduce demo Grundschule Wochenschema from 7 periods to 6.** ` (around line 55 of OPEN_THINGS.md). Delete the entire bullet (one line per autopilot's "OPEN_THINGS is for OPEN items only" rule, no `✅ Shipped` marker).

- [ ] **Step 2: Delete the acknowledged-deferral entry on the prior 7→6 attempt**

Locate the bullet in `## Acknowledged deferrals` that starts:
```
- **Reduce demo Grundschule Wochenschema from 7 to 6 periods.** User intent: Grundschule day caps at 6 periods (matches Hessen "5 Zeitstunden" guidance for grades 3-4 plus a buffer). Attempted in the same PR; the FFD greedy plus the new same-room hard constraint cannot reliably place all 23-26 lessons per class into 30 slots even with LAHC enabled (200ms is not long enough to escape FFD's local minima). Tests flake 1-3 in 5 runs. Stays at 7 periods until FFD becomes smarter about same-room locks during placement (likely a paired change with the quality-bar item above). Wochenschema editor (deferred too) will let admins drop the period count manually before solver work catches up.
```

Delete the entire bullet. The deferral's blocker (FFD same-room lock-in) has been progressively addressed across items 21, 22, 48, 52, 54; the deferral closes naturally with this PR.

- [ ] **Step 3: Refresh the Hessen reference-data parenthetical**

Locate the Hessen reference-data line that contains:
```
The shipped seed uses a 7-period grid (08:00 to 14:05, Periode 7 ab 13:20) to give the MVP greedy solver enough slack; revisit to a 6-period Halbtag once FFD or LAHC ship (PRs 7 and 9 in the active sprint).
```

Replace that sentence (within its surrounding bullet) with:
```
The shipped einzuegig and zweizuegig seeds use a 6-period Halbtag grid (08:00 to 13:05); the dreizuegige Ganztagsschule seed extends to 8 periods (08:00 to 14:50) per the Ganztag pattern.
```

- [ ] **Step 4: Verify OPEN_THINGS.md still parses and item numbering is consistent**

Run:
```bash
grep -c "^13\." docs/superpowers/OPEN_THINGS.md
grep -n "Reduce demo Grundschule Wochenschema" docs/superpowers/OPEN_THINGS.md
grep -n "shipped seed uses a 7-period grid" docs/superpowers/OPEN_THINGS.md
```

Expected:
- First grep: `0` (item-13 bullet deleted).
- Second grep: no match (deferral entry deleted).
- Third grep: no match (parenthetical replaced).

- [ ] **Step 5: Run lint to catch any markdown drift**

Run:
```bash
mise run lint
```

Expected: no errors.

- [ ] **Step 6: Commit Task 2 + Task 3 + Task 4 together as the behavioural commit**

```bash
git add backend/src/klassenzeit_backend/seed/demo_grundschule.py \
         backend/src/klassenzeit_backend/seed/demo_grundschule_zweizuegig.py \
         backend/tests/seed/test_demo_grundschule_shape.py \
         backend/tests/seed/test_demo_grundschule_zweizuegig_shape.py \
         docs/superpowers/OPEN_THINGS.md
git commit -m "feat(seed): grundschule wochenschema shrinks from 7 to 6 periods (item 13)"
```

The commit message scope is `seed` because every file touched lives under `backend/src/klassenzeit_backend/seed/` or `backend/tests/seed/` (the OPEN_THINGS.md edit is a documentation tidy of the semantic change, not its own concern).

---

## Task 5: Final verification before push

- [ ] **Step 1: Inspect the two-commit history**

Run:
```bash
git log --oneline master..HEAD
```

Expected (top-down):
```
<sha2> feat(seed): grundschule wochenschema shrinks from 7 to 6 periods (item 13)
<sha1> refactor(seed): retain dreizuegig 8-period shape independently of _PERIODS
<sha0> docs: add grundschule six-periods design spec
```

- [ ] **Step 2: Inspect the cumulative diff**

Run:
```bash
git diff master..HEAD --stat
```

Expected: ~7 files changed (1 dreizügig source, 2 einzügig + zweizügig source, 2 shape tests, OPEN_THINGS.md, the spec + plan docs in subsequent commits).

- [ ] **Step 3: Run the full lint + test gate one more time**

Run:
```bash
mise run lint && mise run test:py
```

Expected: green.

---

## Self-Review

**1. Spec coverage.** Walked the spec section by section:
- "drop period 7 from `_PERIODS`" → Task 3 Step 2.
- "refresh `WEEK_SCHEME_DESCRIPTION`" (einzügig + zweizügig) → Task 3 Steps 1, 3.
- "keep `_PERIODS_DREIZUEGIG` at 8 periods (Ganztagsschule pattern)" → Task 1.
- "update `test_demo_grundschule_shape.py` (35→30)" → Task 2 Steps 1-5.
- "update `test_demo_grundschule_zweizuegig_shape.py` (5×7→5×6)" → Task 2 Steps 6-8.
- OPEN_THINGS prune → Task 4.
- Verification gate → Task 5.

No gaps.

**2. Placeholder scan.** No "TBD", "TODO", or "implement later" strings in the plan. Every code edit is shown in full. Every command has expected output. Two earlier comments suggested a placeholder ("escalate (see Task 4)" in Task 3 Step 5) — that's a pointer to the Task-4 self-resolution path, not a placeholder.

**3. Type consistency.** No new types, signatures, or method names introduced; all edits modify constants (`_PERIODS`, `WEEK_SCHEME_DESCRIPTION`) or assertion literals. Plan and spec refer to the same identifiers in the same forms.
