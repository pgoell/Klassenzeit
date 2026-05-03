# Collapse `<entity>.columns.actions` i18n keys into `common.actions`

**Date:** 2026-05-03
**Status:** Design approved (autopilot autonomous mode).

## Context

The active "Schwimmen + Sek-I foundations" sprint's tidy phase (item 6 in `docs/superpowers/OPEN_THINGS.md`) calls out `<entity>.columns.actions` drift across the frontend i18n catalog:

> Each entity page passes `actionsHeader={t("subjects.columns.actions")}` (and rooms / teachers / schoolClasses / stundentafeln / lessons all do the same), each resolving to "Actions" / "Aktionen". Carve out `common.actions` in en + de catalogs; migrate the seven call sites in one pass.

Ripgrep against current `master` (commit `fd641b5`) finds eight `t("…columns.actions")` call sites and nine catalog entries. The OPEN_THINGS undercount ("seven") matches the same shape as the EntityListTable refactor PR's undercount: the prose missed the two sub-table sites (`time-blocks-table.tsx` inside the WeekScheme detail dialog and the entries table in `stundentafeln-dialogs.tsx`) and there is one dead catalog key (`weekSchemes.columns.actions`) with no call site, presumably orphaned when WeekSchemes' list page got replaced by the master/detail body.

`frontend/src/i18n/types.d.ts` types `t()` against `en.json`'s shape, so removing the per-entity keys raises a typecheck error at every missed call site. There are no component or e2e specs that assert on the literal "Actions" / "Aktionen" string.

## Goal

Single commit, single PR. Eliminate "Actions" header drift across the catalog by introducing one shared key.

1. Add `"actions"` to `common.*` in both locale catalogs:
   - `frontend/src/i18n/locales/en.json`: `"actions": "Actions"`.
   - `frontend/src/i18n/locales/de.json`: `"actions": "Aktionen"`.
2. Migrate all eight `t()` call sites to `t("common.actions")`:
   - `frontend/src/features/subjects/subjects-page.tsx`
   - `frontend/src/features/rooms/rooms-page.tsx`
   - `frontend/src/features/teachers/teachers-page.tsx`
   - `frontend/src/features/school-classes/school-classes-page.tsx`
   - `frontend/src/features/lessons/lessons-page.tsx`
   - `frontend/src/features/stundentafeln/stundentafeln-page.tsx`
   - `frontend/src/features/week-schemes/time-blocks-table.tsx`
   - `frontend/src/features/stundentafeln/stundentafeln-dialogs.tsx`
3. Remove all nine stale `actions` entries from both catalogs (eight migrated keys + the dead `weekSchemes.columns.actions`).

Visible UI is byte-identical: still "Actions" in en, "Aktionen" in de, in the same column header positions.

## Why one commit

The change is purely structural at every step. There is no behavioural change to peel off, so the project's "structural and behavioural changes never ship in the same commit" rule does not require a split. Splitting the catalog edit from the call-site migration only produces an intermediate commit where both old and new keys coexist, which adds review surface without adding clarity.

## Where `common.actions` lives in the catalog

A flat key under `common.*` next to the existing `cancel` / `save` / `edit` / `delete` / `import` family. The existing `common.*` namespace already mixes verbs and nouns (`search`, `position`, `start`, `end`) without subgrouping, so introducing a `common.columns.*` namespace for one member would be over-engineered. The "column-header" role is conveyed at call sites by the `actionsHeader` prop name on `EntityListTable`, not by the key path.

## Non-goals

- **No new vitest spec asserting `i18n.t("common.actions")` resolves correctly.** The typed catalog (`types.d.ts`) plus the existing component specs (which render entity pages and would surface a missing translation as a raw key path on screen) plus `tsc --noEmit` are sufficient. A standalone i18next-resolution test would be testing the library, not our code.
- **No `mutationErrorKey(entity, action)` helper.** That belongs to the separate sprint item 7 (toast error fallback audit), not this one.
- **No ADR.** No load-bearing decision; one i18n key collapse.
- **No browser-driven verification.** Behaviour is preserved (same rendered string, same column position); the existing component test suite plus the typed `t()` resources are the safety net. Per the frontend rule, behaviour-preserving refactors may skip the dev-server step when the existing tests cover the rendered output.

## Verification

- `cd frontend && mise exec -- pnpm exec tsc --noEmit` passes; the typed `t()` resources catch any missed call site as a build error.
- `mise run fe:test` passes (existing entity-page specs render the headers and would flag a raw key path).
- `mise run lint` passes (Biome, untranslated-string scripts).
- `mise run e2e` is unaffected; no spec asserts on the "Actions" string.
- After the migration, `rg -n 'columns\.actions' frontend/src` returns zero matches and `rg -n 'common\.actions' frontend/src` returns eight matches.
