# Bake-off bench production refresh implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `--budget 5s --seeds 4` shape demo currently committed at `solver/solver-core/benches/BENCH_RESULTS.md` with a production-cell-shape refresh (`--budget 60s --seeds 20`) so the canonical bench table reflects the post-item-31 / post-item-41 / post-item-45 / post-item-46 solver state.

**Architecture:** Single long-running supervisor invocation rewrites the markdown file in place. No code changes anywhere else; only the regenerated markdown plus an `OPEN_THINGS.md` edit ship in this PR. Pre-run housekeeping rebuilds the maturin-backed `klassenzeit_solver` so the CP-SAT subprocess is in step with the latest solver-py state.

**Tech Stack:** Rust solver-bench supervisor (`solver/solver-bench/src/main.rs`), `mise` task runner, `cargo`, maturin-built `klassenzeit_solver` Python module, on host `iuno` (AMD Ryzen 7 3700X, Linux 6.8.0-90-generic).

---

### Task 1: Pre-run housekeeping

**Files:** none modified.

- [ ] **Step 1: Confirm the working tree is clean.**

Run: `git status`

Expected: `nothing to commit, working tree clean` (apart from the freshly-committed spec, which is in the previous commit).

- [ ] **Step 2: Rebuild the maturin-backed `klassenzeit_solver`.**

Run: `mise run solver:rebuild`

Expected: maturin builds `klassenzeit_solver` (the editable wheel that backs the CP-SAT subprocess invocation `python3 -m klassenzeit_solver.cpsat`). Stale binding is the most common false-positive source on long bake-off runs (per `solver/CLAUDE.md`'s bench-binding note); rebuilding here defangs it.

If `solver:rebuild` is unavailable in the local mise tasks, fall back to `uv sync --reinstall-package klassenzeit_solver`.

- [ ] **Step 3: Verify the supervisor defaults match production.**

Run: `grep -nA 6 "fn default_supervisor_args" solver/solver-bench/src/main.rs`

Expected output should include `budget: Duration::from_secs(60)` and `seeds: 20`. If those values differ, stop and reconcile with the spec — passing CLI overrides hides the drift but does not fix the source of truth.

### Task 2: Run the production bake-off

**Files:**
- Will rewrite: `solver/solver-core/benches/BENCH_RESULTS.md`

- [ ] **Step 1: Start the bake-off in the background.**

Run:

```bash
mkdir -p /tmp/kz-bakeoff
nohup mise run bench:bakeoff > /tmp/kz-bakeoff/bakeoff.log 2>&1 &
echo "PID=$!"
```

Expected: PID is printed; `tail -f /tmp/kz-bakeoff/bakeoff.log` shows compile output followed by `cell start: grundschule / lahc`. The supervisor uses defaults (`--budget 60s --seeds 20`) automatically.

- [ ] **Step 2: Watch the log for 16 cell completions.**

Each cell logs `cell start: <fixture> / <backend>` followed eventually by a single line summarising the cell. Cell order is the cross product of the four fixtures (`grundschule`, `zweizuegig`, `dreizuegig`, `lock_in`) and the four backends (`lahc`, `lahc_rr`, `lahc_rr_kempe`, `cpsat`), iterated in the source order, so the final cell logged should be `cell start: lock_in / cpsat`.

Run periodic check:

```bash
grep -c "^cell start:" /tmp/kz-bakeoff/bakeoff.log
```

Expected: monotonically increases from 1 to 16. Each LAHC cell takes roughly 60 s × 20 seeds = 1200 s plus FFD overhead. CP-SAT cells take less wall-clock for grundschule and lock_in (CP-SAT exits early on optimal), more wall-clock for zweizuegig and dreizuegig (CP-SAT runs to its 60 s budget per seed).

- [ ] **Step 3: Soft 6-hour watchdog.**

If the supervisor is still running at 6 h wall-clock, capture `tail -200 /tmp/kz-bakeoff/bakeoff.log`, kill the supervisor with `kill <PID>`, and surface the observation to the user. Do not silently restart; the run is reproducible but expensive.

- [ ] **Step 4: Confirm the supervisor exited zero.**

Run:

```bash
wait <PID>
echo "exit=$?"
```

(or `ps -p <PID>` until the process disappears, then check `tail -5 /tmp/kz-bakeoff/bakeoff.log` for the supervisor's closing line.)

Expected: exit status 0. A non-zero exit means the supervisor itself crashed (host-level issue or a panic outside item 46's catch); diagnose before continuing.

### Task 3: Validate the refreshed file

**Files:**
- Read-only: `solver/solver-core/benches/BENCH_RESULTS.md`

- [ ] **Step 1: Confirm the file was rewritten.**

Run:

```bash
git status solver/solver-core/benches/BENCH_RESULTS.md
```

Expected: `modified: solver/solver-core/benches/BENCH_RESULTS.md`. If the file is not modified, the supervisor wrote to a different path — re-check Task 1 Step 3's `out:` value.

- [ ] **Step 2: Confirm row count and `Seeds = 20`.**

Run:

```bash
awk -F'|' 'NF>=18 && $4 ~ /^[[:space:]]*[0-9]+[[:space:]]*$/ {print $4}' solver/solver-core/benches/BENCH_RESULTS.md | sort | uniq -c
```

Expected: a single line `16  20` (16 data rows, all with `Seeds = 20`). If any other count appears, the supervisor did not finish or the file shape changed unexpectedly.

- [ ] **Step 3: Confirm the panic count is acceptable.**

Run:

```bash
grep -c "panic" solver/solver-core/benches/BENCH_RESULTS.md
```

Expected: 0, 1, or 2 (item 46 lets a single-cell panic produce a placeholder; the second 1 is the explanatory mention in the footer). If the count is 3 or higher, at least two cells panicked. Diagnose against `/tmp/kz-bakeoff/bakeoff.log` (the supervisor logs the panic reason to stderr) before continuing.

- [ ] **Step 4: Confirm the footer matches the writer template.**

Run:

```bash
tail -30 solver/solver-core/benches/BENCH_RESULTS.md
```

Expected:

- The line `Refreshed YYYY-MM-DD on AMD Ryzen 7 3700X 8-Core Processor, Linux 6.8.0-90-generic, rustc <version>.` is present and dated today.
- The trailing line points at `docs/adr/0029-solver-feasibility-bake-off.md` and `docs/adr/0034-bench-cell-subprocess-and-observability.md`.
- The previous file's `_Shape demo at low budget/seeds (--budget 5s --seeds 4); production refresh queued as OPEN_THINGS item 42._` italic line is absent (the writer overwrites the whole file body, so this should hold automatically).

- [ ] **Step 5: Capture a feasibility comparison for the conditional follow-up.**

Run:

```bash
git show HEAD:solver/solver-core/benches/BENCH_RESULTS.md \
  | awk -F'|' 'NF>=18 && $4 ~ /^[[:space:]]*[0-9]+[[:space:]]*$/ {print $2,$3,$5}' \
  > /tmp/kz-bakeoff/before-feasibility.txt

awk -F'|' 'NF>=18 && $4 ~ /^[[:space:]]*[0-9]+[[:space:]]*$/ {print $2,$3,$5}' \
  solver/solver-core/benches/BENCH_RESULTS.md \
  > /tmp/kz-bakeoff/after-feasibility.txt

diff /tmp/kz-bakeoff/before-feasibility.txt /tmp/kz-bakeoff/after-feasibility.txt || true
```

Expected: a diff that compares (fixture, backend, feasibility-cell) tuples. Inspect manually: for each fixture, did `cpsat`'s feasibility count rise above any of the three LAHC backends' counts? If yes, mark the conditional follow-up as TRIGGERED for that fixture and capture the cell name; this drives Task 5 Step 2.

### Task 4: Commit the data refresh

**Files:**
- Modify: `solver/solver-core/benches/BENCH_RESULTS.md`

- [ ] **Step 1: Stage only the bench file.**

Run:

```bash
git add solver/solver-core/benches/BENCH_RESULTS.md
git status
```

Expected: only `solver/solver-core/benches/BENCH_RESULTS.md` is staged. If anything else is staged, unstage it; the OPEN_THINGS edit ships in Task 5's commit.

- [ ] **Step 2: Commit.**

Run:

```bash
git commit -m "chore(solver-bench): refresh BENCH_RESULTS.md at production cell shape (item 42)"
```

Expected: lefthook's pre-commit pipeline runs lint and passes (markdown-only diff, nothing in the lint set targets this file). The commit-msg hook accepts `chore(solver-bench): ...` per the project's commit-types YAML.

### Task 5: OPEN_THINGS update and conditional follow-up

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Delete item 42's bullet from `OPEN_THINGS.md`.**

Open `docs/superpowers/OPEN_THINGS.md`, find the line beginning `42. **Refresh BENCH_RESULTS.md post-item-41 and post-item-31.** ...`, and delete the entire bullet (one paragraph). Renumbering of subsequent bullets is not needed; OPEN_THINGS uses literal numbers, not ordered-list semantics, and other bullets reference `42` as a label not an index.

If a `next pickup` pointer in the active sprint header references item 42, advance it to whichever is the highest-priority unblocked item now (typically the next P0 in the active sprint queue, e.g. item 12 or item 13 if they are still open).

- [ ] **Step 2: Conditionally add an ADR 0032 follow-up bullet.**

If Task 3 Step 5's diff showed any fixture where `cpsat` reached strictly higher feasibility than each of the LAHC backends:

Add a new section header `## Open solver-default follow-ups` (if it does not already exist) just before the `## Backlog` section, and add a bullet of the shape:

```markdown
- **ADR 0032 production-default revisit (item 42 follow-up).** `[P1]` The
  `BENCH_RESULTS.md` refresh in PR #<NNN> showed `cpsat` reaching strictly higher
  feasibility on `<fixture>` than every LAHC backend. Re-apply the ADR 0029
  decision rule against the new data and write ADR 0035 (or next-available number;
  always `ls docs/adr/*.md | sort | tail -1` first) with the verdict.
```

Otherwise (the standard case): no follow-up bullet, no section creation.

- [ ] **Step 3: Commit.**

Run:

```bash
git add docs/superpowers/OPEN_THINGS.md
git commit -m "docs: drop item 42 from OPEN_THINGS, surface ADR 0032 follow-up if needed"
```

Expected: commit lands cleanly. If no follow-up bullet was added, the commit body should be a one-line message; that is fine.

### Task 6: Skill audit, push, PR, CI loop, automerge

(This task lives entirely inside `.claude/commands/autopilot.md` steps 7-9 and is reproduced here only for plan-completeness; the autopilot harness drives it.)

- [ ] **Step 1: Run skill audit per autopilot step 7** — confirm every required-skill row was actually invoked in this session.

- [ ] **Step 2: `mise exec -- git push -u origin chore/bench-results-production-refresh`.**

Pre-push lefthook runs `cargo nextest run --workspace`, `uv run pytest`, and frontend Vitest. None of those should be affected by a markdown-only branch.

- [ ] **Step 3: `gh pr create --base master --head chore/bench-results-production-refresh ...`** with title `chore(solver-bench): refresh BENCH_RESULTS.md at production cell shape (item 42)`.

- [ ] **Step 4: `python3 .claude/commands/post_brainstorm_comments.py <pr>`** to post the brainstorm Q&A.

- [ ] **Step 5: Monitor CI to green.**

- [ ] **Step 6: `gh pr merge <pr> --auto --squash`.**

- [ ] **Step 7: After `state == MERGED`, refresh local master and delete the feature branch.**
