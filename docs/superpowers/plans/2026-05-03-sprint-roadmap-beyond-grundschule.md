# Sprint roadmap reorganisation implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganise `OPEN_THINGS.md` to retire the closed Realer Schulalltag sprint, open Sprint 1 (Schwimmen + Sek-I foundations) as the new active sprint, and append a Planned future sprints section listing Sprints 2 to 7.

**Architecture:** Pure documentation reorganisation. One `OPEN_THINGS.md` rewrite plus an auto-memory roadmap refresh. Tiered edits land in one logical commit because the doc reads top-to-bottom and a partial state would mislead future readers.

**Tech Stack:** Markdown, prose. No code, no schema, no tests.

---

## File structure

- **Modify:** `docs/superpowers/OPEN_THINGS.md`
  - Move the "Active sprint: Realer Schulalltag + better scheduler" section into "Completed sprints" with a closing note ("functionally closed 2026-05-02; drop-tier item 10 conditional on a placement-rate regression that has not surfaced").
  - Add a new "Active sprint: Schwimmen + Sek-I foundations" section above the (now-relocated) completed-sprints block, with a tier list (`P0` school-type enum + grade-range expansion + Schwimmunterricht; `P1` tidy bundle).
  - Add a new "Planned future sprints" section between the active sprint and the existing Acknowledged deferrals section, with one paragraph per Sprint 2 to 7 plus a per-sprint tidy-mix note.
  - Strike "Schwimmunterricht: external location + travel time" from Acknowledged deferrals (it becomes Sprint 1 P0).
  - Strike "SchoolClass headcount / Klassenobergrenze" from Acknowledged deferrals (it becomes Sprint 2 P0).
  - Strike "`demo_gesamtschule` bench fixture (Sek I, deferred)" from Acknowledged deferrals (it becomes Sprint 6).
  - Strike "Doppelbesetzung / Förderstunden with two teachers" from Acknowledged deferrals (it becomes Sprint 5 tidy item).
  - Leave every other deferral as-is.
- **Modify:** `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`
  - Replace the "Realer Schulalltag sprint functionally closed; tidy follow-ups continuing" framing with a "Beyond-Grundschule roadmap opened; Sprint 1 (Schwimmen + Sek-I foundations) active" framing.
  - Update the "How to apply" line to point at Sprint 1 P0 work as the next /autopilot pickup.

---

## Tasks

### Task 1: Move the closed sprint into Completed sprints

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Read the current file structure to confirm line ranges.**

Run: `grep -n "^## " docs/superpowers/OPEN_THINGS.md`
Expected output (the sequence of top-level section headings; sub-sprint headings under "Completed sprints" use `###`):

```
## Active sprint: Realer Schulalltag + better scheduler
## Completed sprints
## Acknowledged deferrals
## Backlog
```

Confirm the active-sprint section runs from its `##` heading down to the `## Completed sprints` heading immediately below.

- [ ] **Step 2: Cut the active-sprint section and re-paste it as the first entry under Completed sprints.**

Use the Edit tool to:

1. Change the heading `## Active sprint: Realer Schulalltag + better scheduler` to `### Realer Schulalltag + better scheduler (functionally closed 2026-05-02)`. The new heading drops the `## Active sprint:` prefix because the section is no longer active and re-targets to a `###` so it fits the existing Completed-sprints sub-heading style.
2. Move the entire (renamed) block immediately below the `## Completed sprints` heading, before the existing `### DX / CI infra hardening (shipped 2026-04-30)` sub-section. The Completed sprints section is ordered most-recent-first, so the freshly-closed sprint goes on top.
3. Append one closing paragraph at the end of the moved block (before the next sub-section heading):

```markdown
**Sprint closure note (2026-05-03).** All P0 / P1 items shipped, plus drop-tier item 9 (per-subject preference weights, ADR 0025). Item 10 (block-aware FFD eligibility) remains conditional on a placement-rate regression on block-heavy schedules that has not surfaced; the deferral now lives under "Acknowledged deferrals". Tidy follow-up shipped 2026-05-03 on `feat/request-id-contextvar` (PR #162) closes structured-logging follow-up (a) (`contextvars`-based request_id propagation).
```

- [ ] **Step 3: Verify the move.**

Run: `grep -n "^## \|^### " docs/superpowers/OPEN_THINGS.md | head -10`
Expected: `## Completed sprints` appears once at the top, with `### Realer Schulalltag + better scheduler (functionally closed 2026-05-02)` directly under it, then `### DX / CI infra hardening (shipped 2026-04-30)`, then `### Solver quality + tidy (shipped 2026-04-29)`, then `### Prototype sprint (shipped 2026-04-24)`.

### Task 2: Add the new active-sprint section

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Insert the new `## Active sprint: Schwimmen + Sek-I foundations` section above `## Completed sprints`.**

The exact section text to insert:

```markdown
## Active sprint: Schwimmen + Sek-I foundations

Goal: ship two foundations that unlock the Beyond-Grundschule sprint sequence: (1) Schwimmunterricht modelling (`Room.is_external` flag plus per-Lesson travel buffers, with a Klasse 3 Schwimmen Doppelstunde landing in the dreizügige Grundschule fixture) and (2) a `Stundentafel.school_type` enum (`Grundschule` / `Hauptschule` / `Realschule` / `Gymnasium` / `Gesamtschule`) plus a grade-range expansion to 13 so future Sek I and Sek II fixtures have a home. Tiers reflect drop-order if the sprint runs long. Phase ordering: schema prereqs first, then solver, then the seed and tidy mix.

### Schema phase

1. **`Stundentafel.school_type` enum + grade-range expansion.** `[P0]` Add `school_type: SchoolType` (Postgres `ENUM('Grundschule','Hauptschule','Realschule','Gymnasium','Gesamtschule')`) on `Stundentafel`. Backfill existing rows to `Grundschule`. Widen `SchoolClass.grade_level` and `Stundentafel.grade_level` validators / Pydantic `Field(ge=1, le=13)` from the implicit 1-4 ceiling so Sek I and Sek II fixtures are representable. Frontend Stundentafel form gets a school-type dropdown; Zod schema mirrors the enum. Per-sprint research: confirm the five-value enum covers the Hessen Schulform list this PR commits to.
2. **`Room.is_external: bool` flag + per-Lesson travel buffer fields.** `[P0]` Add `Room.is_external: bool = False` (server default false) and per-`Lesson` `pre_buffer_minutes: int | None`, `post_buffer_minutes: int | None` (both nullable, default null). Pydantic / Zod / frontend mirrors. Solver consumes the buffers in the next phase.

### Algorithm phase

3. **Schwimmunterricht solver constraint.** `[P0]` `solve_with_config` rejects placements where the lesson's class set or teacher is already booked in the time-block(s) blocked by `pre_buffer_minutes` (preceding) or `post_buffer_minutes` (following). The buffer is enforced per-class and per-teacher; rooms are unaffected (the external room is the auswärtige Schwimmhalle). New `ViolationKind::TravelBufferConflict` records the affected (class | teacher, time-block) pair. Bench: extend the dreizügige fixture with one Klasse 3 Schwimmen lesson and refresh `BASELINE.md` if the criterion drift exceeds the 3 percent refresh threshold.

### Seed phase

4. **Klasse 3 Schwimmen lesson in `demo_grundschule_dreizuegig`.** `[P0]` One Doppelstunde Schwimmen per Klasse-3 Klasse, pinned to a new external Schwimmhalle Room (`is_external=True`), with `pre_buffer_minutes=15` and `post_buffer_minutes=15` (Hessen Wegezeit Mittelwert). Teacher allocation pinned via `_TEACHER_ASSIGNMENTS_DREIZUEGIG`. Solvability test asserts the buffered slots stay free and `body["violations"] == []`. Per-sprint research: confirm Hessen Klasse 3 Schwimmen is one Doppelstunde / Woche and 10 to 15 min Wegezeit each way (already noted in OPEN_THINGS Hessen Grundschule reference data).

### Tidy phase

5. **Einzügige solvability test transient flakiness.** `[P1]` Investigate only if the flake reproduces on master CI during this sprint; otherwise leaves as the existing deferral. Root-cause is suspected nondeterminism in `scheduling/teacher_assignment.py:auto_assign_teachers_for_lessons` under full-suite ordering (subject UUID order leaks into the heuristic's tiebreak).
6. **`common.actions` i18n key collapse.** `[P1]` Each entity page passes `actionsHeader={t("subjects.columns.actions")}` (and rooms / teachers / schoolClasses / stundentafeln / lessons all do the same), each resolving to "Actions" / "Aktionen". Carve out `common.actions` in en + de catalogs; migrate the seven call sites in one pass.
7. **Audit toast error fallbacks for copy reuse.** `[P1]` Several `toast.error(err instanceof Error ? err.message : t("x.action"))` call sites use the button-label key as the error fallback (rendering "Save availability" as an error string). PR #116 fixed two known sites; this sweep adds a `mutationErrorKey(entity, action)` helper that returns the right key by convention, then migrates every remaining call site identified by `rg "toast.error\(.*t\(.*\.save"`.

### Drop tier (P2)

8. **Block-aware FFD eligibility for buffered lessons.** `[P2]` Only land if the dreizügige fixture surfaces a real placement-rate regression on the new buffered Schwimmen lesson; otherwise leaves the existing deferral untouched. The buffered lesson is more constrained than a same-eligibility unbuffered lesson because its surrounding time-blocks must also be free for the class and teacher; folding contiguity into the FFD eligibility computation removes the pressure naturally.

```

- [ ] **Step 2: Verify the section is in place.**

Run: `grep -n "^## " docs/superpowers/OPEN_THINGS.md`
Expected: `## Active sprint: Schwimmen + Sek-I foundations`, then `## Completed sprints`, then `## Acknowledged deferrals`, then `## Backlog`.

### Task 3: Add the Planned future sprints section

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Insert a new `## Planned future sprints` section between the active sprint and `## Completed sprints`.**

Exact text to insert:

```markdown
## Planned future sprints

Each sprint listed below ships its own brainstorm + spec + plan when it begins. Order reflects prereqs (Sek-I primitives before Sek II Kurssystem; both before fixture capstones). Tidy mixes are indicative; the sprint's brainstorm picks the actual quality items at the time.

### Sprint 2: Gymnasium Sek I fixture (G9, einzügig 5-10)

Real `demo_gymnasium_einzuegig` seed researched against the Hessisches Kultusministerium G9 Kontingent-Stundentafel for Klasse 5-10 ([Hessen Gymnasiale Mittelstufe](https://kultus.hessen.de/Schulsystem/Schulformen-und-Bildungsgaenge/Gymnasium/Gymnasiale-Mittelstufe)). First non-Grundschule fixture; exercises the school-type enum and a secondary-school subject set (Latein / Französisch as 2. Fremdsprache, NaWi, PoWi, Sport differenziert nach Geschlecht). Adds `SchoolClass.headcount: int | None` plus a pre-solve check "all rooms a lesson can land in must have `capacity >= school_class.headcount` (or null)". Tidy: per-worker postgres process for backend tests; structured-logging follow-up (b) (uvicorn access-log dedup).

### Sprint 3: Wahlpflichtfächer

New `WahlpflichtGroup` schema (a parent record naming the choice; per-Klasse rows recording which subject each Klasse picked from the group). Solver constraint: each Klasse picks exactly one Wahlpflicht subject per group; placements honour that pick. UI for Wahlpflicht setup. Real Klasse 8 / 9 / 10 Wahlpflicht fixture rows on the Sprint 2 Gymnasium fixture (3. Fremdsprache vs NaWi vs Informatik vs Kunst-AG, per Hessen G9 Mittelstufe lever). Tidy: TRUNCATE-based per-test reset; structured-logging (c) (request body / Content-Length).

### Sprint 4: Differenzierung (E-Kurs / G-Kurs)

`KursAufteilung` schema for Mathematik / Englisch / Deutsch differenziert (E-Kurs / G-Kurs) at Realschule and Gesamtschule. Solver constraint: a differenziertes Lesson-Paar shares its time-block (parallel split) but uses two teachers and two rooms. Full `demo_realschule` fixture researched against the Hessen Realschule Stundentafel ([Realschule Hessen](https://www.realschule-hessen.de/)). Tidy: coverage split (master-only CI job); structured-logging (d) (GCP / ECS field renames).

### Sprint 5: Sek II Kurssystem

The hardest schema pivot: Sek II introduces `Kurs` rows that have no fixed `SchoolClass` (Kurse cross Klassen) and per-Student `Kurswahl` rows recording which 2 LK + ~8 GK each student picks. New phase enum: `Einfuehrungsphase` (E1, E2) plus `Qualifikationsphase` (Q1, Q2, Q3, Q4). Klausuren-Fenster scheduling. Sek-II-shaped schedule view (per-Kurs and per-Schueler). Researched against the Hessen Kerncurriculum Oberstufe ([Kerncurriculum gymnasiale Oberstufe Mathematik](https://kultus.hessen.de/sites/kultus.hessen.de/files/2025-10/kerncurriculum_gymnasiale_oberstufe-mathematik.pdf)). Lands on its own because every assumption from "Lessons are per-Klasse" needs a parallel "Kurse are per-Kurs" track. Tidy: structured-logging (e) (CRUD route handler instrumentation); Doppelbesetzung / Foerderstunden two-teacher schema (`Lesson.co_teacher_id: uuid.UUID | None` plus solver "if set, co-teacher must also be free and qualified").

### Sprint 6: Gesamtschule full Sek-I fixture

Real `demo_gesamtschule` (24 classes Sek I, ~50 teachers, ~31 rooms, ~700 placements) using Wahlpflicht (Sprint 3) + Differenzierung (Sprint 4) primitives. Researched against a Hessen integrierte Gesamtschule (IGS) Stundentafel Sek I. First fixture that exercises both Sek-I primitives at scale; supersedes the long-standing OPEN_THINGS deferral. Tidy mix picked when the sprint brainstorm runs.

### Sprint 7: Gymnasium full G9 fixture (5-13)

Real `demo_gymnasium_g9_full` exercising Wahlpflicht (Sprint 3) + Sek II Kurssystem (Sprint 5) on one school. Capstone fixture: Klasse 5 Latein-Anfaenger to Q4 Leistungskurs-Klausuren, one researched dataset. Tidy mix picked when the sprint brainstorm runs.
```

- [ ] **Step 2: Verify the section ordering.**

Run: `grep -n "^## " docs/superpowers/OPEN_THINGS.md`
Expected: `## Active sprint: Schwimmen + Sek-I foundations`, then `## Planned future sprints`, then `## Completed sprints`, then `## Acknowledged deferrals`, then `## Backlog`.

### Task 4: Strike the four superseded deferrals

**Files:**
- Modify: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Strike the Schwimmunterricht deferral.**

Find the line under "Acknowledged deferrals" beginning `**Schwimmunterricht: external location + travel time.**` and remove the entire bullet (multi-sentence; ends just before the next `- ` bullet, currently the Doppelbesetzung one).

- [ ] **Step 2: Strike the SchoolClass headcount deferral.**

Find the line beginning `**SchoolClass headcount / Klassenobergrenze.**` and remove the entire bullet.

- [ ] **Step 3: Strike the gesamtschule fixture deferral.**

Find the line beginning `**\`demo_gesamtschule\` bench fixture (Sek I, deferred).**` and remove the entire bullet (long; ends just before the next `- ` bullet, currently the synthetic-bench-fixture-generator one).

- [ ] **Step 4: Strike the Doppelbesetzung deferral.**

Find the line beginning `**Doppelbesetzung / Förderstunden with two teachers.**` and remove the entire bullet.

- [ ] **Step 5: Verify the deferrals are gone and no others were removed.**

Run: `grep -c "^- \*\*" docs/superpowers/OPEN_THINGS.md`
Note the count before the edits ran (use `git show HEAD:docs/superpowers/OPEN_THINGS.md | grep -c "^- \*\*"`); the post-edit count must be exactly four lower than the pre-edit count. If it differs, an extra bullet was struck or a wanted-to-keep bullet was left in; revert and retry.

### Task 5: Refresh the auto-memory roadmap entry

**Files:**
- Modify: `/home/pascal/.claude/projects/-home-pascal-Code-Klassenzeit/memory/project_roadmap_status.md`

- [ ] **Step 1: Replace the description and how-to-apply guidance.**

The new content captures the new sprint sequence and points the next /autopilot pickup at Sprint 1 P0 work.

```markdown
---
name: Roadmap status
description: Beyond-Grundschule sprint roadmap opened 2026-05-03. Realer Schulalltag closed 2026-05-02. Sprint 1 (Schwimmen + Sek-I foundations) active; Sprints 2-7 planned (Gymnasium Sek I → Wahlpflicht → Differenzierung → Sek II Kurssystem → Gesamtschule → Gymnasium full G9). Tidy items distributed per sprint.
type: project
originSessionId: 523adf68-4a0d-4893-a585-1ddf2d78697e
---
The project was rebuilt from scratch in April 2026.

**Solver-quality sprint (closed 2026-04-29):** all P0/P1 items shipped, plus P2 item 8 (Doppelstunden, PR #149).

**DX / CI infra hardening (closed 2026-04-30):** PR #151 (xdist), PR #152 (LAHC-deadline configurability + template-DB + budget gate, ADR 0020). Backend Test job dropped from 20:15 to 0:27.

**Realer Schulalltag sprint (closed 2026-05-02):** Algorithm phase fully shipped (cross-class hard constraint + lesson-group co-placement ADR 0022 + home-room ADR 0023 + avoid-last-period ADR 0024 + per-subject preference weights ADR 0025). Drop-tier item 10 (block-aware FFD eligibility) remains conditional on a placement-rate regression that has not surfaced. PR #161 tightened `.test-duration-budget` 600s → 120s; PR #162 closed structured-logging follow-up (a) via a request_id ContextVar + RequestIdFilter on the root stream handler.

**Beyond-Grundschule roadmap (opened 2026-05-03):** Seven sprints, prereqs flowing downhill. Sprint 1 active.

- **Sprint 1 (active): Schwimmen + Sek-I foundations.** P0 work: `Stundentafel.school_type` enum (`Grundschule` / `Hauptschule` / `Realschule` / `Gymnasium` / `Gesamtschule`) + grade-range expansion to 13; `Room.is_external` + per-Lesson travel buffers; solver `ViolationKind::TravelBufferConflict`; Klasse 3 Schwimmen Doppelstunde in `demo_grundschule_dreizuegig`. P1 tidy: einzügige solvability flake (conditional), `common.actions` i18n collapse, audit toast error fallbacks (`mutationErrorKey` helper). P2 (drop-tier): block-aware FFD eligibility for buffered lessons. Spec: `docs/superpowers/specs/2026-05-03-sprint-roadmap-beyond-grundschule-design.md`. Detailed per-sprint specs and plans get written when each sprint starts.
- **Sprint 2:** Gymnasium Sek I fixture (G9, einzügig 5-10) + `SchoolClass.headcount` + room-capacity check. Tidy: per-worker postgres, structured-logging (b).
- **Sprint 3:** Wahlpflichtfächer schema + UI + Sprint 2 fixture rows. Tidy: TRUNCATE reset, structured-logging (c).
- **Sprint 4:** Differenzierung (E-Kurs / G-Kurs) + full `demo_realschule`. Tidy: coverage split, structured-logging (d).
- **Sprint 5:** Sek II Kurssystem (Kurse + Kurswahl + Klausuren-Fenster + Sek-II schedule view). Tidy: structured-logging (e), Doppelbesetzung / Foerderstunden two-teacher schema.
- **Sprint 6:** `demo_gesamtschule` (24 classes Sek I, ~700 placements) using Wahlpflicht + Differenzierung primitives.
- **Sprint 7:** `demo_gymnasium_g9_full` (5-13) capstone, one researched dataset.

**How to apply:** When the user says "continue" / "next thing" / "next step", the next /autopilot pickup is Sprint 1 P0 schema work, in dependency order: (1) `Stundentafel.school_type` enum + grade-range expansion (smallest, unblocks every later sprint and is independent of Schwimmen), then (2) `Room.is_external` + per-Lesson travel buffers, then (3) solver `TravelBufferConflict`, then (4) the Klasse 3 Schwimmen seed lesson. Each is its own brainstorm + spec + plan + PR. P1 tidy items can ship as one bundled PR or interleaved between P0 PRs. The einzügige solvability flake is conditional: only investigate if it reproduces on master CI during this sprint.

**Explicitly deferred for the prototype:** Keycloak / OIDC, MFA, OAuth, typed deletion errors, `active` flag on WeekScheme, auto-infer time-block position, production deployment tier. Staging already auto-deploys on master push.
```

### Task 6: Verification + commit

**Files:**
- Stage: `docs/superpowers/OPEN_THINGS.md`

- [ ] **Step 1: Re-render the section index to confirm shape.**

Run: `grep -n "^## \|^### " docs/superpowers/OPEN_THINGS.md | head -20`
Expected (in order): `## Active sprint: Schwimmen + Sek-I foundations`, sub-headings for the four phases, `## Planned future sprints`, six `### Sprint N: ...` sub-headings, `## Completed sprints`, four sprint sub-headings (Realer Schulalltag first, then DX / CI, then Solver quality, then Prototype), then sub-headings inside DX / CI sub-section ("Profiling note (PR-2)") if any, `## Acknowledged deferrals`, `## Backlog`.

- [ ] **Step 2: Confirm the four struck deferrals are absent.**

Run: `grep -E "Schwimmunterricht: external|SchoolClass headcount / Klassenobergrenze|demo_gesamtschule\` bench fixture|Doppelbesetzung / Förderstunden" docs/superpowers/OPEN_THINGS.md`
Expected: empty output (no matches). The Schwimmunterricht and headcount references that remain in the file are the new sprint sections, not the old Acknowledged-deferrals bullets.

- [ ] **Step 3: Stage and commit.**

Stage exactly: `docs/superpowers/OPEN_THINGS.md`. Auto-memory file lives outside the repo and does not get committed. Confirm with `git status` that nothing else is staged.

```bash
git add docs/superpowers/OPEN_THINGS.md
git status
git commit -m "$(cat <<'EOF'
docs(open-things): retire Realer Schulalltag, open Beyond-Grundschule roadmap

Move the Realer Schulalltag section into Completed sprints with a
closing note (functionally closed 2026-05-02; item 10 conditional).
Open Sprint 1 (Schwimmen + Sek-I foundations) as the new active sprint
with tiered phases (schema → solver → seed → tidy). Append a Planned
future sprints section listing Sprints 2-7 with prereq notes.

Strike four superseded deferrals from Acknowledged deferrals:
Schwimmunterricht (Sprint 1 P0), SchoolClass headcount (Sprint 2 P0),
demo_gesamtschule (Sprint 6), Doppelbesetzung / Foerderstunden
(Sprint 5 tidy item).

Spec: docs/superpowers/specs/2026-05-03-sprint-roadmap-beyond-grundschule-design.md
EOF
)"
```

Pre-commit lefthook will run ruff / ty / vulture / biome / clippy / actionlint / design lint / commit-types / use-effect-sync / unique-fns. None should trigger on a docs-only change. `cog verify` accepts `docs(open-things)` (a `docs` type with the `open-things` scope). If any hook fails, fix the underlying issue (do NOT skip hooks).

---

## Self-review checklist

- **Spec coverage.** Spec lists five goal items: (1) move closed sprint → Task 1; (2) add Sprint 1 active section → Task 2; (3) add Planned future sprints section → Task 3; (4) strike four superseded deferrals → Task 4; (5) refresh auto-memory roadmap → Task 5. Verification + commit → Task 6. Every spec section has a task.
- **Placeholders.** None. Every step shows the exact text or command. The Sprint 6 and Sprint 7 tidy mixes intentionally say "picked when the sprint brainstorm runs" because the per-sprint brainstorm is the right place to pick them; this is a deliberate design choice from the spec, not a placeholder.
- **Type consistency.** `school_type` (snake_case) consistent across spec, Sprint 1 P0 item 1, Sprint 2 prereq mention, and the auto-memory entry. `is_external` (snake_case) consistent across Sprint 1 P0 item 2 and the seed task. `pre_buffer_minutes` / `post_buffer_minutes` consistent across the schema task and the solver task. `ViolationKind::TravelBufferConflict` PascalCase consistent with the existing solver violation taxonomy in `solver-core`.
- **Risks.** The OPEN_THINGS file is large; Tasks 1-4 are sequential edits to one file. A subagent doing multi-line `Edit` operations may hit "old_string not unique" errors if the bullet phrasing matches elsewhere in the file. Use larger surrounding context windows or `replace_all=false` with surrounding text in each Edit call. The verification grep at the end of each task catches mistakes early.
