# Klassenzeit frontend: do-not-do list

Stack: Vite + React 19, TanStack Router + Query, shadcn/ui, React Hook Form + Zod, react-i18next, next-themes. On top of `.claude/CLAUDE.md`.

**Invoke `frontend-design` via the `Skill` tool before writing UI** when the task is "build a page", "redesign this component", or similar. Skipping it is a process violation.

## Layout (`frontend/src/`)

- `routes/` — TanStack Router file-based routes; thin, import page components from `features/`.
- `features/<name>/` — feature-scoped React code (page, hooks, schema); cross-feature imports are a smell.
- `components/ui/` — shadcn primitives (generated).
- `components/` — app-level composites (`theme-toggle.tsx`, `language-switcher.tsx`, `layout/app-shell.tsx`).
- `lib/` — cross-cutting utilities (`api-client.ts`, `auth.ts`, `utils.ts`).
- `i18n/` — `config.ts`, `init.ts`, `locales/{en,de}.json`, `types.d.ts`.
- `styles/app.css` — Tailwind entry + token definitions (`:root`, `.dark`).
- `routeTree.gen.ts`, `lib/api-types.ts` — generated; do not hand-edit.

## Commands

- `mise run fe:dev` (dev server on `:5173`, proxies API to `:8000`); `fe:test` / `fe:test:cov` (Vitest with/without coverage); `fe:cov:update-baseline` (rebaseline); `fe:types` (regenerate `src/lib/api-types.ts` from backend OpenAPI, offline); `fe:build` / `fe:typecheck` (build / `tsc --noEmit`; pre-push runs typecheck, CI's `Frontend CI / Lint + test + build` is required).
- **Adding dependencies:** `mise exec -- pnpm -C frontend add [-D] <pkg>`; don't hand-edit `package.json`.
- **Single test file:** `cd frontend && mise exec -- pnpm vitest run <path>`. Don't use `pnpm -C frontend vitest ...` (filter loses vitest); don't use `mise run fe:test -- --run` (positional args land in a shell `if`).
- **TanStack Router: build before typecheck.** New `src/routes/*.tsx` fails `tsc --noEmit` until the Router Vite plugin regenerates `src/routeTree.gen.ts`. Run `fe:build` (or `fe:dev`) before local typecheck.
- **Run `fe:typecheck` locally before pushing when touching array indexing, narrowing, or after a backend-driven `fe:types` regen.** Biome and Vitest's esbuild type-stripping do not type-check.
- **`fe:types` regen that adds a REQUIRED field cascades to all consumers.** Request-body side: grep `*CreateRequest|*UpdateRequest` consumers (form submit handlers in `features/<name>/*-grid.tsx` etc.) and add the field to the body literal; default to a safe initial value if the form does not yet expose a control. Response side: MSW `HttpResponse.json({...})` handler bodies in `tests/msw-handlers.ts` are untyped against response schemas, so widening a `*Response` requires manually adding EVERY required field (not just the new one) to each handler that returns that shape; otherwise `fe:typecheck` blocks at the call-site type assertion. Land all callers in the same commit as the regen.

## Hooks and state

- **No `useEffect` for derived state** (compute during render; sync via `key` or derive inline; enforced by `scripts/check_use_effect_sync.py`). No `useState` for data you can recompute. No defensive `useMemo` / `useCallback`. No `forwardRef` in new components (React 19 treats `ref` as a plain prop). No array index as `key` (define `const SLOTS = [...]` and key by the string). `useEffect` mount gate is acceptable only for third-party sync (e.g. `next-themes`).
- **Draft-from-fetch editors use outer/inner.** Outer fetches and returns `null` while loading; inner takes the entity as a prop and seeds local draft via `useState(() => seedFrom(entity))`. Pattern: `features/rooms/room-availability-grid.tsx`.
- **Verify a shadcn primitive exists in `frontend/src/components/ui/` before importing it.** New ones require `mise exec -- pnpm -C frontend add @radix-ui/react-<name>` plus pasting the canonical shadcn body.
- **`enabled: id !== null` hooks need `id || null` coercion when the id source is a form field.** RHF + Radix Select emit `""` for unset state, not `null`. Coerce at the call site.

## Server state and routing

- **No fetching in `useEffect` + `useState`** (use TanStack Query and the typed `client`). No direct `fetch` (`client` throws `ApiError` on error responses; inspect `err.status` / `err.data` in `onError`). No `useNavigate` for in-app links (use `<Link>`).
- **No local state for filter / sort / page / selection** that a user would want to share or refresh. Use TanStack Router search params via `useSearch` + Zod `validateSearch`. Page components reading them use `useSearch({ strict: false })` with a typed cast.
- **Mutation hooks throw `ApiError(status, body, message)`** on empty / errored responses. Pattern: singleton `client.PATCH/POST(...)`, throw on `!data`, `onSettled: () => queryClient.invalidateQueries({ queryKey: ["schedule"] })`.
- **Polling hooks for transient resources coerce 404 to `null`,** not throw. A polled GET against a resource that may not exist yet (e.g. `useScheduleProgress` against `/schedule/progress` before the backend has registered the in-flight solve) catches `ApiError` with `err.status === 404` in the `queryFn` and returns `null`; the hook stays `enabled` and re-polls. Re-throw every other status. Pattern: `useScheduleProgress` (`features/schedule/hooks.ts`).
- **Cross-tab UI state reads from the polled snapshot, not local mutation state.** When two browser tabs (or a reload) can observe the same in-flight operation, drive labels and disabled flags from server-snapshot fields (`snapshot.cancel_requested`), not the local `mutation.isPending`. A mid-stop reload of the second tab still sees `Stopping...` because the server flag survives the tab. Pattern: `generate-in-progress.tsx`'s `stopping` derivation.
- **MSW base URL in Vitest is `http://localhost:3000/api/...`.** `useWeekSchemes` list response omits `time_blocks`; only `useWeekSchemeDetail` carries them.

## Forms (RHF + Zod)

- **No uncontrolled to controlled flipping** (seed `defaultValues` with `""`, not `undefined` or `null`). No submit button without `disabled={isPending}` and a pending label. No raw input boxes (use shadcn primitives with the `Form` wrapper). No Zod `.email("msg")` literals for user-facing errors (go through `t("...")`; the message lookup lives in `FormMessage` children). No Zod `.uuid()` for FK form fields (Zod v4 enforces RFC 4122; pattern-UUIDs fail; use `z.string().min(1)`).
- **Keep Zod schemas flat for RHF forms.** `@hookform/resolvers` v5 + RHF v7 + Zod v4 fail to type-check on `.coerce`, `.union`, `.transform`, `.default` because resolver input and output diverge. Do coercion in the form `onChange` and submit handlers.
- **Required-but-nullable Zod fields need a `defaultValues` seed of `null`** (required-with-null-as-valid is not optional).
- **Radix `<Select>` rejects `value=""`; use a sentinel.** Define a local `const NULL_VALUE = "__none__"` and translate to/from `null` at the field boundary.

## Styling

- **No inline hex / OKLCH literals,** no Tailwind arbitrary values (except for one-off spacing where no token fits), no `!important`, no `style={{...}}` for colors or spacing. Use tokens defined in `src/styles/app.css`.
- **Canonical design tokens live in `frontend/DESIGN.md`.** Update DESIGN.md in the same commit when changing a semantic token (primary/secondary/tertiary/neutral/accent/destructive, typography, radius, documented component role) in `app.css`. Implementation-detail tokens (chart-N, sidebar-*) are CSS-only.

## i18n

- **No string concatenation of translated fragments;** use interpolation. **No hardcoded plurals** (use i18next `_one` / `_other`). **No hardcoded user-visible English or German;** every JSX text, `aria-label`, placeholder, toast, and error string goes through `t("...")` with entries in both locales.
- **No date or number formatting with `toString()`.** Use `Intl.*Format` seeded from `i18n.language`.
- **`t()` keys are typed against `en.json`** via `src/i18n/types.d.ts`; renames break call sites at type-check time. Locale JSON lives at `src/i18n/locales/{en,de}.json`, not `src/locales/`.
- **No template-literal keys at call sites.** Build `{ key, label }` arrays with literal keys, or extract a helper that returns a template-literal type (see `src/i18n/day-keys.ts`).
- **Grep `locales/en.json` before assuming a sibling i18n key exists.** Adjacent fields can live in different sub-trees (e.g. `teachers.columns.maxHoursPerWeek` exists but `teachers.fields.maxHoursPerWeek` does not). Adding a new key under an assumed parent silently inflates `en.json` with an unused namespace; verify the namespace your call site reads.

## Accessibility

- **No click handlers on `<div>` or `<span>`** (use `<Button variant="ghost">` or a real `<a>` / `<Link>`). **No color-only signaling** (pair with icon or text). **No dialogs without `DialogTitle` / `DialogDescription`.** **No dynamic content without `aria-live`** for toasts, root errors, async status.
- **Checkbox lists need `htmlFor` to keep labels clickable.** Assign an `id` to the Checkbox and render the label as a sibling `<label htmlFor={id}>`.

## TypeScript

- **No `as Foo` assertions** where a type guard or union would narrow. **No `any`** (prefer `unknown` with a guard). **`erasableSyntaxOnly` only:** no enums, no parameter properties, no namespaces, no `import =`.

## Testing

- **No snapshot tests as the primary assertion;** no `queryBy*` for async content (use `findBy*`); no mocking of `client` or hooks (MSW handles the network boundary); no `data-testid` when a role or accessible name exists.
- **Radix primitives need Pointer Events polyfills in jsdom** (`hasPointerCapture`, `setPointerCapture`, `releasePointerCapture`, `scrollIntoView`); they live in `tests/setup.ts`. The trigger has `role="combobox"`; options render in a Radix portal (use `screen.findByRole("option", ...)`).
- **`userEvent.click(submit)` after a Radix Select may silently no-op** because Radix leaves `pointer-events: none` on `body` briefly. For validation-rejection assertions, use `fireEvent.submit(form)` (`form = nameInput.closest("form")`).
- **shadcn primitives bake default classes alongside your `className`.** Read primitive source in `src/components/ui/` before choosing a class for "applies only to X" assertions.
- **Same translated string in two places breaks `getByText`** (disambiguate with `getAllByText` + filter, or rename a key). Stacked Radix Dialogs count as multiple `role="dialog"` nodes; wait for invalidation before counting.
- **sonner 2.x only renders a `<section>` live region on mount.** Test mount-only behaviour via `document.querySelector('section[aria-label="Notifications alt+T"]')`.
- **Sub-resource MSW handlers need mutable per-test state.** Export a mutable `Record<parentId, Array<child>>` from `tests/msw-handlers.ts` and reset in `beforeEach`.
- **Non-bookable cell variants use an early-return at the top of the cell render path.** When a grid cell has a non-interactive variant (break, header, time-axis), branch at the top with a `data-variant="..."` element instead of threading the variant flag through the drag-source / drop-target / click-affordance decisions downstream. Pattern: `schedule-grid.tsx` break cell.
- **Test utilities live at `frontend/tests/render-helpers.tsx`.** `renderWithProviders` is async; for pure-UI components use a local `QueryClientProvider` wrapper and `render` directly (see `wrapRoomDialog` in `rooms-dialogs.test.tsx`). It mounts at `/` with no `initialEntries`; pages reading TanStack Router search params need `createMemoryHistory({ initialEntries: [...] })` + `createRouter(...)` + `<RouterProvider />` (see `schedule-page.test.tsx`). It returns the `QueryClient`; destructure when a test drives `queryClient.invalidateQueries(...)`.
- **Component tests querying English labels must pin the locale:** `i18n.changeLanguage("en")` in `beforeAll`. Use `<entity>DetailQueryKey(id)` helpers when invalidating queries.
- **Form-button selectors in dialog tests need anchored regex.** `getByRole("button", { name: /save/i })` matches the primary submit AND sibling sub-resource editor "Save qualifications" / "Save availability" buttons; use `/^save$/i` to pick the form's primary button only.
- **Class-view `runScheduleGenerate` is a two-step confirm when placements exist.** When the test seeds placements before clicking, the toolbar's "Generate schedule" button only flips `confirming=true`; the actual `POST /schedule` fires on the confirm banner's "Generate anyway" button. Anchor both clicks: `/^Generate schedule$/i` then `/^Generate anyway$/i`. A bare `/generate/i` matches several siblings (`Re-solve respecting my pins`, `Generate all from scratch`) and silently no-ops.
- **`vi.useFakeTimers()` without `toFake` hangs `waitFor`.** Use `{ toFake: ["Date"] }`.
- **Frontend coverage ratchet** fails CI below `.coverage-baseline-frontend` or the 50% floor; rebaseline with `mise run fe:cov:update-baseline`. MSW handlers are required for every endpoint (`tests/setup.ts` starts `setupServer` with `onUnhandledRequest: "error"`).

## UX conventions

- **Toasts via `sonner`** (`toast.success` / `toast.info` / `toast.error`); `<Toaster />` is mounted in `main.tsx` and `tests/render-helpers.tsx`. Keep `form.setError("root", ...)` for in-dialog validation errors. Error toasts need dedicated i18n keys (do not reuse button-label keys).
- **No placeholder UI that looks like real data;** render a skeleton, empty state, or nothing until the real query resolves.
- **Extend library UX through its API,** not document-level listeners (sonner: `toastOptions.onClick` / `onDismiss`; Radix: `onOpenChange`, `data-state`, controlled props). Don't override library defaults without a reason; name the constant with a one-line comment.

## Bundle

**No `import * as Icons from "lucide-react"`** (named imports only). **No dynamic `import()`** (the Router plugin handles route-level code splitting).
