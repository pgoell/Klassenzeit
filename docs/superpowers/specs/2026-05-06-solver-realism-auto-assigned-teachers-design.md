# Solvability tests mirroring the production route flow (auto-assigned teachers)

**Item:** OPEN_THINGS item 32 (P0). Plus item 11 closure if measurement supports it.

**Goal:** Close the test-realism gap so the demo seed solvability tests exercise the production HTTP route flow end-to-end. Today the canonical solvability tests overwrite `Lesson.teacher_id` from a hand-authored `_TEACHER_ASSIGNMENTS_*` map after `generate-lessons`, which produces a teacher distribution different from the one `auto_assign_teachers_for_lessons` (the production path) produces. Bugs that surface only on the auto-assigned distribution slip through CI.

## Background

`POST /api/classes/{id}/generate-lessons` calls `auto_assign_teachers_for_lessons`, which greedy-assigns a qualified teacher per `Lesson` ordered by qualified-teacher count ascending (scarcity-first). The canonical solvability tests for `demo_grundschule_zweizuegig` and `demo_grundschule_dreizuegig` run this route, then issue an SQL `UPDATE` driven by `_TEACHER_ASSIGNMENTS_<NAME>` so the FFD layout in the next solver call matches the bench fixture exactly. The fixed teacher allocation keeps placement counts stable (zweizuegig asserts `total_placements == 196`, mirroring the Rust bench fixture).

The cost: the canonical tests no longer test the production flow. They test a flow the user cannot reproduce without manually overriding teacher assignments via SQL. Real production data has the auto-assign distribution.

ADR 0031 (reaffirmed by ADR 0032) sets the production solver default to `lahc_rr_kempe`. ADR 0033 raises `solve_deadline_ms` from 200 ms to 5000 ms. The test env (`backend/.env.test`) sets `KZ_SOLVE_DEADLINE_MS=0` (greedy-only) so the wider suite stays fast; this test must opt back in.

## Scope

Add three sibling tests, one per demo seed, that exercise the production route flow without the canonical teacher pin.

In scope:
- New `test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers` in `backend/tests/seed/test_demo_grundschule_zweizuegig_solvability.py`.
- New `test_seeded_grundschule_dreizuegig_solves_with_auto_assigned_teachers` in `backend/tests/seed/test_demo_grundschule_dreizuegig_solvability.py`.
- Rename existing `test_seeded_grundschule_solves_with_zero_violations` in `backend/tests/seed/test_demo_grundschule_solvability.py` to `test_seeded_grundschule_solves_with_auto_assigned_teachers`. Bump its `solve_deadline_ms` monkeypatch from 200 to 5000. If the 20-of-20 flake-loop passes, remove the `pytest.mark.xfail` decorator (closes item 11).
- OPEN_THINGS housekeeping: delete item 32; delete item 11 if 20-of-20 passes; otherwise refresh item 11's wording to refer to the renamed test.

Out of scope:
- Changing the canonical-pin tests' assertions or layout. The 196 placement-count invariant for zweizuegig stays.
- Solving any FFD lock-in case if it surfaces. If a sibling test fails deterministically, surface the bug and stop; the fix lives in a separate spec.
- Bench fixture parity. The auto-assigned variant has no Rust-side counterpart by design.

## Architecture

Each new test mirrors the canonical structure with two differences:
1. No SQL `UPDATE` against `Lesson.teacher_id` after `generate-lessons`.
2. `monkeypatch.setattr(app.state.settings, "solve_deadline_ms", 5000)` to opt back into the production LAHC budget.

Per-test shape:

```python
async def test_seeded_grundschule_zweizuegig_solves_with_auto_assigned_teachers(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnZw,
    login_as: LoginFnZw,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
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
        (await db_session.execute(
            select(SchoolClass).order_by(SchoolClass.grade_level, SchoolClass.name)
        )).scalars().all()
    )
    for school_class in class_rows:
        gen_resp = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
        assert gen_resp.status_code == 201, gen_resp.text

    # NO _TEACHER_ASSIGNMENTS overwrite: rely on auto_assign_teachers_for_lessons.

    for school_class in class_rows:
        sched_resp = await client.post(f"/api/classes/{school_class.id}/schedule")
        assert sched_resp.status_code == 200, sched_resp.text
        body = sched_resp.json()
        assert body["violations"] == [], (school_class.name, body["violations"])
        assert len(body["placements"]) > 0, school_class.name
```

Distinct admin email per sibling test (zweizuegig: `admin-zw-autoassign-...`, dreizuegig: `admin-dr-autoassign-...`, einzügig: `admin-autoassign-...`) avoids unique-constraint collisions when both canonical and auto-assigned tests run in the same xdist worker session.

The einzügig rename is mechanical: change the function name, change the monkeypatch literal, optionally drop the `@pytest.mark.xfail` decorator.

## Assertions

- `body["violations"] == []` per class. Canonical assertion shape; the auto-assigned distribution must produce a clean schedule.
- `len(body["placements"]) > 0` per class. Catches the silent zero-placement / zero-violation pathological case.
- No whole-school placement total. Without a Rust-side bench fixture co-owning the count, the assertion would be a bare regression-detector with no second source of truth.

## Validation strategy

Before pushing, the implementor must:

1. **5-of-5 flake-loop per new sibling.** Run zweizuegig and dreizuegig auto-assigned tests five times each with `--no-cov -p no:xdist` to isolate. If any flakes, demote *that* sibling to `pytest.mark.xfail(strict=False, reason="...")` with a one-sentence reason naming the failure mode (no_suitable_room / which class / etc.), and add an OPEN_THINGS follow-up under "Open solver follow-ups".
2. **20-of-20 loop on the renamed einzügig.** If 20-of-20 passes, drop the `xfail` decorator. If any run fails, keep the xfail with refreshed wording referencing the rename and the new deadline.
3. **`mise run bench:tests`.** Confirm the new tests do not push the suite over the `.test-duration-budget`. If they do, ratchet the budget by the observed delta in the same PR.

The test runner is `mise run test:py -- backend/tests/seed/test_demo_grundschule_*solvability.py -v`.

## Commit split

Three commits, each runnable independently. CLAUDE.md's "structural and behavioral never ship in the same commit" rule is respected because the three tests, although they share a shape, target three distinct seed fixtures with distinct expected outcomes; they are not pure structural tidies.

1. `test(seed): add zweizuegig auto-assigned-teachers solvability sibling`
2. `test(seed): add dreizuegig auto-assigned-teachers solvability sibling`
3. `test(seed): rename einzügig solvability test, bump deadline to production 5000 ms, drop xfail` (or `refresh xfail wording` if 20-of-20 fails)

If a sibling test ships demoted to xfail, demote happens within the same commit that adds the test (no half-formed strict-then-flake interim).

## Risk surface

- **A sibling test fails deterministically on first run.** That is item 32's literal goal: catch bugs the canonical pin hides. The branch pauses, the failure is surfaced to the user with the violation list / no_suitable_room reason / etc., and the bug is fixed in a separate spec before item 32 ships.
- **Suite duration regresses past `.test-duration-budget`.** Ratchet the budget in the same PR per CLAUDE.md guidance; do not skip the gate.
- **`auto_assign_teachers_for_lessons` is non-deterministic across full-suite test ordering.** OPEN_THINGS item 4 ("Einzügige solvability test transient flakiness, suspected nondeterminism in subject-UUID order") flags this. The 5-of-5 loop above is designed to surface it. If a sibling test passes 5-of-5 in isolation but fails inside the broader suite, that is item 4's territory and is out of scope.
- **OPEN_THINGS item 11 wording refresh vs. removal.** Decision split is binary on the 20-of-20 result; document the chosen branch in the PR body so the next reader can trace the reasoning.

## Success criteria

- Three sibling tests exist with the agreed naming.
- Each sibling test runs the production route flow (no canonical UPDATE) at the production deadline (5000 ms).
- The new tests' pass/flake outcome (strict vs. xfail) is documented in the PR body, one line per test.
- OPEN_THINGS item 32 is deleted.
- OPEN_THINGS item 11 is deleted (xfail removed) or refreshed (xfail kept under new name) per measurement.
- `mise run bench:tests` budget gate is green (or ratcheted).
