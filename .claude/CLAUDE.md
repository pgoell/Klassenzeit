# Klassenzeit: Project Instructions

## Where rules live

Project instructions are split across several files so Claude only loads what is relevant to the current working context:

- **This file (`.claude/CLAUDE.md`)**: architecture, workflow, global coding rules, commit-message conventions. Loaded every session.
- **`backend/CLAUDE.md`**: Python / FastAPI / SQLAlchemy / pytest rules. Loaded when Claude reads files under `backend/`.
- **`frontend/CLAUDE.md`**: React / TanStack / shadcn / i18n / Vitest rules. Loaded when Claude reads files under `frontend/`.
- **`solver/CLAUDE.md`**: Rust solver workspace rules (error handling, determinism, PyO3 binding style, maturin dev loop, clippy allows policy, commit scopes). Loaded when Claude reads files under `solver/`.
- **`.claude/rules/*.md`**: rules scoped by file path rather than directory, via `paths:` frontmatter. Today: `pyproject.md` for workspace-wide Python dependency hygiene.

See [Anthropic's memory docs](https://code.claude.com/docs/en/memory) for the loading model.

## Architecture at a glance

- `backend/` — FastAPI + SQLAlchemy async, served under `klassenzeit_backend`. Runtime state (engine, session factory, settings, rate limiter) lives on `app.state`, set in `lifespan`.
- `frontend/` — Vite 7 + React 19 SPA with TanStack Router/Query, shadcn/ui, react-i18next. Proxies API to `:8000` in dev.
- `solver/` — Rust Cargo workspace with `solver-core` (pure), `solver-py` (PyO3 bindings built via maturin), and `solver-bench` (bake-off bench binary).
- `deploy/` — staging compose for the Hetzner VPS. Pulls `ghcr.io/pgoell/klassenzeit-{backend,frontend}` images published by `.github/workflows/deploy-images.yml`, joins the external `web` network run out of `~/Code/server-infra/`. The same workflow's `deploy-staging` job runs on the repo's self-hosted runner (`iuno-klassenzeit`) and auto-redeploys every master push via `docker compose pull && up -d` in `/home/pascal/kz-deploy/`. Runbook: `deploy/README.md`. Decisions: `docs/adr/0009-deployment-topology.md`.
- Dev loop runs via `mise` tasks; Postgres via `podman compose` from `compose.yaml` (root-level compose is local dev only, distinct from `deploy/compose.yaml`).

## Development Workflow

**Skills are not optional when a workflow names them.** Slash commands (notably `workbench:autopilot`) and the workbench skill set call out specific skills by name. "Invoke the skill" means call the `Skill` tool and let it return, then follow what it says. Synthesizing a skill's output freehand, even when it looks right, is skipping the skill and counts as a process violation. If a workflow step names a skill, calling `Skill` is the first action of that step, and the end-of-turn summary must note any listed skill that was unavailable and therefore skipped.

Always use TDD with red-green-refactor, driven by `workbench:test-driven-development`. Development always ends in PRs after documentation was extensively reviewed and updated.

Before opening a PR, run `agent-system-management:capturing-session-learnings` if the session produced learnings worth persisting, and if you ran that, run `agent-system-management:improving-instructions` right after. Both via the `Skill` tool.

Keep things that are out of scope for a step, or that you notice as tech debt or todos, in `docs/OPEN_THINGS.md`, ordered by importance. Don't add duplicates.

**`workbench:autopilot`** runs the full flow end-to-end (brainstorm, spec, plan, implementation, PR, green CI) without checking in at each step. The reusable loop lives in the workbench plugin; Klassenzeit-specific overrides (automerge mode, post-PR brainstorm comment hook, solver-rebuild discipline, OPEN_THINGS hygiene) live in `.workbench/autopilot.md`. Use it whenever the user describes a feature or chore they'd otherwise expect you to walk through step-by-step.

## Work selection: quality first, tidy first

When picking the next item off `docs/OPEN_THINGS.md` without a more specific directive from the user, prefer **tech debt and quality work over new user-facing features**. Follow Kent Beck's "Tidy First?" heuristic: small structural refactors that make subsequent feature work cheaper and safer come before the features themselves. Concretely:

1. Read OPEN_THINGS.md top to bottom. The current `## Active sprint program: ...`, `## Open <topic> follow-ups`, and the quality-tier subsections of `## Backlog` (`### CI / repo automation`, `### Testing`, `### Toolchain & build friction`, `### Auth maintenance`, `### Production readiness`) come first; skim `### Product capabilities` last.
2. Pick the highest-impact item from the active sprint, an open follow-ups section, or one of the quality-tier backlog subsections that is unblocked and fits a single PR.
3. Structural refactors that remove duplication, collapse drift between near-identical call sites, or replace alert/ad-hoc patterns with shared primitives count as tidy-first and are preferred over feature work.
4. A structural change and a behavioral change never ship in the same commit. If a tidy-first refactor uncovers a behavior bug, surface the bug and fix it in a separate commit with its own typed prefix (`fix(...)`), not folded into the refactor.
5. Behavior must be preserved across a tidy commit: tests that passed before must pass after without modification, except where the only change is a test's import path or a mock signature rendered obsolete by the refactor.

If every quality item in OPEN_THINGS.md is blocked or out of scope for one PR, fall back to the next feature item and note why in the PR body.

## Tooling

### Commands

- `mise run dev` — start backend with auto-reload
- `mise run fe:dev` — start frontend dev server on `:5173` (proxies API to `:8000`)
- `mise run test` — run all tests (Rust + Python + frontend)
- `mise run test:py` — Python tests only (`uv run pytest`); path filters must be relative to the repo root, e.g. `mise run test:py -- backend/tests/seed/test_x.py::test_name -v` (not `tests/seed/...`).
- `mise run test:rust` — Rust tests only (`cargo nextest run`)
- `mise run fe:test` — frontend Vitest only
- `mise run e2e` / `mise run e2e:ui`: Playwright suite (starts DB, migrates, seeds the admin user, runs the suite); `mise run e2e:install` is the one-time Chromium install on a fresh clone
- `mise run lint` — all linters (ruff, ty, vulture, clippy, machete, cargo fmt, biome, actionlint)
- `mise run check:actions` — actionlint over `.github/workflows/*.yml` (also runs under `mise run lint`)
- `mise run fmt` — auto-format everything
- `mise run fe:types` — regenerate frontend OpenAPI types from the backend
- `mise run db:up` / `db:stop` / `db:reset` / `db:migrate` — Postgres lifecycle
- `mise run bench:tests` — time the backend pytest suite, compare to `.test-duration-budget` (PR-2 ratchet, mirrors `.coverage-baseline`). CI runs `pytest --durations=30` plus a budget gate; budget tightens once two or three master runs land at the new floor.

### OPEN_THINGS.md / .html sync

- **`docs/OPEN_THINGS.html` is auto-rendered from `docs/OPEN_THINGS.md`** (`scripts/render_open_things_html.py`, surfaced via `mise run gen:openthings-html`). Pre-commit runs both `mise run lint` (which calls `check:openthings-html --check` and FAILS hard if the HTML is out of sync) and a separate `openthings-html` lefthook task (which regens with `stage_fixed: true`). The two run in parallel; the `--check` failure aborts the commit before the regen-and-stage task can rescue it. Practical workflow: after editing `docs/OPEN_THINGS.md`, run `mise run gen:openthings-html` BEFORE staging, then `git add docs/OPEN_THINGS.md docs/OPEN_THINGS.html` together. Skipping this just means the commit fails once, the dedicated lefthook task regens, and the retry succeeds; the proactive regen is cleaner. Surfaced 2026-05-08 closing OPEN_THINGS item 61 (the ADR 0036 PR).

### Flake-loop discipline

When measuring whether a single test goes RED across N invocations (validating an `xfail(strict=False)` removal, proving a new test deterministic, gating a quality-bar relaxation), these patterns recur:

- **Use `|| true`, not `|| break`.** A fixed-N loop with `|| true` lets a single failure not truncate the loop, so the tally over all N runs is honest. Count `passed`/`xpassed`/`failed`/`xfailed` lines across the captured output. Pytest reports an `xfail(strict=False)` test as `xpassed` when assertions hold and `xfailed` when they don't; **treat XPASS as PASS** for the gate. The question is "does the test ever go RED (`failed`)," not "does pytest call it `passed`." Pattern emerged while measuring auto-assigned solvability test stability under item 32; reuse for any future xfail-removal or test-stability gate.
- **Count summary lines, not failure-output lines.** Naive `grep -c " failed"` over a tee'd loop over-counts because pytest's failure output spans multiple lines per run (traceback plus summary), so each failed run contributes more than one matching substring. Count summary lines specifically with `grep -cE "^=+ [0-9]+ (passed|failed|xfailed|xpassed)"`, or pin to the `1 failed in` / `1 passed in` shape; the goal is one count per run, not one count per matching substring across all runs. Surfaced 2026-05-08 in the item 14 flake-loop tally.
- **Leave coverage enabled in the loop.** `-p no:cov` is unusable because `pyproject.toml`'s `[tool.pytest.ini_options]` `addopts` includes `--cov-fail-under=80`; disabling `pytest-cov` makes the flag unrecognised, every iteration errors out with `pytest: error: unrecognized arguments: --cov-fail-under=80`, and the loop produces zero `passed` / `failed` summary lines. Two fixes: (a) `-o addopts=--import-mode=importlib` overrides the whole addopts list and skips coverage, or (b) leave coverage enabled (the ~200 ms startup overhead × N iters is trivial vs a multi-minute test wall-clock). Pick (b) unless the per-iteration wall-clock is itself sub-second, since (a) also drops any other addopts the project later adds. Surfaced 2026-05-12 in item 11's flake-loop measurement.
- **Run once with `--runxfail` before closing.** A flake-loop that reads "20 xfailed / 0 failed" is gate-PASSING per the rule above, but the underlying assertion that fired may have shifted out from under the decorator's `reason=` string without the maintainer noticing. Before treating a flake-loop result as conclusive for closing an OPEN_THINGS item that names a specific bug shape, run the test once with `--runxfail` (`mise run test:py -- <path>::<test> -v --no-header --runxfail`) to confirm the panic shape matches the item's narrative; if the panic is a different bug entirely, the item is closing the wrong target and the next bug down the chain needs filing as a separate OPEN_THINGS item before the xfail decorators can come off. Surfaced 2026-05-11 closing item 76: the einzügig and dreizuegig solvability tests had xfail reasons mentioning FFD double-booking (item 4 territory), but the actual panics were LAHC `canonical_score` drift (item 76); closing item 76 unmasked the FFD double-booking which then surfaced through `--runxfail`. Same shape applies to any future bug-chain where one fix unmasks the next.

### Prerequisites and quirks

- **Rust toolchain** is a hard prerequisite (required for the PyO3 bindings and for the dev tools below).
- **Git hook runner:** [Lefthook](https://github.com/evilmartians/lefthook). Config lives at `.config/lefthook.yaml` (lefthook auto-discovers this path). Verify a config edit with `lefthook dump` before invoking the hook; the dump prints the parsed tree and is fast.
- **Commit message enforcement:** [Cocogitto](https://docs.cocogitto.io) (`cog`), installed via `cargo install cocogitto`. A `commit-msg` hook runs `cog verify` and rejects non-conventional messages.
- **Pre-push runs the full test suite.** `.config/lefthook.yaml`'s `pre-push` runs `cargo nextest run --workspace`, `uv run pytest` (with coverage), and the frontend Vitest suite before the push goes to origin. Even a docs-only push pays the ~30s; this is by design so broken builds never reach the remote. Use `mise exec -- git push` so the pinned lefthook runs.
- **`gh` + `jq` are runtime prerequisites** for repo-automation tasks like `mise run repo:apply-settings`. Neither is pinned via mise; install from the system package manager (or `brew install gh jq`) on fresh clones.
- **`gh pr edit` silently no-ops in this repo** because the GitHub classic-projects deprecation prints a GraphQL warning that gh treats as terminal: the command exits 0 without applying the edit. Workaround: `gh api -X PATCH repos/<owner>/<repo>/pulls/<N> -f title='...'` (and `-f body='...'`). Same shape applies to other gh commands that mutate PRs through GraphQL. Revisit when classic projects are sunset on this repo.
- **`gh run view --log` returns empty output here** (both the run-level form and the `--job=<id>` form), even on green runs whose job timestamps are visible via `--json jobs`. The same warning interaction that breaks `gh pr edit` appears to short-circuit the streaming-log code path. Workaround: fetch each Test job's full log with `gh api repos/<owner>/<repo>/actions/jobs/<job-id>/logs` (where `<job-id>` comes from `gh run view <run-id> --json jobs -q '.jobs[] | select(.name=="Test") | .databaseId'`), then grep for the line of interest (e.g., `Pytest wall-clock:` for the duration-budget gate). Same pattern works for any per-step log line; the API-level endpoint never silently empties.
- **Ad-hoc Python snippets with third-party deps.** The system `python3` has no `pyyaml`, `coloraide`, or similar. For one-off verification or conversion scripts (YAML diffs of workflow permissions, OKLCH to sRGB hex conversion for `frontend/DESIGN.md` updates, etc.), invoke via `uv run --with <pkg1> --with <pkg2> python3 - <<'EOF' ... EOF` so the pinned `uv` provides the dep. Recurring pairs: `--with pyyaml` (YAML parsing), `--with coloraide` (OKLCH ↔ sRGB hex for DESIGN.md / app.css work).
- **Required status checks must always report.** GitHub branch-protection's `required_status_checks.contexts` only blocks a merge if the named check actually runs and reports on the PR's head SHA. A workflow gated by `on.<event>.paths:` does NOT report on PRs that don't touch the listed paths, so a "required" context never resolves and either the PR sticks forever or (worse, on this repo's prior shape) automerge proceeds because GitHub treats the absent check as "expected, but not applicable." Fix shape (now used in `.github/workflows/frontend-ci.yml`): drop the `paths:` filter so the workflow always runs, accept the small CI-minute cost on docs-only PRs. The context strings in `.github/branch-protection.json` are the bare job `name:` field (e.g. `"Lint + test + build"`, NOT `"Frontend CI / Lint + test + build"`); copy them verbatim from the workflow YAML or the gate silently noops. Apply with `mise run repo:apply-settings`; the underlying `scripts/apply-github-settings.sh` supports `--dry-run` and `--check` for safe preview/diff. Surfaced 2026-05-10 closing the typecheck-not-required trap (`fix(ci): close typecheck-not-required trap`).

## Coding standards

- **No bare catchalls.** No untyped `catch` in TypeScript, no `Result<_, _>` swallowed with `_` in Rust. Catch the specific error you can handle; let the rest propagate. (Python framing lives in `backend/CLAUDE.md`.)
- **No dynamic imports.** All imports must be static/top-of-file so the dependency graph is statically analyzable. No `import()` expressions, no `importlib.import_module` in hot paths.
- **Unique function names globally.** Function names must be unique across the entire codebase, even across classes and files. `scripts/check_unique_fns.py` runs in pre-commit and walks TS/TSX, **every `.rs` file** (not just `tests/` integration tests; library source under `src/` is included, including helpers inside `#[cfg(test)] mod tests` blocks), **and all Python files (including methods inside classes)**, so when duplicating a page skeleton across entities or a property-test generator across solver-core test files, rename helpers per feature: `RoomsPageHead` not `PageHead`, `handleRoomSubmit` not `onSubmit`, `confirmRoomDelete` not `confirm`, `lahc_small_problem` not `small_problem`, `lahc_weights` not `weights`. Same applies to test helpers: `wrapScheduleHook` not `wrap`, `ScheduleSkeletonGrid` not `SkeletonGrid`. Same applies to Pydantic `@model_validator` / `@field_validator` methods: prefix the validator name with the schema type so two `*Create` schemas can host the same-shape invariant without collision (`_lesson_hours_divisible_by_block_size` on `LessonCreate`, `_entry_hours_divisible_by_block_size` on `EntryCreate`, not the bare `_hours_divisible_by_block_size` on both). Same applies to inherent methods on Rust types: `QualityComponent::component_label` rather than `label` because `BenchBackend::label` already exists in `solver-bench/src/main.rs`. Before adding a method on a new type, `rg -n 'fn <name>\b' solver/ frontend/ backend/` to confirm no collision; the resolution is to rename the new method, not the existing one (touching call sites is more churn).
- **Dockerfile build context is the repo root.** `backend/Dockerfile` and `frontend/Dockerfile` are built from the repo root with `context: .` and `file: <subdir>/Dockerfile` (see `.github/workflows/deploy-images.yml`). Every `COPY` inside them is therefore written as `COPY backend/ backend/`, `COPY frontend/ ./`, etc. The matching `.dockerignore` lives next to each Dockerfile but its patterns are evaluated against the repo root.
- **Pin the package-manager version in any Dockerfile that calls `corepack enable`.** Corepack bumps its bundled default with Node releases, so a bare `RUN corepack enable` silently downloads whichever pnpm corepack defaults to today. That auto-bump broke `Publish frontend image` on every master push 2026-05-10 → 2026-05-12 with `[ERR_PNPM_IGNORED_BUILDS] esbuild@0.27.7, msw@2.13.4` once corepack started serving pnpm 11.1.0, which stopped honoring the existing `pnpm.onlyBuiltDependencies` allowlist the way pnpm 10 did; `Frontend CI / Lint + test + build` stayed green because it runs through mise's pinned pnpm (failing run for breadcrumbs: `gh run view 25716453818`). Pin in two places: `"packageManager": "pnpm@X.Y.Z"` in `frontend/package.json` (the corepack-native field) AND `RUN corepack prepare pnpm@X.Y.Z --activate` in `frontend/Dockerfile` directly after `RUN corepack enable` (turns the version download into a named, cached layer instead of a lazy side effect of `pnpm install`). Keep the pinned version in sync with `mise.toml`'s `aqua:pnpm/pnpm` entry. Same shape applies to any future Docker image that uses corepack for a different package manager. Closed in `fix(ci): pin pnpm to 10.33.2 in frontend image build`.
- **ADR titles skip the em-dash.** `docs/adr/template.md` renders `# NNNN — Title`, but the user's global preference forbids em- and en-dashes in new prose. Use a colon (`# NNNN: Title`) in new ADRs. The early ADRs that already use em-dashes (0001-0005, 0007, 0008) stay as they are; ADRs are immutable per `docs/adr/README.md`. **Always `ls docs/adr/*.md | sort | tail -1` before assigning the next number;** roadmap memory and stale specs/plans may reference a number that is already taken. **`docs/adr/README.md` indexes ADRs as a Markdown table (`| Number | Title | Status |`), not a bullet list;** a plan that says "add an index entry" instructs the implementer to append a table row that matches the cadence of sibling rows (no trailing period inside cells, status `Accepted` for newly-merged), not write `- [NNNN](...): ...`. Surfaced 2026-05-13 when ADR 0038 (per-backend solver deadline) landed and the plan's bullet-style example had to be adapted to the actual table format.
- **Commit types live in `.github/commit-types.yml`.** `.github/workflows/pr-title.yml` and `CONTRIBUTING.md` carry `BEGIN/END GENERATED: commit-types` regions rendered from the YAML. Edit the YAML, then `mise run gen:commit-types`. `mise run check:commit-types` runs inside `mise run lint` and catches drift.
- **SHA-pin third-party GitHub Actions.** `actions/*` and `github/*` can use `@vN`; everything else (community or single-maintainer actions like `JasonEtco/create-an-issue`, `amannn/action-semantic-pull-request`) pins to a full commit SHA with a trailing `# vX.Y.Z` comment for audit readability. Moving tags on third-party code are a supply-chain risk.
- **Decouple-then-shrink refactors must inline at the receiver-decoupling commit, not spread.** When a structural commit decouples a receiver (e.g. `_PERIODS_DREIZUEGIG`) from a donor whose shape is about to shrink in a follow-up commit (e.g. `_PERIODS` losing position 7), the receiver MUST inline the donor's current entries explicitly, not write `(*donor, appended_soon_to_be_removed_entries)`. The donor still holds those entries at the structural commit's HEAD; the spread + explicit re-add duplicates them and trips constraint validators (UNIQUE on `(week_scheme_id, day_of_week, position)` in the seed case). Plan each commit's edit in equilibrium against THAT commit's HEAD, not against the post-shrink end state. Pattern surfaced when item 13 split the einzügig + dreizügig `_PERIODS` dependency into a structural commit followed by a behavioural shrink.

## Commit messages

This repo enforces [Conventional Commits](https://www.conventionalcommits.org/).

**Format:** `<type>(<optional scope>): <description>`

**PR titles** must also satisfy `subjectPattern: ^[a-z].+$` (checked by `amannn/action-semantic-pull-request`). Start the subject with a lowercase letter even when the first word is an acronym: `feat(frontend): crud pages ...`, not `feat(frontend): CRUD pages ...`.

**Common types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`. Append `!` for breaking changes, or add a `BREAKING CHANGE:` footer.

When creating commits, always produce a Conventional Commits-compliant message. See `CONTRIBUTING.md` for the full type table and examples.

Beyond enforcement, `cog` also handles changelog generation (`cog changelog`) and semver bumps (`cog bump`). Prefer these over hand-rolled equivalents.
