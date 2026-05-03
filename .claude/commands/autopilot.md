---
description: Run the full brainstorm, spec, plan, implement, PR, green-CI flow autonomously for a topic.
argument-hint: <topic description>
---

# /autopilot: autonomous feature flow

You are executing the Klassenzeit autopilot workflow for: **$ARGUMENTS**

This command runs end-to-end without checking in at every step. The user has opted into autonomous mode: make your own recommendations, don't pause for confirmation on routine choices, only stop if the topic is too large for a single spec (then decompose and ask which sub-topic to tackle first).

## Non-negotiables

- **Set automerge once CI is green.** Run `gh pr merge <pr> --auto --squash` so the platform merges the PR when required checks pass. Do not call `gh pr merge` synchronously. After the merge resolves, refresh local master and delete the feature branch (see step 9). Every fix produced during the run lands on the feature branch as additional commits, never as a separate chore PR.
- **Never skip hooks** (`--no-verify`, `--no-gpg-sign`, `LEFTHOOK=0`). If a hook fails, investigate and fix the underlying issue.
- **Never add AI attribution** to commits, PRs, or code. No "Generated with", no "Co-Authored-By: Claude".
- **Every commit must be Conventional Commits compliant** (`feat`, `fix`, `docs`, `build`, `ci`, `chore`, `test`, `refactor`, `perf`, `style`, `revert`). `cog` enforces this.
- **No em-dashes or en-dashes** in prose (per user global preference). Rewrite with commas, periods, colons, semicolons, parentheses.
- **Never synthesize a skill's output freehand.** If this command names a skill, calling the `Skill` tool (and letting it return) is mandatory before producing that step's artifact. Freehand output that looks like a skill ran is a process violation and the work must be redone after invoking the skill. At the end of each turn, double-check that the skills required by the steps you just executed actually appear in your tool-call history.

## Required skill invocations

Every `/autopilot` run must call the `Skill` tool, not read a skill file, not reimplement, for each entry at the step that names it. Before you push in step 7, stop at the **Skill audit** and verify each row: if any skill is missing from the session's `Skill` tool calls, invoke it now, let it reshape the artifact it governs, and commit the correction before continuing.

| Step | Skill | Purpose |
|---|---|---|
| 0 | `superpowers:using-superpowers` | Establish skill discipline for the session |
| 2 | `superpowers:brainstorming` | Structure the self-answered Q&A and the spec template |
| 4 | `superpowers:writing-plans` | Structure the implementation plan |
| 5 | `superpowers:test-driven-development` | Enforce red-green-refactor per chunk |
| 5 | `superpowers:subagent-driven-development` | Always. Dispatch every plan task to a fresh subagent (sequentially if they share state, in parallel when they don't), so the main session keeps context lean. |
| 6 | `claude-md-management:revise-claude-md` | Capture session learnings into CLAUDE.md files |
| 6 | `claude-md-management:claude-md-improver` | Audit the CLAUDE.md files after revision |
| 6 | `fewer-permission-prompts` | Scan the transcript, tighten `.claude/settings.json` |

If a listed skill is unavailable in the current environment, say so explicitly in the end-of-turn summary and skip only that entry. Never silently drop a row.

## Steps

### 0. Establish skill discipline

**First action:** invoke `superpowers:using-superpowers` via the `Skill` tool. Nothing else in this command happens until the skill has returned.

### 1. Prepare the workspace

- `git checkout master && git pull origin master` to get latest.
- If the local branch diverges from origin (e.g. after a squash merge), `git reset --hard origin/master`. Check with the user first if there are unpushed local commits.
- Create a new branch: `git checkout -b <type>/<short-topic-slug>` (e.g. `feat/frontend-scaffolding`, `fix/cookie-refresh-bug`).

### 2. Brainstorm (sequential, self-answered)

**First action:** wipe the scratch dir from any prior run, then invoke `superpowers:brainstorming` via the `Skill` tool. Keep the skill's "one question at a time" rhythm, but self-answer each question instead of waiting for the user: autonomous mode removes the pause, not the sequencing.

```bash
rm -rf /tmp/kz-brainstorm && mkdir -p /tmp/kz-brainstorm
```

The wipe prevents last run's `brainstorm.md` or `post_comments.py` from leaking into this one. Do this before writing the preamble.

Work the Q&A incrementally:

- Start `/tmp/kz-brainstorm/brainstorm.md` with a short preamble (topic, autonomous-mode note) before the first question.
- For each question, in order:
  1. Formulate the question you would have asked the user. Make it multiple-choice when possible, open-ended only when needed.
  2. Answer it yourself: list the options you considered, the decision, and the reasoning (what makes this the right call here, what you'd pick differently in a nearby context).
  3. Append that one Q&A block to `/tmp/kz-brainstorm/brainstorm.md` as `## Q<n>. <question>` with the answer below.
  4. Let the answer shape the next question. Later questions should build on earlier decisions; do not pre-commit to a batch of questions up front.
- Keep going until the open space feels closed: scope, approach, file layout, commit split, risks, success criteria. When you are not uncovering anything new, stop.
- End the file with a short `## Decision` block summarising the shape of the PR you're about to write.
- If the topic is too big for one spec, stop and surface the decomposition for the user to pick a sub-topic.

The sequential rhythm matters: it keeps each answer honest (you do not know the later question until the earlier one is decided) and it produces a readable per-question PR comment thread later.

**Format verify gate.** Before leaving step 2, confirm the file parses as the PR-comment script expects it to. Run:

```bash
grep -c '^## Q' /tmp/kz-brainstorm/brainstorm.md
grep -c '^## Decision' /tmp/kz-brainstorm/brainstorm.md
```

The first count must be greater than zero and match the number of questions you answered; the second must be exactly 1. If either check fails, you wrote the Q&A with inline `Q1:` / `**Q1.**` style instead of top-level `## Q<n>. <question>` / `## Decision` headings; fix the file now. `.claude/commands/post_brainstorm_comments.py` splits on those headings, so mis-formatting surfaces only at step 7 (after the PR is already open) and forces a mid-PR reformat.

### 3. Write the spec

- Use the spec template that `superpowers:brainstorming` surfaced in step 2. Do not hand-roll a spec layout from memory.
- Path: `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md` (today's date, short topic slug).
- Run the spec self-review: placeholders, internal consistency, scope, ambiguity. Fix inline.
- Commit: `docs: add <topic> design spec`.

### 4. Write the implementation plan

**First action:** invoke `superpowers:writing-plans` via the `Skill` tool.

Then:

- Path: `docs/superpowers/plans/YYYY-MM-DD-<topic>.md`.
- Use checkbox syntax (`- [ ]`) per task step so progress is trackable.
- Commit: `docs: add <topic> implementation plan`.

### 5. Execute the plan

**First actions, in order:** invoke `superpowers:test-driven-development`, then `superpowers:subagent-driven-development`. Both via the `Skill` tool. TDD governs every implementation chunk; subagent-driven-development governs how you run those chunks.

**Subagents are mandatory, not optional.** Every plan task runs in its own fresh `general-purpose` subagent via the `Agent` tool. The user prefers this whether or not tasks are independent: fresh agents save cost (no accumulated file contents in the prompt) and keep the main session's context lean for the later review / PR / docs steps.

How to dispatch:

- **Truly independent tasks (no shared files, no ordering dependency)**: send multiple `Agent` calls in a single message so they run in parallel. Typical cases: four entity-page redesigns that don't edit the same i18n catalog, per-package documentation updates.
- **Tasks that share state (i18n JSON, the same `app.css`, the shared route tree, shared component files)**: dispatch one agent at a time, waiting for each to return before dispatching the next. Still one agent per task; they just queue instead of fan out. Batch edits to the shared file into a single prep task if that removes the sharing.
- **Trivial polish (renaming one symbol, a one-line lint fix, a typo)**: still use a subagent when the work touches files the main session hasn't already loaded. Only skip the agent for edits the main session *just* made and still has in context, where spinning up an agent would be pure overhead.

Each subagent prompt must include: the plan task it owns (paste the checkbox block), which files to touch, the relevant commits that preceded it, and the acceptance criteria (tests to run, lint to pass). The main session reviews the agent's diff and commits; the agent should not commit on its own.

**If a subagent errors mid-run (e.g. "Overloaded"):** it may have written files without committing. Before dispatching a continuation, run `git status` + `git diff` from the main session to see what partial work survived the failure. Continuing in the main session is usually cheaper than redispatching when the agent already did the heavy edit; include a brief "Continuing from subagent state" note in the next commit's body so the review trail is legible.

**If a subagent reports `DONE_WITH_CONCERNS` and the concern is in-scope** (perf-budget regression on a solver task, an unfixed lint, a missing test the spec required), fix it before committing, either in the main session or by re-dispatching with the gap added to the acceptance criteria. Do not carry the concern into the PR. For algorithm-phase work specifically, include the BASELINE.md 20% regression budget as an explicit acceptance criterion in the subagent prompt so the agent knows to optimise within its own task rather than surface the breach.

**Solver-binding rebuild discipline.** When a subagent runs Python tests or backend integration tests that consume `klassenzeit_solver` (the maturin-built PyO3 binding) AFTER an earlier task in the same /autopilot run touched `solver/solver-core/` or `solver/solver-py/`, the agent's prompt MUST include `mise run solver:rebuild` as an explicit step before pytest. Otherwise the binding is stale and the agent observes phantom wire-format bugs (a Sprint A subagent misdiagnosed a non-existent `kind`-as-tagged-dict bug from a stale binding). Same shape applies to `mise run fe:types` after backend Pydantic schema changes if the next task touches frontend types.

Then:

- Commit in logical chunks with Conventional Commits scopes matching the module (e.g. `feat(frontend): ...`, `build(mise): ...`, `test(scripts): ...`).
- Run `mise run lint` and relevant `mise run test:*` before each commit. The pre-commit hook also enforces lint.
- If you discover repo-level issues that block progress (broken hooks, wrong default-branch assumptions, flaky scripts), fix them in the same branch with their own typed commit (`build`, `ci`, `fix(scripts)`, etc.). Don't paper over with skips.

### 6. Finalize docs + improvement pass

**First actions, in order:** invoke `claude-md-management:revise-claude-md`, then `claude-md-management:claude-md-improver`, then `fewer-permission-prompts`. All three via the `Skill` tool. The revisions those passes produce are the canonical CLAUDE.md and settings changes for this run; do not hand-edit those files instead of running the skills.

**Autonomous mode: apply every skill's proposed edits directly. Do not pause to ask the user for approval; running `/autopilot` is the approval.** If a skill's default behavior is to ask ("Apply these changes? y/n"), answer for the user and proceed. Briefly report the edits in the end-of-turn summary so the user can see what landed without having blocked on it.

**All edits land on the feature branch in this run.** Settings tweaks from `fewer-permission-prompts`, autopilot.md improvements from this step, CLAUDE.md edits, ADRs, OPEN_THINGS updates, auto-memory updates: every one of those goes into a commit on the current feature branch with a Conventional Commits type that matches the change (`chore(settings):`, `docs(autopilot):`, `docs:`, etc.). No follow-up chore PRs.

Then:

- Update `docs/architecture/overview.md` if subsystems changed.
- Add an ADR at `docs/adr/NNNN-<short-title>.md` for load-bearing decisions (new dep, new subsystem, new toolchain). Index in `docs/adr/README.md`.
- Update `README.md` commands table if new `mise run` tasks landed.
- Update `docs/superpowers/OPEN_THINGS.md`: remove resolved items, add follow-ups ordered by importance.
- **Workflow improvements.** If the run surfaced anything that should change `.claude/commands/autopilot.md` or any other workflow doc, edit it now and commit on the feature branch. Reflect on: decisions that weren't captured anywhere, surprising failure modes, points where the skills didn't fire when they should have. Keep changes minimal and concrete; one sentence per learning.
- **Auto-memory updates.** Refresh `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/` entries (roadmap status, feedback, references) so the next session starts from the current truth.

### 7. Skill audit, then open the PR

**Skill audit (blocking).** Before the push, re-read the "Required skill invocations" table above. For each row whose step number is 0 through 6, confirm you actually called the `Skill` tool for that entry this session. Walk the list one by one; do not skim. If any row is missing, invoke it now, let it reshape the artifact it governs, commit the correction, and only then proceed.

Only after the audit passes:

- `mise exec -- git push -u origin <branch>` (use `mise exec --` so the pinned lefthook runs, not whatever's on `PATH`).
- `gh pr create --base master --head <branch> --title "<Conventional-Commits title>" --body "<body>"`.
- PR body structure: `## Summary`, then scope/non-goals, `## Test plan` checklist, and links to spec + plan + ADR if present.
- Post the brainstorm Q&A: `python3 .claude/commands/post_brainstorm_comments.py <pr>`. The script reads `/tmp/kz-brainstorm/brainstorm.md`, posts a preamble comment, then one `gh pr comment` per `## Q…` / `## Decision` section. It fails with exit 2 if the PR number is missing. Do not regenerate a copy in `/tmp`; the checked-in version is the source of truth.

### 8. CI loop

- Poll with `Monitor` (or `run_in_background` + polling) until every check resolves. `gh pr checks <pr>` is human-readable; for programmatic polling use `gh pr view <pr> --json statusCheckRollup -q '.statusCheckRollup[] | "\(.status):\(.conclusion // "")"'` (the `--json` flag does NOT exist on `gh pr checks`). Loop until no row's status differs from `COMPLETED` and no `conclusion` is `FAILURE` / `CANCELLED` / `TIMED_OUT`.
- If a check fails: open the failed job log with `gh run view <run-id> --log-failed | tail -200`, diagnose, commit the fix, push. Repeat until green.
- Common early failures to expect:
  - Generated files missing in CI (route trees, generated types). Build or codegen must run before the check that needs them.
  - Tool-version drift between local and CI. Verify the pinned versions in `mise.toml` resolve the same in `jdx/mise-action`.
  - Hook/script false positives on new file types. Tighten the script, don't relax the rule.

### 9. Set automerge, wait for merge, refresh master

When step 8 reports the PR fully green:

1. **Set automerge:**

    ```bash
    gh pr merge <pr> --auto --squash
    ```

    `--auto` queues the merge so the platform completes it as soon as required checks pass. The repo's master commit history shows squash merges (one commit per PR with the `(#NNN)` suffix), so `--squash` is the canonical style; only deviate if a specific PR needs commit-history preservation, and explain why in the end-of-turn summary.

2. **Wait for the merge to resolve.** Poll with `Monitor` (or `run_in_background` + polling) on the PR's `state` field:

    ```bash
    gh pr view <pr> --json state -q .state
    ```

    until it returns `MERGED`. If it returns `CLOSED` without merging, GitHub rejected the merge (e.g. branch protection blocked it); investigate and surface the reason to the user instead of retrying.

3. **Refresh local master:**

    ```bash
    git checkout master
    git pull origin master
    git branch -d <feature-branch>
    ```

    The `-d` (lowercase) refuses to delete an unmerged branch; it's the safety net. If `-d` fails, the merge probably did not include all local commits (e.g. a hook stripped them); inspect before forcing.

4. **Report the merged commit hash + PR URL** in the end-of-turn summary.

If the user explicitly says "don't merge" before or during the run, set automerge to off-ramp: `gh pr ready <pr>` plus a request-for-review note, and skip steps 2-4. That escape hatch is for one-off cases; the default is automerge.

## Tone and reporting

- Terse between tool calls. The user sees a diff on the PR; they don't need narration.
- End-of-turn summary: PR URL, one sentence on what changed, next step (usually "review when ready"). Also list any required skill that was unavailable and therefore skipped.
- If you hit an unexpected fork in the road that truly needs the user (not a routine choice), stop and ask. But bias strongly toward deciding yourself, that is the point of this command.
