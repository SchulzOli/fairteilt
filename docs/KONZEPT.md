# Fairteilt – Konzept

Stand: 23.11.2026 · Status: Entwurf v0.3

**Fairteilt** ist eine Familien-App für Haushaltsaufgaben mit XP, periodischem Ausgleich, Monatsziel
und Aufholaufgaben, Erinnerungen sowie Kalender- und Canvas-Planung.

> **Vereinfacht am 23.11.2026** (gilt vor allen älteren Stellen in diesem Dokument):
> - **Nur noch XP.** Guthaben (G), Übertragungen und der Belohnungskatalog sind aus der App entfernt.
>   Geblieben ist das **Monatsziel** der Familie (Rennen gegen Krümel, mit Familienbelohnung), das
>   Erwachsene unter Punkte und Regeln einrichten.
> - **XP-Bonus statt Kopfgeld:** Jeder kann XP als Bonus auf Aufgaben anderer setzen. Das ist ein
>   Geschenk (kostet nichts) mit einem Monatsbudget pro Person (Regel `bonus_xp_per_month`, Standard 30).
> - **Mehrere auf einer Aufgabe:** „+“ übernimmt eine offene Aufgabe oder macht bei einer fremden mit.
>   Alle, die drauf sind, teilen sich XP und Bonus gleichmäßig (wer erledigt, zählt automatisch dazu).
> - **Kein Timer:** kein Starten, Pausieren, keine Dauer und kein „Wie läuft's?“. Eine Aufgabe wird
>   auf ihrer Seite erledigt (oder mit dem letzten Schritt). Einkaufen gilt ab dem ersten Artikel im
>   Wagen als „im Laden“.
> - **Zustand in vier Stufen:** Gut (< 50 % der Runde), Hat Zeit (< 75 %), Fällig (< 100 %),
>   Überfällig; ein Balken, der sich von Grün nach Rot färbt. Der Text „in X Tagen fällig“ entfällt
>   auf den Karten. Fremde Aufgaben stehen auf Heute ausgegraut am Ende.
Plattformunabhängig als **Tauri-2-App**, gebaut vor allem fürs Handy. Als Nächstes folgt eine
gemeinsame Einkaufsliste (§10); ein Finanzteil ist nicht mehr geplant.

---

## 1. Entscheidungen

| Thema | Entscheidung | Warum |
| --- | --- | --- |
| Client | Tauri 2 + SvelteKit (Svelte 5, SPA, `adapter-static`), TypeScript | eine Codebasis für Android, iOS und Desktop, kleine Bundles, flott auf dem Handy |
| Backend | eigener Server in Rust (axum) + PostgreSQL | mehrere Nutzer brauchen eine gemeinsame Wahrheit. Ausgleich und Erinnerungen laufen zeitgesteuert auf dem Server |
| Hosting | Hetzner Cloud (Rechenzentrum in DE), Docker Compose, Caddy (TLS), **`fairteilt.oli-tech.eu`** | öffentlich erreichbar ohne Tailscale auf jedem Familienhandy, DSGVO-freundlich |
| App-Kennung | Name **Fairteilt**, Kennung `eu.olitech.fairteilt` | passt zur Domain `oli-tech.eu`. Nach dem ersten Store-Release nicht mehr änderbar |
| Firebase | **FCM** (Android-Push) und **App Distribution** (Testversionen an die Familie), optional später **Hosting** für eine Web-Version | Backend und Datenbank bleiben auf Hetzner: Firebase führt keinen Rust-Container mit Postgres aus (nur über Cloud Run + Cloud SQL, teurer) |
| Gemeinsame Logik | Rust-Crate `punkte-core` (rein, ohne IO) | dieselbe Punkte- und Ausgleichslogik auf dem Server und (für Offline-Vorschau) in der App, gut testbar |
| Umfang v1 | Aufgaben, Punkte, Ausgleich, Belohnungen/Strafen, Erinnerungen, Kalender/Canvas, danach Einkaufsliste (§10) | Ein Finanzmodul (Haushaltsbuch) ist nicht mehr geplant |

---

## 2. Begriffe

| Begriff | Bedeutung |
| --- | --- |
| **Haushalt** | eine Familie bzw. WG. Ein Nutzer kann in mehreren Haushalten sein (z. B. Patchwork, getrennte Eltern) |
| **Mitglied** | Mitgliedschaft eines Nutzers in einem Haushalt, mit Rolle, Gewicht und Farbe |
| **Vorlage** | wiederverwendbare Aufgabe („Spülmaschine ausräumen“) mit Punkten, Wiederholung, Zuweisungsregel |
| **Aufgabe** | ein konkretes Vorkommen mit Fälligkeit, Zuständigem und Status |
| **Leistungspunkte (LP)** | messen die geleistete Arbeit. Grundlage für den Ausgleich, **nicht übertragbar**. In der Oberfläche heißen sie **XP** (Entscheidung 22.11.2026), im Konzept und Code bleibt es bei LP bzw. `contribution` |
| **Guthaben (G)** | *entfernt am 23.11.2026* (war: übertragbare Währung für Belohnungen und Boni) |
| **Bonus** | XP, die ein Mitglied als Geschenk auf die Aufgabe eines anderen setzt (Monatsbudget). Bekommen, wer sie erledigt, geteilt wie die XP |
| **Periode** | Abrechnungszeitraum (Woche, Monat, frei) für den Ausgleich |

> **Warum zwei Punktearten?** Würde nur eine Zahl zählen, ließe sich die Fairness durch
> Verschenken von Punkten aushebeln. So zählt für den Ausgleich nur echte Arbeit (LP).
> Guthaben darf frei fließen, ohne die Gerechtigkeitsrechnung zu verfälschen.

---

## 3. Rollen und Rechte

| Rolle | Darf |
| --- | --- |
| **Admin** (meist Eltern) | Regeln und Punktegrenzen ändern, Vorlagen verwalten, Aufgaben bestätigen/ablehnen, Aufholaufgaben vergeben, Ausgleich freigeben, Mitglieder einladen/entfernen |
| **Mitglied** | Aufgaben anlegen (im Rahmen der Grenzen), übernehmen, erledigen, Guthaben übertragen und als Bonus setzen, Belohnungen einlösen, Aufgaben anderer bestätigen (wenn erlaubt) |
| **Kind** | wie Mitglied, aber: keine eigenen Aufgaben bestätigen, Übertragungen/Einlösungen ggf. mit Freigabe, keine Regeländerungen. Ohne eigene E-Mail (Profil vom Admin verwaltet). Anmeldung **auf eigenem Gerät per Einladungscode oder per PIN-Profilwechsel auf einem gemeinsamen Familien-Tablet**, beides möglich |

Grundsatz: **Niemand bestätigt die eigene Aufgabe**, wenn der Haushalt Bestätigung verlangt.

**PIN-Anmeldung (Familien-Tablet):** Eine PIN (4–6 Ziffern, Argon2id-Hash) funktioniert nur von einem
Gerät, das selbst im selben Haushalt angemeldet ist. Nach 5 Fehlversuchen ist das Profil 5 Minuten
gesperrt; eine neue PIN durch Admins hebt die Sperre auf. Die Sitzung des Kindes wird auf dem Tablet
nicht gespeichert und endet nach 2 Minuten ohne Eingabe oder mit „Zurück“.

---

## 4. Punktesystem (einstellbar)

### 4.1 Haushaltsweite Regeln (`PointPolicy`)

| Einstellung | Beispiel | Wirkung |
| --- | --- | --- |
| `task_min` / `task_max` | 1 / 100 | harte Grenzen für jede Aufgabe |
| `default_points` | 10 | Vorschlag für neue Aufgaben |
| `escalation` | an, +2 je 24 h, bis Aufgaben-Max | liegengebliebene Aufgaben werden wertvoller |
| `bounty.enabled` | an | Bonus auf Aufgaben erlaubt |
| `bounty.max_per_pledge` | 50 G | Obergrenze je Einsatz |
| `transfer.enabled` | an | Guthaben an Personen übertragen |
| `transfer.max_per_period` | 100 G | Limit je Mitglied und Periode |
| `transfer.child_needs_approval` | an | Kinder brauchen Freigabe |
| `approval` | `children_only` | Wer Bestätigung braucht: `everyone` · `children_only` · `nobody`. Pro Vorlage überschreibbar (immer / nie / mit Foto) |
| `quality_adjust` | an | Bestätiger darf Punkte innerhalb von min…max anpassen |

### 4.2 Punkte je Aufgabe

Jede Vorlage/Aufgabe hat `points_min ≤ points_default ≤ points_max`, alle innerhalb der Haushaltsgrenzen.

Wert bei Erledigung:

```
basis   = clamp(points_default + eskalation(alter), points_min, points_max)
basis'  = Bestätiger-Anpassung (optional, innerhalb points_min…points_max)
LP      += basis'
G       += basis' + Summe aller Boni auf dieser Aufgabe
```

`eskalation(alter) = floor(alter / step_interval) × step_points`. Alter läuft ab Fälligkeit
(oder ab Anlage bei offenen Aufgaben ohne Termin).

### 4.3 Bonus auf Aufgaben (eigene Punkte übertragen)

- Ein Mitglied setzt eigenes Guthaben auf eine Aufgabe („Wer das Bad putzt, bekommt 20 von mir“).
- Das Guthaben wird **sofort reserviert** (Treuhand-Buchung), damit niemand mehr verspricht, als er hat.
- Wird die Aufgabe bestätigt → Auszahlung an den Erlediger. Wird sie abgebrochen oder läuft ab → Rückbuchung.
- Den Bonus auf die eigene, einem selbst zugewiesene Aufgabe zu setzen, ist nicht erlaubt.

### 4.4 Übertragung an Personen

- Direkte Übertragung von Guthaben (Geschenk, Tausch „Ich übernehme deinen Müll-Dienst für 15“).
- Grenzen aus `PointPolicy`, Kinder optional mit Freigabe, immer mit Notiz.
- Leistungspunkte bleiben unberührt.

### 4.5 Punktebuch (Ledger)

Alle Punktebewegungen sind **unveränderliche Buchungen**. Stände sind Summen darüber.

| Feld | Inhalt |
| --- | --- |
| `id` | UUID (vom Client erzeugt → idempotent bei Offline-Sync) |
| `household_id`, `member_id` | wessen Konto |
| `kind` | `task_reward`, `bounty_hold`, `bounty_payout`, `bounty_refund`, `transfer_out`, `transfer_in`, `reward_redeem`, `penalty_deduction`, `settlement_bonus`, `settlement_zero_sum`, `manual_adjust` |
| `contribution_delta` | Änderung der Leistungspunkte (meist 0 außer bei `task_reward`) |
| `balance_delta` | Änderung des Guthabens |
| `ref_type`, `ref_id` | Bezug (Aufgabe, Übertragung, Abrechnung …) |
| `period_id` | in welcher Periode gebucht |
| `created_by`, `created_at`, `note` | Nachvollziehbarkeit |

Korrekturen erfolgen durch Gegenbuchung, nie durch Ändern. Das Guthaben darf nicht negativ werden
(Ausnahme: `penalty_deduction`, wenn der Haushalt das erlaubt).

---

## 5. Periodischer Ausgleich (Gleichstellung)

### 5.1 Einstellungen (`PeriodPolicy`)

| Einstellung | Optionen |
| --- | --- |
| `length` | `weekly(start_weekday)`, `monthly(start_day)`, `custom(days)` |
| `mode` | `fair_share` (relativ zum Gewicht) · `fixed_quota` (feste Mindestpunkte je Mitglied) · `both` |
| `tolerance` | Abweichung ohne Folgen (und ohne Übertrag), in % |
| `deficit_action` | `penalty_tasks` · `balance_deduction(percent)` · `zero_sum` (zahlt an die Überschüssigen) · `carry_over(percent)` (Fehlbetrag erhöht das Ziel der nächsten Periode) · `none` |
| `surplus_action` | `bonus(percent)` · `carry_over(percent)` (Vorsprung senkt das Ziel der nächsten Periode) · `none`. Bei `zero_sum` bekommen Überschüssige ihren Anteil zusätzlich |

**Voreinstellung für neue Haushalte:** monatlich ab dem 1., `fair_share`, **20 % Toleranz**,
Rückstand **und** Vorsprung wandern zu 100 % in den nächsten Monat (`carry_over` auf beiden
Seiten), Auto-Freigabe nach 24 h, Warnung 48 h vor Monatsende. Wer im Rahmen bleibt, nimmt
nichts mit. Ein Prozentsatz unter 100 lässt alte Stände mit der Zeit verfallen, damit sich
kein riesiges Polster oder ein nie abzutragender Rückstand aufbaut.
| `auto_approve_after_hours` | Vorschau für Admins, automatische Freigabe nach X Stunden |
| `warn_before_end_hours` | Warnung an Mitglieder unter dem Ziel vor Periodenende |

### 5.2 Rechnung

Für jedes Mitglied *m* in Periode *P*:

```
verfügbarkeit a_m  = Anteil der Tage ohne Abwesenheit (Urlaub, krank)  ∈ [0, 1]
gewicht      w_m   = Kapazität (Erwachsener 1.0, Teenager 0.6, Kind 0.3 …)
leistung     c_m   = Summe LP in P
gesamt       T     = Σ c_m
ziel         t_m   = T · (w_m·a_m) / Σ(w·a)          (fair_share)
             t_m   = quota_m · a_m                   (fixed_quota)
             t_m   = max(beide)                      (both)
             t_m   = max(0, t_m + carry_in_m)        (Übertrag: Rückstand > 0, Vorsprung < 0)
abweichung   d_m   = c_m − t_m
relativ      r_m   = d_m / t_m
```

| Ergebnis | Bedingung | Folge (je nach Einstellung) |
| --- | --- | --- |
| im Rahmen | `|r_m| ≤ tolerance` | nichts |
| Fehlbetrag | `r_m < −tolerance` | Aufholaufgaben im Wert von ≈ `|d_m|` zuweisen **oder** Guthaben-Abzug **oder** Nullsummen-Ausgleich an die Überschüssigen |
| Überschuss | `r_m > +tolerance` | Guthaben-Bonus `d_m × percent` und/oder Anteil am Nullsummen-Ausgleich |

Ist das Ziel 0 (Gewicht 0 oder niemand hat etwas getan), gilt: wer trotzdem etwas geleistet hat,
liegt im Überschuss, sonst im Rahmen. Mit `fair_share` allein hat also niemand einen Fehlbetrag,
wenn alle nichts tun. Dafür gibt es `fixed_quota`/`both`.

**Nullsummen-Modus:** Wer unter dem Ziel liegt, zahlt Guthaben an die, die darüber liegen.
Der Topf ist `min(Σ Fehlbeträge, Σ Überschüsse)`, verteilt anteilig auf beiden Seiten
(ganzzahlig nach dem Verfahren des größten Rests). Die Summe aller Guthaben bleibt gleich.
Ist das Guthaben eines Zahlers zu klein, kappt der Server den Betrag, sofern der Haushalt keine
negativen Stände erlaubt.

**Aufholaufgaben-Auswahl:** Aus den Vorlagen mit `kind = penalty` werden Aufgaben gewählt, bis
ihre Punkte den Fehlbetrag decken (gierig: die größte zuerst, die noch passt, dann auffüllen).
Aufholaufgaben bringen **Leistungspunkte, aber kein Guthaben** und haben eine Frist.

### 5.3 Ablauf

1. Periodenende → Server berechnet die Abrechnung als **Vorschau** (`settlement.status = draft`).
2. Benachrichtigung an Admins, Übersicht für alle („Wer lag wo?“).
3. Freigabe (manuell oder automatisch nach X h) → Buchungen + Aufholaufgaben werden erzeugt.
4. Zwei Tage vor Periodenende: Warnung an Mitglieder unter dem Ziel.

Die Rechnung steckt in `punkte-core::settlement` und ist mit Tests abgedeckt.

---

## 6. Belohnungen, Aufholaufgaben und Familienziel

| Typ | Beschreibung |
| --- | --- |
| **Belohnungskatalog** | Dinge, die man mit Guthaben „kauft“: „Film aussuchen“ (30 G), „30 min länger wach“ (50 G). Optional Freigabe, Bestand, Abkühlzeit |
| **Belohnungsaufgabe** (`kind = reward`) | beliebte Aufgaben mit Extra-Punkten, freigeschaltet als Belohnung („Kuchen backen“) |
| **Aufholaufgabe** (`kind = penalty`) | wird beim Ausgleich oder von Admins zugewiesen. LP ja, Guthaben nein, mit Frist. Wird die Frist versäumt, folgt eine Nachfrist und ein Hinweis an die Admins. In der Oberfläche heißt sie immer „Aufholaufgabe“, nie „Strafaufgabe“ |
| **Familienziel** | gemeinsames LP-Ziel pro Periode mit einer Familienbelohnung (z. B. Ausflug). Dargestellt als Rennen gegen das Maskottchen **Krümel**, das gleichmäßig pro Tag von 0 bis zum Ziel am Periodenende vorrückt. **Schub:** Jede überfällige Aufgabe schiebt Krümel pro Tag um einen Anteil ihrer XP weiter (Standard 25 %, einstellbar). Alle spielen zusammen, es gibt keine Rangliste |
| **Serie** | Tage in Folge, an denen ein Mitglied **mindestens eine Aufgabe** erledigt hat. Wird oben in „Heute“ gezeigt |

---

## 7. Aufgaben

### 7.1 Vorlage (`TaskTemplate`)

- Titel, Beschreibung, **Raum** (Gliederung wie in Tody), Icon aus der eigenen Icon-Familie, geschätzte Dauer
- Punkte `min/default/max`, `kind` (`regular` · `reward` · `penalty`)
- Rhythmus, pro Vorlage wählbar:
  - **fester Termin**: Wochentage (optional alle N Wochen, z. B. „Müll jeden Dienstag“) oder Tag im Monat.
    Fällig ist am Ende des Tages in der Zeitzone des Haushalts (`punkte_core::Schedule`, als JSON gespeichert)
  - **nach Bedarf**: Intervall ab der letzten Erledigung, z. B. „Bad alle 7 Tage“. Wer früher
    erledigt, setzt die Uhr zurück
  - **auf Abruf** (M8): keine Runde, bis jemand sie braucht („Ist wieder nötig“ oder eine
    Einkaufsliste, die nicht mehr leer ist); optional fällig X Tage danach, siehe §10
- **Zustand** jeder Aufgabe: `frisch → fällig → überfällig`, berechnet aus der Zeit seit dem letzten
  Mal bzw. bis zum Termin. Die Eskalation der Punkte (§4.2) setzt mit „überfällig“ ein. Dargestellt
  als Zustandsbalken grün → honig → rot. Ein Raum zeigt den Zustand seiner dringendsten Aufgabe
- Zuweisung: `fixed(member)` · `rotation(members)` · `least_loaded(members)` (wer in der Periode
  am wenigsten LP hat) · `open` (wer zuerst übernimmt)
- **Schritte** (optional): geordnete Liste von Teilschritten, z. B. „Bad putzen“ = Waschbecken,
  Spiegel, Dusche, Boden, Handtücher. Reihenfolge pro Vorlage: **frei** (jeder hakt ab, wie er mag,
  z. B. Bad putzen) oder **der Reihe nach** (nur der aktuelle Schritt ist abhakbar, z. B. Spülmaschine:
  erst einräumen, dann ausräumen). Mit dem letzten Schritt ist die Aufgabe erledigt; **XP gibt es erst
  dann**. Schritte werden in der Vorlage gespeichert und in jedes Vorkommen kopiert
- **Nachfrage-Dauer** beim Starten: Standard = geschätzte Dauer der Vorlage, änderbar
- Bestätigung: Haushaltsregel übernehmen · immer · nie · mit Foto
- Erinnerungen: Versätze wie `-1d`, `-1h`, `0`, Nachhaken bei Überfälligkeit

### 7.2 Status einer Aufgabe

```
open ──claim──▶ claimed ──start──▶ in_progress ──done──▶ submitted ──approve──▶ completed
  │                │  ▲                 │   ▲                      └──reject──▶ claimed
  │                │  └──── pause ──────┘   └── letzter Schritt = done
  │                └──────────────── done ─────────────▶ submitted / completed
  └── skip / cancel / expire ──▶ cancelled
```

**Starten** (`start`, auch direkt aus `open`): setzt `started_at` und `remind_at`. **Es gibt keinen
Countdown**: Die App zeigt nur, seit wann die Aufgabe läuft (die Zeit zählt hoch), damit kein Druck
entsteht. Zu `remind_at` plant die App eine lokale, freundlich formulierte Nachfrage („Wie läuft's mit
…?“) mit den Aktionen *Erledigt*, *+10 min* (später erneut fragen) und *Später* (pausiert). Eine
gestartete Aufgabe ist in „Heute“ blau umrandet und zeigt „seit X Min.“ und den Schrittfortschritt.
Erledigen geht weiterhin auch ohne Starten.

Ohne Bestätigungspflicht geht `done` direkt auf `completed`. Jeder Statuswechsel wird als
`task_event` protokolliert (Verlauf, Benachrichtigungen, Sync).

### 7.3 Ansichten

| Ansicht | Inhalt |
| --- | --- |
| **Heute** (Start) | oben Familienziel (Rennen gegen Krümel) und eigene Serie, darunter meine Aufgaben mit Zustandsbalken, dann offene Aufgaben mit Bonus. Jede Karte zeigt, wer dran ist (Tier oder „offen“). Erledigt wird nie direkt auf der Karte (seit 23.11.2026): „+“ übernimmt eine offene Aufgabe, Antippen öffnet die Aufgabe; dort Schritte abhaken, „Erledigt“ oder „Abgeben“ |
| **Plan · Räume** | Räume mit Zustand und LP-Summe, Aufgaben je Raum |
| **Plan · Kalender** | Tag / Woche / Monat / Agenda. Ziehen auf einen Zeitslot plant die Aufgabe ein. Wiederholungen werden aufgefaltet. Farbe = Mitglied |
| **Plan · Board** | freie Fläche mit Karten und Zonen (je Person, Raum, Tag). Karte in eine Personen-Zone ziehen = zuweisen, in eine Tages-Zone = einplanen. Zoomen und Schieben mit zwei Fingern |
| **Eigene Ansichten** | gespeicherte Filter (Person, Kategorie, Status, Art, Zeitraum, mit/ohne Bonus) + Darstellung (Liste, Kanban, Kalender, Canvas) + Gruppierung/Sortierung. Privat oder für den Haushalt freigegeben |
| **Punkte** | Kontostand, Verlauf (Ledger), Periodenvergleich, Übertragen, Belohnungen einlösen |

### 7.4 Mobile-UX

- Untere Navigationsleiste: **Heute · Plan · Punkte · Familie** (Plan bündelt Räume, Kalender, Board;
  Familie bündelt Mitglieder, Regeln, Einstellungen)
- Schnell-Anlegen über einen schwebenden Button, Wischen zum Erledigen/Übernehmen
- Erledigen = **kräftiges Konfetti in der Karte**: Häkchen, Vibration, ein dichter Konfetti-Schauer
  vom Häkchen aus und das „+XP“-Schild. Die ganze Animation bleibt innerhalb der Aufgabenkarte
  (kein Overlay über andere Inhalte). Bei „Bewegung reduzieren“ nur Häkchen und +XP
- **Schritt abgehakt** = eigene, kleinere Variante: Häkchen ploppt, ein Ring breitet sich aus,
  kleine Sterne funkeln, das Schild „2 von 5“ gleitet herein, und der nächste Abschnitt des
  Fortschrittsbalkens füllt sich. Alles bleibt in der Zeile. Konfetti gibt es erst für die ganze Aufgabe
- Kinder nutzen dieselbe App ohne Admin-Funktionen (keine eigene Kinder-Oberfläche)
- **Wandanzeige** auf einem Tablet (quer): Uhr, Familienziel, eine Spalte pro Person mit den Aufgaben
  des Tages. Abhaken per Antippen → Tier → PIN. Bildschirm bleibt an, Nachtmodus
- Große Touch-Ziele (≥ 44 px), Safe Areas, Hell/Dunkel nach System
- Canvas auf dem Handy: Listen-Fallback für kleine Bildschirme, Canvas im Querformat/Tablet voll

### 7.5 Gestaltung (Design-Interview 22.11.2026)

| Thema | Entscheidung |
| --- | --- |
| Gefühl | warm und familiär: weiche Formen, warmer heller Grund, Personen im Mittelpunkt |
| Farben | Indigo = Aktionen, Petrol = Leistungspunkte (Balken), Honig = Guthaben (Münze), Zustand grün/honig/rot. Personenfarben getrennt von Systemfarben |
| Hell/Dunkel | beide Themen gleichwertig gestaltet. Standard: folgt dem System, Schalter „Hell · Dunkel · System“ unter Familie → Einstellungen (pro Gerät) |
| Schrift | Bricolage Grotesque für Titel und große Zahlen, Atkinson Hyperlegible Next für Text |
| Avatare | Tiere zum Aussuchen (Fuchs, Eule, Otter, Igel, Bär, Katze, Hund, Wal, Löwe) in der Personenfarbe. Ein Tier darf im Haushalt mehrfach vorkommen (Unterscheidung über die Farbe). Keine Fotos von Kindern auf dem Server |
| App-Icon | **geteilter Keks**: ein Keks, fair in zwei gleiche Hälften gebrochen, mit zwei Krümeln, auf Indigo. Quelle in `app/icon-src/` (Desktop mit runden Ecken, Android adaptiv aus Vorder- und Hintergrund, iOS mit Indigo-Grund) |
| Bilder | eigene Icon-Familie: runde Linien, 24er-Raster, 2 px |
| Klänge | warm & hölzern (Marimba-artig), standardmäßig **leise an**, pro Gerät abschaltbar, getrennt von der Vibration. Lautlos-Modus des Handys wird respektiert. Wandanzeige klingt wie das Handy (außer im Nachtmodus). Momente: Schritt abgehakt (1 leiser Ton), Aufgabe erledigt (kurzes „Ding-Ding!“, 2 Töne aufwärts, < 0,5 s), bestätigt, Guthaben erhalten, Serie verlängert, Krümel überholt (das Familienziel-Motiv auf halbem Weg: Lauf C–D–D#–E–F# landet offen auf G, darunter fächert sich G-Dur auf, ca. 1 s), Familienziel erreicht (abgeleitet vom Überholt-Motiv: derselbe Lauf mit Leitton H, der auf dem hohen C ankommt; darunter fächert sich ein warmer C-Dur-Akkord auf, danach kommt kein weiterer Ton, ca. 1,6 s). Stilvorbild für die großen Momente: Duolingo. **Entschieden (22.11.2026):** „Krümel überholt“ und „Familienziel erreicht“ teilen sich ein Motiv, das aus Kenney „Music Jingles – Pizzicato 02“ (CC0) transkribiert wurde, und werden mit dem eigenen Synth gespielt. Es werden keine Audiodateien mitgeliefert; Kenney wird in den Danksagungen genannt. Gehört mit Kalimba; Tonlage und Stimmung werden beim Einbau final abgestimmt. Klangkette: Tiefpass → Kompressor → Aufholverstärkung, damit die Töne am Handy-Lautsprecher voll klingen, ohne zu übersteuern. Klangfarbe/Tonlage/Stimmung werden im Klang-Spielplatz der Roadmap-Seite ausgewählt. Erzeugt per Web Audio im Webview, keine Audiodateien. Vor der Umsetzung prüfen, wie der Lautlos-Schalter in Android/iOS-Webviews greift |
| Spiel | Serien und Familienziel (Rennen gegen Krümel). Keine Ranglisten, keine Abzeichen |
| Punkte-Name | Was man mit Aufgaben verdient, heißt in der Oberfläche **XP**. Guthaben bleibt „G“ mit Münze |
| Vorbild | Tody: Zustandsbalken, Rhythmus nach Bedarf, Räume, Rennen gegen ein Maskottchen. Figuren und Gestaltung sind eigene Entwürfe |

---

## 8. Erinnerungen und Benachrichtigungen

### 8.1 Ereignisse

| Ereignis | Empfänger |
| --- | --- |
| Erinnerung vor Fälligkeit (nach Versätzen) | Zuständiger |
| Überfällig, Nachhaken (eskalierend) | Zuständiger, nach X h auch Admins |
| Aufgabe zugewiesen / Bonus auf meine Aufgabe | Betroffener |
| Zur Bestätigung eingereicht | Bestätiger |
| Bestätigt / abgelehnt | Erlediger |
| Guthaben erhalten | Empfänger |
| Periodenende naht, du liegst hinter dem Ziel | Betroffener |
| Abrechnung bereit / freigegeben, Aufholaufgabe erhalten | Admins / alle |
| Tagesübersicht zur Wunschzeit | jeder, der sie aktiviert hat |

### 8.2 Zustellung (stufenweise)

| Stufe | Weg | Bemerkung |
| --- | --- | --- |
| 1 | **Lokale Benachrichtigungen** (`tauri-plugin-notification`), von der App aus synchronisierten Daten geplant | geht ohne Push-Dienst und offline. Deckt Erinnerungen zu eigenen Aufgaben ab |
| 1 | **In-App-Postfach** + Live-Updates per WebSocket | alles, solange die App offen ist |
| 2 | **Server-Push**: Android über FCM, iOS über APNs (**entschieden**) | für Ereignisse anderer (zugewiesen, Bonus, Abrechnung). Braucht ein Firebase-Projekt und einen Apple-Developer-Account (99 €/Jahr, erst für iOS). Tauri 2 bringt kein offizielles Push-Plugin mit: vor Stufe 2 prüfen, ob ein Community-Plugin taugt oder ein kleines eigenes Mobile-Plugin (Kotlin/Swift) nötig ist |
| 2 (opt.) | **UnifiedPush/ntfy** auf dem Hetzner-Server | Android ohne Google-Dienste |
| 3 (opt.) | E-Mail / Matrix | Tagesübersicht, Abrechnungsbericht |

Je Mitglied: Ruhezeiten, Kanäle je Ereignisart, Tagesübersicht an/aus mit Uhrzeit.

### 8.3 Server-Scheduler

Eine Job-Tabelle in Postgres (`jobs`, abgeholt mit `FOR UPDATE SKIP LOCKED`), abgearbeitet von
einer Tokio-Schleife im API-Prozess: Wiederholungen auffalten, Erinnerungen auslösen, Eskalation,
Periodenabschluss. Kein zusätzlicher Dienst nötig, mehrere Instanzen wären später möglich.

---

## 9. Architektur

```
┌───────────────── Handy / Desktop ─────────────────┐        ┌────────── Hetzner ──────────┐
│  SvelteKit-UI (Webview)                           │        │  Caddy (TLS, Let's Encrypt) │
│     │  invoke()                                   │ HTTPS  │        │                    │
│  Tauri-Rust-Kern                                  │◀──────▶│  punkte-server (axum)       │
│   ├─ API-Client + Outbox (Offline-Warteschlange)  │  WS    │   ├─ REST /api/v1           │
│   ├─ lokaler Cache (SQLite)                       │        │   ├─ WebSocket (Live)       │
│   ├─ Token im OS-Schlüsselbund                    │        │   ├─ Scheduler (Jobs)       │
│   ├─ lokale Benachrichtigungen                    │        │   └─ punkte-core            │
│   └─ punkte-core (Vorschau, Validierung)          │        │  PostgreSQL                 │
└───────────────────────────────────────────────────┘        └─────────────────────────────┘
```

### 9.1 Repo-Aufbau

```
/
├─ Cargo.toml              Rust-Workspace
├─ crates/core/            punkte-core: Domänentypen, Punkte- und Ausgleichsregeln (ohne IO)
├─ server/                 punkte-server: axum, sqlx, Migrationen, Scheduler
│  └─ migrations/
├─ app/                    SvelteKit-Frontend
│  └─ src-tauri/           Tauri-Kern (Rust)
├─ deploy/                 docker-compose, Caddyfile, Dockerfile, Backup-Skript
└─ docs/
```

### 9.2 Sync (offline-fähig)

- **Lesen:** Jede haushaltsbezogene Änderung erhöht eine fortlaufende Nummer `change_seq`
  je Haushalt. Der Client fragt `GET /api/v1/sync?since=<seq>` und bekommt alle Änderungen seitdem.
  Live-Hinweise per WebSocket stoßen das an.
- **Schreiben:** Änderungen gehen als **Befehle** mit vom Client erzeugter UUID (Idempotenz) in die
  Outbox und werden gesendet, sobald Netz da ist. Der Server prüft Regeln (Guthaben, Rechte, Grenzen)
  und lehnt ggf. ab. Der Client zeigt das an und rollt zurück.
- **Konflikte:** Stammdaten (Vorlagen, Aufgaben-Felder) nach „letzter gewinnt“ je Feld mit `version`.
  Das Ledger ist nur anhängend, dort gibt es keine Konflikte.
- **Umgesetzt (M7):** Statt eines `change_seq`-Feeds lädt die App nach jedem Live-Hinweis und nach
  dem Wiederverbinden den Stand neu (bei Familiengröße billig) und hält eine Kopie auf dem Gerät.
  Die Outbox (`outbox.svelte.ts`) sendet Befehle mit `Idempotency-Key`-Kopf; der Server speichert
  die Antwort je Konto und Schlüssel 7 Tage (`idempotency_keys`) und spielt sie bei Wiederholung ab.
  Abgelehnte Befehle werden verworfen und gemeldet; der Neuladen-Schritt korrigiert die Anzeige.

### 9.3 Sicherheit

- Passwörter mit Argon2id. Zugriffstoken (JWT, 15 min) + rotierendes Refresh-Token (gehasht in der DB, je Gerät widerrufbar)
- Jede Abfrage ist auf den Haushalt beschränkt, Rollenprüfung im Service-Layer
- Rate-Limiting für Anmeldung und Einladungen
- Einladungen per Link/QR-Code mit Ablauf und Rolle
- Server: nur 80/443 offen (Hetzner-Firewall), SSH nur mit Schlüssel (oder nur über Tailscale)
- Backups: nächtlicher `pg_dump`, mit restic auf eine Hetzner Storage Box oder ans NAS daheim

### 9.4 Datenmodell (Kern, PostgreSQL)

| Tabelle | Wichtige Felder |
| --- | --- |
| `users` | id, email (optional bei Kindern), password_hash, display_name, locale |
| `households` | id, name, timezone, point_policy (jsonb), period_policy (jsonb), goal_policy (jsonb), change_seq |
| `memberships` | id, household_id, user_id, role, weight, color, active |
| `invites` | id, household_id, code_hash, role, expires_at, used_by |
| `sessions` | id, user_id, device_name, refresh_hash, expires_at, revoked_at |
| `devices` | id, user_id, platform, push_provider, push_token (eindeutig), last_seen_at |
| `rooms` | id, household_id, name, icon, color, position |
| `task_templates` | id, household_id, title, kind, points_min/default/max, schedule (jsonb), step_order, assignment (jsonb), approval, reminders (jsonb) |
| `tasks` | id, household_id, template_id, room_id, title, icon, kind, status (+ `in_progress`), assignee_id, cycle_start, due_at, scheduled_start/end, started_at, remind_at, points_* (Kopie), completed_by, completed_at, approved_by, approved_at, review_note, version |
| `task_steps` | id, task_id, position, title, done_by, done_at (Vorlagen: `template_steps` mit position, title) |
| `task_events` | id, task_id, actor_id, type, payload, created_at |
| `bounties` | id, task_id, pledger_id, amount, status (held/paid/refunded) |
| `transfers` | id, household_id, from_id, to_id, amount, note, status (done/pending/declined), decided_by |
| `ledger_entries` | siehe §4.5 |
| `periods` | id, household_id, starts_at, ends_at, status (running/settling/settled, höchstens einer läuft) |
| `family_goals` | period_id, target, reward, boost_percent, status (running/won/lost), decided_at |
| `settlements` / `settlement_lines` | Abrechnung je Periode und Mitglied (Ziel, Leistung, Abweichung, Folge), auto_approve_at; Aufholaufgaben verweisen per `tasks.settlement_id` |
| `absences` | member_id, from, to, reason |
| `notifications` / `notification_prefs` | Postfach (Kategorie, Bezug, gelesen, Push gesendet); Push-Kategorien und Ruhezeit je Mitglied |
| `rewards` / `reward_redemptions` | Katalog (Preis, Freigabe, Bestand, Abkühlzeit) und Einlösungen (Preis, Status, Entscheidung) |
| `boards` / `board_items` | gemeinsames Board je Haushalt; Kartenposition je Vorlage (wiederkehrend) oder Aufgabe |
| `saved_views` | owner_id, shared, filter (jsonb), layout, sort |
| `jobs` | id, kind, run_at, payload, attempts, locked_until |

---

## 10. Einkaufsliste und Aufgaben auf Abruf (M8)

Gemeinsame Einkaufslisten, auf die jedes Mitglied Artikel setzt (ohne XP). Jede Liste hängt an einer
Vorlage mit dem Rhythmus **auf Abruf** (`Schedule::OnCall`): Sie hat keine offene Runde, bis sie
ausgelöst wird, von Hand („Ist wieder nötig“) oder weil die Liste nicht mehr leer ist. Die Runde
„Einkaufen“ verhält sich dann wie jede Aufgabe (Zuweisung, XP, Bestätigung, Bonus). „Einkauf fertig“
räumt die abgehakten Artikel ab, übrig gebliebene lösen gleich die nächste Runde aus. Aufschreiben
geht für alle ohne XP; Abhaken legt in den Wagen, wenn man selbst einkauft, sonst ist der Artikel
„nebenbei mitgebracht“ und verschwindet ohne XP. Umsetzung: [ROADMAP.md › M8](ROADMAP.md#m8-einkaufsliste).
Das frühere Finanzmodul (Haushaltsbuch) ist gestrichen.

---

## 11. Plattformen und Build

| Ziel | Voraussetzung | Stand auf diesem PC |
| --- | --- | --- |
| Windows-Desktop | Rust, WebView2 | ✅ vorhanden |
| Android | SDK, NDK, JDK 17+, `ANDROID_HOME`/`NDK_HOME`, Rust-Targets `aarch64-linux-android` u. a. | ✅ SDK, NDK 30 und JDK 21 unter `D:\Android`, Projekt in `app/src-tauri/gen/android` |
| iOS | macOS mit Xcode, Apple-Developer-Account | ❌ auf Windows nicht möglich. Workflow „Apps“ baut auf einem macOS-Runner (ohne Konto nur Simulator) |
| Server-Image | Docker, Build für arm64/amd64 (Hetzner ARM-Instanzen sind günstig) | Docker vorhanden |

---

## 12. Fahrplan

Ausführlich mit nächsten Schritten und Designentwurf: [ROADMAP.md](ROADMAP.md).

| Meilenstein | Inhalt |
| --- | --- |
| **M0** Gerüst | Workspace, `punkte-core` mit Punkte-/Ausgleichsregeln + Tests, axum-Server mit Health und Migration, Tauri+SvelteKit mit mobiler Navigation, Deploy-Dateien |
| **M1** Konten | Registrierung/Anmeldung, Haushalt anlegen, Einladungen (QR), Rollen, Kinderprofile |
| **M2** Aufgaben, Räume & Zustand | Vorlagen, Räume, Rhythmus fest oder nach Bedarf, Zustand, Zuweisungsregeln, Heute-Ansicht |
| **M3** Punkte | Ledger, Bestätigung, Eskalation, Boni, Übertragungen, Punkte-Ansicht |
| **α** Familien-Alpha | Hetzner-Deployment, Backups, signierte APK, echter Familienalltag |
| **M4** Ausgleich & Familienziel | Perioden, Abrechnung mit Vorschau, Aufholaufgaben, Familienziel (Rennen gegen Krümel), Belohnungskatalog |
| **M5** Erinnerungen | lokale Benachrichtigungen, In-App-Postfach, WebSocket, danach Push (FCM/APNs) |
| **M6** Plan & Wandanzeige | Kalender, Board, eigene Ansichten, Tablet-Wandanzeige |
| **M7** Offline + Release | Outbox-Sync, Android-Release (Play Store/APK), iOS über CI |
| **M8** Einkaufsliste | gemeinsame Listen, Aufgaben auf Abruf („Einkaufen“ mit XP, Artikel hinzufügen ohne XP) |

---

## 13. Entscheidungen (22.11.2026)

| # | Frage | Entscheidung |
| --- | --- | --- |
| 1 | Domain | `fairteilt.oli-tech.eu` (Hetzner, Caddy) |
| 2 | Kinder | beides: eigenes Gerät per Einladungscode **oder** PIN-Profilwechsel auf dem Familien-Tablet |
| 3 | Bestätigung | nur bei Kindern (`approval = children_only`), pro Vorlage überschreibbar |
| 4 | Push | FCM (Android) + APNs (iOS). Firebase zusätzlich für App Distribution |
| 5 | Ausgleich-Standard | monatlich, 20 % Toleranz, Rückstand und Vorsprung werden übertragen (§5.1) |
| – | Name / Kennung | **Fairteilt**, `eu.olitech.fairteilt` |
| – | Anmeldung Erwachsene | E-Mail + Passwort (Argon2id), Sitzung pro Gerät. Passkeys optional später |
| – | Firebase-Projekt | wird vor M5 angelegt (FCM + App Distribution). `google-services.json` nicht ins Repo |
| – | Quellcode | öffentlich auf GitHub, Lizenz **AGPL-3.0-or-later** |

### Markenrecherche (22.11.2026, ohne Gewähr)

TMview (EUIPO-Netz, enthält DPMA) kennt genau eine Marke „fairteilt“: DPMA 302020019496.2,
angemeldet 10.09.2020, Klassen 35/36/45 (Werbung, Finanzdienstleistungen, Politikberatung),
Status **beendet**. Für Software (Klasse 9/42) gibt es keinen Eintrag, auch die Websuche und
der Play Store zeigen keine App dieses Namens. Ähnlich klingt „Fairteiler“ (Abgabestellen von
foodsharing), das ist aber ein anderes Wort und ein anderer Bereich. Vor einem öffentlichen
Store-Release noch einmal prüfen, eine eigene Anmeldung in Klasse 9/42 wäre möglich.

### Noch offen

- Nichts Technisches. Push unter Tauri 2 ist entschieden (M5/M6): ein kleiner eigener Android-Teil
  (`Push.kt`) holt das FCM-Token und zeigt Nachrichten, kein Community-Plugin. iOS braucht APNs und
  damit das Apple-Developer-Konto
