# Sprint 4 follow-up: bake-off bench refresh + production-default decision Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close items 26-29 of `docs/superpowers/OPEN_THINGS.md` "Sprint 4 follow-ups": fix `mise run bench:bakeoff` so cpsat works without operator setup, refresh `BENCH_RESULTS.md` at production budget, investigate dreizuegig CP-SAT feasibility, and flip `Settings.solver_backend`'s default based on the data.

**Architecture:** Three structural touches first (mise wrapper, per-cell progress logging, dreizuegig side-experiment), then one wall-clock-heavy data refresh, then one default flip with ADR + regression test. Rollback is reversible by reverting the default-flip commit alone; bench data is artifact-only.

**Tech Stack:** mise, cargo, uv, Rust solver-bench, OR-Tools (CP-SAT), pydantic-settings.

Spec: `docs/superpowers/specs/2026-05-04-solver-bakeoff-followup-design.md`.

---

## File Structure

- `mise.toml`: one-line edit on `[tasks."bench:bakeoff"]`.
- `solver/solver-bench/src/main.rs`: add per-cell stderr progress logging.
- `solver/solver-core/benches/BENCH_RESULTS.md`: regenerated artifact (committed; not hand-edited).
- `backend/src/klassenzeit_backend/core/settings.py:56`: default flip.
- `backend/tests/core/test_settings.py`: regression assertion.
- `docs/adr/0031-solver-production-default.md`: new ADR.
- `docs/adr/README.md`: index entry.
- `solver/CLAUDE.md`: remove the "venv pre-activation required" sentence; add ADR 0031 pointer.
- `docs/superpowers/OPEN_THINGS.md`: check off items 26-29; reorder remaining work.

---

## Task 1: Wrap `bench:bakeoff` in `uv run`

**Files:**
- Modify: `mise.toml:63-65`

- [ ] **Step 1: Edit the mise task**

Replace the `run` line:

```toml
[tasks."bench:bakeoff"]
description = "Run the solver feasibility bake-off bench and rewrite BENCH_RESULTS.md"
run         = "uv run cargo run -p solver-bench --release --"
```

- [ ] **Step 2: Sanity-check from a non-activated shell**

```bash
deactivate 2>/dev/null
mise run bench:bakeoff -- --budget 5s --seeds 2 --fixtures grundschule --out /tmp/sanity.md
```

Expected: command exits 0; `/tmp/sanity.md` shows a `cpsat` row on grundschule with `2/2` feasibility (a 5 s budget is sufficient for grundschule). The command compiles `solver-bench` once, so first invocation may take ~30 s.

If the row shows `0/2` or `cpsat subprocess non-zero exit (seed=N): ModuleNotFoundError: No module named 'ortools'`, the `uv run` wrapper failed to expose the venv. Diagnose: run `uv run python -c "import ortools; print(ortools.__file__)"` from the same shell; the path should be under `.venv/`.

- [ ] **Step 3: Commit**

```bash
git add mise.toml
git commit -m "build(mise): activate venv via uv run for bench:bakeoff"
```

---

## Task 2: Per-cell progress logging in `solver-bench`

**Files:**
- Modify: `solver/solver-bench/src/main.rs:128-138`

- [ ] **Step 1: Add a `log_cell_done` helper above `main` and call it around `run_cell`**

Insert at the top of the `for (name, build) in FIXTURES` loop after `let problem = build();`:

```rust
        for backend in &backends {
            eprintln!("cell start: {} / {}", name, backend.label());
            let cell = run_cell(*backend, &problem, args.budget, args.seeds);
            eprintln!(
                "cell done: {} / {} feasibility {}/{} hard_med={} soft_med={} total_ms_med={:.0}",
                name,
                backend.label(),
                cell.feasibility_count,
                cell.seeds,
                cell.hard_violations_median,
                cell.soft_score_median.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string()),
                cell.total_ms_median,
            );
            write_row(&mut markdown, name, *backend, &cell);
        }
```

- [ ] **Step 2: Confirm clippy is clean**

```bash
mise run lint:rust
```

Expected: PASS. The `eprintln!` calls compile cleanly; no new lint.

- [ ] **Step 3: Sanity-check by re-running the smoke**

```bash
mise run bench:bakeoff -- --budget 5s --seeds 2 --fixtures grundschule --out /tmp/sanity.md
```

Expected stderr lines (interleaved with cargo's compile output):

```
cell start: grundschule / lahc
cell done: grundschule / lahc feasibility 2/2 hard_med=0 soft_med=... total_ms_med=...
cell start: grundschule / lahc_rr
...
cell start: grundschule / cpsat
cell done: grundschule / cpsat feasibility 2/2 hard_med=0 soft_med=... total_ms_med=...
wrote "/tmp/sanity.md"
```

- [ ] **Step 4: Commit**

```bash
git add solver/solver-bench/src/main.rs
git commit -m "feat(solver-bench): emit per-cell progress on stderr"
```

---

## Task 3: Dreizuegig CP-SAT side-experiment (item 27)

**Files:**
- (no source files; produces `/tmp/cpsat-{60s,120s}-dreizuegig.md` and a finding to record in Task 6's ADR)

- [ ] **Step 1: Run dreizuegig CP-SAT at 60 s × 5 seeds**

```bash
mise run bench:bakeoff -- --budget 60s --seeds 5 --fixtures dreizuegig --out /tmp/cpsat-60s-dreizuegig.md 2>/tmp/cpsat-60s-dreizuegig.stderr
```

Wall-clock: ~5 minutes per backend × 4 backends = ~20 minutes. Run in background:

```bash
# Use Bash run_in_background=true for this step in agent execution.
```

Expected output: `/tmp/cpsat-60s-dreizuegig.md` shows four rows for `dreizuegig` (one per backend). The cpsat row's feasibility column tells us whether 60 s is enough.

- [ ] **Step 2: If cpsat at 60 s shows < 4/5, run again at 120 s × 5 seeds**

```bash
mise run bench:bakeoff -- --budget 120s --seeds 5 --fixtures dreizuegig --out /tmp/cpsat-120s-dreizuegig.md 2>/tmp/cpsat-120s-dreizuegig.stderr
```

Wall-clock: ~10 minutes per backend × 4 backends = ~40 minutes.

- [ ] **Step 3: Record findings**

Capture three numbers for the ADR (Task 6):
- `dreizuegig × cpsat × 30s × 5 seeds`: 0/5 (smoke from PR #179, already on master).
- `dreizuegig × cpsat × 60s × 5 seeds`: from Step 1.
- `dreizuegig × cpsat × 120s × 5 seeds`: from Step 2 if Step 2 ran.

Write the three numbers to `/tmp/dreizuegig-cpsat-investigation.txt` so they survive into Task 6's ADR draft. Sample shape:

```
30s: 0/5 (294 hard violations median; CP-SAT timed out without finding feasible)
60s: <N>/5
120s: <N>/5 (if run)
```

- [ ] **Step 4: No commit yet** — this experiment is documentation input for Task 6, not a code change.

---

## Task 4: Run the canonical full refresh of `BENCH_RESULTS.md` (item 26)

**Files:**
- Modify (overwrite): `solver/solver-core/benches/BENCH_RESULTS.md`

- [ ] **Step 1: Tee stderr to a shadow log**

The bench prints per-cell progress to stderr; capturing it preserves observability if the foreground bash exit-codes weirdly.

```bash
mise run bench:bakeoff -- --budget 60s --seeds 20 2>/tmp/bench-bakeoff.log
```

**Run via `Bash run_in_background=true`** (the run is multi-hour; the foreground harness caps at 10 minutes).

Wall-clock estimate: ~5-6 hours total. LAHC variants run to deadline (always the full budget per seed); CP-SAT cells finish under budget when feasible, time out otherwise.

- [ ] **Step 2: Poll the background process**

Periodically read the shadow log:

```bash
tail -50 /tmp/bench-bakeoff.log
```

Expected progression:

```
cell start: grundschule / lahc
cell done: grundschule / lahc feasibility 20/20 ...
cell start: grundschule / lahc_rr
... (all four backends × four fixtures, in order)
wrote "solver/solver-core/benches/BENCH_RESULTS.md"
```

If a cell stays in `cell start:` for >2x the budget × seeds (e.g., LAHC stuck >24 minutes on grundschule), kill the background process and switch to per-fixture invocations (Step 4 fallback).

- [ ] **Step 3: Verify the artifact**

When the background process exits successfully:

```bash
git diff solver/solver-core/benches/BENCH_RESULTS.md
```

Expected: header / footer unchanged; `Refreshed 2026-05-04` updated; 16 rows total (4 fixtures × 4 backends). Each LAHC cell shows `Total wall-clock` ≈ 60000 ms (deadline-bound). CP-SAT cells show variable wall-clocks depending on feasibility outcome.

- [ ] **Step 4 (FALLBACK ONLY): per-fixture invocations if Step 1 cannot run for 5+ hours**

Run the four fixtures separately, in this order (lightest first so we have partial data fast if interrupted):

```bash
mise run bench:bakeoff -- --budget 60s --seeds 20 --fixtures grundschule --out /tmp/refresh-grundschule.md 2>>/tmp/bench-bakeoff.log
mise run bench:bakeoff -- --budget 60s --seeds 20 --fixtures lock_in --out /tmp/refresh-lock_in.md 2>>/tmp/bench-bakeoff.log
mise run bench:bakeoff -- --budget 60s --seeds 20 --fixtures zweizuegig --out /tmp/refresh-zweizuegig.md 2>>/tmp/bench-bakeoff.log
mise run bench:bakeoff -- --budget 60s --seeds 20 --fixtures dreizuegig --out /tmp/refresh-dreizuegig.md 2>>/tmp/bench-bakeoff.log
```

Each invocation writes a fully-formed `BENCH_RESULTS.md`-shape file. To assemble:

1. Take header + first 4 rows from `/tmp/refresh-grundschule.md`.
2. Append rows 5-8 from `/tmp/refresh-zweizuegig.md` (skip header).
3. Append rows 5-8 from `/tmp/refresh-dreizuegig.md`.
4. Append rows 5-8 from `/tmp/refresh-lock_in.md`.
5. Take footer from the last (or any) file.
6. Write to `solver/solver-core/benches/BENCH_RESULTS.md`.

Note row indexes assume each fragment file's row ordering (lahc, lahc_rr, lahc_rr_kempe, cpsat) matches the harness's hardcoded `backends` array. Confirm by inspection before assembling.

- [ ] **Step 5 (FALLBACK ONLY): downscale to `--seeds 10` if even per-fixture invocations fail**

Re-run with `--seeds 10` instead. Document the downscale in Task 6's ADR (criterion: "We did not run --seeds 20 because [reason]; --seeds 10 still gives n=10 confidence on the 0% / 100% feasibility cells that drive the decision").

- [ ] **Step 6: Commit**

```bash
git add solver/solver-core/benches/BENCH_RESULTS.md
git commit -m "docs(solver): refresh BENCH_RESULTS.md at --budget 60s --seeds 20"
```

If downscaled to `--seeds 10`, use:

```bash
git commit -m "docs(solver): refresh BENCH_RESULTS.md at --budget 60s --seeds 10"
```

---

## Task 5: Apply the production-default decision rule

**Files:**
- (no source files; produces a chosen backend value used in Tasks 6 and 7)

- [ ] **Step 1: Read the refreshed table**

```bash
cat solver/solver-core/benches/BENCH_RESULTS.md
```

Identify the four backends' rows per fixture.

- [ ] **Step 2: Apply criterion C (no `0/N` cells)**

For each backend, check: does any fixture row show `0/20` (or `0/10` if downscaled)? If yes, the backend is rejected for production default.

Expected outcome from the smoke + Task 3 investigation:
- `lahc`, `lahc_rr`, `lahc_rr_kempe`: feasible everywhere on smoke; expected to stay feasible on canonical.
- `cpsat`: feasible on grundschule, zweizuegig, lock_in on smoke; dreizuegig depends on Task 3.

- [ ] **Step 3: Apply criterion A (tiebreak among survivors)**

Among backends not rejected in Step 2, rank by:
1. Total `feasibility_count` summed across fixtures (higher is better).
2. Median soft-score across feasible cells (lower is better; aggregate by averaging the four fixture medians — clean tiebreak heuristic).
3. Median total wall-clock across all cells (lower is better; only as third-tier tiebreak).

Compute on a sheet of paper or in a comment; record the winner.

- [ ] **Step 4: Record the decision**

Save the chosen backend name to `/tmp/production-default-decision.txt` for Task 6 / Task 7. Sample shape:

```
chosen: lahc_rr_kempe
runner-up: lahc_rr
rejected: cpsat (0/20 on dreizuegig)
rejected: lahc (15 soft-score median on grundschule vs 0 for the runner-up)
```

- [ ] **Step 5: No commit yet** — this is decision input for Task 6 / Task 7.

---

## Task 6: ADR 0031 (production-default decision)

**Files:**
- Create: `docs/adr/0031-solver-production-default.md`
- Modify: `docs/adr/README.md`

- [ ] **Step 1: Confirm the ADR number is unused**

```bash
ls docs/adr/*.md | sort | tail -5
```

Expected: latest ADR is 0030. New ADR is 0031. If `0031-*.md` exists already, increment.

- [ ] **Step 2: Write the ADR**

Use the spec section "ADR 0031" as the structure. Concrete shape:

```markdown
# 0031: Solver production-default backend

Date: 2026-05-04
Status: Accepted
Supersedes: -
Superseded by: -

## Context

The solver feasibility bake-off (ADR 0029) shipped four backends (`lahc`, `lahc_rr`, `lahc_rr_kempe`, `cpsat`). The smoke `BENCH_RESULTS.md` at `--budget 30s --seeds 5` was suggestive but not authoritative. The production default needs the canonical bench shape (`--budget 60s --seeds 20` per ADR 0029) before it can be set.

## Decision

The production default is `Settings.solver_backend = "<chosen>"`.

## Rationale

The decision rule (per spec `2026-05-04-solver-bakeoff-followup-design.md`):

1. Hard gate: any backend with a `0/N` cell on any fixture is rejected (reliability is the bake-off's first-order goal).
2. Tiebreak among survivors: feasibility rate, median soft-score, median total wall-clock.

Applied to the refreshed numbers:

| Backend | Total feasibility | Median soft-score (feasible) | Median total wall-clock | Verdict |
| --- | --- | --- | --- | --- |
| lahc | <N>/80 | <s> | <ms> | <verdict> |
| lahc_rr | <N>/80 | <s> | <ms> | <verdict> |
| lahc_rr_kempe | <N>/80 | <s> | <ms> | <verdict> |
| cpsat | <N>/80 | <s> | <ms> | <verdict> |

(Fill in from `BENCH_RESULTS.md` and the decision sheet.)

## Dreizuegig CP-SAT investigation

- 30 s × 5 seeds: 0/5 (smoke; PR #179).
- 60 s × 5 seeds: <N>/5.
- 120 s × 5 seeds: <N>/5 (if run).

(Fill in from `/tmp/dreizuegig-cpsat-investigation.txt`.)

Verdict: <CP-SAT is feasible on dreizuegig at <budget> | CP-SAT is marginal on dreizuegig at production budget>. <If marginal, the AddHint warm-start path is recorded as a future-work item under OPEN_THINGS.>

## Consequences

- All four backends remain available behind `KZ_SOLVER_BACKEND`. Switching is a one-env-var change at deploy time, no code release.
- `backend/tests/core/test_settings.py` pins the chosen default as a regression guard.
- `solver/CLAUDE.md` updated to remove the obsolete "venv pre-activation required" sentence (item 28 in OPEN_THINGS) and to point at this ADR.
- Future bake-off refreshes (e.g., when a new fixture or backend lands) re-apply the rule against the refreshed table.

## Reversibility

The default flip is one line in `core/settings.py`. To revert: change the literal back. No data migration, no schema change.
```

- [ ] **Step 3: Add the index entry**

Append to `docs/adr/README.md` in the existing index format:

```markdown
- [0031: Solver production-default backend](0031-solver-production-default.md)
```

(Match the existing list shape — likely a bulleted list at the bottom.)

- [ ] **Step 4: No commit yet** — Task 7 bundles the ADR with the settings flip and the regression test.

---

## Task 7: Flip `Settings.solver_backend` default + pin via test

**Files:**
- Modify: `backend/src/klassenzeit_backend/core/settings.py:56`
- Modify: `backend/tests/core/test_settings.py` (append)

- [ ] **Step 1: Write the failing test first**

Append to `backend/tests/core/test_settings.py`:

```python
def test_solver_backend_default_is_production_choice(monkeypatch: pytest.MonkeyPatch) -> None:
    """Pin the production-default solver backend.

    Flipping this default is intentional: see ADR 0031. Update both this
    assertion and the ADR if a new bake-off refresh changes the verdict.
    """
    monkeypatch.setenv(
        "KZ_DATABASE_URL",
        "postgresql+psycopg://u:p@localhost:5432/kz",
    )
    monkeypatch.delenv("KZ_SOLVER_BACKEND", raising=False)

    settings = Settings(_env_file=None)  # ty: ignore[missing-argument, unknown-argument]

    assert settings.solver_backend == "<chosen>"
```

(Replace `"<chosen>"` with the value from `/tmp/production-default-decision.txt` from Task 5.)

- [ ] **Step 2: Run the test to confirm it fails**

```bash
mise run test:py -- backend/tests/core/test_settings.py::test_solver_backend_default_is_production_choice -v
```

Expected: FAIL with `AssertionError: assert 'lahc' == '<chosen>'` (or PASS if the chosen backend happens to be `lahc`, in which case Task 7 is no-op except for the ADR; document this and skip Step 3 / Step 4 of Task 7).

If the chosen backend IS `lahc` (i.e., the data confirmed the existing default), the test still has value as a regression guard against an accidental flip. Adjust the test message accordingly: `"Pin the production-default backend confirmed by ADR 0031's bake-off refresh (still lahc)."`.

- [ ] **Step 3: Flip the default**

Edit `backend/src/klassenzeit_backend/core/settings.py:56`:

```python
    solver_backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"] = "<chosen>"
```

- [ ] **Step 4: Run the test to confirm it passes**

```bash
mise run test:py -- backend/tests/core/test_settings.py::test_solver_backend_default_is_production_choice -v
```

Expected: PASS.

- [ ] **Step 5: Run the full settings test file as a regression check**

```bash
mise run test:py -- backend/tests/core/test_settings.py -v
```

Expected: all tests pass.

- [ ] **Step 6: Run lint**

```bash
mise run lint
```

Expected: PASS.

- [ ] **Step 7: Commit Task 6 + Task 7 atomically**

The decision ADR, the default flip, the regression test, and the README index entry land in one commit so revert is a single rollback.

```bash
git add docs/adr/0031-solver-production-default.md docs/adr/README.md backend/src/klassenzeit_backend/core/settings.py backend/tests/core/test_settings.py
git commit -m "feat(backend): flip solver_backend default to <chosen>"
```

(Use `feat` if the default actually changes; use `docs(adr): record production-default decision` + `test(backend): pin solver_backend default` as two commits if the default does NOT change but the ADR + test still ship. Default change is the most likely case.)

---

## Task 8: Update `solver/CLAUDE.md` (close out item 28's note)

**Files:**
- Modify: `solver/CLAUDE.md` (the `mise run bench:bakeoff` paragraph, currently mentions "the cargo-spawned `python3` does NOT inherit `.venv` activation from `mise`, so the bench requires the venv pre-activated").

- [ ] **Step 1: Find the paragraph**

```bash
grep -n "cargo-spawned" solver/CLAUDE.md
```

Expected: one match in the bench-workflow section.

- [ ] **Step 2: Rewrite the paragraph**

Replace the venv-activation note with:

```markdown
- **`mise run bench:bakeoff` adds a `cpsat` column** in addition to `lahc` / `lahc_rr` / `lahc_rr_kempe`. The cpsat backend invokes `python3 -m klassenzeit_solver.cpsat` per cell as a subprocess. The mise task wraps `cargo run` in `uv run` so the cargo-spawned `python3` resolves to the workspace venv automatically; no operator-side activation needed. Full refresh wall-clock at production settings (`--budget 60s --seeds 20`) is approximately 5-6 hours. Dev-loop downscale via `--budget 5s --seeds 4 --fixtures grundschule`. Production default per [ADR 0031](../docs/adr/0031-solver-production-default.md).
```

- [ ] **Step 3: Confirm by re-reading the file**

```bash
grep -A 5 "bench:bakeoff" solver/CLAUDE.md | head -10
```

Expected: the new paragraph reads cleanly; no orphan reference to "pre-activated venv" or "source .venv/bin/activate" remains.

- [ ] **Step 4: Commit**

```bash
git add solver/CLAUDE.md
git commit -m "docs(solver): bench:bakeoff now self-activates venv via uv run"
```

---

## Task 9: OPEN_THINGS + auto-memory bookkeeping

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`

- [ ] **Step 1: Mark items 26-29 as shipped**

In `docs/superpowers/OPEN_THINGS.md`'s "Sprint 4 follow-ups (next pickup)" subsection, prefix each item with the ✅ Shipped marker matching the existing convention. Sample shape (preserve the rest of the bullet):

```markdown
26. **Full `--budget 60s --seeds 20` `BENCH_RESULTS.md` refresh on a quiet host.** `[P0]` ✅ Shipped 2026-05-04 in PR <pending>. <one-line summary of canonical refresh>.
27. **Investigate dreizuegige CP-SAT feasibility.** `[P0]` ✅ Shipped 2026-05-04 in PR <pending>. <verdict from ADR 0031>.
28. **`mise run bench:bakeoff` does not auto-activate `.venv` for the cargo subprocess.** `[P0]` ✅ Shipped 2026-05-04 in PR <pending>. Mise task now wraps `cargo run` in `uv run`.
29. **Production-default decision based on the refreshed Pareto frontier.** `[P0]` ✅ Shipped 2026-05-04 in PR <pending>. Default flipped to `<chosen>`; ADR 0031.
```

- [ ] **Step 2: Update the section header**

Change the sprint-section header from "Sprint 4 follow-ups (next pickup)" to "Sprint 4 follow-ups ✅ all four shipped 2026-05-04". Update the active-sprint preamble at the top of OPEN_THINGS to reflect that the bake-off program is fully closed and the next pickup is item 19 (memory + first-feasible columns) OR Beyond-Grundschule Sprint 1 resumption (item 2 in the "Queued sprint: Schwimmen + Sek-I foundations" block).

- [ ] **Step 3: Update auto-memory `project_roadmap_status.md`**

Edit the frontmatter `description` and the active-sprint paragraph to reflect:

- Sprint 4 follow-ups all shipped 2026-05-04.
- Production default is `<chosen>` (ADR 0031).
- Dreizuegig CP-SAT verdict: `<feasible-at-Xs | marginal-at-production-budget>`.
- Next pickup is Beyond-Grundschule Sprint 1 (`Room.is_external` + travel buffers).

Sample edit:

```yaml
---
name: Roadmap status
description: Solver feasibility bake-off CLOSED 2026-05-04 (Sprints 1-4 + follow-ups). Production default = <chosen> (ADR 0031). Next pickup is Beyond-Grundschule Sprint 1 (Room.is_external + travel buffers).
type: project
---
```

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/OPEN_THINGS.md /home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md
git commit -m "docs: close sprint 4 follow-ups in OPEN_THINGS + auto-memory"
```

(Note: auto-memory lives outside the repo. The git add for that path will fail. If it does, commit the OPEN_THINGS edit alone and update auto-memory via the Write tool separately; auto-memory is not a tracked file in this git repo.)

---

## Task 10: Run autopilot's required CLAUDE.md / settings polish

**Files:**
- Possibly: `.claude/CLAUDE.md`, `.claude/settings.json`, `.claude/commands/autopilot.md`, project CLAUDE.md files (driven by skills).

- [ ] **Step 1: Run `claude-md-management:revise-claude-md`**

Per autopilot Step 6. Apply the proposed edits directly (autonomous mode).

- [ ] **Step 2: Run `claude-md-management:claude-md-improver`**

Apply proposed edits directly.

- [ ] **Step 3: Run `fewer-permission-prompts`**

Apply proposed allowlist additions directly.

- [ ] **Step 4: Commit any edits with appropriate Conventional Commits scopes**

Examples: `chore(settings): allow uv run cargo run`, `docs(claude-md): note 'uv run' wrapper for bench:bakeoff`, `docs(autopilot): record bench-refresh-as-background-bash pattern`.

---

## Task 11: Push, open PR, post brainstorm comments

**Files:**
- (none; git + gh)

- [ ] **Step 1: Skill audit**

Per autopilot Step 7. Confirm `superpowers:using-superpowers`, `superpowers:brainstorming`, `superpowers:writing-plans`, `superpowers:test-driven-development`, `superpowers:subagent-driven-development`, `claude-md-management:revise-claude-md`, `claude-md-management:claude-md-improver`, `fewer-permission-prompts` were each invoked via the `Skill` tool this session.

- [ ] **Step 2: Push the branch**

```bash
mise exec -- git push -u origin feat/solver-bakeoff-followup-bench-and-default
```

Expected: pre-push hook runs the full test + lint suite (~30 s); push completes.

- [ ] **Step 3: Open the PR**

```bash
gh pr create --base master --head feat/solver-bakeoff-followup-bench-and-default \
  --title "feat(solver): full bake-off bench refresh + production-default decision (sprint 4 follow-up)" \
  --body "$(cat <<'EOF'
## Summary

Closes Sprint 4 follow-ups (items 26-29 in OPEN_THINGS):

- `mise run bench:bakeoff` now wraps `cargo run` in `uv run` so cpsat works without operator-side venv activation (item 28).
- `solver-bench` emits per-cell progress on stderr so a long-running refresh is observable.
- Refreshed `BENCH_RESULTS.md` at `--budget 60s --seeds 20` (or `--seeds 10` if downscaled, see ADR 0031) (item 26).
- Investigated dreizuegig CP-SAT feasibility at extended budgets (item 27); verdict in ADR 0031.
- Flipped `Settings.solver_backend` default to `<chosen>` based on the bake-off rule (item 29). ADR 0031.

All four backends remain available behind `KZ_SOLVER_BACKEND`; this is a one-env-var swap on a live deploy.

## Bake-off summary

| Backend | Feasibility (across all fixtures) | Median soft-score | Verdict |
| --- | --- | --- | --- |
| (fill in from refreshed BENCH_RESULTS.md) |

## Test plan

- [ ] `mise run bench:bakeoff -- --budget 5s --seeds 2 --fixtures grundschule --out /tmp/sanity.md` passes from a non-activated shell (cpsat row shows 2/2 feasibility).
- [ ] `mise run lint` clean.
- [ ] `mise run test:py -- backend/tests/core/test_settings.py -v` passes (regression assertion for the chosen default).
- [ ] `mise run test:rust` passes.

## Spec / plan / ADR

- Spec: `docs/superpowers/specs/2026-05-04-solver-bakeoff-followup-design.md`
- Plan: `docs/superpowers/plans/2026-05-04-solver-bakeoff-followup.md`
- ADR: `docs/adr/0031-solver-production-default.md`
EOF
)"
```

- [ ] **Step 4: Post brainstorm comments**

```bash
PR_NUMBER=$(gh pr view --json number -q .number)
python3 .claude/commands/post_brainstorm_comments.py "$PR_NUMBER"
```

Expected: one preamble comment + one comment per `## Q…` and `## Decision` section in `/tmp/kz-brainstorm/brainstorm.md`.

- [ ] **Step 5: Set automerge**

```bash
gh pr merge "$PR_NUMBER" --auto --squash
```

- [ ] **Step 6: Wait for CI green and merge**

Per autopilot Step 8. Poll `gh pr view "$PR_NUMBER" --json state -q .state` until `MERGED`.

---

## Self-Review

- **Spec coverage:** all six numbered architecture sections in the spec map to a task. (1 → Task 1, 2 → Task 2, 3 → Task 3, 4 → Task 4, 5 → Task 5, 6 → Task 6 + Task 7. Plus Task 8 / 9 / 10 / 11 for bookkeeping and PR.)
- **Placeholder scan:** the `<chosen>` literal is intentional in Tasks 5-9 and the ADR; it represents a value that is data-determined at run time. Every other step has concrete commands, code, and expected output.
- **Type consistency:** the chosen-backend value flows from Task 5 (`/tmp/production-default-decision.txt`) into Tasks 6, 7, 8, 9 via the same string literal; the test's quoted literal must match `core/settings.py`'s default. The ADR table values flow from `BENCH_RESULTS.md`. No type drift.
- **Rollback:** Tasks 1-4 are revertible artifact / config changes. Task 7 is the one behaviour change; reverting that commit alone restores the prior default. Tasks 8-9 are documentation.

---

## Execution Handoff

Per autopilot, this plan executes in subagent-driven mode (one subagent per task, sequentially because Tasks 4-7 share state — the bench artifact, the chosen default, and the ADR draft). Tasks 1-2 and Task 3 are independent of each other but Task 3 depends on Task 1 (uv run wrapper) and Task 2 (per-cell progress); execute them sequentially in plan order.
