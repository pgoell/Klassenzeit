# 0044: Schwimmunterricht travel buffer enforcement

- **Status:** Accepted
- **Date:** 2026-05-16

## Context

Hessen Grundschulen schedule one Doppelstunde Schwimmunterricht per Klasse 3 in an offsite Schwimmbad. Children and the begleitende Lehrkraft need 10-15 minutes Wegezeit each way between the school building and the pool. The reference WeekScheme places Hofpausen of 15-20 minutes after the second and fourth period, which lines up naturally with the Wegezeit envelope.

Today the solver has no representation of "this lesson needs N minutes of travel before / after". The FFD greedy and the LAHC move sites can place a non-Schwimmen lesson immediately before or after a Schwimmen Doppelstunde on the same class or the same teacher's schedule, producing a literal conflict (the class is "in" two places at once). The validator sextet in `solver-core/src/validate.rs` has no axis covering this shape, so the violation surfaces only as a downstream class- or teacher-non-overlap symptom after the lesson is moved.

## Decision

Persist three new fields and enforce a discrete one-slot rule in every solver path.

- `Room.is_external: bool` (informational only, does not enter the solver wire format).
- `Lesson.pre_buffer_minutes: int` and `Lesson.post_buffer_minutes: int` (0-60, default 0).
- New `ViolationKind::TravelBufferConflict` and post-condition validator `validate_travel_buffer` in `solver-core/src/validate.rs`. The validator septet (was sextet) runs at the tail of `solve_with_config_stats`.
- Discrete one-slot rule: when `pre_buffer_minutes > 0` for a placed lesson at day-position `p`, the time block at position `p-1` on the same day must either have `kind == Break` (Hofpause, by `validate_no_lesson_on_break_slot` no lesson lands there) OR carry no placement for the lesson's class AND no placement for the lesson's teacher. Symmetric for post-buffer. A lesson with `pre_buffer_minutes > 0` cannot be placed at day-position 0. The minute value itself is informational at the solver level (any non-zero value imposes the one-slot rule); minutes are persisted for display and for a future continuous-minute upgrade.
- FFD (`solve.rs::try_place_block`) and every LAHC move site (`try_change_move_n1`, `try_change_block_move`, `try_swap_move`, Kempe chain destination) prune candidates through a shared `would_violate_travel_buffer` predicate. R&R recreate inherits the pruning through `try_place_block`.
- CP-SAT mirrors the constraint as a hard implication on the anchor variables in `solver-py/python/klassenzeit_solver/cpsat.py`.

## Alternatives considered

- **Continuous-minute math via `TimeBlock.duration_minutes`.** Rejected: a 50-plus-site wire-format cascade across solver-core, solver-py, CP-SAT, backend solver_io, and bench fixtures. The discrete one-slot rule matches Hessen Grundschule reality (10-15 min Wegezeit aligned with 15-20 min Hofpause).
- **Couple `Room.is_external` to buffer enforcement.** Rejected as orthogonal facts: a room can be external without a Wegezeit requirement (a nearby Turnhalle owned by the Verein), and a lesson can require Wegezeit without an external room flag (in-building moves between widely separated wings). Keeping the two fields decoupled keeps the predicate explicit at the Lesson level.
- **First-slot-of-day grace via pre-school-day buffer.** Rejected for today's reference data (school start at 7:45-8:15 leaves no realistic pre-school pocket). Filed as a parking-lot follow-up.

## Consequences

- The schedule UI gains a Wegezeit hint on Schwimmen cells and an external-room icon next to Schwimmbad in the room list.
- The dreizügige Ganztagsschule bench fixture extends with one Klasse 3a Schwimmen Doppelstunde (placement count 294 to 296). Einzügig and zweizügig fixtures untouched.
- Buffered lessons at day-position 0 are rejected outright. Schools wanting first-period Schwimmen need the parking-lot first-slot grace work.
- `solver/CLAUDE.md`'s "sextet" wording rolls to "septet" in lockstep with this commit's validator addition.
- Wire-format additivity preserved: both new Lesson fields use `#[serde(default)]`, so older payloads round-trip cleanly.
- Parking lot: continuous-minute math, external-room coupling, first-slot-of-day grace, three-cohort Schwimmen fixture expansion, production-budget BENCH_RESULTS.md refresh.

## Anchors

- ADR 0013 (typed solver violations).
- ADR 0018 (solver Doppelstunden support).
- ADR 0030 (CP-SAT objective mirror).
- ADR 0040 (TimeBlock.kind for break slots).
- OPEN_THINGS item 8 closed by this PR.

## Amendment (2026-05-20)

The "First-slot-of-day grace via pre-school-day buffer" item originally listed
under "Alternatives considered" and "Consequences > Parking lot" is implemented.

`Problem` gains a scalar `pre_first_slot_grace_minutes: u8` field (additive
wire format, `#[serde(default)]`). The validator and hot-path predicate
relax the `pos == 0` rejection for `pre_buffer_minutes > 0` lessons when
`pre_first_slot_grace_minutes >= lesson.pre_buffer_minutes`. CP-SAT mirrors
the relaxation per-lesson at model-build time. Default grace=0 preserves
the original reject-all semantic.

`WeekScheme` gains a matching `pre_first_slot_grace_minutes: SmallInteger`
column (NOT NULL, server_default=0). Pydantic clamps writes to 0-60. The
solver_io passthrough reads the column and stamps the field on the wire
format.

The remaining "Alternatives considered" entries (`Continuous-minute math`,
`Couple Room.is_external to buffer enforcement`) stay parking-lot. The
"last-slot of day post-buffer" relaxation is intentionally NOT shipped:
there is no customer signal, and the physical model (Hort/daycare) differs
from the pre-school pocket the pre-buffer side addresses.
