# Item 53: validate_daily_caps as production post-condition

## Problem

`solver-core` enforces daily caps (`Subject.max_hours_per_day`, `SchoolClass.max_lessons_per_day`) as legality pruning during search (ADR 0033): `try_place_block` and `try_change_move` reject candidates that would exceed a cap. The post-condition validator `validate_daily_caps` walks the final placements and re-checks the same invariant, but it runs only under `#[cfg(debug_assertions)]` at the `solve.rs` call site:

```rust
// solver/solver-core/src/solve.rs:295-301
#[cfg(debug_assertions)]
if let Err(e) = validate_daily_caps(problem, &solution.placements) {
    panic!("daily-cap post-condition violated: {e}");
}
```

A release build that hits a future cap-pruning hole (a new move that mutates partitions without keeping the cap counters in lockstep, e.g.) silently returns a cap-violating plan. The two sibling validators `validate_no_room_hopping` and `validate_no_double_booking` already run as release-mode `?` calls; daily caps is the odd one out.

The same call-site block also has a redundant `#[cfg(debug_assertions)]` `validate_no_double_booking` panic that is mathematically unreachable: the release-mode `?` two lines above already returns `Err` on any failure, so control never reaches the panic. It is dead code in both build profiles.

## Scope

In scope:
- Promote `validate_daily_caps` to a release-mode `?` call at the same site as the other two validators.
- Drop the unreachable `#[cfg(debug_assertions)]` `validate_no_double_booking` panic block.
- Add a property test that pins the new contract: `lahc_rr_kempe` output must satisfy `validate_daily_caps` for any random small problem.
- Update `solver/CLAUDE.md`: drop the "validate_daily_caps is debug-only" bullet (the warning it documents goes away); tighten the "Post-condition validators trio" bullet to assert the now-uniform "one release call, no debug panic block" shape.

Out of scope:
- Auditing every other hard constraint (teacher max-hours-per-week, teacher / room blocked times, room subject suitability, teacher qualification) for missing post-condition validators. Those become their own OPEN_THINGS items if a real pruning gap surfaces.
- ADR 0033 amendments (ADRs are immutable; the release-mode validator is a strengthening, not a contradiction).
- Backend / frontend changes (the public surface is unchanged: `solve()` still returns `Solution` on success or raises on failure; `solver_io.py` already catches `(ValueError, RuntimeError)`).

## Approach

Three-commit PR on `feat/solver-validate-daily-caps-production`. Behavioral change is one commit, structural cleanups land separately to keep `git bisect` legible per `solver/CLAUDE.md`'s "structural and behavioral never in the same commit" rule.

### Commit 1: `refactor(solver-core)`: drop unreachable debug-only validate_no_double_booking call

`solver/solver-core/src/solve.rs:302-305` — delete the block. Behavior preserved: the release `?` at line 293 already propagates `Err(Error::Input)` on any failure; the panic block could only execute on a control path that does not exist. `mise run test:rust` stays green.

### Commit 2: `test(solver-core)`: add lahc_rr_kempe daily-caps property test

`solver/solver-core/tests/lahc_property.rs`:
- Widen the existing import: `use solver_core::validate::{validate_daily_caps, validate_no_double_booking};`.
- Add a new `proptest!` body next to `lahc_rr_kempe_does_not_double_book_class` (around line 425):

  ```rust
  #[test]
  fn lahc_rr_kempe_respects_daily_caps(p in lahc_small_problem()) {
      let cfg = lahc_rr_kempe_cfg(0);
      let solution = solve_with_config(&p, &cfg).expect("lahc_rr_kempe must succeed");
      validate_daily_caps(&p, &solution.placements)
          .expect("validate_daily_caps must pass on lahc_rr_kempe output");
  }
  ```

Mirrors the double-booking test's shape. Forward guard: green from day one; fires only if a future move-path bug breaks cap pruning.

5x128 local sweep before commit per `solver/CLAUDE.md`'s property-test widening rule:

```bash
for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property; done
```

### Commit 3: `fix(solver-core)`: promote validate_daily_caps to release post-condition

`solver/solver-core/src/solve.rs`:
- Replace the `#[cfg(debug_assertions)]` block at lines 295-301 with the same shape as the two preceding validators:

  ```rust
  validate_no_room_hopping(problem, &solution.placements)?;
  validate_no_double_booking(problem, &solution.placements)?;
  validate_daily_caps(problem, &solution.placements)?;
  ```

  The "loud in dev/tests" requirement is met by integration tests `.expect()`-ing the solver result; the unwrap surfaces `Error::Input("input: subject ... exceeds max_hours_per_day on (class ..., day ...): N > M")` verbatim.

- Drop the comment cluster ("Debug-only post-condition... Loud in dev/tests, free in release") since it no longer matches the code.

`solver/CLAUDE.md`:
- Delete the bullet "**`validate_daily_caps` is `#[cfg(debug_assertions)]`-only at the `solve.rs` call site.**" entirely. The unused-import warning it documents goes away because the symbol is now referenced unconditionally.
- Edit the "**Post-condition validators trio.**" bullet so its wiring sentence reads: "Wiring pattern is identical: one release-mode `Result`-form call per validator at the tail of `solve_with_config_stats`; `Err(Error::Input)` propagates via `?` and integration tests `.expect()` it." (Drop the "plus a `#[cfg(debug_assertions)]` panic block" clause and the "Validator failures indicate a solver bug, not malformed input." sentence stays.)

## Tests

Existing coverage that stays green and now exercises the release path:
- `solver-core/src/validate.rs::tests::validate_daily_caps_*` (4 unit tests).
- `solver-core/tests/daily_caps.rs::caps_*` (3 integration tests including the kempe production-caps smoke).

New coverage:
- `solver-core/tests/lahc_property.rs::lahc_rr_kempe_respects_daily_caps` (1 property test, mirror of `lahc_rr_kempe_does_not_double_book_class`).

Validation commands:
- `mise run test:rust` (workspace nextest run).
- `for s in 1 2 3 4 5; do PROPTEST_CASES=128 PROPTEST_SEED=$s cargo nextest run -p solver-core --test lahc_property; done` (property-test widening sweep).
- `mise run lint` (catches the now-resolved unused-import warning if the cleanup missed a sibling site).

No backend or frontend test changes required; existing `backend/CLAUDE.md:42` bullet ("validator trio `bail!` on a violation") becomes literally accurate once this PR lands.

## Risks

- A real-world solve produces a cap-violating plan today and is silently accepted; the release tightening surfaces it as an HTTP 500. **Mitigation:** the kempe smoke at `tests/daily_caps.rs::caps_kempe_solve_under_production_caps_smoke` already runs in debug mode (where the panic fires) on the dreizuegig fixture reshaped to production caps and has been green throughout the active sprint, so no known live cap violation. The new property test extends the guard to randomized small problems.
- The `validate_daily_caps` walk adds O(P) cost to every solve. **Mitigation:** negligible against the 5000ms LAHC deadline (microseconds on `placements.len() <= 300` for production-shape fixtures).

## Acceptance

- A release build of `klassenzeit_solver` returns `Err(Error::Input("input: ..."))` instead of a cap-violating placements vector when daily caps are exceeded.
- The `solver/CLAUDE.md` "validate_daily_caps is debug-only" bullet is gone; the trio bullet describes the uniform release-only shape.
- `lahc_rr_kempe_respects_daily_caps` is in `tests/lahc_property.rs` and passes the 5x128 local sweep.
- `mise run test:rust` and `mise run lint` are green.
- OPEN_THINGS item 53 is deleted.

## Out of scope (captured for follow-up)

If a future bench refresh or a subagent run surfaces a hard constraint that lacks a post-condition validator (teacher max-hours-per-week, blocked-times, qualifications, suitabilities), open a new OPEN_THINGS item rather than expanding this PR. The doctrine line in `solver/CLAUDE.md` "every hard constraint = pruning during search + release validator after" is the checklist gate; this PR does not blanket-add validators for constraints it does not already touch.
