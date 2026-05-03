# `common.actions` i18n Key Collapse Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Carve out one shared `common.actions` i18n key, migrate eight call sites, remove nine stale per-entity entries from both locale catalogs.

**Architecture:** Single commit, structural refactor only. Visible UI is byte-identical (still "Actions" / "Aktionen"). The typed `t()` resources (`frontend/src/i18n/types.d.ts` re-exporting `en.json`'s shape) are the safety net: `tsc --noEmit` flags any call site that references a removed key.

**Tech Stack:** React 19, TanStack Router, react-i18next, i18next-typed catalog (`types.d.ts`).

**Spec:** [`docs/superpowers/specs/2026-05-03-common-actions-i18n-design.md`](../specs/2026-05-03-common-actions-i18n-design.md).

---

### Task 1: Add `common.actions` to both locale catalogs

**Files:**

- Modify: `frontend/src/i18n/locales/en.json` (add `"actions": "Actions"` inside `common.*`)
- Modify: `frontend/src/i18n/locales/de.json` (add `"actions": "Aktionen"` inside `common.*`)

- [ ] **Step 1: Insert `actions` into `common.*` in `en.json`**

Place the new key after `"open": "Open",` (before the `daysShort` block) so the action verbs (`cancel`, `save`, `edit`, `delete`, `import`, `open`, `actions`) cluster at the top of `common.*`.

```json
    "open": "Open",
    "actions": "Actions",
    "daysShort": {
```

- [ ] **Step 2: Insert `actions` into `common.*` in `de.json`**

```json
    "open": "Öffnen",
    "actions": "Aktionen",
    "daysShort": {
```

- [ ] **Step 3: Verify both catalogs parse and the new key exists**

Run:

```bash
jq '.common.actions' frontend/src/i18n/locales/en.json
jq '.common.actions' frontend/src/i18n/locales/de.json
```

Expected output:

```
"Actions"
"Aktionen"
```

If either returns `null` or jq errors with "parse error", the catalog is malformed; fix the JSON syntax before proceeding.

- [ ] **Step 4: Confirm typecheck still green**

Run:

```bash
cd frontend && mise exec -- pnpm exec tsc --noEmit
```

Expected: zero errors. The new key extends the `t()` resource shape; existing call sites are unaffected.

---

### Task 2: Remove all nine stale `actions` entries from both catalogs (TDD red phase)

This task is the "red" step. After it lands, `tsc --noEmit` will report eight type errors (one per surviving call site) until Task 3 migrates them. Do not commit between Task 2 and Task 3; the catalog and call sites move together as one structural change.

**Files:**

- Modify: `frontend/src/i18n/locales/en.json` (remove eight migrated `"actions": "Actions"` entries plus the dead `weekSchemes.columns.actions`)
- Modify: `frontend/src/i18n/locales/de.json` (mirror the removals; values are `"Aktionen"`)

- [ ] **Step 1: Remove `subjects.columns.actions` from both catalogs**

In `en.json` find the `subjects.columns` block:

```json
    "columns": {
      "name": "Name",
      "shortCode": "Code",
      "actions": "Actions"
    },
```

Drop the `"actions"` line and the trailing comma on the previous line:

```json
    "columns": {
      "name": "Name",
      "shortCode": "Code"
    },
```

Mirror in `de.json` (value was `"Aktionen"`).

- [ ] **Step 2: Remove `rooms.columns.actions` from both catalogs**

Same shape: drop the `actions` line and fix the trailing comma on the line above it. Confirm by running `jq '.rooms.columns' frontend/src/i18n/locales/en.json` and inspecting the result no longer contains `actions`.

- [ ] **Step 3: Remove `teachers.columns.actions` from both catalogs**

Same shape.

- [ ] **Step 4: Remove `weekSchemes.columns.actions` from both catalogs (dead key)**

Same shape. This key has no call site; removing it is pure cleanup.

- [ ] **Step 5: Remove `weekSchemes.timeBlocks.columns.actions` from both catalogs**

Note the deeper nesting (`weekSchemes.timeBlocks.columns`).

- [ ] **Step 6: Remove `schoolClasses.columns.actions` from both catalogs**

- [ ] **Step 7: Remove `lessons.columns.actions` from both catalogs**

- [ ] **Step 8: Remove `stundentafeln.columns.actions` from both catalogs**

- [ ] **Step 9: Remove `stundentafeln.entries.columns.actions` from both catalogs**

Note the deeper nesting (`stundentafeln.entries.columns`).

- [ ] **Step 10: Verify catalogs parse and all stale keys are gone**

Run:

```bash
jq '.. | objects | select(.actions == "Actions")' frontend/src/i18n/locales/en.json
```

Expected: only the `common` block (with `cancel`, `save`, `edit`, `actions`, etc.) emits — that single match is the legitimate `common.actions` entry plus its sibling action verbs. No nested `columns: { actions: "Actions" }` block should appear.

Same check on `de.json` with `"Aktionen"`:

```bash
jq '.. | objects | select(.actions == "Aktionen")' frontend/src/i18n/locales/de.json
```

- [ ] **Step 11: Run typecheck to confirm the eight expected failures (red phase)**

Run:

```bash
cd frontend && mise exec -- pnpm exec tsc --noEmit
```

Expected: eight errors of the shape `Argument of type '"<entity>.[…].columns.actions"' is not assignable to parameter of type '<TFunctionDetailedResult union>'`. The eight call sites are listed in the spec (Goal section). If the count is not exactly eight, ripgrep again (`rg -n 'columns\.actions' frontend/src`) and reconcile.

---

### Task 3: Migrate all eight call sites to `t("common.actions")` (TDD green phase)

Each step modifies one file. Use exact string replacement; the call sites are identical across files except for the key path.

**Files:**

- Modify: `frontend/src/features/subjects/subjects-page.tsx:97`
- Modify: `frontend/src/features/rooms/rooms-page.tsx:98`
- Modify: `frontend/src/features/teachers/teachers-page.tsx:112`
- Modify: `frontend/src/features/school-classes/school-classes-page.tsx:128`
- Modify: `frontend/src/features/lessons/lessons-page.tsx:134`
- Modify: `frontend/src/features/stundentafeln/stundentafeln-page.tsx:102`
- Modify: `frontend/src/features/stundentafeln/stundentafeln-dialogs.tsx:292`
- Modify: `frontend/src/features/week-schemes/time-blocks-table.tsx:85`

- [ ] **Step 1: Migrate `subjects-page.tsx`**

Replace:

```tsx
            actionsHeader={t("subjects.columns.actions")}
```

with:

```tsx
            actionsHeader={t("common.actions")}
```

- [ ] **Step 2: Migrate `rooms-page.tsx`**

Replace `t("rooms.columns.actions")` with `t("common.actions")` in the `actionsHeader` prop.

- [ ] **Step 3: Migrate `teachers-page.tsx`**

Replace `t("teachers.columns.actions")` with `t("common.actions")` in the `actionsHeader` prop.

- [ ] **Step 4: Migrate `school-classes-page.tsx`**

Replace `t("schoolClasses.columns.actions")` with `t("common.actions")` in the `actionsHeader` prop.

- [ ] **Step 5: Migrate `lessons-page.tsx`**

Replace `t("lessons.columns.actions")` with `t("common.actions")` in the `actionsHeader` prop.

- [ ] **Step 6: Migrate `stundentafeln-page.tsx`**

Replace `t("stundentafeln.columns.actions")` with `t("common.actions")` in the `actionsHeader` prop.

- [ ] **Step 7: Migrate `stundentafeln-dialogs.tsx` (entries sub-table)**

This site is inside a nested `<TableHead>`, not an `actionsHeader` prop. Replace:

```tsx
                      {t("stundentafeln.entries.columns.actions")}
```

with:

```tsx
                      {t("common.actions")}
```

- [ ] **Step 8: Migrate `time-blocks-table.tsx` (WeekScheme sub-table)**

This site is inside a nested `<TableHead>` (line 85). Replace:

```tsx
                  {t("weekSchemes.timeBlocks.columns.actions")}
```

with:

```tsx
                  {t("common.actions")}
```

- [ ] **Step 9: Run typecheck to confirm green phase**

Run:

```bash
cd frontend && mise exec -- pnpm exec tsc --noEmit
```

Expected: zero errors.

- [ ] **Step 10: Confirm zero stale `columns.actions` references remain**

Run:

```bash
rg -n 'columns\.actions' frontend/src
```

Expected: no output.

Run:

```bash
rg -n 'common\.actions' frontend/src
```

Expected: eight matches across the eight files listed above.

---

### Task 4: Verification sweep

- [ ] **Step 1: Run frontend Vitest suite**

Run:

```bash
mise run fe:test
```

Expected: all suites pass. Existing entity-page specs render the table headers; if any one of them suddenly sees the raw key string `common.actions` on screen, the catalog change failed and `findByRole`/`getByText` assertions on neighbour columns will surface it.

- [ ] **Step 2: Run full lint**

Run:

```bash
mise run lint
```

Expected: pass. Biome inspects the migrated `.tsx` files; the catalog edits go through Biome's JSON formatter via `mise run fmt` if anything is off.

- [ ] **Step 3: Re-run typecheck explicitly**

Run:

```bash
cd frontend && mise exec -- pnpm exec tsc --noEmit
```

Expected: zero errors.

---

### Task 5: Commit

- [ ] **Step 1: Stage and commit**

```bash
git add frontend/src/i18n/locales/en.json \
        frontend/src/i18n/locales/de.json \
        frontend/src/features/subjects/subjects-page.tsx \
        frontend/src/features/rooms/rooms-page.tsx \
        frontend/src/features/teachers/teachers-page.tsx \
        frontend/src/features/school-classes/school-classes-page.tsx \
        frontend/src/features/lessons/lessons-page.tsx \
        frontend/src/features/stundentafeln/stundentafeln-page.tsx \
        frontend/src/features/stundentafeln/stundentafeln-dialogs.tsx \
        frontend/src/features/week-schemes/time-blocks-table.tsx
git commit -m "$(cat <<'EOF'
refactor(frontend): collapse columns.actions into common.actions

Carve out `common.actions` ("Actions" / "Aktionen") in en + de catalogs
and migrate eight call sites that previously each owned their own
`<entity>.[…].columns.actions` entry resolving to the same string. Drop
the dead `weekSchemes.columns.actions` key while in the neighbourhood.

Visible UI unchanged: same string, same column position. Closes the
sprint tidy-phase item; the typed `t()` resources catch any missed call
site as a build error.
EOF
)"
```

The pre-commit hook runs lint and typecheck again; expect green.
