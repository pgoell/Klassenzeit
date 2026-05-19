# 0048: Super-admin cross-school write audit log

## Status

Accepted (2026-05-18).

## Context

PR #291 (item 10h) restricted `/schools/*` CRUD to super-admins. The session-active-school model (ADR 0046) lets a super-admin operate on any school by switching `session.active_school_id`. The combined surface mutates every tenant's data plus the tenant catalogue with no persisted record of who acted on what. OPEN_THINGS 10g specified capturing this trail; the trigger phrasing "compliance asks for the trail" implied queryable, retainable storage.

## Decision

A new `super_admin_audit_log` table records one row per super-admin write where the actor's authorization derives from `is_super_admin` elevation. The trigger predicate captures writes when:

- the user is super-admin AND
- the method is POST / PATCH / PUT / DELETE AND
- EITHER the route is under `/api/schools` (always super-admin-only),
- OR the target school is not in the user's home school plus direct memberships.

Capture is a single starlette `BaseHTTPMiddleware` that runs after the handler. The middleware is self-sufficient: it reads the `kz_session` cookie, looks up the session + user + active school in its own AsyncSession (borrowed via `app.dependency_overrides[get_session]` so tests inherit the per-test savepoint), and inserts the row. Audit failure logs to JSON but never 500s the user.

Snapshot columns (`actor_user_email`, `target_school_name`) preserve the trail across `ON DELETE SET NULL` on `users(id)` and `schools(id)`. Body capture is JSONB capped at 64 KiB with a truncation flag.

Read API + UI shipped as part of follow-up 10g.1: `GET /api/auth/admin/audit-log` (super-admin-gated, paginated, filterable by actor / target school / time range) and a `/audit-log` React page rendering the list view. psql remains available for ad-hoc queries. Detail-drawer rendering of JSONB blobs shipped via item 10g.1b with server-side redaction of sensitive keys in request bodies.

### Mechanism details

Two implementation details depart from the surface the predicate would suggest in isolation, and are part of the design rather than incidental:

- **`/api/auth/*` is exempt from audit.** The middleware adds an `_AUDIT_EXEMPT_PREFIXES = ("/api/auth/",)` short-circuit before predicate evaluation. `/api/auth/switch-school` is the canonical case: it is a self-action against the actor's own session, not a tenant-aggregate write, but it always carries a target school that is non-member when a super-admin switches into another tenant's scope. Without the exemption the predicate would fire on every switch and conflate identity actions with tenant writes. Login, logout, and password change fall under the same category.
- **`DELETE /api/schools/{school_id}` writes `target_school_id=None` and relies on `target_school_name` as the only trail.** The school row is deleted by the handler in the same AsyncSession used for the audit insert; inserting the audit row with `target_school_id = <orphan UUID>` immediately violates the `schools(id)` FK. The middleware fetches the school's name BEFORE the handler runs (`pre_target_name`) and stores it in the snapshot column; the FK column is set to `None` rather than the orphan UUID. The `ON DELETE SET NULL` clause on the FK covers the symmetric retention case for non-DELETE rows.

## Rationale

Middleware over per-handler explicit calls: ~30 existing write routes plus future ones means per-handler audit calls drift. The middleware sees every route automatically.

Self-sufficient middleware over dependency-stashing: avoids editing `get_current_user` / `get_scope_school_id` signatures. Two extra indexed UUID lookups per super-admin write are negligible.

DB table over logs: compliance retention needs queryable storage independent of log rotation. JSON logs remain in place for debugging.

Separate transaction over same-transaction: a log-insert failure must not roll back the user's actual write.

Principled trigger (super-admin elevation actually used) over literal "scope != home": post-ADR-0046, a super-admin who is also a regular member of school B writing to B is acting in admin capacity; capturing that is noise.

## Consequences

- Every super-admin elevated write performs two extra DB lookups (session, user) plus one insert. Acceptable given the rarity of super-admin requests.
- New write routes do not need explicit audit wiring; the middleware applies the predicate by route template.
- A 64 KiB body cap is enforced. Larger bodies are stored truncated with a flag; readers must check the flag.
- Audit-row read access: list view shipped (item 10g.1). Detail-drawer rendering JSONB blobs (`path_params`, `request_body`) shipped as 10g.1b with redaction.
- DELETE rows lose the `target_school_id` FK link by design; queries reconstructing "which school did this row target" join on the snapshot name or fall through to the snapshot alone.

## Alternatives considered

- **Per-handler `await record_audit(...)`.** Rejected: forgettable, drifts with new routes.
- **FastAPI dependency that inserts the row.** Rejected: runs before the handler, can't see response status.
- **SQLAlchemy event listener on `Session.flush()`.** Rejected: no route / actor context.
- **Structured logs only.** Rejected: no retention guarantee beyond log rotation.

## Anchor

- OPEN_THINGS items 10g (closed by this ADR), 10g.1 (read API + UI; shipped), and 10g.1b (detail-drawer with redaction; shipped).
- Builds on ADR 0045 (tenancy decisions) and ADR 0046 (multi-school membership + session-active-school).
- Coordinates with item 10h (PR #291) super-admin gate on `/schools/*`.
