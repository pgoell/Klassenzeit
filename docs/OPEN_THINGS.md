# Open Things

Running log of open work and deferrals. Sprint items first, then the next-up queue, then the scheduler gaps roadmap, then everything else as a one-line backlog.

## Active sprint: none in flight

The 2026-05-12 shipping of items 11 + 79 + 80 closed the multi-school flake-loop surface; "click Generate Schedule" is one-shot reliable on every Grundschule fixture today. ADR 0036 set the picker direction, ADR 0037 flipped the production default to `lahc_rr`, ADR 0038 widened `solve_deadline_ms` per backend.

## Next up

- `[P1]` Per-class / per-teacher attribution on QualityReport (item 59).
- `[P2]` Production-refresh ratification of ADR 0037 (item 81; opportunistic, ~10-12h host window).
- `[P2]` Non-Timefold third-backend spike (item 56; triggered when Rust LAHC + CP-SAT both plateau).
- `[Paused]` Schwimmunterricht modelling, resumes after the active sprint closes.

## Scheduler gaps for the best Grundschule product

Honest list of what is still missing or lackluster, ordered by impact on a real Hessen Grundschule deployment. Pick one off this list when no higher-priority sprint is in flight.

### P0 (next sprint candidates)

1. **Production-refresh ratification of ADR 0037.** Today's unpinned multi-school cells in `BENCH_RESULTS.md` are smoke-validated at 5s × 1 seed; the production 60s × 20-seed cell shape on unpinned is the missing data point. Re-run `mise run bench:bakeoff -- --teacher-pins on` followed by `-- --teacher-pins off --append` (~10-12h). If `lahc_rr_kempe` regains a soft-score edge at production scale, file ADR 0038-revision; if `lahc_rr` holds, ADR 0037 is ratified. Trigger: opportunistic on a quiet host window. Anchor: item 81.

2. **Per-class / per-teacher attribution on `QualityReport`.** Today `class_gap_hours` is a sum across all classes; the admin demoing a schedule cannot answer "which class is the worst-spread offender on dreizügig". Add `class_gap_hours_by_class: HashMap<SchoolClassId, u32>` plus matching `teacher_gap_hours_by_teacher`, `home_room_misses_by_class`, `class_day_balance_cost_by_class`. Property test: `sum(map.values()) == legacy_field` per axis. Allocation cost is post-solve only (cold path); the LAHC hot loop is unaffected. Anchor: item 59.

3. **Live progress feedback during solve.** The 5s LAHC budget feels long without a UI signal; users perceive "Generate" as broken. Surface partial progress (placement count, current best soft-score) via a streaming endpoint or polling on `app.state.solver_progress`; pair with a soft-cancel that returns the best-so-far solution. Trigger: the next demo where a user clicks Generate twice because the first click "didn't do anything". New gap (no existing OPEN_THINGS anchor).

### P1 (real Grundschule pain still un-modelled)

4. **Pausen / Aufsichtspflichten + Vertretungsreserve.** Teachers in a Hessen Grundschule owe supervision-duty minutes during Hofpausen on a rota, and substitution reserve reduces teaching capacity below `max_hours_per_week`. The schema has no break metadata on `TimeBlock` (breaks are implicit gaps) and no `reserve_hours_per_week` on `Teacher`. Two sub-changes: (a) `TimeBlock.kind` enum or sibling `Break` table so supervision can be scheduled, (b) `Teacher.reserve_hours_per_week` subtracted from `max_hours_per_week` in the solver's capacity check. Trigger: a customer school tracks supervision.

5. **Teilzeit-Tage patterns on Teacher.** Part-time teachers are contracted to specific weekdays (e.g. Dana works Mo, Di, Mi). Today this is representable via 15 `teacher_availabilities` rows per off-day, which is verbose and the seed leaves it empty. Add `Teacher.working_days: set[int]` (or a higher-level "weekly pattern" model); the solver consults it before per-block availability. Trigger: a second real Teilzeit teacher enters demo data and the verbose availability grid starts to feel painful.

6. **Soft / tentative pin semantic (UX + solver).** Today's `LessonPin` is binary (`hard` only); a Klassenlehrer wanting "I prefer this placement, but reroute it if you must" has no way to express it. Add `LessonPin.kind: PinKind` (`hard` | `soft`); soft pins enter the LAHC objective as a penalty (new `ConstraintWeights.soft_pin_miss`) rather than a hard constraint. Two badges in the schedule grid (icon variant + tooltip copy). Triggers a new ADR; out of scope until a workflow surfaces a real "preferred-not-required" case.

7. **Quality-issue endpoint + UI surface.** `quality_checks.QualityIssue[]` is returned by `POST /api/classes/{id}/schedule` but not persisted, and `GET /api/classes/{id}/schedule` returns placements only. The schedule view shows violations as a count but no actionable next-step ("3 issues" with no breakdown). Add `GET /api/schedule/quality-issues` (reuses `quality_checks.py`) and a sidebar in the schedule grid that lists the issues with a click-to-highlight gesture on the offending cell. Trigger: any demo where the admin asks "okay, but WHY is this incomplete?".

### P2 (longer-arc; reopen on trigger)

8. **Non-Timefold third-backend spike.** Trigger: a future `BENCH_RESULTS.md` refresh shows Rust LAHC variants and CP-SAT both plateau on the same quality axis. Candidate priority (per `docs/research/2026-05-08-third-solver-backend-candidates/`): (1) Pumpkin (Rust LCG-CP), (2) PySAT + RC2 MaxSAT, (3) good_lp + HiGHS MIP. Choco excluded (Java); GLPK excluded (GPL). A "no third-backend" ADR is a permissible outcome since no published benchmark documents a measurable win over CP-SAT on school timetabling. Anchor: item 56.

9. **Schwimmunterricht buffers.** One Doppelstunde Schwimmen per Klasse 3 needs `Room.is_external: bool` + per-Lesson `pre_buffer_minutes`, `post_buffer_minutes` (Hessen Wegezeit ~10-15 min each way). Solver enforces buffer per-class and per-teacher (rooms unaffected). New `ViolationKind::TravelBufferConflict`. Bench: extend dreizügige fixture with the Klasse 3 Schwimmen lesson. Trigger: active sprint promotes this paused program back to active.

10. **Block-aware FFD eligibility, LAHC Change move, and Swap move.** Today `ordering::ffd_order` ranks by `free-teacher-blocks * suitable-rooms`, ignoring contiguity (a length-2 lesson with same eligibility as length-1 is MORE constrained). LAHC's Change move skips block placements entirely; LAHC has no Swap move. Folding contiguity into FFD eligibility requires precomputing per-(teacher, day) free-position runs of length `>= n`; the block-aware Change move and Swap move both need a third RNG draw per iteration (determinism RNG-budget invariant shifts). Trigger: a fixture surfaces a real soft-score gap on block-heavy schedules.

11. **Multi-school tenancy.** The schema has no `School` table and no `school_id` on any aggregate root; every authenticated user shares the global pool. Multi-tenancy is a coordinated change: schools table, `school_id` FK on every aggregate + per-school join row, `school_id` on `User` (or membership table), query scoping in every route, seed scripts, E2E tests. Required before customer school #2. Anchor: today's `### Production readiness` first entry.

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

### Toolchain & build friction

- **`ty` preview status.** Astral's type checker is pre-1.0; revisit if it proves unstable.
- **`pytest-postgresql` or `testcontainers-python`** alternative to compose-based test infra. Revisit if onboarding friction emerges.
- **Structured logging follow-ups.** Mute uvicorn `--access-log`, body / `Content-Length` logging (PII review), CloudWatch field renames, `solver-py` Rust-side logging, CRUD success / error instrumentation.
- **Split `frontend/CLAUDE.md` into topic files under `.claude/rules/`.** Trigger at ~150 lines or topic-mixing.
- **Migrate frontend pnpm pin to pnpm 11.** Pin sits at 10.33.2; revisit when a pnpm 11 feature becomes desired.

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
