# Klassenzeit — Timetabler for schools

A school timetabling system with a FastAPI backend, a Rust solver exposed to Python via PyO3, and a React + Vite admin frontend.

## Docs

- [`docs/architecture/overview.md`](docs/architecture/overview.md) —
  system overview and monorepo layout.
- [`docs/architecture/database.md`](docs/architecture/database.md) —
  database layer reference.
- [`docs/adr/`](docs/adr/) — architectural decision records.
- [`docs/README.md`](docs/README.md) — full docs map.

## Dev Setup

1. Install [mise](https://mise.jdx.dev/).
2. `mise install` — installs the pinned Rust, Python, uv, Node, pnpm, cocogitto, and lefthook.
3. `mise run install` — installs git hooks, syncs the Python workspace (`uv sync`, which also builds the Rust solver extension), and installs frontend dependencies.
4. `mise run db:up` — start the dev Postgres via `podman compose`.
5. `cp backend/.env.example backend/.env` — seed your local env file.
6. `mise run db:migrate` — apply database migrations.
7. `mise run test` — confirm everything works.

## Common tasks

| Command | What it does |
|---|---|
| `mise run dev`      | Run the backend with auto-reload. |
| `mise run fe:dev`   | Run the frontend dev server on `http://localhost:5173` (proxies API calls to the backend). |
| `mise run test`     | Run all Rust, Python, and frontend tests. |
| `mise run test:py:parallel` | Run Python tests in parallel via pytest-xdist (`-n auto`). Mirrors the CI invocation. |
| `mise run lint`     | Lint Rust (fmt, clippy, machete), Python (ruff, ty, vulture), and frontend (Biome). |
| `mise run fmt`      | Auto-format Rust, Python, and frontend. |
| `mise run cov`      | Produce Rust and Python coverage reports. |
| `mise run fe:test:cov` | Run frontend tests with coverage (writes `frontend/coverage/`). |
| `mise run fe:cov:update-baseline` | Rebaseline `.coverage-baseline-frontend` after intentional coverage drops. |
| `mise run audit`    | Supply-chain audit (`cargo deny`, `pip-audit`). |
| `mise run repo:apply-settings` | Apply GitHub repo + branch-protection settings (use `-- --dry-run` first). |
| `mise run bench`    | Run solver-core benches (criterion; perf-regression budget). |
| `mise run bench:bakeoff` | Run the solver feasibility bake-off and rewrite `solver/solver-core/benches/BENCH_RESULTS.md`. |
| `mise run bench:tests` | Time the backend pytest suite and compare to `.test-duration-budget`. |
| `mise run fe:build` | Production build of the frontend into `frontend/dist/`. |
| `mise run fe:types` | Regenerate `frontend/src/lib/api-types.ts` from the backend's OpenAPI schema. |
| `mise run gen:commit-types`   | Regenerate the commit-types sections in `pr-title.yml` and `CONTRIBUTING.md` from `.github/commit-types.yml`. |
| `mise run check:commit-types` | Verify the two derived files match `.github/commit-types.yml` (also runs inside `mise run lint`). |
| `mise run seed:grundschule` | Seed a demo Hessen Grundschule (4 classes, 6 teachers, 7 rooms). Refuses in `KZ_ENV=prod`. |

## Deployment

Klassenzeit deploys to `klassenzeit-staging.pascalkraus.com` on a Hetzner
VPS via container images published to GHCR on every push to `master`. The
runbook lives in [`deploy/README.md`](deploy/README.md). Architecture
decisions are captured in [ADR 0009](docs/adr/0009-deployment-topology.md).

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for commit message rules.
