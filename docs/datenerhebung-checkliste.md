# Datenerhebungs-Checkliste für Klassenzeit

Ziel: vollständige reale Datensätze von zwei Grundschulen (Frau und Schwiegermutter), um Solver und UI gegen echte Stundenpläne zu validieren. Beide liefern jeweils einen separaten, vollständigen Schul-Datensatz. Wo sinnvoll, ist Anonymisierung vermerkt (Lehrkräfte als Kürzel, keine Schülerdaten).

## A. Pro Schule: Stammdaten

- [ ] Schulname, Schulart (Grundschule, Verlässliche GS, Ganztag, OGS), Bundesland
- [ ] Zügigkeit pro Jahrgangsstufe (1, 2, 3, 4 → ein-, zwei-, dreizügig?)
- [ ] Anzahl Klassen pro Jahrgang inkl. Bezeichnung (1a, 1b, …)
- [ ] Schülerzahl pro Klasse (Richtwert reicht, anonym)
- [ ] Jahrgangsgemischte Klassen ja/nein, ggf. Kombination (z. B. 1/2-Mischklasse)
- [ ] Schuljahresgrenzen, Ferienkalender, Feiertage des Landes
- [ ] Pädagogische Tage / SchiLF-Tage, an denen kein Unterricht stattfindet

## B. Pro Schule: Zeitstruktur

- [ ] Stundenraster: Beginn und Ende jeder Stunde (z. B. 1. Std 7:55 – 8:40)
- [ ] Pausen mit genauen Uhrzeiten (Hofpause, Frühstückspause, Mittagspause)
- [ ] Anzahl regulärer Unterrichtsstunden pro Wochentag
- [ ] Gibt es 0. Stunden, Nachmittagsunterricht, AG-Schienen?
- [ ] Verlässliche Betreuungszeiten (von … bis), inkl. Übergangszeit zur OGS
- [ ] Sonderregelungen für Erstklässler in den ersten Wochen (kürzere Tage)

## C. Pro Schule: Räume

- [ ] Liste aller Klassenräume mit Bezeichnung und Stockwerk
- [ ] Fachräume: Sporthalle(n), Musikraum, Werkraum, Computerraum, Küche, Aula
- [ ] Räume mit Doppelnutzung (Mehrzweck), Belegungseinschränkungen
- [ ] Externe Räume (Sporthalle nicht im Gebäude, Schwimmhalle), Wegezeit in Minuten
- [ ] Raumkapazität, falls relevant
- [ ] Welche Klasse hat einen festen "Heimraum" (Klassenraumprinzip)?

## D. Pro Schule: Lehrkräfte (anonymisiert als Kürzel)

- [ ] Kürzel-Liste aller Lehrkräfte plus Förderkräfte, Referendar:innen, FSJ
- [ ] Beschäftigungsumfang in Wochenstunden (Soll-Stunden)
- [ ] Lehrbefähigungen / Fakultas pro Person (welche Fächer unterrichtbar)
- [ ] Klassenleitung: welche Lehrkraft führt welche Klasse?
- [ ] Teilzeit-Sperrungen: feste freie Tage / Halbtage, Sprechtage, Schwerbehindertenstunden
- [ ] Reduktionen (Altersermäßigung, Schulleitungsanrechnung, Mentor:in)
- [ ] Lehrkräfte, die an mehreren Schulen arbeiten (Pendel-Zeiten)
- [ ] Konfessionszuordnung für Reli-Lehrkräfte (ev, kath, Ethik)
- [ ] Erfahrungswerte: welche Lehrkraft sollte nicht in der 1. oder 6. Stunde sein, warum?

## E. Pro Schule: Fächer und Stundentafel

- [ ] Stundentafel je Jahrgangsstufe (Soll-Stunden pro Fach pro Woche)
- [ ] Differenzierung Reli/Ethik: parallele Schienen, Gruppengrößen
- [ ] Englisch: ab welchem Jahrgang, wie viele Stunden
- [ ] Sport: einzelne Stunden oder Doppelstunden, Schwimmunterricht in welchem Jahrgang
- [ ] Förderunterricht / DaZ / LRS / Dyskalkulie: Umfang und Gruppierung
- [ ] AGs / Wahlpflicht / Chor / Forscher-AG: Teilnehmerlogik, Schiene
- [ ] Doppelbesetzungen / Teamteaching (z. B. Inklusion): welche Stunden, welches Personal
- [ ] Fachunterricht vs. Klassenlehrerprinzip: welche Fächer dürfen nur von der Klassenleitung

## F. Pro Schule: Kopplungen und Constraints

- [ ] Welche Klassen müssen parallel laufen (z. B. Reli/Ethik-Schiene)
- [ ] Welche Klassen dürfen nicht parallel (gemeinsame Sporthalle)
- [ ] Doppelstunden-Pflicht (Sport, Schwimmen, Kunst, Werken)
- [ ] Maximale Anzahl Stunden eines Fachs pro Tag pro Klasse
- [ ] Hauptfächer (Deutsch/Mathe) bevorzugt in den ersten Stunden?
- [ ] Sport nicht direkt nach der Mittagspause?
- [ ] Pausenaufsichten: wer hat wann, wie oft pro Woche
- [ ] Vertretungsreserve / Springer-Stunden, welche Lehrkraft pro Tag

## G. Pro Schule: Bestehende Pläne als Goldstandard

- [ ] Aktueller Stundenplan pro Klasse (PDF, Foto oder Excel) im laufenden Schuljahr
- [ ] Aktueller Lehrerplan (pro Lehrkraft) im laufenden Schuljahr
- [ ] Raumbelegungsplan, falls separat geführt
- [ ] Plan aus dem Vorjahr (Vergleich Stabilität / Heimraum-Konstanz)
- [ ] Vertretungsplan-Muster der letzten ein bis zwei Wochen (Realitätstest)
- [ ] Mit welchem Tool wurde geplant (Untis, Plan-S, Excel, händisch)?
- [ ] Wie lange dauert eine Planungsrunde, was sind die größten Schmerzpunkte?

## H. Pro Schule: Pädagogische Sonderfälle

- [ ] Inklusionsschüler:innen mit Bedarfen (anonym, nur Anzahl + Unterstützungsstunden)
- [ ] DaZ-Gruppen, Kurszuschnitt, Personaleinsatz
- [ ] Begabtenförderung / Forderunterricht
- [ ] Kooperation mit Kita (Vorschulkinder) oder weiterführender Schule
- [ ] Feste schulische Veranstaltungen (Wochenausflug, Lesenacht), die regelmäßig den Plan brechen
- [ ] Schwimmunterricht extern: Buszeiten, Begleitlehrkräfte

## I. Workflows und Schmerzpunkte (Interview, nicht Liste)

Diese Punkte am besten im Gespräch mit beiden Personen einsammeln und protokollieren.

- [ ] Wie entsteht der Stundenplan heute? Wer entscheidet, wer prüft, wer beschwert sich?
- [ ] Welche Wünsche darf jede Lehrkraft äußern, welche werden meist erfüllt?
- [ ] Was geht schief, wenn jemand krank wird oder kurzfristig kündigt?
- [ ] Welche Constraints sind "harte" Schul- bzw. Rechtsvorgaben, welche sind Komfort?
- [ ] Was wäre der Wow-Moment, wenn ein Tool das automatisch löst?
- [ ] Welche Daten liegen bereits digital vor, welche nur auf Papier oder im Kopf?

## J. Datenschutz und Übergabeformat

- [ ] Lehrkräfte als Kürzel, keine Klarnamen, keine Personalnummern
- [ ] Keine Schülerdaten, nur aggregierte Zahlen pro Klasse
- [ ] Übergabe per verschlüsseltem Anhang oder USB-Stick, nicht per WhatsApp-Foto in der Cloud
- [ ] Schriftliche Zustimmung beider Schulleitungen, dass anonymisierte Strukturdaten genutzt werden dürfen
- [ ] Klärung, ob die Daten nur lokal für Tests oder auch im seed-Datensatz landen dürfen

## K. Lieferformat (Wunsch, soweit zumutbar)

- [ ] Eine Excel/CSV pro Schule mit Tabs: Klassen, Lehrkräfte, Fächer, Stundentafel, Räume, Zeitraster, Kopplungen
- [ ] Alternativ: Originale (PDF/Foto) plus 30 Minuten gemeinsames Abtippen
- [ ] Ein kurzes Glossar schulinterner Begriffe (z. B. "Bandstunde", "Sternstunde", "Lernzeit")

## Pragmatischer Reihenfolge-Vorschlag

1. Schwiegermutter und Frau jeweils 20 Minuten Interview zu I (Workflows, Schmerzpunkte).
2. Aktuellen Stundenplan + Lehrerplan + Stundentafel + Raumliste anfordern (G, B, C, E).
3. Lehrkräfteliste mit Kürzel und Fakultas (D), Kopplungen (F).
4. Sonderfälle (H) und Datenschutzfreigabe (J) klären.
5. Zweite Schule analog, danach Diff prüfen: was unterscheidet die Schulen strukturell?
