# Sprint C: manual schedule editing with pinned placements

**Status:** design (2026-05-03)
**Owner:** /autopilot run on `feat/sprint-c-manual-editing`
**Brainstorm:** `/tmp/kz-brainstorm/brainstorm.md` (Q&A also posted as PR comments)
**Roadmap:** Sprint C of the Scheduling UX program (after Sprint A PR #166, Sprint B PR #168). Closes the program.

## 1. Problem

The whole-school generator (Sprint A) and the teacher / room views (Sprint B) make a Stundenplan visible. They do not let a user fix it. Today an admin who spots a wrong placement has only one tool: re-run the solver and hope it picks differently. There is no way to nudge a single cell, lock a good placement so the next solve does not undo it, or swap two lessons.

Sprint C adds the missing edit surface so admins can drag-and-drop placements, pin the ones they want stable, and trigger a re-solve that treats their pins as hard constraints. With this in place the Scheduling UX program ships its full vision: generate a school-wide plan, browse it by class / teacher / room, fix the issues by hand.

## 2. Goals and non-goals

**Goals.**

- Persistent `pinned` flag on `ScheduledLesson` rows, defaulting to `false`.
- Three new HTTP endpoints to move, pin-toggle, and atomically swap placements.
- A `respect_pins` flag on `POST /api/schedule/all` that distinguishes "re-solve respecting my pins" (default `true`) from "Generate all from scratch" (`respect_pins=false`).
- Per-class re-solve respects own-class pins in addition to the existing sibling-pin behaviour shipped in Sprint A.
- Drag-and-drop on the class-view schedule grid via `@dnd-kit/core` with optimistic update plus rollback on error.
- Hover-revealed pin toggle on each cell with a persistent indicator on pinned cells.
- A "Re-solve respecting my pins" toolbar action visually distinct from the existing "Generate all" action.
- ADR 0028 records the user-facing pin semantics layered on top of ADR 0027's wire format.

**Non-goals.**

- Drag-and-drop in the teacher view or room view. They stay read-only this sprint.
- Cross-class drag (each view shows only one class's placements; cross-class swap stays out of scope until follow-up feedback surfaces).
- Mobile touch optimisation beyond what `@dnd-kit/core` provides out of the box.
- A separate "soft pin" semantic. A move pins; explicit unpin is the escape hatch.
- A new ScheduledLesson surrogate primary key. The composite `(lesson_id, time_block_id)` carries through the URLs.
- Production deployment changes. Staging auto-deploys on master push as it has all sprint.

## 3. Architecture

### 3.1 Schema

One new column on `scheduled_lessons`:

```python
pinned: Mapped[bool] = mapped_column(
    Boolean, nullable=False, server_default=text("false")
)
```

Migration is a single `op.add_column(...)` with the matching downgrade. Postgres applies the default to existing rows in one statement, so no data backfill is needed. Test-template + per-worker DBs need to be dropped after pulling the migration locally per `backend/CLAUDE.md`.

### 3.2 Wire format

No Rust change. `solver/solver-core` already accepts `Problem.pinned_placements: Vec<PinnedPlacement>` (additive `#[serde(default)]`, ADR 0027). Sprint A's `collect_pinned_placements` walks `ScheduledLesson` rows and turns each into a `PinnedPlacement`. Sprint C extends that helper:

- The whole-school `POST /api/schedule/all` path passes `pinned_placements` derived from rows where `pinned = true` (only when `respect_pins` is true).
- The per-class `POST /api/classes/{id}/schedule` path passes pinned_placements derived from (a) every sibling class's persisted placements, exactly as today, plus (b) the requested class's own rows where `pinned = true`. The class's unpinned rows continue to be deleted and re-solved.

The Pydantic `LessonInput` for the solver wire format gains no new field; the column flows through the helper, not through the schema.

### 3.3 Endpoints

All three new endpoints address a placement by its composite key. They live in a new `scheduling/routes/placements.py` registered under `/api/placements`. Authorisation: same `@require_admin` decorator already in use on the per-class schedule routes.

| Method | Path | Purpose | Body | Returns |
|---|---|---|---|---|
| `PATCH` | `/api/placements/{lesson_id}/{time_block_id}` | Move a placement to a new time block + room. | `{ time_block_id, room_id }` | `PlacementResponse` (lesson_id, time_block_id, room_id, pinned). |
| `PATCH` | `/api/placements/{lesson_id}/{time_block_id}/pin` | Toggle the pin flag. | `{ pinned: bool }` | `PlacementResponse`. |
| `POST` | `/api/placements/swap` | Atomically swap two placements within a single transaction. | `{ a: { lesson_id, time_block_id }, b: { lesson_id, time_block_id } }` | `{ a: PlacementResponse, b: PlacementResponse }`. |

**Pin side effects.**

- The move endpoint sets `pinned=true` on the moved row. Q3 of the brainstorm: any user-driven write is intentional placement.
- The swap endpoint sets `pinned=true` on both ends.
- The pin-toggle endpoint sets `pinned` to the body value verbatim. It is the only endpoint that can move a placement from pinned to unpinned without changing slot.

**Validation rules (server-side, all return 422 if violated).**

- Composite key must reference an existing `ScheduledLesson`.
- Target `time_block_id` and `room_id` must reference existing rows.
- The target `time_block_id` must belong to a `WeekScheme` used by the lesson's `school_class`.
- Move would not result in two `ScheduledLesson` rows with the same `(lesson_id, target_time_block_id)` PK pair.

**Soft conflicts (NOT rejected).**

- Teacher double-booking, room double-booking, lesson-group placement gaps, every other `ViolationKind` from `solver-core`. The schedule grid already renders these as badges; manual edits join that flow. Q7 of the brainstorm.

**`POST /api/schedule/all?respect_pins=` flag.**

- Default `true`. Existing callers passing no flag now opt in to pin handling. This is a behaviour change vs. Sprint A and is recorded in ADR 0028.
- `respect_pins=false`: all `pinned=true` rows are still deleted and re-placed by the solver. Pin state in the database is unchanged.
- The frontend's existing "Generate all" action passes `respect_pins=false`. The new "Re-solve respecting my pins" action passes `respect_pins=true`.

**`POST /api/classes/{id}/schedule`** keeps its current shape. Internally `collect_pinned_placements` now also walks the requested class's own `pinned=true` rows.

### 3.4 Frontend

**Component layout.** No new top-level routes. All work happens inside `schedule-page-class-view.tsx` and the existing `schedule-grid.tsx`. The teacher and room views are unchanged.

**Drag-and-drop integration.** A new `<DndContext>` wraps the class-view grid. Each placement card becomes a `useDraggable` consumer, each empty cell a `useDroppable` consumer, each populated cell a `useDroppable` consumer that dispatches swap. A new hook `useScheduleDragAndDrop(scheduleQueryKey)` exposes `onDragEnd` that:

1. Reads the drag source's `(lesson_id, time_block_id)` and the drop target's `(time_block_id, room_id)` from `useDroppable` data.
2. If target slot is empty, calls `useMovePlacement` (which hits `PATCH .../move`).
3. If target slot is occupied, calls `useSwapPlacements` (which hits `POST .../swap`) with both composite keys.

**Optimistic update.** Each mutation hook uses `onMutate` to snapshot the schedule cache, mutate it in place, and return the snapshot. `onError` rolls back. `onSettled` invalidates the schedule key so server truth wins on settlement. Concurrent drags are safe because TanStack Query chains `onMutate` snapshots per mutation invocation.

**Pin toggle UI.** A small `Pin` / `PinOff` Lucide icon button appears in the top-right corner of each placement card. Pinned cells additionally render a subtle `border-primary/40` and the icon stays visible. Unpinned cells reveal the icon on hover and via `:focus-within`. Click toggles via `usePinPlacement`.

**Toolbar action.** `schedule-toolbar.tsx`'s class-view discriminated-union variant gains a third button: "Re-solve respecting my pins" (primary), with the existing "Generate all" demoted to a secondary visual treatment but still labelled (the new copy keys are `schedule.generate.respectPinsAction` and `schedule.generate.fromScratchAction`). The "Generate this class" button stays where it is.

**Conflict awareness.** Mutation toasts confirm the action ("moved", "swapped", "pinned") without recomputing violations server-side; the schedule cache invalidates after each mutation and the existing violation badges in the grid recolour automatically from the refetch. Computing per-move violation deltas would require running a solver pass on every drag and is out of scope.

**i18n.** New keys in `frontend/src/locales/en.json` and `de.json`:

- `schedule.actions.pin` / `schedule.actions.unpin`
- `schedule.actions.move` (visible-on-drag-handle title)
- `schedule.toasts.moveSuccess`
- `schedule.toasts.swapSuccess`
- `schedule.toasts.pinned`
- `schedule.toasts.unpinned`
- `schedule.generate.respectPinsAction`
- `schedule.generate.fromScratchAction`

### 3.5 Files touched

```
backend/src/klassenzeit_backend/db/models/scheduled_lesson.py        edit
backend/alembic/versions/<rev>_add_scheduled_lesson_pinned.py        new
backend/src/klassenzeit_backend/scheduling/solver_io.py              edit
backend/src/klassenzeit_backend/scheduling/routes/placements.py      new
backend/src/klassenzeit_backend/scheduling/routes/__init__.py        edit
backend/src/klassenzeit_backend/scheduling/schemas/placement.py      new
backend/src/klassenzeit_backend/scheduling/routes/schedule.py        edit (respect_pins flag on /all)
backend/tests/scheduling/test_placements_routes.py                   new
backend/tests/scheduling/test_schedule_all_respect_pins.py           new
backend/tests/scheduling/test_solver_io.py                           edit (own-class pin coverage)
frontend/package.json                                                edit (@dnd-kit/core)
frontend/src/features/schedule/hooks.ts                              edit (move/swap/pin hooks)
frontend/src/features/schedule/use-schedule-drag-and-drop.ts         new
frontend/src/features/schedule/schedule-grid.tsx                     edit
frontend/src/features/schedule/schedule-toolbar.tsx                  edit
frontend/src/features/schedule/schedule-page-class-view.tsx         edit
frontend/src/features/schedule/use-schedule-drag-and-drop.test.tsx   new
frontend/src/features/schedule/schedule-grid.test.tsx                edit
frontend/src/features/schedule/schedule-toolbar.test.tsx             edit
frontend/src/features/schedule/hooks.test.tsx                        edit
frontend/src/lib/api-types.ts                                        regenerated via mise run fe:types
frontend/src/locales/en.json                                         edit
frontend/src/locales/de.json                                         edit
e2e/tests/schedule-drag-and-drop.spec.ts                             new
docs/adr/0028-manual-pin-semantics.md                                new
docs/adr/README.md                                                   edit (ADR 0028 row)
docs/superpowers/OPEN_THINGS.md                                      edit (mark Sprint C shipped, surface follow-ups)
docs/architecture/overview.md                                        edit if subsystem story changes
```

## 4. Data flow examples

### 4.1 User drags a placement to an empty cell

1. `onDragEnd` reads source `(L1, TB1)` and target `(TB2, R)`.
2. `useMovePlacement.mutate({ lesson_id: L1, source_time_block_id: TB1, time_block_id: TB2, room_id: R })`.
3. `onMutate` cancels in-flight schedule queries, snapshots cache, optimistically replaces the placement at `(L1, TB1)` with one at `(L1, TB2, R, pinned=true)`.
4. Backend receives `PATCH /api/placements/L1/TB1`, deletes the row, inserts the new one with `pinned=true`, returns `PlacementResponse`.
5. `onSettled` invalidates the schedule key, server response wins, toast confirms success; the grid's violation badges recolour from the refetch.

### 4.2 User drags onto an occupied cell

1. `onDragEnd` reads source `(L1, TB1)` and target `(TB2, R)` and finds an existing placement `(L2, TB2, R)` in the cache.
2. `useSwapPlacements.mutate({ a: { L1, TB1 }, b: { L2, TB2 } })`.
3. `onMutate` snapshots cache, swaps the two placements optimistically (both end up `pinned=true`).
4. Backend receives `POST /api/placements/swap`, runs both updates in one transaction, returns both `PlacementResponse`s.
5. `onSettled` invalidates the schedule key.

### 4.3 User clicks "Re-solve respecting my pins"

1. Toolbar action fires `useGenerateAllSchedules.mutate({ respect_pins: true })`.
2. `POST /api/schedule/all?respect_pins=true` runs the solver with every `pinned=true` row threaded through `pinned_placements`. Unpinned placements are deleted and re-placed.
3. Response carries per-class violation counts as today.
4. Schedule cache is invalidated; toolbar toast announces success.

## 5. Error handling

- **Invalid composite key:** 404 from the move/pin endpoints, 404 on either side from swap. UI surfaces "this placement no longer exists; refreshing" and forces a refetch.
- **Target time block belongs to a different week scheme:** 422 with a typed error code (`placement.time_block_mismatch`). UI rolls back and shows a toast referencing the i18n key.
- **Optimistic rollback on network error:** `onError` restores the cache snapshot; toast says "could not save move; reverted."
- **`respect_pins=false` while every row is pinned:** the solver still runs and may produce the same result (pins remain on the rows; the solver simply ignores them for this run). No special handling.
- **Soft violations** stay surfaced in the violation badge, not in error toasts.

## 6. Testing

**Backend integration (`backend/tests/scheduling/`).**

- `test_placements_routes.py`:
  - move happy path (slot stays in same week scheme) sets `pinned=true`.
  - move to a slot owned by a different week scheme returns 422.
  - move to a non-existent time block returns 404.
  - pin-toggle round trip (true → false → true).
  - swap happy path: both rows end up pinned, both get the other's slot, single transaction.
  - swap with a non-existent right-hand side returns 404 and rolls back the left-hand side.
- `test_schedule_all_respect_pins.py`:
  - `respect_pins=true` (default): pinned rows survive, unpinned rows may move.
  - `respect_pins=false`: pinned rows may move but `pinned` flag stays in DB.
  - one parametrised regression test confirms existing callers (no flag passed) get the new default.
- `test_solver_io.py` extension: `collect_pinned_placements` for the per-class path now includes own-class `pinned=true` rows in addition to siblings.

**Frontend (`frontend/src/features/schedule/`).**

- `use-schedule-drag-and-drop.test.tsx`: `onDragEnd` dispatches move on empty target and swap on occupied target; optimistic snapshot is restored on error.
- `hooks.test.tsx`: `usePinPlacement`, `useMovePlacement`, `useSwapPlacements` invalidate the schedule key on settle.
- `schedule-grid.test.tsx` extension: pin icon visible on pinned cells, hidden until hover on unpinned cells.
- `schedule-toolbar.test.tsx` extension: class-view variant renders both new actions, each fires the right `respect_pins` flag.

**Playwright e2e (`e2e/tests/schedule-drag-and-drop.spec.ts`).**

- Seeded grundschule fixture, log in, open `/schedule?view=class&class=…`, drag a placement to an adjacent empty cell, assert the cell content moved, reload the page, assert the new position persisted, assert the pin badge is visible.

**No new Rust tests.** Wire format is identical to Sprint A.

## 7. Risks

- **Sprint A test surface:** flipping the `respect_pins` default to `true` may break tests asserting on a from-scratch outcome. Mitigation: grep `POST /api/schedule/all` callers and verify each. Most should pass through unchanged because `pinned=False` for all rows initially.
- **Composite-PK URL parsing edge cases:** confirm FastAPI path conversion accepts back-to-back UUID4 segments without the converter falling through. Pre-flight with a tiny smoke test added in the route module.
- **Optimistic-update race:** two drags in quick succession must each preserve their own snapshot. The Vitest test in `use-schedule-drag-and-drop.test.tsx` covers this.
- **Bundle size:** `@dnd-kit/core` adds ~10 KB gzipped. Verified small enough not to need a code-split. Re-run `mise run fe:dev` smoke test post-merge.
- **No new Rust code, but** schema-changing PRs leave stale test-template / per-worker DBs from the previous schema. The implementation plan calls out the recovery in the migration step.

## 8. Acceptance criteria

- All backend tests in section 6 pass; coverage does not regress past `.coverage-baseline`.
- All frontend tests in section 6 pass under Vitest.
- Playwright e2e passes locally and in CI.
- `mise run lint` is clean (ruff, ty, vulture, clippy, machete, biome, actionlint).
- `mise run bench:tests` stays under `.test-duration-budget`.
- ADR 0028 is registered in `docs/adr/README.md`.
- `OPEN_THINGS.md` removes the Sprint C entry, preserves any deferred follow-ups (e.g., teacher / room view editing) under Acknowledged deferrals.
- Auto-memory `project_roadmap_status.md` updates to "Sprint C shipped 2026-05-03; next pickup: Sprint 1 resume at `Room.is_external`."

## 9. Out of scope (explicitly deferred)

- Edit surfaces in teacher view and room view.
- Cross-class drag-and-drop.
- Mobile-first drag UX (long-press menu, touch-coalescing).
- A "soft pin" / "tentative move" semantic.
- A surrogate `id` column on `ScheduledLesson`.
- A graphical conflict-preview overlay during drag (current sprint surfaces conflicts post-drop via the violation badge).
