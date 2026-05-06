# Production refresh of `BENCH_RESULTS.md` post-item-31 / post-item-41 (active sprint, item 42)

> **Status (2026-05-06): deferred.** The production refresh attempted under this spec discovered a P0 `lahc_rr_kempe` double-booking caught by item 39's `validate_no_double_booking` post-condition validator (`grundschule / lahc_rr_kempe` cell, panic at `solver-bench/src/main.rs:421`). The bench supervisor is fail-fast on cell errors and aborted after 2 of 16 cells. Filed as OPEN_THINGS item 45 (P0). Item 42 stays open and is now blocked on item 45's fix; this spec stays in tree as a record of the attempt and the analysis. Anything below this line reflects the original intent, not what shipped.

**Sprint program.** Solver feasibility correctness + observability (active program).
**Phase.** Sprint-tidy phase: item 42.
**Goal.** Re-run `mise run bench:bakeoff` at production cell shape (`--budget 60s --seeds 20`, ~80 min wall-clock) and commit the regenerated `solver/solver-core/benches/BENCH_RESULTS.md` so the table reflects (a) item 31's five new quality columns (`Worst spread (median)`, `Worst home-room ratio (median)`, `Total interior gaps (median)`, `Late-period ratio (median)`, `Quality (pass / 4)`) and (b) item 41's full-cost `soft_score` reconciliation. The committed file is currently 12 columns of demo-shape data (`--budget 5s --seeds 4`); the renderer now emits 17 columns. Production-shape numbers are the artefact ADR 0032's production-default decision was supposed to keep current.

**Non-goal.** Not amending ADR 0032. ADRs are immutable per `docs/adr/README.md`; if the new medians change the LAHC-vs-cpsat ordering the ADR captured, surface the delta in a fresh OPEN_THINGS entry rather than rewriting history (brainstorm Q6). Not running `mise run bench:record` to refresh `BASELINE.md`; the criterion bench is independent of the bake-off bench and is blocked on item 15. Not promoting `room_hop` or `day_too_long` to bench columns (item 43, conditional on a refresh showing non-zero counts). Not unblocking item 14 (Grundschule quality-bar `xfail` removal); that follow-up is conditional on item 12 landing first. Not bundling item 44 (post-item-12 late-period column refresh) into this PR; item 12 is still open, the late-period column will still render `-` after this refresh, and item 44 stays in OPEN_THINGS.

## Context

The OPEN_THINGS item 42 bullet asks for a single mechanical action: regenerate `BENCH_RESULTS.md` at the production cell shape ADR 0032 chose (`--budget 60s --seeds 20`). The brainstorm (`/tmp/kz-brainstorm/brainstorm.md` for this run) refined the action into a buildable design. Key refinements:

- The "Blocked on item 15" tag in the OPEN_THINGS body is misleading. Item 15's body says the panic is in `mise run bench` (the criterion bench) and explicitly notes "the bake-off bench (`mise run bench:bakeoff`) uses production active-default weights and is unaffected." The bake-off run is unblocked. The OPEN_THINGS phrasing gets corrected in the closeout commit so a future reader does not bounce off the same false signal. Brainstorm Q1.
- The committed file is *shape*-stale, not just *budget*-stale. The bench's `write_header` in `solver/solver-bench/src/main.rs:709` emits 17 columns today (the original 12 from item 30 plus the five from item 31). The committed file has 12 columns. Splicing the five new columns by hand against demo data would mix demo data with production data inside a single row; the only sane fix is a regeneration. Brainstorm Q2.
- Run shape is exactly `--budget 60s --seeds 20`. ADR 0032's production-default decision used the same shape, so re-using it keeps the comparison apples-to-apples. Tightening seeds or shortening budget changes nothing the bake-off cells need. Brainstorm Q3.
- Host is the local Ryzen 7 3700X, matching the previous refresh footer. The bake-off bench is host-sensitive on wall-clock and Peak RSS columns and host-stable on feasibility / hard-violation columns (per ADR 0029); switching hosts would introduce a one-time wall-clock regression future readers would mistake for an algorithm regression. The bench writer auto-renders the `Refreshed YYYY-MM-DD on <host>, <kernel>, <rustc>` footer, so provenance lands automatically. Brainstorm Q4.
- Wall-clock fits one autopilot run if the bench is kicked off in the background and the autopilot writes the spec / plan / closeout edits in parallel. Foreground Bash maxes out at 10 min so blocking is not an option; splitting into two PRs is overkill for a `cargo run` invocation. Brainstorm Q5.
- ADR 0032 stays as-is. Item 42's body explicitly says "if the LAHC vs. cpsat ordering changes, surface in a follow-up plan rather than amending ADR 0032 in the same PR." A production-default change would be a new ADR, not an edit to 0032; bundling it into a sprint-tidy bench refresh would mix a structural decision into a chore commit. Brainstorm Q6.
- The only stale prose in the committed file is the trailing "_Shape demo at low budget/seeds (`--budget 5s --seeds 4`); production refresh queued as OPEN_THINGS item 42._" line. The bench's footer-writer is the authoritative source for the post-table prose; the regenerated file should drop the shape-demo line and inherit whatever the bench writer emits today. Hand-edits inside the file fight the `<!-- Regenerated by ... Do not hand-edit. -->` warning at the top. Brainstorm Q7.
- Commit shape is one `chore(solver)` for the regenerated `BENCH_RESULTS.md` and one `docs:` for the OPEN_THINGS / auto-memory closeout. PR #187 (the previous bake-off refresh, also chore-shape) used `chore(solver): refresh bake-off bench + adr 0032 production-default revisit`; that's the precedent. Brainstorm Q8.
- Success is a green CI run plus a 17-column / 16-row file (4 fixtures × 4 backends), no `?` or `panic` cells, no shape-demo footer line, and a PR body that documents whether the ADR-0032 ordering held. Brainstorm Q9.

Anchor item: `docs/superpowers/OPEN_THINGS.md` item 42. Anchor brainstorm: `/tmp/kz-brainstorm/brainstorm.md` (this run). Companion ADR: none. ADR 0029 (bake-off methodology), ADR 0032 (production-default revisit), and ADR 0034 (cell subprocess) are referenced for context but not amended.

## Scope

**In scope.**

- Run `cargo build -p solver-bench --release` once to warm the build cache so the bench's ~80 min wall-clock is dominated by solve time, not compile time.
- Run `mise run bench:bakeoff -- --budget 60s --seeds 20` and let it write the regenerated `solver/solver-core/benches/BENCH_RESULTS.md`. The bench's per-cell-subprocess architecture (ADR 0034) means a single cell panic does not cancel the rest; the bench tees its progress to stderr and the markdown writer emits one row per cell.
- Verify the regenerated file: 17 columns in the header, 16 rows (`grundschule × {lahc, lahc_rr, lahc_rr_kempe, cpsat}` plus zweizügig, dreizügig, lock_in mirrors), no `?` or `panic` strings in any cell, the shape-demo footer line is gone (this is the only hand-checked invariant; everything else is the bench writer's responsibility).
- Compare new medians to ADR 0032's findings on the cells ADR 0032 referenced (LAHC variants on grundschule + lock_in: 20/20 feasible; cpsat dreizügig: still infeasible; LAHC vs cpsat soft-score ordering: same direction as ADR 0032). Document the comparison in the PR body. If any of the three observations does not hold, file a fresh OPEN_THINGS entry in the active-sprint tail.
- Commit the regenerated file as `chore(solver): refresh BENCH_RESULTS.md at production cell shape (item 42)`.
- Edit `docs/superpowers/OPEN_THINGS.md`: delete item 42 (closeout per autopilot's "no `✅ Shipped` markers" rule); update item 43 and item 44 if the refresh data changes their conditional triggers (item 43: `room_hop` / `day_too_long` columns conditional on non-zero counts; item 44: post-item-12 late-period refresh).
- Update `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`: shift the next-pickup pointer from item 42 to whatever the next sprint-tidy or backend-tidy item is (item 34 backend-tidy if all sprint-tidy items close; item 43 / 44 if they remain conditional on item 12).
- Commit the closeout edits as `docs: close out item 42; advance roadmap pointer`.

**Out of scope.**

- Any change to the bench writer (`solver/solver-bench/src/main.rs`). The bench is feature-complete after items 30 + 31; this PR exercises it, not edits it.
- Any change to ADR 0032. See "Non-goal" above.
- Any change to `solver/solver-core/benches/BASELINE.md`. That refresh is gated by item 15 (zweizügig criterion-bench panic) and is genuinely blocked.
- Any new ADR. If the LAHC-vs-cpsat ordering changes, the follow-up is an OPEN_THINGS entry, not an ADR (the new ADR would land in the PR that proposes a production-default change, not in this refresh PR).
- Any code edits to `solver-core` or `solver-py`.
- Any change to `mise.toml` bench task definitions.

## Out of scope or non-goals

(See "Non-goal" header at the top.)

## Execution plan (preview)

The full plan in `docs/superpowers/plans/2026-05-06-bench-results-refresh.md` will sequence the bench run alongside the closeout edits. High level:

1. Pre-warm the build cache (`cargo build -p solver-bench --release`).
2. Kick off `mise run bench:bakeoff -- --budget 60s --seeds 20` in the background.
3. While the bench runs, draft the OPEN_THINGS edits, the auto-memory updates, and the PR body comparing new medians to ADR 0032.
4. When the bench completes, eyeball the regenerated file (header column count, row count, no panic strings, no shape-demo footer).
5. Commit the regenerated file with `chore(solver)` scope.
6. Apply the OPEN_THINGS / auto-memory edits, commit with `docs:` scope.
7. Push, open PR with the comparison narrative in the body.

## Risks and mitigations

1. **Bench panics mid-run.** A single cell panic (item 15-style, but on the bake-off side) might emit a partially populated markdown. The bench's per-cell-subprocess architecture (ADR 0034) means one panic does not cancel the rest. Mitigation: the regen-verify step is the gate; if any cell is `panic` or `?`, the cell gets re-run individually before the chore commit lands.
2. **Bench produces a regression vs. ADR 0032.** Mitigation: don't amend ADR 0032; file a new OPEN_THINGS entry under the active-sprint tail, document in the PR body. Brainstorm Q6.
3. **`uv run cargo run -p solver-bench --release` rebuilds from scratch and balloons wall-clock.** Mitigation: pre-warm with `cargo build -p solver-bench --release` before the timed run.
4. **Disk pressure or runaway memory.** cpsat cells peak around 190 MB RSS on dreizügig (per the demo run). 4 fixtures × 4 backends × 20 seeds = 320 subprocesses sequenced; transient peak is one subprocess at a time. No mitigation needed beyond running on the local Ryzen with its 32 GB RAM.

## Acceptance criteria

- `solver/solver-core/benches/BENCH_RESULTS.md` regenerated at production cell shape; header has 17 columns; body has 16 rows.
- No cell contains `?` or `panic`. Infeasible cells render `-` for soft-score / wall-clock columns per the bench writer's rules; this is expected on cpsat dreizügig.
- The "Shape demo at low budget/seeds" footer line is gone.
- The bench's auto-rendered footer names today's host / kernel / rustc.
- `mise run lint` passes; `mise run test` passes.
- PR body includes a one-paragraph ADR-0032 comparison narrative (LAHC variants 20/20 feasible on grundschule + lock_in: yes/no; cpsat dreizügig still infeasible: yes/no; LAHC vs cpsat soft-score ordering: same direction or flipped).
- OPEN_THINGS item 42 deleted; auto-memory next-pickup pointer advanced.
- CI green; PR auto-merged via `gh pr merge --auto --squash`.
