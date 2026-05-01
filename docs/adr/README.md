# Architecture Decision Records

An **Architecture Decision Record** (ADR) captures one architectural
decision, the context that produced it, and the consequences of
living with it.

## Rules

- **One decision per ADR.** If you find yourself writing about two,
  split them.
- **Short.** 150–400 words. One screen.
- **Immutable.** Once an ADR is merged, it is never edited. If you
  change your mind, write a new ADR that *supersedes* the old one and
  update the old one's Status line to `Superseded by NNNN`.
- **Numbered sequentially.** New ADRs take the next unused number.
- **Name pattern.** `NNNN-short-dash-separated-title.md`.

## Writing one

Copy [`template.md`](template.md) to `NNNN-your-title.md`, fill in the
sections, and add the new entry to the index below.

## Index

| # | Title | Status |
|---|---|---|
| 0001 | [Monorepo with Cargo and uv workspaces](0001-monorepo-two-workspaces.md) | Accepted |
| 0002 | [Rust solver split into solver-core and solver-py](0002-rust-solver-pyo3-bindings.md) | Accepted |
| 0003 | [Postgres everywhere](0003-postgres-everywhere.md) | Accepted |
| 0004 | [SQLAlchemy 2.0 async plus Alembic](0004-sqlalchemy-async-alembic.md) | Accepted |
| 0005 | [Transaction-rollback test isolation](0005-transaction-rollback-tests.md) | Accepted |
| 0006 | [Self-rolled cookie-session auth](0006-self-rolled-cookie-session-auth.md) | Accepted |
| 0007 | [React + Vite SPA for the frontend](0007-react-vite-spa-frontend.md) | Accepted |
| 0008 | [Frontend theming, i18n, and coverage ratchet](0008-frontend-theming-i18n-ratchet.md) | Accepted |
| 0009 | [Deployment topology for the staging tier](0009-deployment-topology.md) | Accepted |
| 0010 | [Uniform `/api` prefix on the backend](0010-api-prefix.md) | Accepted |
| 0011 | [Subject color and simplified room suitability](0011-subject-color-and-simplified-suitability.md) | Accepted |
| 0012 | [DESIGN.md as canonical design artifact](0012-design-md-canonical-artifact.md) | Accepted |
| 0013 | [Typed solver violations](0013-typed-solver-violations.md) | Accepted |
| 0014 | [SolveConfig API and FFD ordering](0014-solve-config-and-ffd-ordering.md) | Accepted |
| 0015 | [Solver LAHC local-search loop with seeded RNG](0015-solver-lahc-stochastic-search.md) | Accepted |
| 0016 | [Structured logging across the backend](0016-structured-logging.md) | Accepted |
| 0017 | [Subject-level pedagogy preferences](0017-subject-preferences.md) | Accepted |
| 0018 | [Solver Doppelstunden support](0018-solver-doppelstunden.md) | Accepted |
| 0019 | [Backend pytest-xdist with per-worker test databases](0019-backend-pytest-xdist.md) | Accepted |
| 0020 | [Configurable LAHC deadline on the solver JSON adapter](0020-configurable-lahc-deadline.md) | Accepted |
| 0021 | [Many-to-many Lesson school classes](0021-multi-class-lessons.md) | Accepted |
| 0022 | [Lesson-group co-placement constraint](0022-lesson-group-coplacement.md) | Accepted |
| 0023 | [Home-room preference soft constraint](0023-home-room-preference.md) | Accepted |
| 0024 | [Avoid-last-period soft constraint](0024-avoid-last-period.md) | Accepted |
| 0025 | [Per-Subject preference weights](0025-subject-preference-weights.md) | Accepted |
