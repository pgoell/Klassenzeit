# 0047: Reference-data tenancy: subjects per-school, pin-kind global enum

## Status

Accepted (2026-05-18).

## Context

`OPEN_THINGS.md` item 10f deferred a tenancy decision on two pieces of reference data: `Subject` and `PinKind`. ADR 0045 (multi-school tenancy) opted into per-school FKs on every scheduling aggregate, including `Subject`, but flagged that "reference data" might warrant a shared catalog if customer onboarding revealed material homogeneity. The 10a Subject aggregate ship (PR #280) physically implemented `subjects.school_id` NOT NULL FK plus composite uniqueness `UNIQUE(school_id, name)` and `UNIQUE(school_id, short_name)`; the per-school direction is in production. `PinKind` has been a global `enum.StrEnum {HARD, SOFT}` from its introduction; the values encode universal solver semantics (hard constraint vs soft cost in `solver-core`).

The decision was deferred pending a customer-onboarding trigger ("a school whose subject list differs materially from the default"). No customer trigger has fired, but the in-flight tenancy program is closing out and the question deserves a recorded answer rather than an indefinite deferral.

## Decision

1. **Subject stays per-school.** The `subjects` table keeps its `school_id` NOT NULL FK and composite uniqueness. Each school maintains its own subject list end-to-end. No shared catalog and no override layer.
2. **PinKind stays a global `enum.StrEnum {HARD, SOFT}`.** Not promoted to a per-school table. The carrier row (`ScheduledLesson.pin_kind`) is itself per-school via `school_id`; the enum type encodes universal solver semantics that schools do not customise.

## Consequences

- Each school maintains its own subject catalog. Operator-created subjects do not benefit from a shared seed; bootstrapping a new school still requires manual subject CRUD (or seed-script use for the demo Grundschule).
- Solver wire format and `solver-core` constraint handling are unchanged. The two pin semantics remain stable.
- A future customer school cannot tell the system "use the same subjects as school A" without explicit copy support. That capability can be added later as a per-school operation; it is not a shared catalog.

## Alternatives considered

- **Shared subject catalog with per-school activation.** Rejected: customer schools have material variation in short_names (Hessen "SU" for Sachunterricht; other Länder differ), colors, and preference-weight defaults. A shared catalog forces homogenisation or a heavy override layer. The per-school infrastructure is already shipped; a rollback adds risk for zero near-term value.
- **Per-school PinKind table.** Rejected: HARD and SOFT are universal solver semantics, not school-customisable labels. Schools differing on what "hard" means would fragment the solver wire format and confuse the soft-cost objective.
- **Auto-seed a starter subject set on `POST /schools`.** Out of scope for this ADR. Surface as a backlog item only if onboarding friction is reported.

## Triggers to revisit

- A customer school formally objects to maintaining its own subject catalog AND a second customer requests identical subjects (for the shared-catalog flip).
- A future feature requires more than two pin semantics in a school-customisable way (for the PinKind-as-table flip).
