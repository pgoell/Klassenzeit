# 0042: Soft pin semantic as a canonical-score axis

- **Status:** Accepted
- **Date:** 2026-05-14

## Context

ADRs 0027 and 0028 established the pin primitive as a hard constraint: `Problem.pinned_placements` is a list the solver MUST honour, FFD seeds pinned lessons verbatim, LAHC's `try_change_move` skips pinned lessons, and the persisted `ScheduledLesson.pinned: bool` column survives every re-solve. The contract has one user intent: "this lesson MUST stay here." A Klassenlehrer wanting "I prefer this placement, but reroute it if you must" had no way to express it; the closest workaround was dropping the pin entirely, which lost the signal. OPEN_THINGS item 5 carried this gap since the Sprint C ship and the "soft / hard pin distinction" alternative in ADR 0028 explicitly deferred it to a future ADR.

## Decision

Pin intent gets a third state. `ScheduledLesson.pin_kind: PinKind | None` replaces the binary `pinned: bool` column with three states (`hard`, `soft`, `null`). The solver wire format gains `PinnedPlacement.kind: PinKind` with `#[serde(default)]` defaulting to `Hard`; callers omitting the field continue to deserialise as today's hard-only behaviour. `ConstraintWeights.soft_pin_miss: u32` becomes a new canonical-score axis; `PRODUCTION_ACTIVE_WEIGHTS.soft_pin_miss = 5` mirrors `prefer_home_room` as a tentative starting weight. `validate_pins` partitions pin entries by `kind`: hard pins continue to seed FFD and block LAHC; soft pins skip FFD seeding and instead populate a `HashSet<(LessonId, TimeBlockId)>` carried on `GreedyState`. `score_solution` counts, per call, the soft-pinned `(lesson_id, time_block_id)` pairs not present in the candidate solution and adds `weights.soft_pin_miss * miss_count` to the running score. LAHC's `try_change_move` recomputes `state.canonical_score` at the accept site (supervision-spread precedent, ADR 0041) rather than threading a per-move delta. The CP-SAT scorer mirrors the term with one bool var per soft pin so cross-backend bench cells compare on the same scalar. `QualityReport.soft_pin_misses: u32` surfaces the raw count to the backend. The frontend cycles one schedule-grid button through three states (null to hard to soft to null) with two `Pin` icon variants (filled / outlined) and two cell tint tokens.

This ADR extends ADRs 0027 and 0028 without superseding either: the wire-format primitive is preserved (callers still send `pinned_placements`), and the manual-pin auto-set on move and swap continues to write `pin_kind = 'hard'`. Users soften a pin explicitly through the UI cycle.

## Alternatives considered

- **Per-move LAHC delta for soft pins.** A Change move toggles at most one `(lesson_id, time_block_id)` membership in the placement set; a delta would be O(1). Rejected on consistency grounds: supervision spread (ADR 0041) shipped on the recompute path and the `try_change_move` accept-site `score_solution` recompute already costs O(placements) per accept. Adding a second delta path would fork the canonical-score invariant maintenance into two shapes for marginal gain at today's problem scale.
- **Bool column with a sentinel "soft" string.** Keep `pinned: bool` and add a sibling `pin_soft: bool` column. Rejected because the three states are mutually exclusive (a row is unpinned, hard-pinned, or soft-pinned, never two at once); a single enum encodes the constraint at the schema level.
- **Soft pins as `respect_pins=false` semantics.** Re-purpose the existing flag to mean "treat all pins as soft on this run." Rejected because the flag's documented contract (ADR 0028) is per-run "ignore the pin set on input"; the soft-pin intent is per-pin, not per-run. Conflating them would break the recovery path the flag exists for.
- **Weight learned from a customer corpus.** Defer shipping until a real Klassenlehrer corpus surfaces a preference signal to fit the weight against. Rejected because the binary-pin gap is the immediate UX blocker; tentative weight 5 mirrors `prefer_home_room`'s tuning and is ratifiable opportunistically (OPEN_THINGS P2 follow-up).

## Consequences

Easier: a Klassenlehrer expresses "preferred-not-required" without losing the signal; the score axis surfaces the cost of overruling the preference in `QualityReport.soft_pin_misses`; the wire format stays backwards-compatible so older callers continue to send hard-only pins; the recompute-path precedent from ADR 0041 generalises cleanly to a second aspirational axis.

Harder: `ConstraintWeights` widens by one axis and every struct-literal site fixes up; `QualityReport` widens by one field and the backend `expected_fields` frozen set updates in lockstep; the CP-SAT scorer must mirror the new objective term exactly for cross-backend parity (pinned by `test_cpsat_soft_pin.py`); `respect_pins` remains a hard-only skip-set flag and its docstring clarifies that soft pins are honoured as a penalty axis regardless of the flag's value. The tentative weight 5 ships unratified at production budget; an opportunistic 20-iter 60s x 20 seeds x 4 fixtures bench refresh confirms it does not regress soft-score competitive shape (OPEN_THINGS P2 follow-up).

Revisit if a customer school surfaces a workflow needing solver-time soft-pin chain semantics (move-with-anchor where a moved soft-pinned lesson transfers its pin to the swap target), if a Klassenlehrer corpus suggests a weight other than 5, or if a future Quality-issue UI surface (OPEN_THINGS item 6) needs a dedicated tile for the new axis.
