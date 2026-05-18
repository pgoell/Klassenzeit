# 0046: Multi-school membership

**Status:** Accepted (2026-05-18)

## Context

ADR 0045 introduced single-school tenancy with `users.school_id` as the
NOT NULL FK and a per-request super-admin `?school_id=<uuid>` override
on `get_scope_school_id`. The override is a stop-gap; OPEN_THINGS item
10c calls for true M:N membership so a contracted instructional coach
can switch between schools the same way they switch projects in any
other multi-tenant SaaS.

## Decision

- `user_school_memberships` join table (`user_id`, `school_id`,
  `created_at`, UNIQUE(user_id, school_id)) records *additional*
  school access. The home school stays on `users.school_id`.
- `user_sessions.active_school_id` is NOT NULL, defaulted at session
  create to the user's home school, mutable through
  `POST /auth/switch-school`.
- `get_scope_school_id` reads `session.active_school_id`. The
  `?school_id=<uuid>` URL pattern is removed.
- `/auth/me` returns `active_school_id`, `active_school_name`, and
  `accessible_schools: [{id, name}]` in addition to the home school's
  fields.
- Super-admins' `accessible_schools` returns every school in the
  catalog (unbounded; pagination is a P3 follow-up if the catalog
  grows beyond ~50).
- Frontend sidebar swaps the static school-name span for a shadcn
  `<Select>` picker when `accessible_schools.length > 1`. Selecting a
  different school POSTs to `/auth/switch-school` and clears the
  TanStack Query cache to refresh every scoped view.
- Access validation runs at session-create and at switch time. There
  is no per-request re-validation; revocation requires a re-login
  until the admin grant/revoke endpoint (10j) ships.

## Consequences

- One source of truth for the request scope (the session row),
  removing the parallel `?school_id=` path.
- Home school FK on users stays, so the 9 tenanted aggregates and
  their tests need no migration.
- Cache invalidation on switch is broad (`queryClient.clear()`); users
  see brief loading states across the app. This is deliberate:
  selective invalidation across all aggregate queries is more brittle
  than a full reset.
- Admin endpoints for granting / revoking memberships are out of
  scope. Operators provision multi-school access via psql until
  item 10j ships, mirroring how super-admin promotion works today.
- Supersedes the per-request override decision in ADR 0045's
  2026-05-18 super-admin addendum.
