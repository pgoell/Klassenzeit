# Klassenzeit: Project Instructions

## Where rules live

Project instructions are split across several files so Claude only loads what is relevant to the current working context:

- **This file (`.claude/CLAUDE.md`)** — architecture, workflow, global coding rules, commit-message conventions. Loaded every session.
- **`backend/CLAUDE.md`** — Python / FastAPI / SQLAlchemy / pytest rules. Loaded when Claude reads files under `backend/`.
- **`frontend/CLAUDE.md`** — React / TanStack / shadcn / i18n / Vitest rules. Loaded when Claude reads files under `frontend/`.
- **`solver/CLAUDE.md`**: Rust solver workspace rules (error handling, determinism, PyO3 binding style, maturin dev loop, clippy allows policy, commit scopes). Loaded when Claude reads files under `solver/`.
- **`.claude/rules/*.md`** — rules scoped by file path rather than directory, via `paths:` frontmatter. Today: `pyproject.md` for workspace-wide Python dependency hygiene.

See [Anthropic's memory docs](https://code.claude.com/docs/en/memory) for the loading model.

## Architecture at a glance

- `backend/` — FastAPI + SQLAlchemy async, served under `klassenzeit_backend`. Runtime state (engine, session factory, settings, rate limiter) lives on `app.state`, set in `lifespan`.
- `frontend/` — Vite 7 + React 19 SPA with TanStack Router/Query, shadcn/ui, react-i18next. Proxies API to `:8000` in dev.
- `solver/` — Rust Cargo workspace with `solver-core` (pure) and `solver-py` (PyO3 bindings built via maturin).
- `deploy/` — staging compose for the Hetzner VPS. Pulls `ghcr.io/pgoell/klassenzeit-{backend,frontend}` images published by `.github/workflows/deploy-images.yml`, joins the external `web` network run out of `~/Code/server-infra/`. The same workflow's `deploy-staging` job runs on the repo's self-hosted runner (`iuno-klassenzeit`) and auto-redeploys every master push via `docker compose pull && up -d` in `/home/pascal/kz-deploy/`. Runbook: `deploy/README.md`. Decisions: `docs/adr/0009-deployment-topology.md`.
- Dev loop runs via `mise` tasks; Postgres via `podman compose` from `compose.yaml` (root-level compose is local dev only, distinct from `deploy/compose.yaml`).

## Development Workflow

**Skills are not optional when a workflow names them.** Slash commands (notably `/autopilot`) and the superpowers skill set call out specific skills by name. "Invoke the skill" means call the `Skill` tool and let it return, then follow what it says. Synthesizing a skill's output freehand, even when it looks right, is skipping the skill and counts as a process violation. If a workflow step names a skill, calling `Skill` is the first action of that step, and the end-of-turn summary must note any listed skill that was unavailable and therefore skipped.

Always use TDD with red-green-refactor, driven by `superpowers:test-driven-development`. Development always ends in PRs after documentation was extensively reviewed and updated.

Before opening a PR, run `claude-md-management:revise-claude-md` if the session produced learnings worth persisting, and if you ran that, run `claude-md-management:claude-md-improver` right after. Both via the `Skill` tool.

Keep things that are out of scope for a step, or that you notice as tech debt or todos, in `docs/superpowers/OPEN_THINGS.md`, ordered by importance. Don't add duplicates.

**`/autopilot <topic>`** runs the full flow end-to-end (brainstorm, spec, plan, implementation, PR, green CI) without checking in at each step. See `.claude/commands/autopilot.md` for the exact sequence, its required-skill-invocations table, and the skill audit that runs before the PR opens. Use it whenever the user describes a feature or chore they'd otherwise expect you to walk through step-by-step.

## Work selection: quality first, tidy first

When picking the next item off `docs/superpowers/OPEN_THINGS.md` without a more specific directive from the user, prefer **tech debt and quality work over new user-facing features**. Follow Kent Beck's "Tidy First?" heuristic: small structural refactors that make subsequent feature work cheaper and safer come before the features themselves. Concretely:

1. Read OPEN_THINGS.md top to bottom. Skim the "Product capabilities" section last.
2. Pick the highest-impact item from the "CI / repo automation", "Testing", "Toolchain & build friction", "Auth maintenance", or "Production readiness" sections that is unblocked and fits a single PR.
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

- **Rust toolchain** is a hard prerequisite (required for the PyO3 bindings and for the dev tools below).
- **Git hook runner:** [Lefthook](https://github.com/evilmartians/lefthook). Config lives at `.config/lefthook.yaml` (lefthook auto-discovers this path).
- **Commit message enforcement:** [Cocogitto](https://docs.cocogitto.io) (`cog`), installed via `cargo install cocogitto`. A `commit-msg` hook runs `cog verify` and rejects non-conventional messages.
- **Pre-push runs the full test suite.** `.config/lefthook.yaml`'s `pre-push` runs `cargo nextest run --workspace`, `uv run pytest` (with coverage), and the frontend Vitest suite before the push goes to origin. Even a docs-only push pays the ~30s; this is by design so broken builds never reach the remote. Use `mise exec -- git push` so the pinned lefthook runs.
- **`gh` + `jq` are runtime prerequisites** for repo-automation tasks like `mise run repo:apply-settings`. Neither is pinned via mise; install from the system package manager (or `brew install gh jq`) on fresh clones.
- **`gh pr edit` silently no-ops in this repo** because the GitHub classic-projects deprecation prints a GraphQL warning that gh treats as terminal: the command exits 0 without applying the edit. Workaround: `gh api -X PATCH repos/<owner>/<repo>/pulls/<N> -f title='...'` (and `-f body='...'`). Same shape applies to other gh commands that mutate PRs through GraphQL. Revisit when classic projects are sunset on this repo.
- **`gh run view --log` returns empty output here** (both the run-level form and the `--job=<id>` form), even on green runs whose job timestamps are visible via `--json jobs`. The same warning interaction that breaks `gh pr edit` appears to short-circuit the streaming-log code path. Workaround: fetch each Test job's full log with `gh api repos/<owner>/<repo>/actions/jobs/<job-id>/logs` (where `<job-id>` comes from `gh run view <run-id> --json jobs -q '.jobs[] | select(.name=="Test") | .databaseId'`), then grep for the line of interest (e.g., `Pytest wall-clock:` for the duration-budget gate). Same pattern works for any per-step log line; the API-level endpoint never silently empties.
- **Ad-hoc Python snippets with third-party deps.** The system `python3` has no `pyyaml`, `coloraide`, or similar. For one-off verification or conversion scripts (YAML diffs of workflow permissions, OKLCH to sRGB hex conversion for `frontend/DESIGN.md` updates, etc.), invoke via `uv run --with <pkg1> --with <pkg2> python3 - <<'EOF' ... EOF` so the pinned `uv` provides the dep. Recurring pairs: `--with pyyaml` (YAML parsing), `--with coloraide` (OKLCH ↔ sRGB hex for DESIGN.md / app.css work).

## Simplicity first

Write the minimum code that solves the problem. Nothing speculative.

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

## Coding standards

- **No bare catchalls.** No untyped `catch` in TypeScript, no `Result<_, _>` swallowed with `_` in Rust. Catch the specific error you can handle; let the rest propagate. (Python framing lives in `backend/CLAUDE.md`.)
- **No dynamic imports.** All imports must be static/top-of-file so the dependency graph is statically analyzable. No `import()` expressions, no `importlib.import_module` in hot paths.
- **Unique function names globally.** Function names must be unique across the entire codebase, even across classes and files. `scripts/check_unique_fns.py` runs in pre-commit and walks TS/TSX, Rust integration tests under `tests/`, **and all Python files (including methods inside classes)**, so when duplicating a page skeleton across entities or a property-test generator across solver-core test files, rename helpers per feature: `RoomsPageHead` not `PageHead`, `handleRoomSubmit` not `onSubmit`, `confirmRoomDelete` not `confirm`, `lahc_small_problem` not `small_problem`, `lahc_weights` not `weights`. Same applies to test helpers: `wrapScheduleHook` not `wrap`, `ScheduleSkeletonGrid` not `SkeletonGrid`. Same applies to Pydantic `@model_validator` / `@field_validator` methods: prefix the validator name with the schema type so two `*Create` schemas can host the same-shape invariant without collision (`_lesson_hours_divisible_by_block_size` on `LessonCreate`, `_entry_hours_divisible_by_block_size` on `EntryCreate`, not the bare `_hours_divisible_by_block_size` on both).
- **Dockerfile build context is the repo root.** `backend/Dockerfile` and `frontend/Dockerfile` are built from the repo root with `context: .` and `file: <subdir>/Dockerfile` (see `.github/workflows/deploy-images.yml`). Every `COPY` inside them is therefore written as `COPY backend/ backend/`, `COPY frontend/ ./`, etc. The matching `.dockerignore` lives next to each Dockerfile but its patterns are evaluated against the repo root.
- **ADR titles skip the em-dash.** `docs/adr/template.md` renders `# NNNN — Title`, but the user's global preference forbids em- and en-dashes in new prose. Use a colon (`# NNNN: Title`) in new ADRs. Existing ADRs 0001-0008 stay as they are; ADRs are immutable per `docs/adr/README.md`. **Always `ls docs/adr/*.md | sort | tail -1` before assigning the next number;** roadmap memory and stale specs/plans may reference a number that is already taken.
- **Commit types live in `.github/commit-types.yml`.** `.github/workflows/pr-title.yml` and `CONTRIBUTING.md` carry `BEGIN/END GENERATED: commit-types` regions rendered from the YAML. Edit the YAML, then `mise run gen:commit-types`. `mise run check:commit-types` runs inside `mise run lint` and catches drift.
- **SHA-pin third-party GitHub Actions.** `actions/*` and `github/*` can use `@vN`; everything else (community or single-maintainer actions like `JasonEtco/create-an-issue`, `amannn/action-semantic-pull-request`) pins to a full commit SHA with a trailing `# vX.Y.Z` comment for audit readability. Moving tags on third-party code are a supply-chain risk.

## Commit messages

This repo enforces [Conventional Commits](https://www.conventionalcommits.org/).

**Format:** `<type>(<optional scope>): <description>`

**PR titles** must also satisfy `subjectPattern: ^[a-z].+$` (checked by `amannn/action-semantic-pull-request`). Start the subject with a lowercase letter even when the first word is an acronym: `feat(frontend): crud pages ...`, not `feat(frontend): CRUD pages ...`.

**Common types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`. Append `!` for breaking changes, or add a `BREAKING CHANGE:` footer.

When creating commits, always produce a Conventional Commits-compliant message. See `CONTRIBUTING.md` for the full type table and examples.

Beyond enforcement, `cog` also handles changelog generation (`cog changelog`) and semver bumps (`cog bump`). Prefer these over hand-rolled equivalents.
