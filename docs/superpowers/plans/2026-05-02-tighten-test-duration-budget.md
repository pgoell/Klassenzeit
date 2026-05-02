# Tighten `.test-duration-budget` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lower the CI Python test wall-clock budget from 600 s to 120 s, and shorten the OPEN_THINGS row to its forward-looking half. One commit, one PR.

**Architecture:** Pure config / docs change. The CI gate (`.github/workflows/ci.yml` step "Check Python test wall-clock budget") already reads the value from `.test-duration-budget`; no workflow edit is needed. The OPEN_THINGS update preserves the recurring-ratchet directive while removing the satisfied "wait for two or three master runs" precondition.

**Tech Stack:** Plain text config file, Markdown docs.

---

## Task 1: Lower the budget and update OPEN_THINGS

**Files:**

- Modify: `.test-duration-budget` (single line)
- Modify: `docs/superpowers/OPEN_THINGS.md:161` (one bullet under "CI / repo automation")

- [ ] **Step 1: Edit `.test-duration-budget`**

Replace the file content `600` with `120`:

```text
120
```

- [ ] **Step 2: Verify the new value parses**

Run:

```bash
test "$(cat .test-duration-budget)" = "120" && echo OK
```

Expected: `OK`.

- [ ] **Step 3: Edit `docs/superpowers/OPEN_THINGS.md` line 161**

Replace the existing bullet:

```markdown
- **Tighten `.test-duration-budget`.** PR-2 (2026-04-30) set the budget at 600 s as a generous floor against the pre-PR 1215 s baseline. The first post-merge CI run came in at 27 s for the Python test step. Lower the budget to ~120 s in a single-line PR once two or three more master runs confirm the new floor; repeat after later changes shift the floor again.
```

with:

```markdown
- **Re-tighten `.test-duration-budget`** after later changes shift the floor. Current ceiling is 120 s against an observed master-run floor of ~25 s (set 2026-05-02 in PR `chore/tighten-test-duration-budget`). When a future PR-2-class wall-clock drop lands, repeat the ratchet: wait for two or three master runs at the new floor, then lower the budget to roughly 4 to 5x the new max in a single-line PR.
```

- [ ] **Step 4: Diff inspection**

Run:

```bash
git diff .test-duration-budget docs/superpowers/OPEN_THINGS.md
```

Expected: exactly two hunks. The budget hunk replaces `600` with `120`; the OPEN_THINGS hunk replaces the single bullet.

- [ ] **Step 5: Lint locally**

Run:

```bash
mise run lint
```

Expected: pass (no Markdown lint, no script lint affected by these two edits).

- [ ] **Step 6: Commit**

```bash
git add .test-duration-budget docs/superpowers/OPEN_THINGS.md
mise exec -- git commit -m "chore(ci): tighten .test-duration-budget from 600s to 120s"
```

The pre-commit hook runs `mise run lint`; the commit-msg hook runs `cog verify` (the `chore(ci)` shape is conventional). Both must pass.

---

## Self-Review Checklist

- **Spec coverage.** Spec calls for two file edits and one commit. Task 1 covers both. The "no new test or workflow change" non-goal is honoured (no workflow edit, no script edit).
- **Placeholder scan.** No "TBD"; no vague "handle errors". The exact `120` value, the exact replacement text, and the exact commit message are present.
- **Type consistency.** Trivial (no types). The branch name, commit subject, and PR title triple all read `chore/tighten-test-duration-budget` and `chore(ci): tighten .test-duration-budget from 600s to 120s`.

## Verification After Merge

- The next master CI run reports `Pytest wall-clock: ~25s (budget: 120s)` from the "Check Python test wall-clock budget" step.
- A future PR that pushes the suite past 120 s now fails the gate; that is the intended regression alarm.
