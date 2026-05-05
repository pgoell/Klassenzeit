# Sprint 4 follow-ups: full bake-off bench refresh, dreizuegig CP-SAT investigation, production-default decision

Status: brainstorm closed 2026-05-04 (autonomous mode under `/autopilot`). One PR ships items 26-29 of `docs/superpowers/OPEN_THINGS.md` "Sprint 4 follow-ups (next pickup)".

Brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (will be posted as PR comments via `.claude/commands/post_brainstorm_comments.py`).

## Goal

Close the bake-off program by running the canonical bake-off bench at production budget settings, deciding which backend ships as the production default, and updating the harness so future bake-off refreshes do not require operator-side venv activation.

The bake-off implementation closed 2026-05-04 with all four backends shipped (`lahc`, `lahc_rr`, `lahc_rr_kempe`, `cpsat`). The smoke `BENCH_RESULTS.md` (`--budget 30s --seeds 5`) is suggestive but not authoritative; the production-default decision needs the canonical `--budget 60s --seeds 20` shape per ADR 0029.

## Non-goals

- No new solver algorithm. All four backends are frozen; this PR is bench data + a default flip.
- No `peak_memory_kb` / `time_to_first_feasible_ms` columns (item 19); deferred until after this refresh per the OPEN_THINGS section.
- No FFD-unplaced rescue, no `RR_K` sweep, no Kempe depth sweep. Those items in OPEN_THINGS gate on bench data showing the relevant sensitivity.
- No CP-SAT model rework. If 60 s / 120 s budget is insufficient for dreizuegig, the verdict is "CP-SAT marginal on this fixture at this encoding"; deeper rework gates on a future incident, not on this PR.

## Architecture changes

Three structural changes plus one data refresh and one default flip.

### 1. `mise run bench:bakeoff` wraps `cargo run` in `uv run`

`mise.toml`'s `[tasks."bench:bakeoff"]` runs `uv run cargo run -p solver-bench --release --` instead of bare `cargo run`. The `uv run` prefix activates the workspace virtual environment so the cargo subprocess inherits a `PATH` that resolves `python3` to the `.venv` interpreter, which has `ortools` and the editable `klassenzeit_solver` wheel.

Side effect: `uv run` will resync the lockfile if needed (a few seconds on a clean tree). This is desirable for a bench refresh: it prevents stale-wheel skew between the LAHC backend (which the bench links into directly) and the cpsat backend (which goes through the editable `klassenzeit_solver` wheel).

### 2. `solver-bench` emits per-cell progress on stderr

`solver-bench/src/main.rs` writes `eprintln!("cell start: {fixture} / {backend}", ...)` and `eprintln!("cell done: {fixture} / {backend} feasibility {n}/{seeds} ...")` around each `run_cell` call. No format change to `BENCH_RESULTS.md`. One-line addition in two places.

This makes the 5-hour run observable through `Bash run_in_background=true` plus `BashOutput`. Without this, the refresh is a black box until it finishes.

### 3. Investigation: dreizuegig CP-SAT at extended budgets

Before running the canonical refresh, run a targeted side-experiment:

```bash
mise run bench:bakeoff -- --budget 60s --seeds 5 --fixtures dreizuegig --out /tmp/cpsat-60s.md
mise run bench:bakeoff -- --budget 120s --seeds 5 --fixtures dreizuegig --out /tmp/cpsat-120s.md
```

If 60 s yields >= 4/5 feasibility on cpsat-dreizuegig, the canonical refresh's 60 s budget is sufficient and the investigation is closed with an entry in ADR 0031.

If 60 s yields 0/5 but 120 s yields >= 4/5, document the gap; CP-SAT is feasibility-capable on dreizuegig but not within the 60 s production budget. ADR 0031 records this as a reason CP-SAT is not the production default for this fixture class.

If both yield 0/5, the warm-start path (`model.AddHint(var, value)` from a feasible LAHC solution) is the next experiment. Optional, only if Q3's preconditions are met.

### 4. Canonical refresh of `BENCH_RESULTS.md`

```bash
mise run bench:bakeoff -- --budget 60s --seeds 20
```

Wall-clock estimate: 4 fixtures × 4 backends × 20 seeds × 60 s = 320 minutes for LAHC variants alone (each LAHC seed runs to deadline). CP-SAT cells finish well under budget when feasible; cells that time out add their full budget. Total expected wall-clock: 5-6 hours.

Run via `Bash run_in_background=true`. Tee stderr to `/tmp/bench-bakeoff.log` so per-cell progress survives a process kill.

If background Bash kill-after-some-duration is shorter than the run, fall back to per-fixture invocations:

```bash
mise run bench:bakeoff -- --budget 60s --seeds 20 --fixtures grundschule --out /tmp/refresh-grundschule.md
mise run bench:bakeoff -- --budget 60s --seeds 20 --fixtures zweizuegig --out /tmp/refresh-zweizuegig.md
mise run bench:bakeoff -- --budget 60s --seeds 20 --fixtures dreizuegig --out /tmp/refresh-dreizuegig.md
mise run bench:bakeoff -- --budget 60s --seeds 20 --fixtures lock_in --out /tmp/refresh-lock_in.md
```

Then concatenate the rows by hand (the harness writes a fresh header / footer per run; the rows for each fixture × backend pair are stable across invocations on a quiet host).

If the host is loaded enough that wall-clock columns swing wildly between fixture invocations, downscale to `--seeds 10` and document the downscale in ADR 0031.

### 5. Production-default decision

Apply this rule to the refreshed numbers:

1. **Hard gate (criterion C from brainstorm Q5):** any backend with a `0/N` cell on any fixture in `BENCH_RESULTS.md` is rejected. Reliability is the bake-off's first-order goal.
2. **Tiebreak among survivors (criterion A):**
   - Highest median feasibility rate (sum of `feasibility_count` across fixtures / total seeds).
   - Lowest median soft-score across feasible cells.
   - Lowest median total wall-clock (only as a third tiebreak).

The default flips in `backend/src/klassenzeit_backend/core/settings.py:56`:

```python
solver_backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"] = "<chosen>"
```

The chosen value is whichever backend wins the rule. All four backends remain available behind `KZ_SOLVER_BACKEND`.

A regression test pins the chosen default in `backend/tests/core/test_settings.py`. The current shape of the file already covers `solver_backend`; the existing assertion either changes value or a sibling assertion is added depending on what's there.

### 6. ADR 0031: production-default decision

New ADR at `docs/adr/0031-solver-production-default.md`. Records:

- The bench shape that produced the data.
- The decision rule (criterion C hard gate + criterion A tiebreak).
- The chosen default.
- A "why we did not pick X" paragraph for each loser.
- The dreizuegig CP-SAT investigation findings.
- A "reversibility" note: this default is set per-process via `KZ_SOLVER_BACKEND`; staging or prod can swap with one env-var change without a code release.

Index entry added to `docs/adr/README.md`.

## Data flow

The bench harness already produces a deterministic markdown table given a `(--budget, --seeds, --fixtures)` triple and the four backends. The artifacts:

```
mise.toml
  ↓ (uv run)
cargo run -p solver-bench --release
  ├─ run_cell(Lahc, ...)  → CellResult
  ├─ run_cell(LahcRr, ...)
  ├─ run_cell(LahcRrKempe, ...)
  └─ run_cell(CpSat, ...) → spawn python3 -m klassenzeit_solver.cpsat per seed
  ↓
BENCH_RESULTS.md  ← committed
```

The cpsat path's subprocess argv is unchanged. Only the wrapping shell environment changes.

## Error handling

- **`uv run` not on PATH.** mise pins `uv` already (`mise.toml`'s `[tools]` section); fresh clones go through `mise install` which fetches it. No new failure mode.
- **`ortools` import error inside the cpsat subprocess.** Already raises `RuntimeError` from `klassenzeit_solver.cpsat`; the bench harness logs `cpsat subprocess non-zero exit (seed=N): ...` and counts the cell as failure.
- **CP-SAT subprocess runaway.** Theoretical risk; OR-Tools' `max_time_in_seconds` is a soft limit. Not pre-mitigated. If a cell hangs >2x budget during the actual refresh, kill the subprocess by hand and rerun that fixture. If this becomes recurrent, add `child.kill()` after a `Duration::from_secs(budget.as_secs() * 2)` watchdog (deferred; YAGNI).
- **Background Bash killed mid-run.** Fall back to per-fixture invocations as documented above.

## Testing

- **Unit:** `solver-bench/src/main.rs::tests` covers CLI parsing and row formatting. The per-cell stderr lines are observably tested via existing tests if any add to them; otherwise no new unit test (a `println!` change does not warrant one).
- **Settings:** `backend/tests/core/test_settings.py` pins `solver_backend` default to the chosen value. A 1-3 line addition.
- **No new integration tests.** The bake-off bench itself is the integration test (it loads each backend through its full path); changing the harness's stderr does not warrant new test coverage.

## Open follow-ups (carry forward in OPEN_THINGS, not in this PR)

- Item 19: `peak_memory_kb` and `time_to_first_feasible_ms` columns. Now actionable since CP-SAT is the first backend where these have meaning. The next pickup after this PR if no Beyond-Grundschule work has resumed.
- Items 11-15: lift `xfail` on Grundschule solvability + quality, activate `prefer_late_period`, drop 7th period, fix zweizuegig criterion bench.
- Items 20-25: bake-off-data-driven LAHC tuning (R&R rescue, RR_K sweep, Kempe depth sweep, lesson-group co-swap).

## Migration impact

None. The default flip changes behaviour only on processes that did not set `KZ_SOLVER_BACKEND`. Staging deploys read the env var via standard pydantic-settings; if staging today has `KZ_SOLVER_BACKEND=lahc` set explicitly (it doesn't, per `deploy/.env.staging`), the flip is invisible. If unset, staging picks up the new default on the next deploy.

Backend tests run with `KZ_SOLVE_DEADLINE_MS=0` (greedy-only); the solver-backend default is moot for the test suite. Production-default flip is a chore commit, not a feat: behaviour is unchanged for callers that already pin the env var, and the change is reversible in one line.
