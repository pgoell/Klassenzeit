# FFD lock-in diagnostic spec (Sprint diagnostic phase, items 1 + 2)

**Sprint.** Solver same-room reliability + Grundschule quality bar (active).
**Phase.** Diagnostic (items 1 + 2 of `docs/superpowers/OPEN_THINGS.md` active sprint).
**Goal.** Land a deterministic Rust reproducer of the FFD `no_suitable_room` flake on the demo Grundschule, and a feature-gated trace of FFD's inner-loop decisions, so the Decision phase (item 3) can pick Path A / B / C with evidence rather than speculation.

**Non-goal.** No behaviour change. No fix to FFD. No removal of the integration `xfail`. No ADR. No solver-py or backend changes.

## Context

PR #171 landed the same-room hard constraint (each `(class, day, subject)` triple must use one room). Two integration tests were left `xfail`-ed because FFD greedy on `demo_grundschule` flakes ~50% of runs with `ViolationKind::NoSuitableRoom`. Per `solver/CLAUDE.md` L44 the failure mode is: FFD locks an early `(class, day, subject) → room` choice, then a later placement of the same-day same-subject hour for another class cannot find a room because every candidate is held by a sibling class's lock. LAHC cannot escape (LAHC moves accepted placements, it does not re-place violations).

The flake is non-deterministic on the seed because `auto_assign_teachers_for_lessons` ties on subject UUIDs, and subject UUIDs are random per `seed_demo_grundschule` invocation. In Rust we hand-pin everything; one chosen teacher allocation will reliably trigger the lock-in or reliably avoid it.

## Scope

**In scope.**
- New `#[test] fn ffd_locks_in_on_demo_grundschule_and_returns_no_suitable_room()` in `solver/solver-core/tests/same_room_property.rs`. Builds a Grundschule-shaped Problem with hand-pinned teacher allocation. Asserts the solution carries at least one `ViolationKind::NoSuitableRoom`. Sibling assertion pulls the locked `(class, day, subject)` triple out of the violation and asserts it matches the expected one (so future regressions where the lock-in moves also surface).
- New `solver-trace` feature in `solver/solver-core/Cargo.toml`'s `[features]` table. Default off.
- New `solver/solver-core/src/trace.rs` module, fully gated by `#[cfg(feature = "solver-trace")]`. Exposes `pub(crate) fn ffd_trace(...)` with a stable line format (see below).
- Trace call sites inside `solve.rs::try_place_block` at every `continue`, every `best = Some(...)`, and the terminal "no candidate window" + commit branches. Each call site is itself wrapped in `#[cfg(feature = "solver-trace")]` so production builds compile to the same machine code as today.

**Out of scope.**
- `try_place_group` and `lahc::run`: demo Grundschule never enters either; the trace stays narrow per "Don't add features beyond what the task requires".
- Any backend / solver-py / Python change.
- Decision (Path A / B / C). Item 3 owns it.

## Reproducer fixture

Builder `ffd_lock_in_grundschule()` lives next to `same_room_grundschule()` in the test file. It mirrors `backend/src/klassenzeit_backend/seed/demo_grundschule.py` at fixed deterministic UUIDs (`Uuid::from_bytes([n; 16])` style, same `same_room_uuid` helper):

| Entity | Count | Detail |
|---|---|---|
| `time_blocks` | 35 | 5 days × 7 periods, ids `100..134`. |
| `school_classes` | 4 | 1a-4a, ids `70..73`. Each has `home_room_id` = own Klassenraum. |
| `rooms` | 7 | Klasse 1a..4a (ids `50..53`, suit academic subject set), Turnhalle (`54`, only Sport), Musikraum (`55`, only MU), Kunstraum (`56`, only KU). |
| `subjects` | 9 | D, M, SU, E, ETH, KU, MU, SP, FÖ (the union of the grades-1-2 and grades-3-4 hours tables; RK and RE from the seed are dropped because no class teaches them). Per-subject preference fields mirror the seed (D and M `prefer_early_period=1, avoid_last_period=1`; SP `avoid_first_period=1`; rest 0). |
| `teachers` | 6 | MUE, SCH, WEB, FIS, BEC, HOF; `max_hours_per_week` = `[28,28,28,28,18,21]`. |
| `teacher_qualifications` | 23 | Same matrix as the seed (e.g. MUE: D, M, SU, KU). |
| `lessons` | 34 | Stundentafel: grades 1-2 = `D=6, M=5, SU=2, ETH=2, KU=2, MU=1, SP=3, FÖ=2` (8 lessons per class); grades 3-4 = `D=5, M=5, SU=4, E=2, ETH=2, KU=2, MU=1, SP=3, FÖ=2` (9 lessons per class). 2 × 8 + 2 × 9 = 34. SU has `preferred_block_size=2`. |
| `room_subject_suitabilities` | per the seed | Klassenräume suit `D, M, SU, RK, RE, ETH, E, FÖ`; specials suit only their dedicated subject. |
| `pinned_placements` | 0 | |
| `room_blocked_times`, `teacher_blocked_times` | 0 | |

**Hand-pinned teacher allocation.** A `[(class_short, subject_short) → teacher_short]` table chosen so FFD's lowest-id-room walk locks `(class 1a, day 0, D) → Klasse 4a` (or whatever empirically falls out). The exact triple is what the test asserts on; `Q9 risk 1` mitigation says iterate the allocation if the first choice doesn't lock in. The chosen allocation is documented in the test's docstring with one line per (class, subject).

**Solver config.** `SolveConfig` with the production active-default weights so the lock-in dynamics are the production ones; deadline kept short (or `None` for greedy-only) so the test isn't bound by LAHC wall-clock. Concretely:

```rust
SolveConfig {
    weights: ConstraintWeights {
        class_gap: 10,
        teacher_gap: 10,
        prefer_early_period: 1,
        avoid_first_period: 1,
        prefer_home_room: 5,
        avoid_last_period: 1,
        prefer_late_period: 1,
        class_day_balance: 5,
    },
    deadline: None, // greedy only; LAHC cannot escape lock-in anyway
    ..SolveConfig::default()
}
```

## Trace mechanism

**Feature flag.** `solver-trace` in `[features]` of `solver/solver-core/Cargo.toml`, no transitive dep activations. Default off. Solver-py never enables it.

**Module.** New file `solver/solver-core/src/trace.rs`, declared as `#[cfg(feature = "solver-trace")] mod trace;` in `lib.rs`. Holds:
- `static FFD_TRACE_SEQ: AtomicU64` (per-process counter; concurrent tests interleave, the sequence number lets a human reconstruct one test's order by filtering on lesson id).
- `pub(crate) fn ffd_trace(lesson_id: LessonId, day: u8, position: u8, room: Option<RoomId>, reason: &'static str)`.
- One thin formatter that prints `ffd_trace seq=<n> lesson=<8hex> day=<u8> pos=<u8> room=<8hex|-> reason=<reason>` to `stderr`.

**Reasons (closed set).** `non_contiguous_window`, `teacher_busy`, `teacher_blocked`, `class_busy`, `teacher_over_capacity`, `score_pruned`, `locked_room_conflict`, `locked_room_mismatch`, `room_unsuitable`, `room_busy`, `room_blocked`, `window_candidate`, `placed`, `unplaced_no_suitable_room`, `unplaced_no_free_time_block`, `unplaced_teacher_over_capacity`. The terminal `unplaced_*` reason is computed from `unplaced_kind` so the trace's terminal line matches the violation kind that lands in `Solution.violations`.

**Call-site map.** Every call site inside `solve.rs::try_place_block` is wrapped in its own `#[cfg(feature = "solver-trace")]` block immediately before the existing `continue` / acceptance branch. Mapping (cited from the current `solve.rs`):

| line | branch | reason |
|---|---|---|
| L342 | non-contiguous neighbour, `continue 'outer` | `non_contiguous_window` |
| L351-353 | teacher busy or blocked, `continue 'outer` | split: `teacher_busy` vs `teacher_blocked` |
| L355-358 | class busy, `continue 'outer` | `class_busy` |
| L362-364 | teacher over capacity, `continue` | `teacher_over_capacity` |
| L413-417 | score-pruned skip of room scan, `continue` | `score_pruned` |
| L438-440 | multi-class lock conflict, `continue` | `locked_room_conflict` |
| L447-450 | locked-room mismatch in room loop, `continue` | `locked_room_mismatch` |
| L452-454 | room not suited to subject, `continue` | `room_unsuitable` |
| L457-459 | room busy or blocked, `continue 'rooms` | `room_busy` vs `room_blocked` |
| L468 | window+room accepted (may be improved later) | `window_candidate` |
| L484-486 | terminal "no candidate window" | `unplaced_<kind>` from `unplaced_kind` |
| L488-524 | terminal commit (the chosen window) | `placed` |

`try_place_group` is not instrumented (Q7 retraction).

**Visibility.** All calls compile out completely when the feature is off (`#[cfg(feature = "solver-trace")]` on the `mod trace;` declaration AND on every call site keeps the type checker honest in both modes). `cargo build` on default features produces the same binary as today; `cargo test --features solver-trace -- --nocapture` produces the trace.

## Testing matrix

| Command | What it asserts |
|---|---|
| `cargo nextest run -p solver-core --test same_room_property` | The new test fails (`NoSuitableRoom` violation present), the existing two pass. |
| `cargo nextest run -p solver-core` | Workspace tests pass minus the new one. |
| `cargo build -p solver-core --features solver-trace` | Trace compiles. |
| `cargo test -p solver-core --features solver-trace --test same_room_property -- --nocapture` | Trace lines emit; redirect to a file, grep for the `placed` lines that precede the failing lesson's `unplaced_no_suitable_room`. |
| `mise run lint` | Clippy (`-D warnings`), machete, rustfmt, all-features clippy. |
| `mise run test` | Workspace + Python + frontend; nothing in this PR should affect Python or frontend, so this is a paranoia check. |

## Diagnostic note (PR body)

After CI is green, run the trace, capture the output, and write a section in the PR body that answers the four questions OPEN_THINGS asks for:

1. Which `(class, day, subject)` triple FFD locks first?
2. Which room does FFD pick for that lock and what `(class_delta_w + teacher_delta_w + subject_pref)` score does it see?
3. Which subsequent `(class, day, subject)` placement fails, and which room rejects with which reason?
4. What is the smallest change to `solve.rs::try_place_block` (path A) or to FFD as a whole (path B / C) that would unblock it?

The note cites file:line and references the `BlockCandidate.score` and `state.locked_room` data structures so PR-3's author can read the diagnosis without rerunning the trace.

## Risks

- **The hand-pinned allocation may not lock in.** Iterate the allocation (or insert one `RoomBlockedTime` simulating a competing class's lock) until the test fails 10/10 runs. Document the knob in the test docstring.
- **`solver-trace` rots from neglect.** `mise run lint` already runs `cargo clippy --workspace --all-targets --all-features -- -D warnings`; verify on the PR. If it doesn't, add `--all-features` to the relevant lint task in this PR.
- **`#![deny(missing_docs)]`.** Every new pub item (the feature flag is not a pub item, but the trace module's pub fns are) gets `///` rustdoc.

## Acceptance

- The new test fails on master without the feature flag (the integration `xfail` is a parallel signal but this PR adds the in-Rust regression that PR-3 will eventually flip).
- `mise run lint` and `mise run test:rust` pass.
- The PR body's diagnostic note answers the four questions above with cited evidence.
- OPEN_THINGS items 1 + 2 are checked off (or moved to "shipped" with a PR-link annotation), in the same final commit that updates CLAUDE.md if needed.
