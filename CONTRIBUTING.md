# Contributing

## Prerequisites

The only thing you install by hand is [mise](https://mise.jdx.dev/). mise provides every other tool (Rust, Python, uv, cocogitto, lefthook, cargo-nextest, cargo-llvm-cov, cargo-machete, cargo-deny) at the pinned versions defined in `mise.toml`.

## First-time setup

```bash
mise install              # install the pinned toolchain
mise run install          # install git hooks and sync deps (builds the solver via maturin)
mise run db:up            # start the dev Postgres (podman compose)
cp backend/.env.example backend/.env
mise run db:migrate       # apply database migrations
mise run test             # confirm everything works
```

After this, `mise run test`, `mise run lint`, and `mise run dev` all work. See [`README.md`](README.md) for the full task table.

`mise run lint` also runs `actionlint` against `.github/workflows/*.yml` so YAML or expression errors in workflows surface locally before CI catches them.

## Commit messages

This repo uses [Conventional Commits](https://www.conventionalcommits.org/). A `commit-msg` hook runs `cog verify` and will reject non-conforming messages.

**Format:**

```
<type>(<optional scope>): <description>

[optional body]

[optional footer(s)]
```

**Allowed types:**

<!-- BEGIN GENERATED: commit-types -->
| Type       | Use for                                                 |
|------------|---------------------------------------------------------|
| `feat`     | A new feature (minor version bump)                      |
| `fix`      | A bug fix (patch version bump)                          |
| `docs`     | Documentation-only changes                              |
| `style`    | Formatting only, no code change                         |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf`     | Performance improvement                                 |
| `test`     | Adding or correcting tests                              |
| `build`    | Build system or external dependency changes             |
| `ci`       | CI configuration changes                                |
| `chore`    | Other changes that don't touch src or tests             |
| `revert`   | Reverts a previous commit                               |
<!-- END GENERATED: commit-types -->

**Breaking changes:** append `!` after the type/scope, e.g. `feat(api)!: drop support for X`, or add a `BREAKING CHANGE:` footer.

**Examples:**

```
feat(auth): add refresh token rotation
fix(parser): handle empty input without panicking
docs: explain lefthook setup in CONTRIBUTING
chore(deps): bump cocogitto
refactor!: replace sync HTTP client with async
```

### Tips

- Use `cog commit feat auth "add refresh token rotation"` as a guided helper for writing compliant commits.
- `cog check` validates a range of existing commits — useful in CI for PRs.
- `cog changelog` generates `CHANGELOG.md` from the commit history.
- `cog bump` performs semver version bumps based on commit types.

### Applying repo settings

The repo's GitHub-side configuration (merge strategies, required status checks, linear history) lives in `.github/repo-settings.json` and `.github/branch-protection.json`. Apply them with:

```bash
mise run repo:apply-settings -- --dry-run   # recommended first run
mise run repo:apply-settings                 # actually apply
```

The script reads branch protection back and exits non-zero on drift.

## Authentication

For how the session/cookie auth works, how to protect a route, and how to bootstrap the first admin, see [`docs/architecture/authentication.md`](docs/architecture/authentication.md).

## Database

The DB layer lives under `backend/src/klassenzeit_backend/db/`. For
the contributor-facing reference — how to add a model, how to write
a migration, how tests are isolated — see
[`docs/architecture/database.md`](docs/architecture/database.md).

A few invariants you must not break:

- **Never edit a merged migration.** Write a corrective migration
  instead.
- **Every model must be re-exported from
  `db/models/__init__.py`.** Models not re-exported are invisible
  to Alembic autogenerate.
- **Never remove the `MetaData` naming convention on `Base`.**
  It is load-bearing for stable autogenerate diffs.
- **No module-level engine or session.** The engine is built in the
  FastAPI `lifespan` and lives on `app.state`.
- **Integration tests hit a real Postgres** (via the `klassenzeit_test`
  database). They never mock the DB.
