# 0039: Live solve progress via Arc<atomic> beacon, polling transport, soft-cancel best-so-far

- **Status:** Accepted
- **Date:** 2026-05-13

## Context

`POST /api/classes/{id}/schedule` blocks for up to the per-backend
`solve_deadline_ms` while LAHC searches (5000 ms on `lahc_rr`, 120000 ms
on `cpsat`; ADR 0038). The frontend ran a single spinning "Generating..."
button with no progress signal. Demo users clicked Generate a second time
because the first click "didn't do anything"; the same demos also lacked
a way to stop a solve early once the schedule looked good enough. The
implementation needed three independent decisions to settle: how the
solver hot loop communicates with an external observer, what transport
the HTTP layer uses to surface the snapshot, and what happens to the
running solution when a user clicks Stop.

## Decision

**Solver-internal channel: `Arc<ProgressBeacon>` of atomics with `Relaxed`
ordering.** `ProgressBeacon` wraps `AtomicU64` (iter, placement_count,
best_score), `AtomicBool` (feasible, cancel_requested). The LAHC inner
loop calls `beacon.record(...)` every iteration and checks
`beacon.cancel_requested()` at the loop head; the PyO3 `ProgressHandle`
wraps the same Arc on the Python side. `Relaxed` is sufficient: the only
invariant is "every read returns some value the loop wrote", which every
target platform guarantees. There are no happens-before relationships
between fields and no causal chain across atomics. No mutex, no channel,
no allocation in the hot path.

**Solver-core integration: private `_inner(progress: Option<...>)`
delegation.** The public `solve_with_config_stats` keeps its signature
and delegates to a private `solve_with_config_stats_inner` that takes an
optional beacon. `solve_with_progress` is the new public variant that
threads the beacon through. The deadline-only path (no beacon) is
byte-equal to the pre-feature path; a fifty-seed determinism integration
test pins `Solution` equality across `beacon=None` vs
`beacon=Some(fresh)` (no `request_cancel()` called).

**Transport: polling, not SSE / WebSocket / streaming HTTP.** The
frontend `useScheduleProgress` hook hits `GET .../schedule/progress`
every 500 ms via TanStack Query's `refetchInterval`; the endpoint reads
the registry, snapshots the beacon, and returns JSON. Returns 404 when
no solve is in flight for the class; the hook coerces 404 to `null` so
the query stays enabled across the brief window between mutation start
and registry registration. Mid-solve elapsed-vs-deadline drives a
determinate progress bar; `placement_count / total_lessons` drives a
"K / N lessons placed" badge.

**Soft-cancel returns best-so-far.** `POST .../schedule/cancel` flips
the beacon's `cancel_requested` flag and returns 204. The LAHC loop
exits at the next iteration boundary; the originating POST returns 200
with `was_cancelled=true` and the running-best placements. The frontend
binds the Stop button's "Stopping..." label to the polled snapshot's
`cancel_requested` field (not local mutation state) so a mid-stop reload
still surfaces the state. CP-SAT does not honor the beacon yet (the
backend logs `solver.solve.progress_unsupported` and falls through to
the existing CP-SAT path); the progress endpoint returns zero counters
in that case but the solve still works.

**Backend lifecycle: `app.state.solver_progress[class_id]` registry +
`register_progress` context manager.** A per-solve `RegistrationEntry`
(handle, started_at, deadline_ms, total_lessons) is registered for the
lifetime of the solve and removed in a `finally` so a crashing solver
still leaves the registry clean. `solver_progress` is wired in
`lifespan` and mirrored in the test `client` fixture per backend
CLAUDE.md.

## Alternatives considered

- **SSE / streaming HTTP transport.** Single long-lived connection per
  solve, real push semantics. Rejected: the 500 ms polling cadence is
  smaller than the per-iteration LAHC cost on dreizügig (~5 ms × 1000+
  iterations within the 5 s window) so polling has fine-enough
  resolution. SSE buys nothing for two endpoints with two clients (the
  one polling, the one Stop) and adds connection-state handling that
  the FastAPI request scope avoids today. Reversible: swap
  `refetchInterval` for an `EventSource` and the endpoint for a
  generator response if real-time push becomes load-bearing.
- **`Acquire` / `Release` ordering on the atomics.** Necessary if any
  read crossed an atomic boundary to enforce a happens-before
  relationship on a non-atomic write. No such relationship exists here.
  Rejected as over-engineered.
- **`Mutex<ProgressState>`.** Simpler to reason about. Rejected because
  the LAHC inner loop runs ~1000-50000 iterations per second on a
  Grundschule fixture and acquiring a mutex per iteration costs more
  than the entire `beacon.record(...)` block under `Relaxed` atomics.
- **`tokio::sync::watch::channel` from the worker.** Channel-shaped
  API. Rejected: `asyncio.to_thread` already detaches LAHC from the
  event loop, so an `await`-able channel back into the request handler
  is wrong-shaped. The handler reads on demand from a different
  endpoint (the GET) rather than awaiting a producer.
- **Hard-cancel that kills the worker.** Returns no partial schedule;
  loses the placements the loop already accepted. Rejected on UX
  grounds: "Stop" should mean "stop here with what you have", not
  "throw away the work". The cooperative loop-head check is cheap and
  the original POST returning best-so-far is the natural fit for a
  user who clicks Stop because the schedule already looks good.
- **`Solution.was_cancelled` as a separate `CancelStats` struct.**
  Rejected: a single bool on `Solution` is the minimum surface; the
  field cascades to ~10 mechanical `was_cancelled: false`
  struct-literal sites in solver-bench (no behavior change) per the
  solver/CLAUDE.md "Adding a field to Solution cascades to ~15 sites"
  rule.

## Consequences

Positive:

- Hot loop overhead is one un-contended atomic store per iteration per
  field and one atomic load per iteration on `cancel_requested`. The
  byte-equal seed-sweep determinism test pins zero behavioral drift on
  the no-beacon path.
- Soft-cancel preserves work: a user clicks Stop and gets the
  running-best schedule, not an empty page.
- Polling sidesteps stream lifecycle: a closed tab stops polling, a
  reload resumes polling, no connection-shutdown bookkeeping.
- The registry pattern (`register_progress` + `finally`) is reusable
  for any future per-class long-running operation.

Negative:

- 500 ms polling sends ~10 GET requests per 5 s solve. Each handler is
  ~ms; not a load concern at single-tenant scale but visible in dev
  logs.
- CP-SAT's progress is a known gap (zero counters during a 120 s
  solve). Wiring CP-SAT to the beacon needs a Python-side callback
  with `cp_model.CpSolverSolutionCallback` and a poll on the cancel
  flag; tracked as follow-up if a demo surfaces the gap.
- `Arc<ProgressBeacon>` adds a heap allocation per solve. Negligible
  vs the solve itself.

## Reversibility

The deadline-only path is byte-equal to the pre-feature path; removing
the beacon is a revert of the four feature commits plus the registry
wiring. The schema-level cost is `Solution.was_cancelled: bool` which
defaults to `false` and is harmless to leave on the type if downstream
consumers ignore it.

## References

- OPEN_THINGS item 3 (closed by this PR).
- ADR 0033 (raised the per-backend deadline to 5000 ms, made the
  perceived wall-clock long enough to need a progress UI).
- ADR 0038 (per-backend deadline configuration; CP-SAT's 120 s budget
  is the main motivator for soft-cancel).
- Solver-core changes: `solver/solver-core/src/progress.rs`,
  `solver/solver-core/src/solve.rs`, `solver/solver-core/src/lahc.rs`.
- Backend integration:
  `backend/src/klassenzeit_backend/scheduling/progress.py`,
  `backend/src/klassenzeit_backend/scheduling/routes/schedule.py`.
- Frontend: `frontend/src/features/schedule/generate-in-progress.tsx`,
  `frontend/src/features/schedule/hooks.ts`.
