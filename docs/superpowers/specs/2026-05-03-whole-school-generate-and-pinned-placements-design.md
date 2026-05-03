# Sprint A: whole-school `Generate all` and pinned-placement re-ingestion

Sprint identifier: A in the Scheduling UX program (active sprint queued 2026-05-03 ahead of the paused Schwimmen + Sek-I sprint, see `docs/superpowers/OPEN_THINGS.md`).

Brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (autopilot scratch, posted to the PR as comments via `.claude/commands/post_brainstorm_comments.py`).

## Goal

Two outcomes shipped in one PR:

1. A single `POST /api/schedule/all` route that solves the timetable for every class in one transaction and persists per-class placements atomically. Frontend exposes it as a primary "Generate all" button on `/schedule`.
2. A new solver wire-format primitive `Problem.pinned_placements: Vec<PinnedPlacement>` that the per-class re-solve uses to pin sibling classes' already-persisted placements. This closes the "Whole-school cross-class consistency" deferral on line 147 of `OPEN_THINGS.md`: today, generating Class A then Class B silently lets B overwrite assumptions that A's persisted placements made about teacher / room availability.

The `pinned_placements` primitive is also the carrier that Sprint C will reuse for user-pinned manual edits, so Sprint A's wire-format design must be future-proof for that consumer (see ADR 0027).

## Non-goals

Explicitly out of scope for this PR; tracked separately in OPEN_THINGS or in subsequent sprints.

- Manual editing of placements (drag-drop, swap, pin toggle): Sprint C.
- Teacher-centric and room-centric schedule views: Sprint B.
- A `pinned: bool` column on `ScheduledLesson`: Sprint C only. Sprint A treats every persisted ScheduledLesson row as auto-pinned during a per-class re-solve; user-vs-system distinction is a Sprint C concern.
- Per-class re-solve respecting the requested class's own persisted placements as pins. Sprint A re-solves the requested class fresh; only siblings are pinned. Sprint C introduces the user-pinning capability that would make self-pinning meaningful.
- Conflict-resolution UI when a sibling pin makes the requested class infeasible. Sprint A surfaces violations the existing way (toast count + `ViolationKind` records) and lets the user run "Generate all" to reseed. Auto-recovery is out of scope.
- Migration of pre-existing drifted schedules. The PR body documents the one-time-recovery story (run "Generate all" once after deploy); no backfill script ships.
- Extending the violations API or persistence model (`schedule_violations` table). Tracked separately in OPEN_THINGS Acknowledged-deferrals.

## Architecture changes

### Solver-core (`solver/solver-core/`)

#### Wire format additions

`Problem` (in `src/types.rs`) gains one additive field:

```rust
#[serde(default)]
pub pinned_placements: Vec<PinnedPlacement>,
```

New struct in the same module:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedPlacement {
    pub lesson_id: LessonId,
    pub time_block_id: TimeBlockId,
    pub room_id: RoomId,
}
```

`Violation` (in `src/types.rs`) gains one new variant:

```rust
ViolationKind::PinnedConflict {
    lesson_id: LessonId,
    reason: String,
}
```

`reason` is a short identifier (`"unknown_lesson"`, `"unknown_time_block"`, `"unknown_room"`, `"duplicate_slot"`, `"block_size_mismatch"`) so the frontend's `ViolationResponse.kind` literal in `backend/.../scheduling/schemas/schedule.py` can mirror precisely.

Pydantic `Literal[...]` widening in `backend/src/klassenzeit_backend/scheduling/schemas/schedule.py` to add `"PinnedConflict"`. The two tests in `backend/tests/scheduling/test_solver_io.py` that hardcode the closed kind set update in lockstep (per the `solver/CLAUDE.md` rule).

#### Solver behavior

Inside `solve_with_config` (`src/solve.rs`):

1. **Validation pass.** Walk `pinned_placements`; for each entry verify the lesson exists in the input, the time-block exists, the room exists, no `(time_block, room)` is double-booked across pins, and pins for the same lesson form a contiguous run of `preferred_block_size` time-blocks on the same day. Each failure pushes a `Violation { kind: PinnedConflict { lesson_id, reason }, .. }` and removes the offending pins from the active set so the rest of the solve proceeds. The solver does not abort on bad input.
2. **FFD partition.** Lessons are partitioned into `pinned: HashSet<LessonId>` (any pin entry references the lesson) and `free`. The initial Solution is seeded directly from pin entries (one Placement per pin row, including block placements split across multiple time-blocks). FFD then runs only on `free` lessons, with the existing eligibility computation seeing the pinned slots as already-occupied (because they are written into the Solution before FFD starts).
3. **LAHC enforcement.** `try_change_move` already has a one-line guard for lesson-group placements (skip the move, return without consuming the second random_range). Add a sibling guard: if `placements[placement_idx]` belongs to a pinned lesson, treat as no-op accept-or-reject identical to the lesson-group case. **Critical determinism rule** (per `solver/CLAUDE.md`): the iteration consumes both `random_range` calls (placement_idx, new_tb_idx) regardless of which guard fires; conditional draws break the property test in `tests/lahc_property.rs`.

#### Tests added in this PR

`solver-core/tests/lahc_property.rs` gains `lahc_pinned_placements_preserved` (proptest): a Problem with random pinned_placements yields a Solution where every pin appears verbatim in the placements vec.

`solver-core/src/solve.rs#[cfg(test)] mod tests` gains:

- `solve_skips_ffd_for_pinned_lesson` (greedy-only, `solve_with_config`): a pinned lesson appears in Solution at its pinned slot and is never re-placed by FFD.
- `solve_emits_pinned_conflict_for_unknown_lesson_id`: malformed pin yields `PinnedConflict` violation, real lessons still solve.

#### Bench impact

20% regression budget per `solver/CLAUDE.md` and OPEN_THINGS active-sprint policy. The added cost per LAHC iteration is one `HashSet::contains` lookup before the `random_range` calls, which fits within budget. Run `mise run bench` against the empty-pins case (today's input shape) and confirm no regression beyond the 3 percent refresh threshold. If drift is within budget but above 3 percent, refresh `BASELINE.md` via `mise run bench:record`.

### Solver-py (`solver/solver-py/`)

`solve_json` and `solve_json_with_config` are unchanged in signature; the wire format additivity means callers either include `pinned_placements` or omit it. Update the hand-maintained `.pyi` stub at `solver/solver-py/python/klassenzeit_solver/_klassenzeit_solver.pyi` to document the new field's presence in the JSON shape (the stubs document JSON shape via docstrings, not types, so this is a comment update).

New binding test `solver/solver-py/tests/test_solve_json_pinned_placements.py`:

- Round-trips a Problem JSON with one pinned_placement through `solve_json_with_config(json, deadline_ms=None)` and asserts the returned JSON's placements vec contains the pin verbatim.
- `deadline_ms=None` per the binding-contract rule in `solver/CLAUDE.md` (avoid the 200 ms LAHC default in binding tests).

### Backend (`backend/src/klassenzeit_backend/scheduling/`)

#### `solver_io.py`

New helper:

```python
async def collect_pinned_placements(
    db: AsyncSession,
    exclude_class_ids: set[uuid.UUID],
) -> list[dict]:
    """Return persisted ScheduledLesson rows for classes NOT in
    exclude_class_ids, formatted as solver wire-format pinned_placements
    entries (lesson_id, time_block_id, room_id strings).

    Empty exclude_class_ids returns every persisted placement.
    A set containing every class id returns an empty list.
    """
```

Implementation walks `ScheduledLesson` joined to `LessonSchoolClass` (a placement belongs to a class via its lesson's class membership). The output is ordered by `(lesson_id, time_block_id)` so the wire format is deterministic for testing.

`build_problem_json` signature extended:

```python
async def build_problem_json(
    db: AsyncSession,
    class_id: uuid.UUID | None = None,
    *,
    pinned_placements: list[dict] | None = None,
) -> tuple[dict, set[uuid.UUID], dict]:
```

`class_id=None` means whole-school scope. `class_lesson_ids` (the second tuple element) is the set of lesson ids that belong to `class_id` for response filtering; for whole-school scope it is the union of all classes (effectively unused; the new route does its own per-class filtering for the response breakdown). `pinned_placements` is embedded into the returned wire JSON unchanged; `None` defaults to `[]`.

#### `routes/schedule.py`

The existing `POST /api/classes/{class_id}/schedule` handler updates to:

```python
sibling_pins = await solver_io.collect_pinned_placements(db, {class_id})
problem_json, class_lesson_ids, input_counts = await solver_io.build_problem_json(
    db, class_id, pinned_placements=sibling_pins,
)
```

Everything downstream is unchanged.

New handler in the same module:

```python
@router.post("/schedule/all")
async def generate_schedule_for_all_classes(
    request: Request,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> WholeSchoolScheduleResponse:
    problem_json, _, input_counts = await solver_io.build_problem_json(
        db, class_id=None, pinned_placements=[]
    )
    deadline_ms = request.app.state.settings.solve_deadline_ms
    solution = await solver_io.run_solve(
        problem_json, scope_id=None, input_counts=input_counts, deadline_ms=deadline_ms,
    )
    summaries = await solver_io.persist_solution_for_all_classes(db, solution)
    return WholeSchoolScheduleResponse(
        classes=summaries,
        total_placements=sum(s.placements_count for s in summaries),
        total_violations=sum(s.violations_count for s in summaries),
    )
```

`scope_id` is a small refactor on `run_solve`: today it is `class_id` (used only for log line context). Renaming to `scope_id` and accepting `None` widens the surface to whole-school logging without changing semantics.

`persist_solution_for_all_classes` is new in `solver_io.py`. It:

1. Groups the solution's placements by lesson, then by class via `LessonSchoolClass`.
2. Inside a single `async with db.begin_nested()` (or the session's own transaction, depending on what the existing per-class persist does): for each class, `DELETE FROM scheduled_lessons WHERE lesson_id IN (...)` for that class's lessons, then `INSERT` the new placements. This matches the existing per-class `persist_solution_for_class` shape (delete-then-insert; last-writer-wins is acceptable per the existing OPEN_THINGS deferral on advisory locks) but does it once for every class.
3. Returns `list[ClassScheduleSummary]` with per-class `placements_count` and `violations_count` (violations attributed to a class via the lesson involved; lessons that span multiple classes count their violation once per affected class).

#### `schemas/schedule.py`

New Pydantic models:

```python
class ClassScheduleSummary(BaseModel):
    class_id: UUID
    placements_count: int
    violations_count: int

class WholeSchoolScheduleResponse(BaseModel):
    classes: list[ClassScheduleSummary]
    total_placements: int
    total_violations: int
```

`ViolationResponse.kind` literal widens to add `"PinnedConflict"`.

`mise run fe:types` regenerates `frontend/src/lib/api-types.ts` after these change.

### Frontend (`frontend/src/`)

#### `features/schedule/hooks.ts`

New hook:

```ts
export function useGenerateAllSchedules() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const res = await fetch('/api/schedule/all', { method: 'POST' });
      if (!res.ok) throw new ApiError(res.status, await res.text());
      return (await res.json()) as components['schemas']['WholeSchoolScheduleResponse'];
    },
    onSuccess: () => {
      // Invalidate every per-class schedule query so any open class re-fetches.
      queryClient.invalidateQueries({ queryKey: ['schedule'] });
    },
  });
}
```

#### `features/schedule/schedule-toolbar.tsx`

Add a primary "Generate all" button next to the existing per-class generate button. Disabled while either mutation is in flight. On success, fires `toast.success(t("schedule.generate.allSuccessToast", { classes, placements, violations }))`.

#### `i18n/en/schedule.json` and `i18n/de/schedule.json`

Three new keys under `schedule.generate.*`:

- `allAction`: "Generate all" / "Alle generieren"
- `allSuccessToast`: "Generated {{classes}} classes ({{placements}} placements, {{violations}} violations)" / "{{classes}} Klassen generiert ({{placements}} Platzierungen, {{violations}} Verstöße)"
- `allErrorToast`: "Could not generate all schedules" / "Konnte nicht alle Stundenpläne erstellen"

The existing `schedule.generate.action` key stays as the per-class "Generate" label.

## Tests

Catalog of test additions, mirroring `solver/CLAUDE.md` and `backend/CLAUDE.md` rules.

**Solver-core (Rust):**

- `tests/lahc_property.rs::lahc_pinned_placements_preserved`. Proptest: pinned entries appear unchanged in the returned Solution. RNG draw count invariant verified by the existing iteration counter.
- `src/solve.rs#[cfg(test)] mod tests::solve_skips_ffd_for_pinned_lesson`. Greedy-only.
- `src/solve.rs#[cfg(test)] mod tests::solve_emits_pinned_conflict_for_unknown_lesson_id`. Greedy-only.

**Solver-py (Python binding):**

- `tests/test_solve_json_pinned_placements.py::test_solve_json_round_trips_pinned_placement`. `deadline_ms=None`.

**Backend (pytest):**

- `tests/scheduling/test_solver_io.py::test_collect_pinned_placements_excludes_target_class`. Seeds two classes, persists placements for both, calls `collect_pinned_placements(db, {class_a_id})`, asserts only class B's placements come back.
- `tests/scheduling/test_solver_io.py::test_collect_pinned_placements_returns_empty_when_all_excluded`.
- `tests/scheduling/test_solver_io.py::test_build_problem_json_threads_pinned_placements_into_wire_format`. The wire JSON contains the pins verbatim.
- `tests/scheduling/test_solver_io.py::test_count_violations_by_kind_includes_pinned_conflict`. Existing test file's closed-enum coverage extends to the new variant.
- `tests/seed/test_demo_grundschule_dreizuegig_whole_school_schedule.py::test_post_schedule_all_persists_every_class`. Runs `POST /api/schedule/all` against the fixture; asserts every class has placements_count == its expected hours and total_violations == 0 (the fixture solves clean today).
- `tests/seed/test_demo_grundschule_dreizuegig_whole_school_schedule.py::test_per_class_resolve_preserves_sibling_persisted_placements`. After `POST /api/schedule/all`, takes a snapshot of every class's persisted placements, calls `POST /api/classes/{first_class}/schedule`, asserts every OTHER class's persisted placements are byte-identical to the snapshot.

**Frontend (Vitest):**

- `frontend/src/features/schedule/schedule-toolbar.test.tsx::renders_generate_all_button_and_posts_to_schedule_all_endpoint`. Mocks `fetch`, clicks the new button, asserts the POST is to `/api/schedule/all` and the toast renders with the correct interpolated copy.

Existing tests stay green without modification (the wire field is additive, the new route is additive, the per-class route's behavior for non-drifted persisted siblings is identical because pin enforcement matches what the solver was already doing for in-flight placements).

## Verification

Pre-PR local runs:

- `mise run lint` (covers ruff, ty, vulture, clippy, machete, cargo fmt, biome, actionlint, unique-fns).
- `mise run test:rust` (cargo nextest, includes the new property and unit tests).
- `mise run test:py` (backend integration tests against the dreizügige fixture).
- `mise run fe:test` (Vitest for the new toolbar test).
- `mise run bench` and compare against `BASELINE.md`. Pin enforcement adds ~one HashSet lookup per LAHC iteration; expected drift well within the 20% budget. Refresh BASELINE only if drift exceeds 3 percent.
- `uv run pytest solver/solver-py/tests` (binding contract tests).

Pre-push pre-commit hook runs the full test suite per `.config/lefthook.yaml`. CI runs the same plus the duration-budget gate.

Manual smoke (after the PR is up): start backend + frontend with the dreizügige seed loaded, click "Generate all", confirm three classes' schedules appear in the UI, then click per-class "Generate" on Class 1 and confirm Class 2 and Class 3's schedules in the UI are unchanged.

## Risks and trade-offs

- **One-time recovery for users with drifted persisted schedules.** Anyone running the old per-class flow before this PR ships may have sibling overlap in their persisted placements. After this PR, the first per-class re-solve they run surfaces those overlaps as `PinnedConflict` violations rather than silently overwriting. Mitigation: PR body and ADR 0027 document the recovery story (run "Generate all" once). Staging is the only deployment that has any persisted schedules today; the operator runs "Generate all" as part of the deploy.
- **Whole-school solve under tight wall-clock budget.** The default `solve_deadline_ms=200` covers per-class problems comfortably; whole-school is N times bigger. Production callers may need to raise the deadline (or the existing `KZ_SOLVE_DEADLINE_MS` env knob). The dreizügige fixture (102 lessons, 294 placements) solves within 200 ms today on the recording host (per `BASELINE.md`); larger fixtures land in Sprints 2 and 6 and may force a deadline bump. Tracked under OPEN_THINGS as a follow-up if benches surface a problem.
- **Last-writer-wins on `POST /api/schedule/all`.** Two concurrent clicks would interleave their delete-then-insert transactions. Mitigation: the existing per-class advisory-lock deferral applies here too; not introducing a hot-path lock in this PR. If a demo surfaces it, a `pg_advisory_xact_lock(hashtext('schedule-all'))` is the canonical fix.
- **Response shape divergence.** Per-class `POST` returns full placements + violations; whole-school `POST` returns slim summaries. Frontends that want full per-class data on the whole-school path must call the per-class GET after the POST. This is intentional (avoids a large response on a frequent action) but tracks as a possible future cleanup if a consumer surfaces.
- **`build_problem_json` signature widening.** Adding `pinned_placements` as a kwarg to a 200+ line function increases its blast radius slightly. Compensating control: every existing caller passes `None` (default), so behavior is unchanged unless explicitly opted in; the new helpers `collect_pinned_placements` and `persist_solution_for_all_classes` keep the orchestration cohesive without folding more SQL into `build_problem_json`.
