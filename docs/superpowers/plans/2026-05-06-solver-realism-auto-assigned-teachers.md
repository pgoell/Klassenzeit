# Solvability tests mirroring the production route flow (auto-assigned teachers) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three sibling solvability tests that drive the production HTTP route flow on each `demo_grundschule*` seed without the canonical `_TEACHER_ASSIGNMENTS_*` overwrite, so CI catches bugs that surface only on the auto-assigned teacher distribution.

**Architecture:** Each new test mirrors the corresponding canonical test, with two surgical differences: skip the SQL `UPDATE` against `Lesson.teacher_id`, and `monkeypatch.setattr(app.state.settings, "solve_deadline_ms", 5000)` so the production LAHC budget runs (`backend/.env.test` sets `KZ_SOLVE_DEADLINE_MS=0` for the rest of the suite). The einzügig case already runs without the canonical pin and is renamed in lockstep, with the legacy `solve_deadline_ms=200` monkeypatch bumped to 5000 ms; the `xfail` decorator is dropped if a 20-of-20 flake-loop confirms determinism.

**Tech Stack:** pytest 8 + pytest-asyncio + httpx AsyncClient + SQLAlchemy async + FastAPI test client. Run via `mise run test:py`.

---

## File Structure

**Modified files (all in `backend/tests/seed/`):**

- `test_demo_grundschule_zweizuegig_solvability.py` — adds `test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers` alongside the canonical. Two test functions in the file at end of task 1.
- `test_demo_grundschule_dreizuegig_solvability.py` — adds `test_seeded_grundschule_dreizuegig_solves_with_auto_assigned_teachers` alongside the canonical. Two test functions in the file at end of task 2.
- `test_demo_grundschule_solvability.py` — renames the existing function, bumps deadline 200 to 5000, conditionally drops `xfail`. One test function in the file (renamed) at end of task 3.

**Modified files (project-level):**

- `docs/superpowers/OPEN_THINGS.md` — delete item 32; delete or refresh item 11 per measurement.

No new files. No new factories. No new helpers.

---

## Task 1: Zweizügig auto-assigned-teachers sibling

**Files:**
- Modify: `backend/tests/seed/test_demo_grundschule_zweizuegig_solvability.py` (append a new test function below the existing one)

- [ ] **Step 1: Add `app` and `pytest` imports if missing, append the new test function**

The file currently imports nothing from `klassenzeit_backend.main` (it does not need to monkeypatch settings) and does not import `pytest`. Add both.

Edit `backend/tests/seed/test_demo_grundschule_zweizuegig_solvability.py`:

Add to the import block (alphabetic order within stdlib/third-party/firstparty groups):

```python
import pytest

from klassenzeit_backend.main import app
```

Append below `test_seeded_grundschule_zweizuegig_solves_with_zero_violations`:

```python
async def test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnZw,
    login_as: LoginFnZw,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Production-route mirror: skips the canonical _TEACHER_ASSIGNMENTS_ZWEIZUEGIG
    UPDATE so the test exercises whatever teacher distribution
    auto_assign_teachers_for_lessons produces inside the generate-lessons route.

    Item 32: a feasibility regression that only manifests on the
    auto-assign distribution would slip through the canonical-pin sibling.
    """
    monkeypatch.setattr(app.state.settings, "solve_deadline_ms", 5000)
    await seed_demo_grundschule_zweizuegig(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-zw-autoassign-seedtest@example.com",
        password="seed-zw-autoassign-test-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    class_rows = (
        (
            await db_session.execute(
                select(SchoolClass).order_by(SchoolClass.grade_level, SchoolClass.name)
            )
        )
        .scalars()
        .all()
    )

    for school_class in class_rows:
        gen_resp = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
        assert gen_resp.status_code == 201, gen_resp.text

    for school_class in class_rows:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])
        assert len(body["placements"]) > 0, school_class.name
```

The unused imports `select`, `update`, `Lesson`, `LessonSchoolClass`, `Subject`, `Teacher`, and `_TEACHER_ASSIGNMENTS_ZWEIZUEGIG` from the canonical test stay because the canonical test still consumes them; do not touch the existing imports.

- [ ] **Step 2: Run the new test once to verify it passes**

Run from repo root:
```
mise run test:py -- backend/tests/seed/test_demo_grundschule_zweizuegig_solvability.py::test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers -v --no-cov -p no:xdist
```

Expected: PASS in roughly 5 to 30 s (LAHC at 5000 ms wall-clock budget per class, exits early at the optimum floor).

If the test fails deterministically:
- Capture the violation list / status code body.
- Stop. Item 32's literal goal is to surface bugs the canonical pin hides; this is the deliverable, not a fix-on-the-spot. Surface to the user, do not commit, treat the discovered bug as the new top of the sprint stack and open a separate spec.

If it errors with `KeyError` or unexpected schema mismatch, fix the test author error before continuing.

- [ ] **Step 3: Flake-loop the new test 5 times in isolation**

Run:
```
for i in 1 2 3 4 5; do echo "=== run $i ==="; mise run test:py -- backend/tests/seed/test_demo_grundschule_zweizuegig_solvability.py::test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers -v --no-cov -p no:xdist || break; done
```

Expected: 5 of 5 PASS.

If any of the 5 fails (and the failure is non-deterministic — the test passes on retry), demote to xfail in the same commit:

```python
@pytest.mark.xfail(
    strict=False,
    reason=(
        "auto_assign_teachers_for_lessons distribution occasionally "
        "produces a teacher allocation that the production solver path "
        "(lahc_rr_kempe at 5000 ms) cannot recover from. Tracked under "
        "OPEN_THINGS item 4 (subject UUID order leak in scarcity-first "
        "tiebreak)."
    ),
)
async def test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers(
    ...
) -> None:
    ...
```

Document `5/5 PASS` or `N/5 PASS, demoted to xfail` in the PR body.

- [ ] **Step 4: Run sibling canonical test to confirm no regression**

Run:
```
mise run test:py -- backend/tests/seed/test_demo_grundschule_zweizuegig_solvability.py -v --no-cov -p no:xdist
```

Expected: 2 PASS (or 1 PASS + 1 XFAIL if step 3 demoted).

- [ ] **Step 5: Commit**

```
git add backend/tests/seed/test_demo_grundschule_zweizuegig_solvability.py
git commit -m "test(seed): add zweizuegig auto-assigned-teachers solvability sibling"
```

If the test demoted to xfail in step 3, the commit message stays the same; the xfail is part of the test's strict-from-day-one shape.

---

## Task 2: Dreizügig auto-assigned-teachers sibling

**Files:**
- Modify: `backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py`

- [ ] **Step 1: Add imports and append the new test function**

Add to imports:
```python
import pytest

from klassenzeit_backend.main import app
```

Append below `test_seeded_grundschule_dreizuegig_solves_with_zero_violations`:

```python
async def test_seeded_grundschule_dreizuegig_solves_with_auto_assigned_teachers(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnDr,
    login_as: LoginFnDr,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Production-route mirror for the dreizuegige seed: skips the canonical
    _TEACHER_ASSIGNMENTS_DREIZUEGIG UPDATE so the test exercises the
    auto-assign distribution end-to-end. Item 32.

    The cross-class Religion trio is still pinned at seed time (it has to
    be, because LessonSchoolClass relationships drive auto-assign
    eligibility); only the per-class non-Religion teacher allocation
    differs from the canonical sibling.
    """
    monkeypatch.setattr(app.state.settings, "solve_deadline_ms", 5000)
    await seed_demo_grundschule_dreizuegig(db_session)
    await db_session.flush()

    admin, password = await create_test_user(
        email="admin-dr-autoassign-seedtest@example.com",
        password="seed-dr-autoassign-test-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    class_rows = (
        (
            await db_session.execute(
                select(SchoolClass).order_by(SchoolClass.grade_level, SchoolClass.name)
            )
        )
        .scalars()
        .all()
    )

    for school_class in class_rows:
        gen_resp = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
        assert gen_resp.status_code == 201, gen_resp.text

    for school_class in class_rows:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])
        assert len(body["placements"]) > 0, school_class.name
```

- [ ] **Step 2: Run once to verify it passes**

```
mise run test:py -- backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py::test_seeded_grundschule_dreizuegig_solves_with_auto_assigned_teachers -v --no-cov -p no:xdist
```

Expected: PASS within 60 to 120 s wall-clock (12 classes x up to 5 s LAHC each, typically much less). Same failure-handling rules as Task 1 step 2: if it fails deterministically, stop and surface; do not paper over.

- [ ] **Step 3: Flake-loop 5 times in isolation**

```
for i in 1 2 3 4 5; do echo "=== run $i ==="; mise run test:py -- backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py::test_seeded_grundschule_dreizuegig_solves_with_auto_assigned_teachers -v --no-cov -p no:xdist || break; done
```

Expected: 5 of 5 PASS. Same xfail-demote shape as Task 1 step 3 if any flake.

- [ ] **Step 4: Run sibling canonical test to confirm no regression**

```
mise run test:py -- backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py -v --no-cov -p no:xdist
```

Expected: 2 PASS.

- [ ] **Step 5: Commit**

```
git add backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py
git commit -m "test(seed): add dreizuegig auto-assigned-teachers solvability sibling"
```

---

## Task 3: Rename einzügig test, bump deadline, drop xfail

**Files:**
- Modify: `backend/tests/seed/test_demo_grundschule_solvability.py`

- [ ] **Step 1: 20-of-20 flake-loop on the renamed shape (read-only verification first)**

Before editing, first verify the existing test reliably passes at 5000 ms by temporarily patching the deadline value. Do this via a one-line edit, run the loop, then revert if measurement fails.

Edit `backend/tests/seed/test_demo_grundschule_solvability.py:57` to bump the literal `200` to `5000`, then run:

```
for i in $(seq 1 20); do echo "=== run $i ==="; mise run test:py -- backend/tests/seed/test_demo_grundschule_solvability.py -v --no-cov -p no:xdist 2>&1 | tail -5 || break; done
```

Expected: each of 20 runs ends with `1 passed` OR `1 xpassed` (xfail(strict=False) treats unexpected-pass as a passing run). Treat XPASS as PASS for this gate; the question is "does it ever go red," not "does pytest call it green."

If 20 of 20 pass: proceed with full rename + xfail removal in step 2.

If any run goes RED (FAILED): proceed with rename only, keep the xfail decorator, but refresh its `reason=` text to refer to the renamed test and the 5000 ms deadline. The xfail wording is in the existing source.

Document the result (`20/20 PASS, removing xfail` OR `N/20 PASS, keeping xfail`) in the PR body.

- [ ] **Step 2: Apply the rename + deadline bump (+ xfail removal if measured green)**

If 20-of-20 passed in step 1, edit `backend/tests/seed/test_demo_grundschule_solvability.py` to:

1. Remove the entire `@pytest.mark.xfail(...strict=False)` decorator block.
2. Rename `test_seeded_grundschule_solves_with_zero_violations` to `test_seeded_grundschule_solves_with_auto_assigned_teachers`.
3. Confirm the deadline literal is `5000` (already bumped in step 1).
4. Update the comment block above the `monkeypatch.setattr(...)` call (lines 53 to 56) to drop the now-stale "the rest of the suite stays greedy-only via KZ_SOLVE_DEADLINE_MS=0" framing; the new comment should read:

```python
    # Production deadline: 5000 ms LAHC budget per ADR 0033. Test env
    # default is KZ_SOLVE_DEADLINE_MS=0 (greedy-only); this opts back in
    # so the test exercises the production solver path on the
    # auto-assign teacher distribution.
```

5. Update the module-level docstring to reflect that the test now drives the production-route flow without the canonical pin (it already does). Keep the rest of the docstring shape.

If 20-of-20 did NOT pass in step 1, KEEP the `@pytest.mark.xfail(...)` decorator but rewrite the `reason=` text:

```python
@pytest.mark.xfail(
    strict=False,
    reason=(
        "Auto-assigned teacher distribution under lahc_rr_kempe at "
        "5000 ms LAHC budget intermittently hits 'no_suitable_room' on "
        "FFD greedy. R&R + Kempe usually escapes but not always; "
        "tracked under OPEN_THINGS item 4 (subject UUID order leak in "
        "scarcity-first auto-assign tiebreak). Strict=False so XPASS "
        "doesn't fail the suite once the underlying flake is fixed."
    ),
)
```

- [ ] **Step 3: Run the renamed test once**

```
mise run test:py -- backend/tests/seed/test_demo_grundschule_solvability.py::test_seeded_grundschule_solves_with_auto_assigned_teachers -v --no-cov -p no:xdist
```

Expected: PASS (or XFAIL/XPASS if the decorator was kept).

- [ ] **Step 4: Run the full backend test suite to catch any name-resolution stragglers**

```
mise run test:py -- backend/tests/seed/ -v --no-cov
```

Expected: all PASS / XFAIL as appropriate. The rename is a leaf identifier (no imports of the test name from any other file), so this is a precaution.

- [ ] **Step 5: Update OPEN_THINGS.md**

Read `docs/superpowers/OPEN_THINGS.md` and:

1. Delete item 32 (the entire entry under `### Test realism phase`, lines 21 to 23 region).
2. If the xfail was removed in step 2, delete item 11 (under `## Open solver follow-ups`).
3. If the xfail was kept (with refreshed wording), refresh item 11's wording in place to refer to the renamed test name.
4. Update the active-sprint preamble at the top of the file (line 9 region) so the "Next pickup" sentence advances from item 32 to item 34 (`Backend tidy phase`). Item 34 is the `solve_deadline_ms_by_backend` follow-up; with item 32 closed, it becomes the next P1 in the active sprint program.

- [ ] **Step 6: Commit**

If the xfail was removed:
```
git add backend/tests/seed/test_demo_grundschule_solvability.py docs/superpowers/OPEN_THINGS.md
git commit -m "test(seed): rename einzuegig solvability test, bump deadline to 5000ms, drop xfail"
```

If the xfail was kept (rewording only):
```
git add backend/tests/seed/test_demo_grundschule_solvability.py docs/superpowers/OPEN_THINGS.md
git commit -m "test(seed): rename einzuegig solvability test, bump deadline to 5000ms, refresh xfail wording"
```

---

## Task 4: Suite-duration budget gate

**Files:**
- Modify (only if budget regression observed): `.test-duration-budget`

- [ ] **Step 1: Run the bench:tests budget gate**

```
mise run bench:tests
```

Expected: green (the script computes the wall-clock against `.test-duration-budget`).

- [ ] **Step 2: Ratchet if needed**

If the gate fails, the script prints the new wall-clock; replace the contents of `.test-duration-budget` with the new value plus the same comfort margin the file currently uses (read the file first; the format and margin policy are documented inline).

- [ ] **Step 3: Commit (only if a ratchet was needed)**

```
git add .test-duration-budget
git commit -m "build(tests): ratchet duration budget for new auto-assigned solvability tests"
```

If no ratchet was needed, skip this commit.

---

## Self-review checklist

- Spec coverage:
  - Zweizügig sibling test: Task 1.
  - Dreizügig sibling test: Task 2.
  - Einzügig rename + bump + conditional xfail removal: Task 3.
  - Item 32 deletion / item 11 deletion or refresh: Task 3 step 5.
  - `mise run bench:tests` budget gate: Task 4.
  - 5-of-5 / 20-of-20 measurement gates: each respective task.
- Placeholder scan: none. Every code block is concrete; every command is exact.
- Type consistency: `CreateUserFnZw` / `CreateUserFnDr` / `CreateUserFn` already exist in their respective files; the new tests reuse the local alias of the file they live in. Imports re-use existing aliases.
