# Open Things

Running log of open work and deferrals. Sprint items first, then the next-up queue, then the scheduler gaps roadmap, then everything else as a one-line backlog.

## Active sprint: P0 solver regression from `try_home_room_repair_move`

88. **`[P0]` `try_home_room_repair_move` (ADR 0050, #300) violates post-condition validators on `lock_in` and `dreizuegig` LAHC cells.** 2026-05-20 production refresh: 8 cells panic with `Err(Error::Input(...))` at `solver/solver-bench/src/main.rs:980` (`.expect("solve")`). The Err is one of three validator messages: `room hopping for class ... day N subject ...: rooms RoomId(a) and RoomId(b)` (from `validate_no_room_hopping`); `double-booking: class ... at time-block ...` and `double-booking: teacher ... at time-block ...` (from `validate_no_double_booking`). Affected (phase / fixture / backend): pinned dreizuegig lahc_kempe, pinned lock_in lahc, pinned lock_in lahc_rr_kempe, unpinned dreizuegig lahc, unpinned dreizuegig lahc_kempe, unpinned lock_in lahc, unpinned lock_in lahc_kempe, unpinned lock_in lahc_rr_kempe. `lahc_rr` (the production default per ADR 0037) clears every cell. Suspected root cause per `solver/CLAUDE.md` rules: `try_home_room_repair_move`'s cross-day path does not consult `state.locked_room` at the destination `(class, day, subject)` triple (cause: double-booking + room hopping when the move shifts a placement's day), and the same-day room-only path does not walk siblings on `(class, day, subject)` to preserve `validate_no_room_hopping` (cause: room hopping in same-day moves). RED reproducer: any of the 8 affected cells at production budget; smoke `--budget 5s --seeds 4` may or may not hit a seed that exercises the bad move sequence. GREEN: validators clean across all 20 (fixtures × LAHC variants × 2 phases) cells. Options: (a) guard the validators inside the home-room-repair move helpers per the rule warnings; (b) revert #300 and reopen item 86 with the production default `lahc_rr` as the kernel surface. Anchor: ADR 0050, item 86, `solver/solver-core/src/lahc.rs::try_home_room_repair_move`.

The 2026-05-12 shipping of items 11 + 79 + 80 closed the multi-school flake-loop surface; "click Generate Schedule" is one-shot reliable on every Grundschule fixture today. ADR 0036 set the picker direction, ADR 0037 flipped the production default to `lahc_rr`, ADR 0038 widened `solve_deadline_ms` per backend.

## Next up

- `[P2]` Non-Timefold third-backend spike (item 56; triggered when Rust LAHC + CP-SAT both plateau).
- `[P2]` Day-balance penalty row on the Quality metrics section (`class_day_balance_cost_by_class`). Deferred from the 2026-05-15 per-class attribution ship: the Python recompute path needs a port of solver-core's `score::class_day_balance_cost_for_class` scaled-L1 formula (parity risk versus the authoritative Rust scorer) and a paired UX decision for the unweighted-cost metric, which reads as opaque without a "daily spread" mini-chart or equivalent. Trigger: per-class attribution gets product attention, OR the day-balance axis becomes a customer-school complaint.

## Scheduler gaps for the best Grundschule product

Honest list of what is still missing or lackluster, ordered by impact on a real Hessen Grundschule deployment. Pick one off this list when no higher-priority sprint is in flight.

### P2 (longer-arc; reopen on trigger)

83. **CP-SAT cannot solve dreizuegig at 60s budget.** 2026-05-20 production refresh confirms CP-SAT pinned + unpinned both return 0/20 feasibility on dreizuegig (median 296 hard violations, 0 placements; Schwimmen ship lifted hard floor from 294 to 296). Stable across 2026-05-12, 2026-05-15, and 2026-05-20. CP-SAT distinguishes INFEASIBLE from UNKNOWN by wall-clock per `solver/CLAUDE.md`; 62s × 481MB peak RSS (pinned) and 70s × 2.4GB peak RSS (unpinned, model-fan-out from teacher widening) suggests OR-Tools' CP-SAT search did not find a first-feasible within the budget. Options: (a) widen `solve_deadline_ms` for `cpsat` on dreizuegig-shaped inputs (per ADR 0038 widening shape), (b) model-level reductions (AllDifferentExcept, redundant constraint set, presolve tuning), (c) accept CP-SAT as Grundschule-only and route dreizuegig through LAHC always (matches ADR 0037 production default). Trigger: opportunistic on CP-SAT pass, or when dreizuegig-shaped customer school surfaces with LAHC-quality insufficient. Anchor: ADR 0030.

84. **CP-SAT zweizuegig soft-score regression worsens (pinned + unpinned).** 2026-05-20 production refresh: pinned soft 1131→2445 (+116% vs the 2026-05-12 floor; was 1394 on 2026-05-15); unpinned soft 1303→2333 (+79%; was 1600 on 2026-05-15). Per-component on 2026-05-20: pinned day_balance=24, total interior gaps=54, class_gap=54, teacher_gap=76; unpinned day_balance=24, total interior gaps=49, class_gap=49, teacher_gap=63. LAHC variants on the same fixture stay clean (gaps=0, day_balance=12) on both phases. Per ADR 0030 the CP-SAT objective mirrors `score_solution` exactly, so the regression is search-budget-bound, not weight-misalignment. Options: tighten the CP-SAT model, raise the deadline, or split the dispatch by problem size. Trigger: same as item 83. Anchor: ADR 0030.

85. **CP-SAT zweizuegig unpinned occasional infeasibility (19/20).** 2026-05-20 production refresh: 1 of 20 unpinned seeds returns INFEASIBLE-or-UNKNOWN on zweizuegig at 60s; remaining 19 feasible (unchanged from 2026-05-15). Same axis as items 83 + 84 (CP-SAT search budget) but a different mechanism (occasional non-convergence, not stable infeasibility). Trigger: same as item 83.

89. **`[P2]` CP-SAT zweizuegig PINNED non-convergence (14/20, regressed from 17/20).** 2026-05-20 production refresh: CP-SAT zweizuegig pinned dropped from 17/20 (2026-05-15) to 14/20 (3 additional seeds now return INFEASIBLE-or-UNKNOWN at 60s × 196 placements). Same CP-SAT-search-budget axis as items 83-85 but a different mechanism (pinned-side regression, not unpinned-only). Per profile rule 12 (partial improvement / regression on the same axis with a different mechanism: file a fresh item). Trigger: same as item 83. Anchor: ADR 0030.

90. **`[P2]` `try_home_room_repair_move` does not lower dreizuegig `home_room_miss` median.** 2026-05-20 production refresh: dreizuegig pinned `home_room_miss median` across the four LAHC variants is `lahc=139, lahc_rr=140, lahc_rr_kempe=139` (lahc_kempe is the panic cell, no data); 2026-05-15 baseline was `lahc=139, lahc_rr=138, lahc_kempe=139, lahc_rr_kempe=139`. The new move was item 86's deliverable (ADR 0050); its target axis showed no improvement under the production-budget refresh. Blocked-by: item 88 (validator violations must be fixed before the move kernel can be re-evaluated). Trigger: same shape as item 86. Anchor: ADR 0050, item 86.

91. **`[P1]` interior_gap weight bump (#299, item 87) regressed dreizuegig `gaps_med` from 0 to 3-4.** 2026-05-20 production refresh: dreizuegig pinned `lahc` and `lahc_rr` show `Total interior gaps (median)` of 4 and 3 respectively; 2026-05-15 baseline was 0 on both. Quality bucket dropped from 4/5 to 3/5 on those cells. Grundschule + zweizuegig stayed at 0 across LAHC variants (the einzuegig-target axis held). Per profile rule 12 (partial improvement / regression: file a fresh item for the residual on a different fixture). Options: (a) tune `interior_gap` weight back down and accept the einzuegig flake until a different fix lands; (b) leave the weight and accept the dreizuegig bench gap regression as a known cost; (c) gate the weight by fixture-shape signal. Trigger: opportunistic on solver-quality pass. Anchor: item 87, `solver-core/src/types.rs:PRODUCTION_ACTIVE_WEIGHTS`.

### P3 (longer-arc; reopen on trigger)

- **Continuous-minute travel buffer.** Today's buffer is a discrete one-slot rule (any non-zero pre/post buffer requires the adjacent slot to be Break OR free for class+teacher). A future continuous-minute model threads `TimeBlock.duration_minutes` through the solver wire format and walks adjacent blocks accumulating Break minutes until the buffer is covered. Trigger: a customer school's WeekScheme has a Hofpause shorter than the lesson's Wegezeit. Anchor: ADR 0044.
- **Hard-couple `Room.is_external` to buffer requirement.** Today `is_external` is informational (UI label only). A future revision could enforce "buffered lessons must use external rooms" as a new `ViolationKind::ExternalRoomRequired`. Trigger: a customer school configures a buffered lesson in a non-external room by mistake. Anchor: ADR 0044.
- **WeekScheme.pre_first_slot_grace_minutes admin UI.** Today the field is set via API only. Schools that operate a pre-first-period grace pocket need a numeric input in the WeekScheme edit dialog with the same 0-60 clamp as the Pydantic validator. Trigger: customer ask for first-period Schwimmen materialises. Anchor: ADR 0044.
- **Schwimmen expansion to Klasse 3b + 3c in bench fixture.** Today's dreizuegig fixture has one Klasse 3a Schwimmen Doppelstunde. Adding 3b and 3c quadruples the buffer constraints and may surface LAHC infeasibility shapes the single-cohort fixture misses. Trigger: production refresh shows feasibility margin on the buffered axis. Anchor: ADR 0044.
- **Soft-score baseline shift in BENCH_RESULTS.md as of 2026-05-20.** All LAHC + CP-SAT cells show soft-score inflation vs 2026-05-15 (grundschule LAHC 32→62, zweizuegig LAHC 184-193→368-417, lock_in LAHC 305-340→393-407). Per-component columns hold flat on grundschule (gaps=0, day_balance=2 both periods); the shift is from a `PRODUCTION_ACTIVE_WEIGHTS` rebalance (#299 raised `interior_gap` weight) compounded with ADR 0044 buffer-grace summands. Cross-period BENCH_RESULTS.md soft-column comparison is no longer apples-to-apples across the 2026-05-15 / 2026-05-20 boundary; per-component cells remain comparable. Trigger: pre-PR review of an algorithm change that would otherwise compare against pre-2026-05-20 soft scalars. Anchor: `solver-core/src/types.rs:PRODUCTION_ACTIVE_WEIGHTS`, item 87.

## Backlog

### Product capabilities

- **Repository / unit-of-work layer.** Revisit when a handler grows past ~80 lines.
- **`active` flag on WeekScheme.** Add when a schedule-switcher needs it.
- **Auto-infer WeekScheme time-block position.** Backend assigns position by chronological rank and validates non-overlap. Trigger: UX polish pass.
- **Advisory lock on concurrent POSTs for the same class.** `pg_advisory_xact_lock(hashtext(class_id))` prevents wasted parallel solves. Add if real interleaving traffic appears.
- **Multi-week-scheme support in teacher / room views.** Cell-builder uses the first class's scheme only. Trigger: customer school with 2+ active schemes.
- **DnD polish across teacher / room / unified views.** Wire `@dnd-kit/core` on teacher and room views, cross-class swap, `TouchSensor` long-press. Trigger: tablet demo.
- **Cross-class merged Religion beyond a single Jahrgang.** Wait until a customer school surfaces the pattern.
- **Lebensarbeitszeitkonto.** Teacher seniority time credit; payroll concept, not a scheduling constraint.
- **Synthetic bench fixture generator.** `gen_synthetic_problem(...)` for 100+ classes; add if named fixtures cannot answer a future bench question.
- **List-view columns for preference flags + home room.** Subjects (flag icon) and SchoolClasses (Klassenraum name) at-a-glance columns. Add when users ask.
- **Per-class subject preference overrides.** `school_class_subject_preferences` table for class-specific deviations.
- **Solver knobs on the wire.** `?deadline_ms=<n>` query param, `SolveConfig.max_iterations` pass-through, LAHC telemetry (`iterations`, `accepted`, `rejected`) on `Solution`. Add per ask.
- **Surface `lesson_group_id` in lesson edit dialog.** Read-only "Gruppe X" badge first; editable dropdown when co-placement constraint exists.
- **Visual merge of adjacent same-lesson cells.** `rowspan`-merged Doppelstunden display; needs cross-cell awareness.
- **Block-size flexibility on Lesson.** Mixed sizes per lesson (`h=3, n=2`) + `preferred_block_size > 2`. Both lift when a real ask surfaces.
- **Prune slowest seed solvability tests.** `test_demo_*_solvability.py` boots solver per fixture; cache where behaviour is not under test.
- **Deep-linked entity edit.** `?edit=<id>` search param to open the matching dialog on mount.
- **Teacher-centric schedule view.** "Where is Frau Müller all week" via aggregation or new endpoint. Ship after a demo asks.
- **Room-centric schedule view.** Mirror of the teacher view.
- **Duplicate a Stundentafel.** `POST /stundentafeln/{id}/duplicate` + "Duplizieren" button.
- **Collapse the loading / error / empty + Toolbar wrapper.** `<EntityListShell>` primitive once a polish pass surfaces it.
- **Sub-resource setup in the create flow.** Auto-reopen edit dialog so qualifications / suitability / time blocks are the next step.
- **Bulk delete across entity tables.** Checkbox columns + `DELETE /<entity>?ids=...` once a workflow demands it.
- **Import / export buttons.** Wire the placeholder buttons to CSV / JSON endpoints once those land.
- **Toast + delete-error polish.** Replace `form.setError("root", ...)` with `toast.error(...)` for 409s, carve a dedicated `toasts.*` i18n namespace, add typed 409 / pre-flight is-used check across entities.
- **`entry_count` / `total_hours` on `StundentafelListResponse`.** Total-hours column. Add when users ask.
- **Translate Zod validation errors beyond login.** Global Zod error map once a second non-login form surfaces them.
- **Raise the frontend coverage floor.** 50 -> 70 -> 80% as baseline clears each tier.
- **DX polish.** Parallel `mise run dev` for backend + frontend, self-hosted fonts, time-of-day-aware welcome greeting, untranslated-string lint rule.
- **Production deployment + data-migration framework.** Docker / reverse proxy / secrets management for the prod pathway; real data-migrations once data exists beyond `demo_grundschule`.
- **Auth surface beyond email+password.** MFA / TOTP / passkeys, email-based reset, OAuth / OIDC / social login, self-service registration. Closed-system today; revisit when user base or threat model grows.
- **WCAG AA contrast on light-mode buttons.** `button-primary` 3.88:1, `button-secondary` 2.57:1 (both below 4.5:1). Decide when light-mode a11y audit is on the roadmap.
- **Auto-assign-teachers count in the "Generate lessons" toast.** "M teachers auto-assigned" line; needs en + de keys.

### CI / repo automation

- **Re-tighten `.test-duration-budget`.** Wait for two or three master runs at the new floor, then lower to ~4-5x the new max in a single-line PR.
- **Per-worker postgres process for backend tests.** `pytest-postgresql` per-worker instances; lower priority after PR-2 dropped the suite to 27s.
- **`TRUNCATE`-based per-test reset for backend tests.** Drop transaction-rollback isolation in favour of `TRUNCATE` on dirty tables; cheaper to prototype than per-worker postgres.
- **Coverage split (master-only job).** Move coverage instrumentation into a master-only CI job; mostly redundant unless coverage runs grow slow again.
- **Dependabot for the Python / uv ecosystem.** Revisit when dependabot ships first-class uv support, or switch to Renovate.
- **`mise run e2e` leaves seed data in `klassenzeit_test`.** Subsequent `test:py` runs hit unique-constraint violations on seed rows. Fix as part of a broader test-isolation overhaul.
- **`db.commit()` audit across all routes.** One-PR sweep over every `routes/` module to confirm nothing else silently rolls back.

### Testing

- **Session-scoped event loop interference at scale.** All async tests share one event loop; revisit if the suite grows large or async timeouts get added.
- **E2E entity coverage beyond Subjects.** Per-entity Playwright flows (Rooms, Teachers, WeekSchemes, SchoolClasses, Stundentafel, Lesson).
- **E2E cross-browser matrix.** Firefox + WebKit are disabled; enable when external users appear.
- **E2E accessibility audits.** `@axe-core/playwright` integration.
- **E2E visual regression, remaining approaches.** Pixel-diff snapshots once design stabilises; vision-LLM diff as a fuzzy follow-up.
- **E2E parallel workers + per-worker DBs.** Move off single-worker shared DB once CI time matters.
- **E2E session cleanup in `/__test__/reset`.** Reset endpoint preserves `sessions` today; revisit if tests need clean session state.
- **E2E nightly extended run.** Add when the suite justifies tiering.
- **E2E test-only router hardening.** Bind `/__test__` to localhost only if the surface grows beyond `env == "test"`.
- **E2E integration test for conditional mount.** Assert `/__test__/*` returns 404 under `KZ_ENV=dev`. Add if a refactor risks breaking the wiring silently.
- **Shell-exported `KZ_ENV=dev` defeats router mounting.** Conftest `setdefault` no-ops if the shell exports `KZ_ENV`; add a warning or switch to `pytest-env`.
- **Admin email must not use `.local` TLD.** `email-validator` rejects reserved domains. Revisit when a more realistic test domain matters.
- **Branch-protection required check + `e2e-gate` aggregator.** `if: always()` aggregator so `e2e` can be required + path-filterable. Add once the suite blocks merges safely.
- **`TRUNCATE ... RESTART IDENTITY CASCADE` may reset sequences beyond the savepoint.** Revisit if tests rely on predictable sequence values.
- **Investigate `SAWarning: transaction already deassociated from connection` across backend tests after audit middleware.** The 10g `SuperAdminAuditMiddleware` borrows the per-request AsyncSession via `app.dependency_overrides[get_session]` and commits the audit row in the same session the handler used; the test fixture's per-test savepoint absorbs the rollback, but pytest emits the SAWarning on many tests after the middleware landed (ADR 0048). Likely benign session / savepoint lifecycle interaction; does not affect determinism or row visibility. Trigger: warnings become noise during debugging, or the underlying lifecycle becomes a real bug.

### Toolchain & build friction

- **`ty` preview status.** Astral's type checker is pre-1.0; revisit if it proves unstable.
- **`pytest-postgresql` or `testcontainers-python`** alternative to compose-based test infra. Revisit if onboarding friction emerges.
- **Structured logging follow-ups.** Mute uvicorn `--access-log`, body / `Content-Length` logging (PII review), CloudWatch field renames, `solver-py` Rust-side logging, CRUD success / error instrumentation.
- **Split `frontend/CLAUDE.md` into topic files under `.claude/rules/`.** Trigger at ~150 lines or topic-mixing.
- **Migrate frontend pnpm pin to pnpm 11.** Pin sits at 10.33.2; revisit when a pnpm 11 feature becomes desired.
- **E2E webServer serves stale `frontend/dist/`.** `frontend/e2e/playwright.config.ts` runs `vite preview` against the last build; a code change re-run without `mise run fe:build` exercises the OLD bundle and produces false-greens (or false-reds against a correct fix). Mitigations: chain `pnpm build` into the webServer command, switch to `vite dev`, or have `mise run e2e` depend on `fe:build`. Trigger: a future E2E iteration burns time on a stale-dist false-result.

### Auth maintenance

- **Session cleanup cron.** Automate `mise run auth:cleanup-sessions` when session volume justifies it.
- **Per-IP rate limiting.** Defer to reverse proxy; current limiter is per-email only.
- **Password breach check (HIBP).** Online k-anonymity check on top of the offline blocklist.
- **Audit log.** Full audit trail beyond `last_login_at`.

### Production readiness & metadata

- **Production DB configuration.** Connection pooling, read replicas, `statement_timeout`, `pg_stat_statements`. Out of scope until the deployment spec.
- **Move Postgres init-SQL source into server-infra.** `server-infra/docker-compose.yml` mounts an absolute Klassenzeit path; move the file into server-infra.
- **License.** No `license` field in `Cargo.toml`, no `LICENSE` file. Revisit when the distribution model is clearer.
- **Chat interface agent.** Conversational agent that creates / modifies / deletes entities, surfaces schedule problems, suggests fixes.

## Reference data

### Hessen Grundschule reference data

Researched 2026-04-22 and mapped onto the schema during the seed brainstorm (the seed-design spec, since archived). Values below are the source figures; the seed reflects every "Yes" row and documents every "Not encoded" row against an existing OPEN_THINGS deferral.

- *Stundentafel Klasse 1/2:* 21 Pflichtstunden (Deutsch 6, Mathematik 5, Sachunterricht 2, Religion/Ethik 2, Kunst/Werken/Musik 3, Sport 3) plus 2 Stunden Förderunterricht/AGs.
- *Stundentafel Klasse 3/4:* 25 Pflichtstunden (Deutsch 5, Mathematik 5, Sachunterricht 4, Fremdsprache 2, Religion/Ethik 2, Kunst/Werken/Musik 4, Sport 3) plus 2 Stunden Förderunterricht/AGs. Über alle vier Jahrgänge also 92 Wochenstunden gesamt.
- *Lehrer-Pflichtstunden (Grundschule):* 28 Wochenstunden Vollzeit, ab 01.02.2026 reduziert auf 27,5. Teilzeit wird anteilig als Bruch geführt (typische Werte: 14/28, 18/28, 21/28). Lebensarbeitszeitkonto: 0,5 Stunden pro Woche Gutschrift bis zum 60. Lebensjahr (anteilig bei Teilzeit).
- *WeekScheme-Zeitraster:* Unterrichtsbeginn 7:45 bis 8:15, Unterrichtsstunde = 45 Minuten. Zwei Hofpausen (je 15 bis 20 Minuten, nach der 2. und nach der 4. Stunde) plus eine kurze Frühstückspause im Klassenraum (ca. 10 Minuten). Tagesende 11:30 bis 13:20 im Halbtag, bis 14:55 im Ganztag. Ganztagsschulen ergänzen eine Mittagspause von 45 bis 60 Minuten. The shipped einzuegig and zweizuegig seeds use a 6-period Halbtag grid (08:00 to 13:05); the dreizuegige Ganztagsschule seed extends to 8 periods (08:00 to 14:50) per the Ganztag pattern.
- *Quellen:* Hessisches Kultusministerium, Hessischer Bildungsserver, GEW Hessen Pflichtstundenverordnung.
