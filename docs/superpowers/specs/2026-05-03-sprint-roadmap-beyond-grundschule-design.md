# Sprint roadmap: Beyond Grundschule

**Date:** 2026-05-03
**Status:** Design approved (autopilot autonomous mode).

## Context

The Realer Schulalltag sprint (active 2026-04-30 to 2026-05-02) closed all P0/P1 plus drop-tier item 9 (per-subject preference weights, ADR 0025); item 10 (block-aware FFD eligibility) remains conditional on a placement-rate regression that has not surfaced. PR #162 (request-id contextvar, structured-logging follow-up (a)) shipped 2026-05-03 as the first post-sprint tidy item.

The next phase widens the demo from "einzügige Hessen Grundschule alltag" to a multi-Schulform domain that exercises Sekundarstufe I and II patterns with research-backed real-world fixture data. The user explicitly framed this as a multi-sprint roadmap with cleanups distributed across sprints; this PR captures the roadmap and replaces the closed-sprint section in `OPEN_THINGS.md` accordingly.

Light Hessen Schulform research informed the sprint definitions:

- Hessen Gymnasium runs G9 (Klasse 5-10) on a Kontingent-Stundentafel; schools have G8/G9 Wahlfreiheit. Wahlpflicht / dritte Fremdsprache is a Klasse 9/10 Mittelstufe lever ([Hessisches Kultusministerium - Gymnasiale Mittelstufe](https://kultus.hessen.de/Schulsystem/Schulformen-und-Bildungsgaenge/Gymnasium/Gymnasiale-Mittelstufe), [G8 und G9](https://kultus.hessen.de/Schulsystem/Schulformen-und-Bildungsgaenge/Gymnasium/G8-und-G9)).
- Gesamtschule integriert Hauptschule + Realschule + Gymnasium-Mittelstufe in one institution; Realschule remains a separate Bildungsgang at eigenständige Realschulen, verbundene Haupt-/Realschulen, Mittelstufenschulen, and kooperative Gesamtschulen ([Realschule Hessen](https://www.realschule-hessen.de/)).
- Sek II Kurssystem: Einführungsphase (E1, E2) plus zweijährige Qualifikationsphase (Q1, Q2, Q3, Q4); 5h Leistungskurs (+ optionales Tutorium), 4h Grundkurs in D / M, 3h Grundkurs in FL / PoWi / Geschichte / Geographie / NaWi; Abitur portfolio 24 GK + 8 LK across drei Aufgabenfelder, max 6 Minderleistungen ([Kerncurriculum gymnasiale Oberstufe Mathematik](https://kultus.hessen.de/sites/kultus.hessen.de/files/2025-10/kerncurriculum_gymnasiale_oberstufe-mathematik.pdf)).

Per-sprint research goes into each sprint's own spec when it is brainstormed; this PR's research is intentionally light, just enough to keep the sprint scopes accurate.

## Goal

One docs-only PR that:

1. Moves the Realer Schulalltag sprint section in `OPEN_THINGS.md` into Completed sprints with a closing note.
2. Adds a new active sprint section for Sprint 1 (Schwimmen + Sek-I foundations) with its tier list.
3. Adds a Planned future sprints section listing Sprints 2 to 7 with one-paragraph definitions and prereq notes.
4. Pulls in-scope deferrals into the matching sprint sections (Schwimmunterricht, SchoolClass headcount, the Doppelbesetzung two-teacher item) and leaves out-of-scope deferrals where they are.
5. Refreshes the auto-memory roadmap entry to point at the new active sprint.

## The seven sprints

### Sprint 1: Schwimmen + Sek-I foundations (active)

Schwimmunterricht (external rooms + travel-time buffers + a Klasse 3 Schwimmen Doppelstunde landed in the dreizügige Grundschule fixture) plus the load-bearing prereq for every later sprint: a `Stundentafel.school_type` enum (`Grundschule` / `Hauptschule` / `Realschule` / `Gymnasium` / `Gesamtschule`) and a grade-range expansion to 13. Tidy mix: einzügige solvability flake (only if it reproduces on master CI), `common.actions` i18n collapse, audit toast error fallbacks via a `mutationErrorKey` helper.

### Sprint 2: Gymnasium Sek I fixture (G9, einzügig 5-10)

Real `demo_gymnasium_einzuegig` seed researched against the Hessisches Kultusministerium G9 Kontingent-Stundentafel for Klasse 5-10. First non-Grundschule fixture; exercises the school-type enum and a secondary-school subject set (Latein / Französisch as 2. Fremdsprache, NaWi, PoWi, Sport differenziert nach Geschlecht). Adds `SchoolClass.headcount: int | None` plus a pre-solve check "all rooms a lesson can land in must have `capacity >= school_class.headcount` (or null)". Tidy: per-worker postgres process for backend tests, structured-logging follow-up (b) (uvicorn access-log dedup).

### Sprint 3: Wahlpflichtfächer

New `WahlpflichtGroup` schema (a parent record naming the choice; per-Klasse rows recording which subject each Klasse picked from the group). Solver constraint: each Klasse picks exactly one Wahlpflicht subject per group; placements honour that pick. UI for Wahlpflicht setup. Real Klasse 8 / 9 / 10 Wahlpflicht fixture rows (3. Fremdsprache vs NaWi vs Informatik vs Kunst-AG, per Hessen G9 Mittelstufe lever). Builds on Sprint 2's Gymnasium fixture. Tidy: TRUNCATE-based per-test reset, structured-logging (c) (request body / Content-Length).

### Sprint 4: Differenzierung (E-Kurs / G-Kurs)

`KursAufteilung` schema for Mathematik / Englisch / Deutsch differenziert (E-Kurs / G-Kurs) at Realschule and Gesamtschule. Solver constraint: a differenziertes Lesson-Paar shares its time-block (parallel split) but uses two teachers and two rooms. Full `demo_realschule` fixture researched against Hessen Realschule Stundentafel. Tidy: coverage split (master-only CI job), structured-logging (d) (GCP / ECS field renames).

### Sprint 5: Sek II Kurssystem

The hardest schema pivot: Sek II introduces `Kurs` rows that have no fixed `SchoolClass` (Kurse cross Klassen) and per-Student `Kurswahl` rows recording which 2 LK + ~8 GK each student picks. New phase enum: `Einfuehrungsphase` (E1, E2) plus `Qualifikationsphase` (Q1, Q2, Q3, Q4). Klausuren-Fenster scheduling. Sek-II-shaped schedule view (per-Kurs and per-Schueler). Lands on its own because every assumption from "Lessons are per-Klasse" needs a parallel "Kurse are per-Kurs" track. No new fixture in this sprint; existing fixtures remain Sek I only. Tidy: structured-logging (e) (CRUD route handler instrumentation), Doppelbesetzung / Foerderstunden two-teacher schema (the existing `Lesson.co_teacher_id: uuid.UUID | None` deferral; natural fit because Sek II Kurse often have a Tutor in addition to the Fachlehrer).

### Sprint 6: Gesamtschule full Sek-I fixture

Real `demo_gesamtschule` (24 classes Sek I, ~50 teachers, ~31 rooms, ~700 placements) using Wahlpflicht (Sprint 3) + Differenzierung (Sprint 4) primitives. Researched against a Hessen integrierte Gesamtschule (IGS) Stundentafel Sek I. First fixture that exercises both Sek-I primitives at scale; supersedes the long-standing OPEN_THINGS deferral.

### Sprint 7: Gymnasium full G9 fixture (5-13)

Real `demo_gymnasium_g9_full` exercising Wahlpflicht (Sprint 3) + Sek II Kurssystem (Sprint 5) on one school. Capstone fixture: Klasse 5 Latein-Anfaenger to Q4 Leistungskurs-Klausuren, one researched dataset.

## Tidy distribution

| Sprint | Tidy items |
|---|---|
| 1 | Einzügige solvability flake (conditional), `common.actions` i18n collapse, audit toast error fallbacks (`mutationErrorKey` helper) |
| 2 | Per-worker postgres for backend tests, structured-logging (b) (uvicorn access-log dedup) |
| 3 | TRUNCATE-based per-test reset, structured-logging (c) (request body / Content-Length) |
| 4 | Coverage split (master-only CI job), structured-logging (d) (GCP / ECS field renames) |
| 5 | Structured-logging (e) (CRUD route handler instrumentation), Doppelbesetzung / Foerderstunden two-teacher schema |
| 6 | (filled per-sprint when brainstormed) |
| 7 | (filled per-sprint when brainstormed) |

Sprints 6 and 7 are fixture-heavy and may not need a tidy mix; the per-sprint brainstorm picks any open quality items at the time.

## Non-goals (this PR)

- **No code changes.** Pure docs reorganisation. Each sprint produces its own code PRs when brainstormed.
- **No new ADR.** Sprint roadmap is operational planning, not an architectural decision; ADR shape is reserved for load-bearing design decisions inside individual sprint PRs.
- **No detailed per-sprint research.** Each sprint's first PR brainstorm runs the deeper research (specific Stundentafel hours per Jahrgang, Lehrerdeputat per Schulform, room types per Bildungsgang). This roadmap PR's research is intentionally limited to confirming the sprint themes are real.

## Verification

- `OPEN_THINGS.md` parses (markdown headings render as a list of completed sprints, one active sprint, a planned-sprints section, and the existing Acknowledged deferrals + Backlog sections in their existing order).
- The Realer Schulalltag section appears once, under Completed sprints, with the closing note "functionally closed 2026-05-02".
- The Schwimmunterricht deferral no longer lives in Acknowledged deferrals; the SchoolClass headcount deferral no longer lives there. Both appear inside their respective sprint sections.
- The auto-memory roadmap entry points at Sprint 1 as the new active sprint.
