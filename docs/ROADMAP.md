# Fairteilt – Roadmap

Stand: 22.11.2026 (nach dem Design-Interview). Details zu Regeln und Datenmodell: [KONZEPT.md](KONZEPT.md),
Gestaltung: [KONZEPT.md §7.5](KONZEPT.md#75-gestaltung-design-interview-22112026).

Jeder Meilenstein endet mit etwas, das man in der Hand hat. Nach M3 ist die App zum ersten Mal
im Familienalltag nutzbar (Familien-Alpha). Umfang grob: S / M / L.

## Nächste Schritte

Erledigt: CI, TypeScript-Typen aus `punkte-core` und Server, Design-Grundlage in der App, **M1**.

| Wer | Schritt |
| --- | --- |
| Code | `punkte-core`: Rhythmus „nach Bedarf“ und Zustand (frisch → fällig → überfällig), mit Tests |
| Code | M2: Räume, Vorlagen mit Rhythmus und Schritten, Zustand, Aufgabe starten, Heute mit echten Daten |
| Entscheidung | Klang auswählen (Klang-Spielplatz auf der Roadmap-Seite). Krümel, Serie, Schritte und Tiere sind entschieden |
| Betrieb | Hetzner-Cloud-Server bestellen, DNS `fairteilt.oli-tech.eu` setzen (bis zur Alpha) |
| Betrieb | Tablet für die Wandanzeige festlegen. Firebase vor M5, Apple-Developer-Account vor M7 |

## Meilensteine

| | Meilenstein | Ziel („fertig, wenn …“) | Umfang |
| --- | --- | --- | --- |
| ✅ | **M0 Gerüst** | Konzept, `punkte-core`, Server, App-Hülle, Android-Build, Designrichtung | – |
| ✅ | **M1 Konten & Haushalt** | Ein Admin legt den Haushalt an, ein zweiter tritt per QR-Code bei, ein Kind sucht sich ein Tier aus und meldet sich per PIN an | L |
| ▶ | **M2 Aufgaben, Räume & Zustand** | „Bad putzen, alle 7 Tage“ wird gelb und rot; Alex startet es, hakt fünf Schritte ab, und Fairteilt fragt freundlich nach, falls er hängen bleibt | L |
| | **M3 Punkte & Bestätigung** | Kind erledigt, Elternteil bestätigt mit angepassten Punkten, die Serie zählt hoch | M |
| | **α Familien-Alpha** | Läuft auf Hetzner mit Backups, signierte APK, zwei Wochen Alltag ohne Datenverlust | S |
| | **M4 Ausgleich, Familienziel & Belohnungen** | Monatsabschluss läuft automatisch, Rennen gegen Krümel entscheidet über die Familienbelohnung | M |
| | **M5 Erinnerungen & Push** | Lokale Erinnerungen beim Kippen auf „fällig“, Postfach, Push über FCM | M |
| | **M6 Plan & Wandanzeige** | Kalender, Board, eigene Ansichten, Tablet-Wandanzeige mit PIN-Abhaken | L |
| | **M7 Offline & Stores** | Offline-Warteschlange, Play Store, iOS über CI, Datenexport und Löschen | L |
| | **M8 Haushaltsbuch** | Ausgaben, Budgets und Belege in denselben Haushalten | L |

### M1 Konten & Haushalt

**Abgeschlossen am 22.11.2026.** Server: Registrierung/Login (Argon2id, Rate-Limit), JWT (15 min) +
rotierendes Refresh-Token pro Gerät, Haushalte, Einladungscodes, Mitglieder (Tier, Farbe, Rolle, Gewicht),
Kinderprofile ohne E-Mail (vom Admin angelegt oder per Kinder-Code auf eigenem Gerät), PIN-Anmeldung nur
von einem Gerät des Haushalts mit Sperre nach 5 Fehlversuchen. App: Willkommen (Anmelden, Registrieren,
Kinder-Code), Einrichten (anlegen/beitreten, QR-Scan am Handy), Familie (Mitglieder, Einladen mit QR,
Kinderprofil, PIN), Profilwechsel am Tablet (Rückkehr nach 2 min), Refresh-Token im Schlüsselbund des
Systems (Android: privater App-Speicher). Das M1-Szenario ist als Server-Test und in der App geprüft.


- Server: Registrierung und Login (E-Mail + Passwort, Argon2id), Zugriffstoken + rotierendes
  Refresh-Token pro Gerät, Haushalt mit Standardregeln, Einladungscodes mit Ablauf und Rolle,
  Kinderprofile ohne E-Mail mit gehashter PIN, Rate-Limit
- App: Assistent Konto → Haushalt → Mitglieder, Tier und Farbe aussuchen, Einladen per Code/QR,
  Navigation Heute · Plan · Punkte · Familie, Token im Schlüsselbund des Geräts

### M2 Aufgaben, Räume & Zustand

- Vorlagen mit Punkten min/default/max, Räume als Gliederung
- Rhythmus: fester Termin (RRULE) oder „alle X Tage nach dem letzten Mal“
- Zustand frisch → fällig → überfällig, die Punkte-Eskalation folgt dem Zustand
- Aufgabe **starten** (Status „läuft“): Zeit zählt nur hoch (kein Countdown), freundliche lokale
  Nachfrage nach der geplanten Dauer (Erledigt / +10 min / Später)
- **Schritte** innerhalb einer Aufgabe, in Vorlagen speicherbar, der Reihe nach abhaken
- Zuweisung: fest, reihum, wer am wenigsten hat, offen; Live-Updates per WebSocket
- App: Heute mit Zustandsbalken, Ansicht „Aufgabe läuft“ (vergangene Zeit + Schritte), Plan · Räume,
  Schritt abhaken mit Funkeln, Aufgabe erledigen mit kräftigem Konfetti, beides innerhalb der Karte

### M3 Punkte & Bestätigung

- Punktebuch (nur anhängen), Bestätigen/Ablehnen mit Anpassung im Rahmen und Nachricht ans Kind
- Bonus auf Aufgaben (Treuhand), Übertragungen, Serien pro Person
- App: Bestätigen-Blatt, Punkte-Ansicht mit Verlauf

### M4 Ausgleich, Familienziel & Belohnungen

- Periodenabschluss mit Entwurf, Freigabe, Übertrag und Aufholaufgaben
- Familienziel pro Monat, Krümel rückt gleichmäßig bis zum Ziel am Monatsende vor
- Belohnungskatalog inklusive Familienbelohnung

## Design (Kurzfassung, Details in KONZEPT §7.5)

- **Wir gegen Krümel:** Das Familienziel ist ein gemeinsames Rennen gegen ein Maskottchen. Es gibt
  keine Ranglisten und keine Abzeichen, nur Serien.
- **Zustand statt Zettel:** Zustandsbalken grün → honig → rot pro Aufgabe und Raum (Vorbild Tody)
- **Aufholen statt Strafe:** Im UI heißt es immer „Aufholaufgabe“
- **Warm & familiär:** Indigo/Petrol/Honig auf warmem Grund, Tier-Avatare, eigene Icon-Familie,
  Bricolage Grotesque + Atkinson Hyperlegible Next
- **Klänge:** warm & hölzern, leise an, Lautlos wird respektiert. Schritt < Aufgabe < Familienziel
- **Kinder wachsen hinein:** dieselbe App ohne Admin-Funktionen. Dazu eine Wandanzeige für das Tablet
