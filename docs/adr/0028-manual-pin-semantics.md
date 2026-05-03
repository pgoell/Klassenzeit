# 0028: Manual pin semantics

- **Status:** Accepted
- **Date:** 2026-05-03

## Context

Sprint A introduced `Problem.pinned_placements` as the wire-format primitive for both auto-pinned siblings (during per-class re-solve) and user-pinned manual edits (Sprint C, [ADR 0027](0027-pinned-placements-wire-format.md)). Sprint C lands the manual-edit surface: `PATCH /api/placements/{id}` (move), `PATCH /api/placements/{id}/pin` (toggle), `POST /api/placements/swap` (atomic swap), and a frontend drag-and-drop layer via `@dnd-kit/core`. Open question once those exist: what does a pin actually mean? A hint the solver may overrule, or a hard guarantee the user can rely on? And what happens to pin state across re-solves?

## Decision

A pin is a hard constraint, not a hint. Manual move and swap auto-set `pinned=true` on every affected `ScheduledLesson` row; the user moving a placement is the user pinning it. The `pinned` flag survives all re-solves regardless of the `respect_pins` flag value: "from scratch" is per-run (the solver ignores the pin set on input) but never destructive of the persisted pin state. `POST /api/schedule/all` defaults `respect_pins=true`; the explicit "Generate all from scratch" toolbar button passes `respect_pins=false`. Per-class re-solve respects own-class pins in addition to siblings' persisted placements (`collect_own_class_pins` + `collect_all_pins` in `solver_io`).

The persist-helper invariant: `persist_solution_for_class` and `persist_solution_for_all_classes` union the input pin set with the existing DB pin set before deleting and re-inserting rows. A pin survives a re-solve as long as its placement survives, regardless of the `respect_pins` flag.

## Consequences

Any caller that previously expected from-scratch behaviour from `POST /api/schedule/all` must now pass `respect_pins=false` explicitly. The frontend toolbar splits this into two buttons ("Re-solve respecting my pins" and "Generate all from scratch") and is the only in-tree caller. External clients break loudly, which is the right failure mode for a semantic change.

## Alternatives considered

- **A new `/respect-pins` endpoint.** Two routes for two flag values doubles the surface area without adding expressivity; rejected.
- **Always respect pins; require explicit unpin.** Forces a multi-step "unpin everything" workflow before a clean from-scratch solve; rejected because the from-scratch case is a real recovery path (drifted historic data).
- **Soft / hard pin distinction.** A second `pinned_kind` column with a soft variant the solver scores against. Rejected: no Sprint-C use case needs it, and the score axis would have to be tuned per fixture; filed under "Acknowledged deferrals" for a future ADR.
