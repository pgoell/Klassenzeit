# Open Things

Running log of open work and deferrals. Sprint items first, then the next-up queue, then the scheduler gaps roadmap, then everything else as a one-line backlog.

## Active sprint: none in flight

The 2026-05-12 shipping of items 11 + 79 + 80 closed the multi-school flake-loop surface; "click Generate Schedule" is one-shot reliable on every Grundschule fixture today. ADR 0036 set the picker direction, ADR 0037 flipped the production default to `lahc_rr`, ADR 0038 widened `solve_deadline_ms` per backend.

## Next up

- `[P2]` Non-Timefold third-backend spike (item 56; triggered when Rust LAHC + CP-SAT both plateau).
- `[P2]` Day-balance penalty row on the Quality metrics section (`class_day_balance_cost_by_class`). Deferred from the 2026-05-15 per-class attribution ship: the Python recompute path needs a port of solver-core's `score::class_day_balance_cost_for_class` scaled-L1 formula (parity risk versus the authoritative Rust scorer) and a paired UX decision for the unweighted-cost metric, which reads as opaque without a "daily spread" mini-chart or equivalent. Trigger: per-class attribution gets product attention, OR the day-balance axis becomes a customer-school complaint.

## Scheduler gaps for the best Grundschule product

Honest list of what is still missing or lackluster, ordered by impact on a real Hessen Grundschule deployment. Pick one off this list when no higher-priority sprint is in flight.

### P2 (longer-arc; reopen on trigger)

10. **Multi-school tenancy program.** All 9 aggregate roots shipped: `Room`, `SchoolClass`, `Teacher`, `Subject`, `Stundentafel`, `WeekScheme`, `Lesson`, `SupervisionAssignment`, and `ScheduledLesson` (with `TimeBlock`, `LessonSchoolClass`, `RoomSubjectSuitability`, `RoomAvailability`, `TeacherQualification`, `TeacherAvailability`, and `ClassGroup` inheriting via parent FK). `schools` table, `users.school_id`, per-aggregate `school_id` FK, per-school uniqueness on each tenanted name/short_name pair, scoped CRUD + scoped non-CRUD readers, `/auth/me` carries `school_id` + `school_name`, sidebar renders the school name. `super_admin` role on `User` with a per-request scope resolved from `session.active_school_id` (mutable via `POST /auth/switch-school`) on all tenanted routes via the `get_scope_school_id` dependency, sidebar "Super Admin" badge and admin nav widened to include super-admin. ADR 0045 captures tenancy decisions; ADR 0046 documents multi-school membership and the session-active-school model that replaced the per-request URL override. Required before customer school #2.

    **Follow-ups (P2, ordered by blast radius):**

    - **10g: Audit log of super-admin cross-school writes.** Capture (super_admin_id, target_school_id, route, payload_summary, timestamp) for every write where `scope_school_id != current_user.school_id`. Trigger: a second super-admin user is provisioned, or compliance asks for the trail.
    - **10h: Tighten `/schools` admin visibility.** Today admin of school A sees all schools' names/IDs via `GET /schools` because the schools table is the tenant root. Decide whether admin should see only their own school via a new `GET /schools/me`, and whether the full listing remains super-admin only. Trigger: a customer raises information-disclosure concerns about school directory leakage.
    - **10i: API endpoint to promote / demote super-admin.** Today operators promote via psql. A `POST /admin/users/{id}/role` endpoint gated by `require_super_admin` would let support staff manage the role without DB access. Trigger: support staff needs to provision a new super-admin without engineering hands-on time.
    - **10j: API endpoint to grant / revoke school memberships.** Today operators wire memberships via psql. A `POST /admin/users/{id}/memberships` + `DELETE /admin/users/{id}/memberships/{school_id}` pair gated by `require_super_admin` would let support staff manage memberships without DB access. Trigger: support staff needs to provision a coach with multi-school access without engineering hands-on time. Anchor: ADR 0046.

83. **CP-SAT cannot solve dreizuegig at 60s budget.** 2026-05-15 production refresh confirms CP-SAT pinned + unpinned both return 0/20 feasibility on dreizuegig (median 294 hard violations, 0 placements). Stable across 2026-05-12 and 2026-05-15. CP-SAT distinguishes INFEASIBLE from UNKNOWN by wall-clock per `solver/CLAUDE.md`; 60s × 547MB peak RSS (pinned) and 65s × 2.4GB peak RSS (unpinned, model-fan-out from teacher widening) suggests OR-Tools' CP-SAT search did not find a first-feasible within the budget. Options: (a) widen `solve_deadline_ms` for `cpsat` on dreizuegig-shaped inputs (per ADR 0038 widening shape), (b) model-level reductions (AllDifferentExcept, redundant constraint set, presolve tuning), (c) accept CP-SAT as Grundschule-only and route dreizuegig through LAHC always (matches ADR 0037 production default). Trigger: opportunistic on CP-SAT pass, or when dreizuegig-shaped customer school surfaces with LAHC-quality insufficient. Anchor: ADR 0030.

84. **CP-SAT zweizuegig soft-score regression vs 2026-05-12 baseline (pinned + unpinned).** Pinned soft 1131→1394 (+23%); unpinned soft 1303→1600 (+23%). LAHC variants on the same fixture improved (-5 to -20% pinned, baseline-infeasible→feasible unpinned). Per-component cells show CP-SAT day_balance=30 and total interior gaps=42-45 dominating; LAHC sits at 12 / 0 on the same axes. Per ADR 0030 the CP-SAT objective mirrors `score_solution` exactly, so the regression is search-budget-bound, not weight-misalignment. Options: tighten the CP-SAT model, raise the deadline, or split the dispatch by problem size. Trigger: same as item 83. Anchor: ADR 0030.

85. **CP-SAT zweizuegig unpinned occasional infeasibility (19/20).** 1 of 20 seeds returns INFEASIBLE-or-UNKNOWN on unpinned zweizuegig at 60s; remaining 19 feasible. Same axis as items 83 + 84 (CP-SAT search budget) but a different mechanism (occasional non-convergence, not stable infeasibility). Per profile rule 13, filed as a fresh item rather than rolled into 83. Trigger: same as item 83.

86. **LAHC dreizuegig home_room miss ~140 (cross-cuts pin variants, all 4 LAHC backends).** 2026-05-15 pinned medians: `home_room_miss = (139, 138, 139, 139)` for `(lahc, lahc_rr, lahc_kempe, lahc_rr_kempe)` on a 294-placement schedule (~47% miss rate). Unpinned worse: `(157, 151, 157, 151)`. LAHC's canonical objective includes `prefer_home_room` (PRODUCTION_ACTIVE_WEIGHTS), but the optimizer plateaus here at 60s; FFD greedy picks home_room only as a slice-tied tiebreaker (item 49 history) and the LAHC neighbour explore lifts < 1 home_room miss per second of search at the plateau. Options: (a) seed FFD with `home_room` as primary key on dreizuegig-shaped fixtures (attempted 2026-05-16, see follow-up below), (b) add a home_room-targeted move alongside Change + Swap + R&R + Kempe, (c) widen `solve_deadline_ms` for dreizuegig specifically. Trigger: home_room ratio appears in user-facing quality metrics (item 6 surface) and gets product attention. Anchor: PRODUCTION_ACTIVE_WEIGHTS.

    **2026-05-16 attempt on option (a):** promoted `home_pairs` from FFD tertiary to secondary sort key in `solver-core/src/ordering.rs` (post-scarcity-primary, respecting `solver/CLAUDE.md`'s "scarcity-first primary key" rule). New unit test went RED → GREEN as expected; `mise run test:rust` 392/392; `mise run test:py` 589 passed + 1 xpassed. The 20-iteration `test_grundschule_schedule_meets_quality_bar` flake-loop (profile rule 10) denied at 18/20: the two failures landed on a DIFFERENT axis — `interior_gap` (`total_gaps=3 vs threshold max_gaps_per_class=2` on class 1a) on the einzuegig demo Grundschule. Per profile rule 12 the failure axis (interior_gap / class_gap component of canonical score) does not match this item's home_room axis, so the experiment reverts rather than landing partial. Mechanism: pushing home-room-scarce lessons to the front under the new secondary key perturbs class-day balance enough that ~10% of seeds tip class 1a over the interior-gap threshold. Direction (ascending vs descending) is not the issue — the secondary-key promotion structurally reshapes the FFD-seed gap distribution. Next attempt should pivot to option (b) (home-room-targeted LAHC move) so the home_room axis improves WITHOUT touching the seed's class-day-balance shape. Option (a) at this granularity is ruled out.

87. **LAHC einzuegig interior_gap flake on class 1a/2a (`total_gaps=3 > max_gaps_per_class=2`).** 2026-05-16 20-iteration flake-loop of `test_grundschule_schedule_meets_quality_bar` at production wall-clock denied 17/20 with all 3 denials landing on `QualityIssue(kind='interior_gap', detail={'total_gaps': 3, 'max_gaps_per_class': 2})` on einzuegig classes 1a (twice) and 2a (once). Same axis as the embedded attempt narrative in item 86 (the 2026-05-16 home_pairs FFD-secondary-key experiment denied 18/20 on this exact mechanism), so this is a stable latent einzuegig-fixture flake distinct from item 86's primary home_room axis. The Schwimm-travel-buffer PR (item 8 closure) did NOT introduce it (dreizuegig fixture was the only one modified; einzuegig untouched). Options: (a) widen `solve_deadline_ms` for einzuegig until the LAHC search dependably clears the class-day-balance perturbation, (b) add an interior-gap-targeted LAHC move, (c) accept the 15% denial rate by relaxing `max_gaps_per_class` to 3 in the integration test (rules out via "test the contract, not the implementation"). Trigger: integration-test reliability becomes blocker on a customer-facing feature. Anchor: ADR 0044 (latent flake observed at the Schwimm closure).

### P3 (longer-arc; reopen on trigger)

- **Continuous-minute travel buffer.** Today's buffer is a discrete one-slot rule (any non-zero pre/post buffer requires the adjacent slot to be Break OR free for class+teacher). A future continuous-minute model threads `TimeBlock.duration_minutes` through the solver wire format and walks adjacent blocks accumulating Break minutes until the buffer is covered. Trigger: a customer school's WeekScheme has a Hofpause shorter than the lesson's Wegezeit. Anchor: ADR 0044.
- **Hard-couple `Room.is_external` to buffer requirement.** Today `is_external` is informational (UI label only). A future revision could enforce "buffered lessons must use external rooms" as a new `ViolationKind::ExternalRoomRequired`. Trigger: a customer school configures a buffered lesson in a non-external room by mistake. Anchor: ADR 0044.
- **First-slot-of-day buffer grace.** Today a buffered lesson cannot be placed at day-position 0 (no preceding slot). Schools whose school day starts ~30 min before the first period could accept Schwimmen at period 0 if pre-school grace covered the Wegezeit. Trigger: customer ask for first-period Schwimmen. Anchor: ADR 0044.
- **Schwimmen expansion to Klasse 3b + 3c in bench fixture.** Today's dreizuegig fixture has one Klasse 3a Schwimmen Doppelstunde. Adding 3b and 3c quadruples the buffer constraints and may surface LAHC infeasibility shapes the single-cohort fixture misses. Trigger: production refresh shows feasibility margin on the buffered axis. Anchor: ADR 0044.
- **Production-budget BENCH_RESULTS.md refresh after Schwimmen ship.** The Klasse 3a Schwimmen Doppelstunde changes the dreizuegig placement count from 294 to 296. The pinned and unpinned cross-sections in BENCH_RESULTS.md predate the change. Run `mise run bench:bakeoff` at production budget (60s × 20 seeds × 4 fixtures × 2 phases) and commit the refreshed artifact as `chore(bench): refresh BENCH_RESULTS.md after item 8 ship`. Trigger: opportunistic post-merge. Anchor: ADR 0044.

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
