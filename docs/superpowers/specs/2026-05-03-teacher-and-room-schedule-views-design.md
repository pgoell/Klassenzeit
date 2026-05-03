# Teacher- and room-centric schedule views (Sprint B)

**Date:** 2026-05-03
**Status:** Spec
**Sprint:** Scheduling UX, Sprint B (queued after Sprint A `feat: whole-school generate + pinned-placement re-ingestion`, before Sprint C manual editing).
**Brainstorm:** `/tmp/kz-brainstorm/brainstorm.md` (autonomous run)

## Summary

Add read-only "where is teacher X all week" and "where is room R all week" views to `/schedule`, alongside the existing class view. Backend grows two GET routes that return the same `ScheduleReadResponse` shape filtered by teacher or room. Frontend grows a tab strip on the schedule page driven by a `view` search param (`class | teacher | room`), with a per-view entity picker and three sibling page-body components sharing the existing `ScheduleGrid` renderer unchanged. No solver change, no schema change, no migration.

## Goals

- Closes the OPEN_THINGS Sprint B description verbatim: `GET /api/teachers/{id}/schedule` and `GET /api/rooms/{id}/schedule` plus tabs on `/schedule` (Class / Teacher / Room) plus per-view entity selector.
- Reuses `ScheduleGrid` (`frontend/src/features/schedule/schedule-grid.tsx`) without touching its render code.
- Preserves the existing `?class=<id>` URL contract: legacy bookmarks open the class view.
- Keeps wire format minimal: same `ScheduleReadResponse` (placements only) for all three reads.

## Non-goals

- No drag-and-drop, no manual editing, no pinning. That ships in Sprint C.
- No new wire format fields; no backend OpenAPI shape change beyond the two new route entries.
- No Playwright spec in this sprint. The feature is read-only and the existing class-view smoke spec already exercises the shared grid.
- No multi-day or single-day filtering; all three views render the existing weekly grid.
- No "Generate from teacher view" or "Generate from room view" affordance. Generate stays a class operation; Generate-all stays a school-wide button.

## Backend design

### Two new GET routes in `scheduling/routes/schedule.py`

```python
@router.get("/teachers/{teacher_id}/schedule")
async def read_schedule_for_teacher_route(
    teacher_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> ScheduleReadResponse:
    placements = await solver_io.read_schedule_for_teacher(db, teacher_id)
    return ScheduleReadResponse(placements=placements)


@router.get("/rooms/{room_id}/schedule")
async def read_schedule_for_room_route(
    room_id: uuid.UUID,
    _admin: Annotated[User, Depends(require_admin)],
    db: Annotated[AsyncSession, Depends(get_session)],
) -> ScheduleReadResponse:
    placements = await solver_io.read_schedule_for_room(db, room_id)
    return ScheduleReadResponse(placements=placements)
```

### Two new helpers in `scheduling/solver_io.py`

```python
async def read_schedule_for_teacher(
    db: AsyncSession,
    teacher_id: UUID,
) -> list[PlacementResponse]:
    """Return persisted placements where Lesson.teacher_id matches.

    Raises HTTPException(404) if the teacher does not exist.
    """
    teacher = await db.get(Teacher, teacher_id)
    if teacher is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Teacher not found")
    rows = (
        (await db.execute(
            select(ScheduledLesson)
            .join(Lesson, Lesson.id == ScheduledLesson.lesson_id)
            .where(Lesson.teacher_id == teacher_id)
        ))
        .scalars()
        .all()
    )
    return [PlacementResponse(lesson_id=r.lesson_id, time_block_id=r.time_block_id, room_id=r.room_id) for r in rows]


async def read_schedule_for_room(
    db: AsyncSession,
    room_id: UUID,
) -> list[PlacementResponse]:
    """Return persisted placements where ScheduledLesson.room_id matches."""
    room = await db.get(Room, room_id)
    if room is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Room not found")
    rows = (
        (await db.execute(
            select(ScheduledLesson).where(ScheduledLesson.room_id == room_id)
        ))
        .scalars()
        .all()
    )
    return [PlacementResponse(lesson_id=r.lesson_id, time_block_id=r.time_block_id, room_id=r.room_id) for r in rows]
```

### Schema reuse

`ScheduleReadResponse` (`scheduling/schemas/schedule.py`) is reused as-is. No new Pydantic types.

### 404 contract

Each route raises `HTTPException(404, "Teacher not found")` / `"Room not found"` if the entity is missing. Empty placements list (entity exists, no scheduled lessons referencing it) returns 200 with `placements: []`. Mirrors the existing `read_schedule_for_class_route` semantics.

### Routing wiring

`scheduling/routes/__init__.py` already mounts `schedule.router` on the API. Adding the two new routes inside the same router file requires no router-level wiring change.

## Frontend design

### URL search-param schema

`frontend/src/routes/_authed.schedule.tsx` extends the Zod schema:

```ts
const scheduleSearchSchema = z.object({
  view: z.enum(["class", "teacher", "room"]).optional(),
  class: z.string().min(1).optional(),
  teacher: z.string().min(1).optional(),
  room: z.string().min(1).optional(),
});
```

Default behaviour: when `view` is absent, fall back to `"class"`. Legacy `?class=<id>` URLs continue to render the class view unchanged.

### Component layout

```
features/schedule/
  schedule-page.tsx              # shell: header + tabs + view-aware Toolbar; picks one view child
  schedule-page-class-view.tsx   # extracted body of today's SchedulePage (unchanged behaviour)
  schedule-page-teacher-view.tsx # new
  schedule-page-room-view.tsx    # new
  schedule-toolbar.tsx           # view-aware: per-view picker label + per-view options; hide per-class Generate on non-class tabs
  schedule-grid.tsx              # unchanged
  schedule-status.tsx            # used only by the class view (violations are class-scoped)
  hooks.ts                       # adds useTeacherSchedule, useRoomSchedule
```

The shell:

```tsx
export function SchedulePage() {
  const { t } = useTranslation();
  const search = useSearch({ strict: false }) as { view?: string; class?: string; teacher?: string; room?: string };
  const view = (search.view ?? "class") as "class" | "teacher" | "room";
  return (
    <div className="space-y-5">
      <SchedulePageHeader title={t("schedule.title")} subtitle={t("schedule.subtitle")} />
      <ScheduleTabs active={view} />
      {view === "class" ? <SchedulePageClassView /> : null}
      {view === "teacher" ? <SchedulePageTeacherView /> : null}
      {view === "room" ? <SchedulePageRoomView /> : null}
    </div>
  );
}
```

`ScheduleTabs` is a thin shadcn `Tabs`-styled component (or a tab-strip-of-`<Link>`s if pure shadcn `Tabs` would force controlled state into the shell). Each tab navigates to `/schedule` with the new `view` and clears the irrelevant entity params.

### Per-view bodies

Each `*View` component owns:

- One detail hook (`useClassSchedule(classId)` / `useTeacherSchedule(teacherId)` / `useRoomSchedule(roomId)`).
- One picker shadcn `Select` populated from `useSchoolClasses` / `useTeachers` / `useRooms` (the latter two already exist).
- One cell-builder mapping placements → `ScheduleCell[]` with the view-specific secondary line.

The class view body is mostly today's `SchedulePage` body extracted verbatim (the Toolbar wiring, the empty-state CTA, the `ScheduleStatus`, the loading / error guards). No behaviour change in commit 2 of the chain.

### `ScheduleCell` evolution

```ts
export interface ScheduleCell {
  key: string;
  day: number;
  position: number;
  subjectName: string;
  classNames?: string;          // new: undefined for class view
  teacherName?: string;         // unchanged shape; remains undefined for teacher view
  roomName: string;
}
```

Each builder fills the secondary slots that are not the view's primary filter:

- Class view: sets `teacherName` and `roomName`. (Today's behaviour.)
- Teacher view: sets `classNames` (joined `Lesson.school_classes[].name`) and `roomName`.
- Room view: sets `classNames` and `teacherName`.

The renderer stays exactly:

```tsx
<span className="text-[10px] text-muted-foreground">
  {[cell.classNames, cell.teacherName, cell.roomName].filter(Boolean).join(" · ")}
</span>
```

(The current renderer joins `[teacherName, roomName]`. Extending the array to `[classNames, teacherName, roomName]` is a one-line edit and stays backward-compatible because `classNames` is undefined in the class view.)

### New hooks

```ts
export function teacherScheduleQueryKey(teacherId: string) {
  return ["schedule", "teacher", teacherId] as const;
}

export function useTeacherSchedule(teacherId: string | undefined) {
  return useQuery({
    enabled: Boolean(teacherId),
    queryKey: teacherId ? teacherScheduleQueryKey(teacherId) : ["schedule", "teacher", "disabled"],
    queryFn: async (): Promise<ScheduleGetResponse> => {
      if (!teacherId) throw new ApiError(400, null, "useTeacherSchedule called without teacherId");
      const { data } = await client.GET("/api/teachers/{teacher_id}/schedule", {
        params: { path: { teacher_id: teacherId } },
      });
      if (!data) throw new ApiError(500, null, "Empty response from GET /teachers/{id}/schedule");
      return data;
    },
  });
}

// Symmetric useRoomSchedule.
```

`useGenerateAllSchedules`'s existing `invalidateQueries({ queryKey: ["schedule"] })` already cascades to the new keys (the prefix `["schedule", ...]` matches).

### Toolbar evolution

`ScheduleToolbar` becomes view-aware:

```tsx
interface ScheduleToolbarProps {
  view: "class" | "teacher" | "room";
  // class-view-specific (optional unless view === "class"):
  classes?: SchoolClass[];
  classId?: string;
  onClassChange?: (id: string) => void;
  onGenerate?: () => void;
  onCancelConfirm?: () => void;
  placementsCount?: number;
  confirming?: boolean;
  pending?: boolean;
  // teacher-view-specific:
  teachers?: Teacher[];
  teacherId?: string;
  onTeacherChange?: (id: string) => void;
  // room-view-specific:
  rooms?: Room[];
  roomId?: string;
  onRoomChange?: (id: string) => void;
}
```

The Toolbar renders the picker for the active view, hides per-class Generate on non-class tabs, and keeps Generate-all visible on every tab. The replace-confirmation banner only renders on the class view (it carries class-scoped semantics).

### Empty / loading states

- Class view: existing `EmptyState` with three steps + `Generate` CTA. Unchanged.
- Teacher view: short `<p className="text-sm text-muted-foreground">{t("schedule.empty.teacherBody")}</p>` when no teacher is selected. When a teacher is selected and has zero placements, render the same blank weekly grid plus a one-line muted explanation referencing the existing `t("schedule.empty.title")`.
- Room view: symmetric.

No "Generate" CTA on Teacher / Room empty states.

### i18n keys

Added to `en.json` and `de.json`:

```json
"schedule": {
  "tabs": { "class": "Class", "teacher": "Teacher", "room": "Room" },
  "picker": {
    "class": { "label": "Class", "placeholder": "Select a class…", "none": "Select a class to view its schedule." },
    "teacher": { "label": "Teacher", "placeholder": "Select a teacher…", "none": "Select a teacher to view their week." },
    "room":    { "label": "Room", "placeholder": "Select a room…", "none": "Select a room to view its week." }
  },
  "empty": {
    "teacherBody": "Select a teacher above to see their weekly placements.",
    "roomBody": "Select a room above to see its weekly placements."
  }
}
```

The flat `schedule.picker.label` / `schedule.picker.placeholder` / `schedule.picker.none` keys are removed; their only consumer is the Toolbar, which becomes view-aware in the same commit.

## Tests

### Backend

`backend/tests/scheduling/test_schedule_routes.py` (new file alongside the existing class-route tests; or extend the existing test module if it covers the GET route already):

- `test_read_schedule_for_teacher_returns_placements`: seed one class with one scheduled lesson, call `GET /api/teachers/{id}/schedule`, assert one placement returned with the right `lesson_id` / `time_block_id` / `room_id`.
- `test_read_schedule_for_teacher_404_when_missing`: random UUID returns 404.
- `test_read_schedule_for_teacher_empty_when_no_placements`: seed a teacher with no scheduled lessons, expect 200 + empty list.
- `test_read_schedule_for_room_returns_placements`: symmetric.
- `test_read_schedule_for_room_404_when_missing`: random UUID returns 404.
- `test_read_schedule_for_room_empty_when_no_placements`: symmetric.

Auth: each test goes through the existing admin client fixture (mirrors `read_schedule_for_class_route` tests).

### Frontend

`frontend/src/features/schedule/hooks.test.tsx` (extended):

- Existing tests stay.
- New: `useTeacherSchedule` happy path + `useRoomSchedule` happy path (MSW handlers stub one placement each).

`frontend/src/features/schedule/schedule-page-teacher-view.test.tsx` (new):

- Renders the teacher picker with the seeded teachers.
- Selecting a teacher fetches their schedule and renders cells with `subject · classNames · roomName`.
- Empty state renders the muted body when no teacher is selected.

`frontend/src/features/schedule/schedule-page-room-view.test.tsx` (new): symmetric.

`frontend/src/features/schedule/schedule-page.test.tsx` (extended): new "tab navigation" test asserts that switching from `?view=class` to `?view=teacher` swaps the visible body component. Existing class-view assertions stay.

MSW handlers: extend `tests/msw-handlers.ts` with `GET /api/teachers/{id}/schedule` and `GET /api/rooms/{id}/schedule` returning a small fixture (one placement each).

### Lint, typecheck, build

- `mise run lint` (ruff, ty, biome, vulture, machete, clippy, actionlint).
- `mise run fe:types` regenerates `frontend/src/lib/api-types.ts` after the backend routes land.
- `mise run fe:build` then `cd frontend && mise exec -- pnpm exec tsc --noEmit` (per frontend CLAUDE.md, the build is required before strict typecheck so the route tree regenerates).
- `mise run test:py` and `mise run fe:test` cover the new tests.

## Commit chain

1. **`feat(backend): GET /api/teachers/{id}/schedule and /api/rooms/{id}/schedule`.** Routes + two `solver_io` helpers + integration tests + `mise run fe:types` regen lands in this commit so the frontend can use the typed `client.GET(...)` calls in the next step. Pre-commit lint stays green.
2. **`refactor(frontend): extract class-view body from SchedulePage`.** No behaviour change. Move the existing `SchedulePage` body into `schedule-page-class-view.tsx`. The shell keeps the page header + Toolbar wiring; the class-view body component takes over the rest. Existing `schedule-page.test.tsx` continues to pass without modification.
3. **`feat(frontend): teacher and room schedule views`.** Adds tab strip, `ScheduleTabs`, view-aware Toolbar, two new view bodies, two new hooks, MSW handlers, i18n keys. New Vitest specs cover the new components.
4. **`docs: close Sprint B in OPEN_THINGS and update auto-memory`.** No ADR. The URL-discriminator pattern is conventional TanStack Router search-param usage; no new dependency, no schema change, no wire-format change. The spec + the OPEN_THINGS Sprint-B closure note are the durable record.

Each commit ships green lint + green tests; pre-push runs the full suite.

## Risks and mitigations

- **Risk: per-view picker dropdowns get crowded with hundreds of teachers.** Mitigation: out of scope for Sprint B. The dreizügige Grundschule fixture has eight teachers; the gesamtschule fixture (Sprint 6) will have fifty. Add picker search / filter when scale demands it.
- **Risk: classNames join exceeds the cell line width when a Religion merge group spans three classes.** Mitigation: rely on existing `text-[10px]` and `leading-tight` plus CSS truncation via `text-ellipsis` if the cell overflows. Visual-regression check during browser verification.
- **Risk: legacy `?class=<id>` bookmarks break.** Mitigation: explicit fallback (`view ?? "class"`) plus an integration test asserting the legacy URL renders the class view.
- **Risk: lessons without `teacher_id` (the in-test edge case where seed code adds `Lesson` rows directly without auto-assign) appear in placements but don't filter into the teacher view.** Mitigation: production paths always have `teacher_id` set by `auto_assign_teachers_for_lessons`; the seed-only edge case stays out of scope.

## Open questions

None. The brainstorm self-answered all ten questions; the spec captures the chosen path for each.
