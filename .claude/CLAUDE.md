# Klassenzeit: Project Instructions

## Where rules live

- `.claude/CLAUDE.md`: architecture, workflow, global rules, commit conventions. Loaded every session.
- `backend/CLAUDE.md`: Python / FastAPI / SQLAlchemy / pytest. Loaded under `backend/`.
- `frontend/CLAUDE.md`: React / TanStack / shadcn / i18n / Vitest. Loaded under `frontend/`.
- `solver/CLAUDE.md`: Rust solver workspace. Loaded under `solver/`.
- `.claude/rules/*.md`: rules scoped by file path via `paths:` frontmatter.

## Architecture at a glance

- `backend/` — FastAPI + SQLAlchemy async, served under `klassenzeit_backend`. Runtime state lives on `app.state`, set in `lifespan`.
- `frontend/` — Vite 8 + React 19 SPA with TanStack Router/Query, shadcn/ui, react-i18next. Proxies API to `:8000` in dev.
- `solver/` — Rust workspace: `solver-core` (pure), `solver-py` (PyO3 via maturin), `solver-bench` (bake-off binary).
- `deploy/` — staging compose for the Hetzner VPS. Runbook: `deploy/README.md`. Decisions: `docs/adr/0009-deployment-topology.md`.
- Dev loop via `mise` tasks; Postgres via `podman compose` from root `compose.yaml` (local dev only, distinct from `deploy/compose.yaml`).
- **Multi-tenant by `school_id`.** Every aggregate root carries a NOT NULL `school_id` FK to `schools.id`; users belong to exactly one school. Routes scope by `scope_school_id: Annotated[uuid.UUID, Depends(get_scope_school_id)]` (transitively gates on `require_admin`); cross-tenant access returns 404, not 403. Super-admin users (`user.role == "super_admin"`) can override the operating school per request via `?school_id=<uuid>`; non-super-admins have the parameter ignored. See ADR 0045 for tenancy decisions; OPEN_THINGS item 10 captures the shipped surface, with open P2 follow-ups for multi-school membership (10c), E2E suite (10d), and per-school reference-data (10f).

## Development Workflow

**Skills are not optional when a workflow names them.** Call the `Skill` tool and follow what it returns; freehand synthesis is a process violation. End-of-turn summary notes any listed skill that was unavailable. Always use TDD with red-green-refactor via `workbench:test-driven-development`. Development ends in PRs after docs are reviewed and updated. Keep tech debt and out-of-scope items in `docs/OPEN_THINGS.md`, ordered by importance, no duplicates. `workbench:autopilot` runs the full flow end-to-end; Klassenzeit overrides live in `.workbench/autopilot.md`.

## Work selection: quality first, tidy first

Prefer tech debt and quality work over new user-facing features. Kent Beck's "Tidy First?":

1. Read OPEN_THINGS.md top to bottom; active sprint and follow-ups first, product capabilities last.
2. Pick the highest-impact unblocked item that fits one PR.
3. Structural refactors that remove duplication or replace ad-hoc patterns with shared primitives are tidy-first and preferred over feature work.
4. A structural change and a behavioral change never ship in the same commit. A bug uncovered by a tidy commit goes in a separate `fix(...)` commit.
5. Behavior must be preserved across a tidy commit: tests that passed before must pass after without modification.

If every quality item is blocked, fall back to the next feature item and note why in the PR body.

## Tooling

**Commands:** `mise run dev` / `fe:dev` (servers); `mise run test` / `test:py` / `test:rust` / `fe:test` (test:py path filters are repo-root-relative); `mise run e2e` / `e2e:ui` / `e2e:install` (Playwright); `mise run lint` / `fmt`; `mise run check:actions`; `mise run fe:types` (regenerate OpenAPI types); `mise run db:up` / `db:stop` / `db:reset` / `db:migrate` / `db:revision -- -m '<msg>'` (alembic autogenerate, runs in `backend/`) / `db:downgrade`; `mise run seed:grundschule` (load the demo Grundschule into the dev DB); `mise run bench:tests` (pytest suite vs `.test-duration-budget`).

**OPEN_THINGS sync.** `docs/OPEN_THINGS.html` auto-renders from `.md` via `mise run gen:openthings-html`. The lefthook `openthings-html` entry auto-regen-and-stages only when `OPEN_THINGS.md` is in the *staged* set. The pre-commit `lint` step also runs `check:openthings-html` for every commit, and it fails on drift regardless of which files are staged. When committing an unrelated file while `OPEN_THINGS.md` has unstaged edits, run `mise run gen:openthings-html` first so the working tree's `.md` and `.html` are in sync.

**Flake-loop discipline.** Use `|| true`, not `|| break`, so one failure does not truncate the loop. Count summary lines with `grep -cE "^=+ [0-9]+ (passed|failed|xfailed|xpassed)"`, not failure-output lines. Treat `xpassed` as `passed` for gate logic. Leave coverage enabled; `-p no:cov` errors against the project `--cov-fail-under=80` addopts. Run once with `--runxfail` before closing an item whose narrative names a specific bug shape.

**Prerequisites and quirks.** Rust toolchain is required. Lefthook (`.config/lefthook.yaml`) runs git hooks; Cocogitto (`cog`, `cargo install cocogitto`) enforces commit messages. Pre-push runs the full test suite; use `mise exec -- git push`. `gh pr edit` no-ops here, use `gh api -X PATCH repos/<owner>/<repo>/pulls/<N> -f title='...'`; `gh run view --log` returns empty output, fetch via `gh api repos/<owner>/<repo>/actions/jobs/<job-id>/logs`.

## Coding standards

- **No bare catchalls.** No untyped `catch` in TypeScript, no `Result<_, _>` swallowed with `_` in Rust. Python framing in `backend/CLAUDE.md`.
- **No dynamic imports.** Static / top-of-file only. No `import()`, no `importlib.import_module` in hot paths.
- **Unique function names globally.** `scripts/check_unique_fns.py` walks TS/TSX, every `.rs` file (including `#[cfg(test)] mod tests`), and all Python files (including methods). Before adding a method, run the script (`uv run scripts/check_unique_fns.py`) or grep all three syntaxes: `rg -n '(fn|def|function) <name>\b' solver/ frontend/ backend/` (a bare `fn` grep misses Python `def` and TS `function` / arrow-binding declarations). Rename the new method, not the existing one.
- **Dockerfile build context is the repo root.** `COPY` paths and `.dockerignore` patterns evaluate against the repo root.
- **ADR titles use a colon, not an em-dash** (`# NNNN: Title`). Always `ls docs/adr/*.md | sort | tail -1` before assigning a number. `docs/adr/README.md` indexes ADRs as a Markdown table; append a row.
- **Commit types live in `.github/commit-types.yml`.** `mise run gen:commit-types` rewrites generated regions; `mise run check:commit-types` runs inside lint.
- **SHA-pin third-party GitHub Actions.** `actions/*` and `github/*` may use `@vN`; everything else pins to a full SHA with a trailing `# vX.Y.Z` comment.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/) enforced. Format: `<type>(<optional scope>): <description>`. PR titles must satisfy `subjectPattern: ^[a-z].+$`, so start with a lowercase letter even when the first word is an acronym. Common types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`. Append `!` or add a `BREAKING CHANGE:` footer. See `CONTRIBUTING.md`.
