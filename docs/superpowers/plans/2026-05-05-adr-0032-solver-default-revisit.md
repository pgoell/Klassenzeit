# ADR 0032 Solver Production-Default Revisit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refresh `BENCH_RESULTS.md` against the post-item-37 solver state and write ADR 0032 with the corrected verdict against the ADR 0029 decision rule. Flip the production default if the verdict moves; otherwise document the hold.

**Architecture:** Single PR on `chore/solver-default-revisit-item-29`. Three or four commits depending on verdict: spec (already shipped), plan, bench refresh + ADR + cross-references (one or two commits depending on flip), OPEN_THINGS bookkeeping. Backed by a fresh canonical `mise run bench:bakeoff` run (`--budget 60s --seeds 20`, all four fixtures × all four backends, ~4.5 hours wall-clock) launched in the background while drafts are written.

**Tech Stack:** Cargo workspace (`solver-bench` binary), uv-managed Python (`klassenzeit_solver` editable wheel for the cpsat backend), markdown docs, `pytest` for the settings-default regression pin, mise for tasks, lefthook for the pre-commit / commit-msg / pre-push gates, cog for Conventional Commits enforcement.

---

## File Structure

- Create:
    - `docs/adr/0032-solver-production-default-revisit.md` (~120 lines, `0031`-shaped)
    - `docs/superpowers/plans/2026-05-05-adr-0032-solver-default-revisit.md` (this file)
- Modify:
    - `solver/solver-core/benches/BENCH_RESULTS.md` (regenerated wholesale by `mise run bench:bakeoff`; do not hand-edit)
    - `docs/adr/README.md` (add the 0032 row to the index, keep ordering)
    - `docs/superpowers/OPEN_THINGS.md` (delete item 29, repoint sprint-header "next pickup" line, no other touch)
    - `solver/CLAUDE.md` (cross-reference ADR 0032 alongside the existing ADR 0031 mention; one line)
    - `backend/CLAUDE.md` (`KZ_SOLVER_BACKEND` paragraph: cross-reference ADR 0032; one line)
- Modify (only if verdict flips):
    - `backend/src/klassenzeit_backend/core/settings.py` (one line: the `solver_backend` Literal default)
    - `backend/tests/core/test_settings.py` (`test_solver_backend_default_is_production_choice` assertion: one line; docstring already references ADR 0031, append "and ADR 0032" cross-reference)

The bench harness writes `BENCH_RESULTS.md` directly; the human writer never edits it. The settings file change is one literal flip if the verdict moves.

---

### Task 1: Plan in place

**Files:**
- Create: `docs/superpowers/plans/2026-05-05-adr-0032-solver-default-revisit.md` (this file)

- [ ] **Step 1: Commit the plan**

```bash
git add docs/superpowers/plans/2026-05-05-adr-0032-solver-default-revisit.md
git commit -m "docs: add adr 0032 solver default revisit plan (sprint 5 item 29)"
```

Expected: lefthook commit-msg hook accepts the message; cog flags none.

---

### Task 2: Confirm the bake-off harness sees the post-item-37 binding

**Files:** none directly modified. This task is a state-confirmation gate.

- [ ] **Step 1: Verify `solver:rebuild` already ran cleanly in this session**

```bash
ls -la /home/pascal/.cache/uv/builds-v0 | head
mise run solver:rebuild 2>&1 | tail -5
```

Expected: `🛠 Installed klassenzeit-solver-0.1.0` (or "already up to date"). `mise run solver:rebuild` is idempotent and seconds-fast on a clean tree.

- [ ] **Step 2: Confirm the bench is still running and has not crashed**

```bash
tail -50 /tmp/claude-1000/-home-pascal-Code-Klassenzeit/<task-uuid>/tasks/<bg-id>.output
```

Expected: `cell start: <fixture> / <backend>` lines progressing through the matrix; no panics or `error:` lines from cargo. If the bench crashed, restart with `mise run bench:bakeoff 2>&1 | tee /tmp/kz-bakeoff.log` (run_in_background) and re-confirm.

---

### Task 3: Wait for the bench to complete and persist the new BENCH_RESULTS.md

**Files:**
- Modify: `solver/solver-core/benches/BENCH_RESULTS.md` (regenerated wholesale by the harness)

- [ ] **Step 1: Block until the background bench process exits**

Either monitor the bg task to completion, or check periodically with:

```bash
ps -p $(pgrep -f 'solver-bench') 2>&1 || echo "bench finished"
```

Expected: process gone; tail of the log shows `wrote BENCH_RESULTS.md` (the harness's final line) and exit 0.

- [ ] **Step 2: Sanity-check the regenerated file**

```bash
cat solver/solver-core/benches/BENCH_RESULTS.md
```

Expected:
- 16 data rows: 4 fixtures (`grundschule`, `zweizuegig`, `dreizuegig`, `lock_in`) × 4 backends (`lahc`, `lahc_rr`, `lahc_rr_kempe`, `cpsat`).
- All `Seeds` columns at `20`.
- `Refreshed YYYY-MM-DD on <CPU>, <kernel>, <rustc>` footer dated today.
- No row with `Feasibility = 0/20` unless the corrected solver legitimately fails that cell. If a cell goes 0/20, capture it for the ADR; do not regenerate.

- [ ] **Step 3: Stage the regenerated file**

```bash
git add solver/solver-core/benches/BENCH_RESULTS.md
```

Hold the commit until task 4 (which writes the ADR) and task 5 (which decides the flip) complete; the bench refresh + ADR + (if flip) settings change ship as one or two coherent commits.

---

### Task 4: Apply the decision rule and decide hold vs. flip

**Files:** none modified. This is a reading + analysis step.

- [ ] **Step 1: Walk the rule against the corrected table**

Read `solver/solver-core/benches/BENCH_RESULTS.md` and apply the rule from the spec:

1. Hard gate: any backend with a `0/N` cell on any fixture is rejected.
2. Tiebreak 1: higher feasibility rate wins.
3. Tiebreak 2: lower soft-score sum across feasible cells wins.
4. Tiebreak 3: lower median total wall-clock across feasible cells wins.

Tally manually. Most likely outcome: all four backends pass the hard gate; LAHC variants tie at soft-score sum near zero; `cpsat` remains far behind on soft-score sum (PR #181's table had it at 9721); the decision falls between `lahc`, `lahc_rr`, `lahc_rr_kempe` on soft-score, and if those tie at zero, on wall-clock.

- [ ] **Step 2: Record the verdict**

Note in the working notes (not committed): "verdict: hold (lahc_rr_kempe)" or "verdict: flip to <backend>". Drives task 5.

---

### Task 5: Write ADR 0032

**Files:**
- Create: `docs/adr/0032-solver-production-default-revisit.md`
- Modify: `docs/adr/README.md`

- [ ] **Step 1: Write the ADR**

Use this skeleton, fill verdict-dependent sections from task 4's outcome:

```markdown
# 0032: Solver production-default revisit

- **Status:** Accepted
- **Date:** 2026-05-05

## Context

ADR 0031 (Accepted 2026-05-05) picked `Settings.solver_backend = "lahc_rr_kempe"` off the canonical bench output committed in PR #181. Items 26 + 27 (R&R anchor filter + property tests, PR #183), 28 (placement-count gate in the bake-off harness, PR #184), and 37 (R&R row-keyed rollback, PR #186) all landed AFTER PR #181 and changed both the solver state under test and the harness's ability to detect silent placement drops. ADR 0031's verdict therefore rode on partly-corrupted data: rows for `lahc_rr` and `lahc_rr_kempe` were silently dropping placements that the harness pre-item-28 could not flag.

This ADR re-applies the ADR 0029 / ADR 0031 decision rule against `solver/solver-core/benches/BENCH_RESULTS.md` regenerated post-item-37 at canonical settings (`--budget 60s --seeds 20`, all four fixtures × all four backends).

## Decision

The production default is `Settings.solver_backend = "<verdict>"`.

[Match ADR 0031's "Decision" section exactly; one short sentence.]

## Rationale

Decision rule (re-applied verbatim from ADR 0029 / ADR 0031):

1. Hard gate: any backend with a `0/N` cell on any fixture is rejected.
2. Tiebreak: feasibility rate, then median soft-score across feasible cells, then median total wall-clock.

Applied to the refreshed bench:

| Backend | 80/80? | Soft-score sum | Wall-clock | Verdict |
| --- | :-: | ---: | --- | --- |
| `lahc` | <yes/no> | <sum> | <range> | <reject reason or tied/chosen> |
| `lahc_rr` | <yes/no> | <sum> | <range> | <...> |
| `lahc_rr_kempe` | <yes/no> | <sum> | <range> | **<chosen>** or <reject> |
| `cpsat` | <yes/no> | <sum> | <range> | <...> |

[Drop the verbatim numbers from the regenerated `BENCH_RESULTS.md`. The Decision rationale that follows is verdict-conditional:]

[If hold:]

The verdict reproduces ADR 0031 against corrected data. Items 26 + 28 + 37 fixed silent placement drops in `lahc_rr` and `lahc_rr_kempe` without changing the relative ordering of the LAHC variants on soft-score. The tie between `lahc_rr` and `lahc_rr_kempe` resolves to `lahc_rr_kempe` for the same superset-of-search reason ADR 0031 cited. No code change to `core/settings.py`; the assertion pin in `tests/core/test_settings.py` continues to assert `lahc_rr_kempe`.

[If flip:]

Items 26 + 28 + 37 fixed silent placement drops that had been hiding a soft-score advantage for `<new-default-backend>`. Specifically: <one-sentence diagnosis from the table>. The default flips from `lahc_rr_kempe` to `<new-default-backend>` in lockstep with this ADR; `core/settings.py` and `tests/core/test_settings.py::test_solver_backend_default_is_production_choice` move together in the same commit. The other three backends remain available behind `KZ_SOLVER_BACKEND`.

## Consequences

- All four backends remain available behind `KZ_SOLVER_BACKEND`.
- `solver/CLAUDE.md` and `backend/CLAUDE.md` cross-reference ADR 0032 alongside the existing ADR 0031 mention so future readers find the corrected-data confirmation.
- Future bake-off refreshes re-apply the rule against the refreshed table; if a future fixture or backend changes the ordering, ADR 0033 (or later) re-applies the rule against the next refresh.
- ADR 0031's "default flip is one line in `core/settings.py`" reversibility note still holds; that line is the only ABI for the production default.

## Reversibility

Same as ADR 0031. The default is one line in `backend/src/klassenzeit_backend/core/settings.py`. To revert: change the literal back. No data migration, no schema change.
```

Save to `docs/adr/0032-solver-production-default-revisit.md`. Title format: `# 0032: Solver production-default revisit` (colon, no em-dash, per the recent `.claude/CLAUDE.md` ADR rule).

- [ ] **Step 2: Update the ADR index**

```bash
$EDITOR docs/adr/README.md
```

Add a row under the existing 0031 entry mirroring its shape; keep the index sorted by ADR number ascending.

- [ ] **Step 3: Verify ADR title format**

```bash
head -1 docs/adr/0032-solver-production-default-revisit.md
```

Expected: `# 0032: Solver production-default revisit` (no em-dash, no en-dash; the colon-and-space pattern matches the project rule).

---

### Task 6: Cross-reference ADR 0032 from solver/CLAUDE.md and backend/CLAUDE.md

**Files:**
- Modify: `solver/CLAUDE.md` (find the line "Production default per [ADR 0031]" and append "and revisited under [ADR 0032]")
- Modify: `backend/CLAUDE.md` (`KZ_SOLVER_BACKEND` paragraph: "default `<verdict>` per [ADR 0031](...) and [ADR 0032](...)")

- [ ] **Step 1: Locate the ADR 0031 references**

```bash
grep -n "ADR 0031" solver/CLAUDE.md backend/CLAUDE.md
```

Expected: one or two lines per file. Edit each to additionally cite ADR 0032; do not delete the ADR 0031 reference (history preserves the audit trail).

- [ ] **Step 2: Edit each reference**

Apply the smallest possible diff: replace the bare ADR 0031 link with `[ADR 0031](.../0031-solver-production-default.md) and [ADR 0032](.../0032-solver-production-default-revisit.md)` (or whichever inline link form the file uses today). One sentence on the corrected-data confirmation is permissible; no broader restructuring.

---

### Task 7: If verdict flips, change the default + assertion pin

**Run only if task 4's verdict moves the default; skip otherwise.**

**Files (flip-only):**
- Modify: `backend/src/klassenzeit_backend/core/settings.py:56` (the `solver_backend` Literal default)
- Modify: `backend/tests/core/test_settings.py::test_solver_backend_default_is_production_choice` (assertion expected value)

- [ ] **Step 1: Read the current default**

```bash
grep -n 'solver_backend' backend/src/klassenzeit_backend/core/settings.py
```

Expected: a `Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"] = "lahc_rr_kempe"` line.

- [ ] **Step 2: Flip the literal default**

```bash
$EDITOR backend/src/klassenzeit_backend/core/settings.py
```

Change the trailing `= "lahc_rr_kempe"` to `= "<new-default>"`. Leave the union list untouched.

- [ ] **Step 3: Flip the assertion pin**

```bash
$EDITOR backend/tests/core/test_settings.py
```

Change the `assert settings.solver_backend == "lahc_rr_kempe"` line to the new default. Update the docstring to additionally mention "and ADR 0032" so the regression-guard explanation does not rot.

- [ ] **Step 4: Run the test in isolation**

```bash
mise run test:py -- backend/tests/core/test_settings.py::test_solver_backend_default_is_production_choice -v
```

Expected: PASS. Pre-flip the test would have asserted the old default and passed; post-flip it asserts the new default and continues to pass.

---

### Task 8: Commit the bench refresh + ADR + cross-references in one atomic commit

**Files staged:**
- `solver/solver-core/benches/BENCH_RESULTS.md`
- `docs/adr/0032-solver-production-default-revisit.md`
- `docs/adr/README.md`
- `solver/CLAUDE.md`
- `backend/CLAUDE.md`
- (flip-only) `backend/src/klassenzeit_backend/core/settings.py`
- (flip-only) `backend/tests/core/test_settings.py`

- [ ] **Step 1: Stage the files**

```bash
git add solver/solver-core/benches/BENCH_RESULTS.md docs/adr/0032-solver-production-default-revisit.md docs/adr/README.md solver/CLAUDE.md backend/CLAUDE.md
# flip-only:
git add backend/src/klassenzeit_backend/core/settings.py backend/tests/core/test_settings.py
```

- [ ] **Step 2: Commit**

If the verdict holds:

```bash
git commit -m "chore(solver): refresh bake-off bench + adr 0032 production-default revisit (sprint 5 item 29)"
```

If the verdict flips:

```bash
git commit -m "chore(solver): flip production default + adr 0032 (sprint 5 item 29)"
```

The flip-only commit-message variant remains a single Conventional Commits-compliant message; the body cites the rule's tiebreak that drove the flip.

- [ ] **Step 3: Verify the commit**

```bash
git log --stat -1
```

Expected: 5 to 7 files changed depending on verdict. The pre-commit hook ran `mise run lint`; if `mise run lint` failed, fix the issue and re-commit (do not bypass with `--no-verify`).

---

### Task 9: OPEN_THINGS bookkeeping

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Delete item 29**

```bash
$EDITOR docs/superpowers/OPEN_THINGS.md
```

Remove the entire `29.` block under `### Bench prevention phase`. Per the OPEN_THINGS rule "When an item ships, DELETE it from OPEN_THINGS entirely. Do not leave a `✅ Shipped` annotation behind."

- [ ] **Step 2: Repoint the sprint header's "next pickup" line**

The current sprint description names item 29 as the next pickup. Update it to name item 30 (peak RAM / time-to-first-feasible / time-to-optimal) as the next pickup. The phase headers stay; only the sprint-header sentence at the top changes.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/OPEN_THINGS.md
git commit -m "docs: strip item 29 from open things + repoint sprint header (item 29 ships)"
```

---

### Task 10: End-to-end gate before push

**Files:** none directly modified.

- [ ] **Step 1: Lint**

```bash
mise run lint
```

Expected: zero failures. Same as pre-commit; the gate confirms nothing slipped between commits.

- [ ] **Step 2: Test**

```bash
mise run test
```

Expected: zero failures. The flip-only assertion-pin test passes against the new default; no other tests touch the production default at construction time.

- [ ] **Step 3: Push**

```bash
mise exec -- git push -u origin chore/solver-default-revisit-item-29
```

Expected: pre-push hook runs the full Rust + Python + frontend suite; ~30s extra; push completes.

---

## Self-review

- Spec coverage:
    - Bench refresh at canonical settings: tasks 2 + 3.
    - `solver:rebuild` before bench: task 2 (state-confirmation; the rebuild already ran in this session).
    - ADR 0032 with verdict-conditional sections: task 5.
    - ADR title format (colon, no em-dash): task 5 step 3.
    - Settings + test pin lockstep on flip: task 7 + task 8 (both files in the same commit).
    - CLAUDE.md cross-references: task 6.
    - OPEN_THINGS bookkeeping: task 9.
    - Lint + test gate before push: task 10.
- Placeholders: the `<verdict>`, `<sum>`, `<range>`, `<chosen>`, `<reject reason>`, and `<new-default-backend>` slots in task 5's ADR template are intentional; they are filled at the moment the bench data lands and the rule is applied. They are not "TBD"; the engineer fills them deterministically from the table at task 5 step 1. No other placeholders.
- Type / signature consistency: the `Settings.solver_backend` Literal in task 7 is the same field referenced in tasks 6 and the ADR. The assertion pin name `test_solver_backend_default_is_production_choice` is identical across tasks 7 and 5's "Consequences" paragraph.
- Commit-message Conventional-Commits compliance: each commit message in tasks 1, 8, and 9 starts with a lowercase type / scope / subject. cog accepts each.
