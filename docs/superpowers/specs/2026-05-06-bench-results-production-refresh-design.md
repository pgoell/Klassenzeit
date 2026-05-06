# Bake-off bench production refresh (`BENCH_RESULTS.md`, item 42) spec

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Tidy / data-refresh phase: item 42.
**Goal.** Replace the `--budget 5s --seeds 4` shape demo currently committed at
`solver/solver-core/benches/BENCH_RESULTS.md` with a production-cell-shape refresh
(`--budget 60s --seeds 20`, ~4.5 h on the recording host's AMD Ryzen 7 3700X), so
the canonical 17-column bench table reflects the post-item-31 / post-item-41 /
post-item-45 / post-item-46 solver state.

**Non-goal.** No code changes anywhere outside `BENCH_RESULTS.md` itself. No ADR
0032 amendment, even if the new numbers shift the LAHC-vs-CP-SAT ordering on a
fixture (item 42's OPEN_THINGS bullet routes that to a follow-up plan, not an ADR
edit). No new bench columns, fixtures, or backends. No `solve_deadline_ms` rework
(item 34 stays in its queued slot). No schedule-quality threshold edits (items 11,
12, 13, 14 remain on the active sprint queue).

## Context

The committed `BENCH_RESULTS.md` reads `Seeds = 4` in every row and carries the
manual footer `_Shape demo at low budget/seeds (--budget 5s --seeds 4); production
refresh queued as OPEN_THINGS item 42._`. That shape demo landed alongside the
column-format work and was always intended to be replaced by a production refresh
once the pre-conditions cleared.

Pre-conditions, in landing order:

- **Item 41** (PR #185 era) closed the slice-vs-full reporting parity gap, so a
  production-budget run reports the same medians regardless of how the runner
  slices the seed range.
- **Item 31** added the five right-most quality columns (`Worst spread`,
  `Worst home-room ratio`, `Total interior gaps`, `Late-period ratio`,
  `Quality (pass / 4)`). The committed shape-demo table already uses the 17-column
  shape, so the column count does not change in this refresh.
- **Item 45** (PR #196) fixed the Kempe BFS bipartiteness violation that caused
  same-iteration class double-booking. Without that fix a long bake-off run on the
  `lahc_rr_kempe` backend was likely to surface a hard-violation cell instead of
  the genuine soft-score median.
- **Item 46** (PR #198) made the supervisor render a `panic` placeholder for any
  cell whose subprocess fails and continue to the next cell. Before that fix a
  single-cell crash inside the ~4.5 h run scrapped the whole table; now the table
  always lands.

With those four items shipped on master (latest commit on the branch base is
`33b93ea fix(solver-bench): supervisor renders panic placeholder ...`), the
production refresh is unblocked. This spec only covers the refresh itself; the
follow-up items 43 (`room_hop` and `day_too_long` column promotion, P2) and 44
(item 12 enables `prefer_late_period`, then re-refresh) remain in their queued
slots.

Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run). Reproduces on
master tip `33b93ea`.

## Scope

**In scope.**

- Run `mise run bench:bakeoff` at default args. The supervisor's
  `default_supervisor_args` in `solver/solver-bench/src/main.rs:107-114` already sets
  `--budget 60s --seeds 20` and writes the file in place at
  `solver/solver-core/benches/BENCH_RESULTS.md`. No CLI overrides needed.
- The supervisor's footer regeneration (`write_footer`,
  `solver/solver-bench/src/main.rs:795-853`) overwrites the entire file body and
  drops the hand-added `_Shape demo …_` italic line for free. No manual file edit
  is needed to remove the stale footer.
- Pre-run housekeeping: the bake-off subprocess invokes `python3 -m
  klassenzeit_solver.cpsat` for the CP-SAT backend (`solver/CLAUDE.md`'s
  bench-binding note). If `klassenzeit_solver` is stale relative to `solver-py`,
  the CP-SAT cells could observe a phantom wire-format mismatch. Run `mise run
  solver:rebuild` before the bench. (No solver-py edits in this branch, so this
  is a defensive step; it is fast and removes a class of false-positive failures
  from the run log.)
- Data validation after the bench finishes: confirm 16 data rows present,
  `Seeds = 20` in every row, footer date matches today (2026-05-06 or whatever the
  run finishes on), the `_Shape demo …_` italic line is gone, and at most one row
  shows `panic` in the Feasibility column.
- Commit the refreshed file as
  `chore(solver-bench): refresh BENCH_RESULTS.md at production cell shape (item 42)`.
- Delete item 42's bullet from `docs/superpowers/OPEN_THINGS.md`. The `Active
  sprint program` section's next-pickup pointer should advance to whichever item
  is next on the queue (likely item 43 or item 12, whichever is highest priority
  unblocked at finalize time).
- If LAHC-vs-CP-SAT feasibility ordering flipped on any fixture between the 5s
  shape-demo numbers and the 60s production numbers, add a follow-up bullet
  under `## Open solver-default follow-ups` (creating that section if it does not
  exist) flagging an ADR 0032 production-default revisit. The PR body summarises
  the flip cell-by-cell; the ADR edit itself is out of scope for this PR.

**Out of scope.**

- Editing ADR 0032 in this PR, regardless of what the data shows.
- Adding new bench columns. Items 43 and 44 own future column work.
- Changing the supervisor or any backend's CLI surface.
- Tightening any test thresholds (`max_spread`, `min_ratio`, etc.). Items 11-14
  own those moves and depend on this refresh's data, but neither this PR nor its
  follow-up should modify the thresholds inline.

## Acceptance criteria

- `solver/solver-core/benches/BENCH_RESULTS.md` after the run has, in order:
    - Title line, generation comment, header row, separator row.
    - Exactly 16 data rows: 4 fixtures (`grundschule`, `zweizuegig`, `dreizuegig`,
      `lock_in`) × 4 backends (`lahc`, `lahc_rr`, `lahc_rr_kempe`, `cpsat`).
    - Every data row's `Seeds` column reads `20`.
    - Each row has 17 pipe-delimited cells (16 columns plus the leading
      empty-cell-from-leading-pipe).
    - Footer matches the supervisor's `write_footer` template exactly: refresh
      date is today, the trailing reference line points at ADR 0029 + ADR 0034,
      and the `_Shape demo …_` italic line from the previous file is absent.
- A `panic` cell on at most one row is acceptable per item 46. Two or more
  `panic` rows triggers an investigation, not a ship.
- Pre-push hooks pass: `cargo nextest run --workspace`, `uv run pytest`, and
  frontend Vitest. None of those should be affected by a markdown-only diff.
- Conditional follow-up: if any fixture's `cpsat` row's `feasible_seeds` count
  exceeds the corresponding row's value across all three LAHC backends, add the
  ADR 0032 follow-up bullet to `OPEN_THINGS.md` in the same PR. Otherwise, no
  follow-up is needed.

## Risks

| Risk | Likelihood | Mitigation |
| --- | --- | --- |
| Supervisor exits non-zero (host OOM or kernel signal) | Low | Capture stderr, restart from scratch. Item 46's resilience handles per-cell panic, not supervisor-level crashes. |
| Wall-clock far exceeds 4.5 h (host loaded) | Low | Soft 6 h watchdog: surface to the user before continuing if the supervisor is still running at 6 h. |
| Two or more cells render `panic` | Very low | Inspect supervisor stderr; if the failure is solver-side, escalate as a separate fix-PR rather than shipping a half-empty table. |
| Rankings flip dramatically | Low | Add a follow-up OPEN_THINGS bullet (per the `Conditional follow-up` clause above). Do not interpret the data inline in this PR's body beyond a one-line pointer. |
| Pre-existing `validate_daily_caps` unused-import warning | Cosmetic | Pre-existing on master, orthogonal to item 42. Leave for a separate tidy. |

## Reproduction

1. `git checkout chore/bench-results-production-refresh`
2. `mise run solver:rebuild`
3. `mise run bench:bakeoff` (~4.5 h on the recording host)
4. `git diff solver/solver-core/benches/BENCH_RESULTS.md` should show every
   `Seeds=4` flipping to `Seeds=20`, every numeric cell refreshed, and the
   trailing `_Shape demo …_` italic line removed.
5. `git status` should show only `BENCH_RESULTS.md` modified at this point.
