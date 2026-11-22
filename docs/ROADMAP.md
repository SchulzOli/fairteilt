# Fairteilt – Roadmap

Stand: 22.11.2026 (nach dem Design-Interview). Details zu Regeln und Datenmodell: [KONZEPT.md](KONZEPT.md),
Gestaltung: [KONZEPT.md §7.5](KONZEPT.md#75-gestaltung-design-interview-22112026).

Jeder Meilenstein endet mit etwas, das man in der Hand hat. Nach M3 ist die App zum ersten Mal
im Familienalltag nutzbar (Familien-Alpha). Umfang grob: S / M / L.

## Nächste Schritte

Erledigt: CI, TypeScript-Typen aus `punkte-core` und Server, Design-Grundlage in der App, **M1**, **M2**, **M3**.

| Wer | Schritt |
| --- | --- |
| Code | Familien-Alpha: Deployment auf Hetzner durchspielen, signierte APK, Backup und Wiederherstellung testen |
| Code | M4: Monatsabschluss, Familienziel gegen Krümel, Belohnungskatalog |
| Entscheidung | Klang auswählen (Klang-Spielplatz auf der Roadmap-Seite). Krümel, Serie, Schritte und Tiere sind entschieden |
| Betrieb | Hetzner-Cloud-Server bestellen, DNS `fairteilt.oli-tech.eu` setzen (bis zur Alpha) |
| Betrieb | Tablet für die Wandanzeige festlegen. Firebase vor M5, Apple-Developer-Account vor M7 |

## Meilensteine

| | Meilenstein | Ziel („fertig, wenn …“) | Umfang |
| --- | --- | --- | --- |
| ✅ | **M0 Gerüst** | Konzept, `punkte-core`, Server, App-Hülle, Android-Build, Designrichtung | – |
| ✅ | **M1 Konten & Haushalt** | Ein Admin legt den Haushalt an, ein zweiter tritt per QR-Code bei, ein Kind sucht sich ein Tier aus und meldet sich per PIN an | L |
| ✅ | **M2 Aufgaben, Räume & Zustand** | „Bad putzen, alle 7 Tage“ wird gelb und rot; Alex startet es, hakt fünf Schritte ab, und Fairteilt fragt freundlich nach, falls er hängen bleibt | L |
| ✅ | **M3 Punkte & Bestätigung** | Kind erledigt, Elternteil bestätigt mit angepassten Punkten, die Serie zählt hoch | M |
| ▶ | **α Familien-Alpha** | Läuft auf Hetzner mit Backups, signierte APK, zwei Wochen Alltag ohne Datenverlust | S |
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

**Abgeschlossen am 22.11.2026.** `punkte-core`: Rhythmus (einmalig, alle X Tage nach dem letzten Mal,
Wochentage, Monatstag; feste Termine enden am Ende des Tages in der Zeitzone des Haushalts), Zustand,
Zuweisung (fest, reihum, wer am wenigsten XP hat, offen), Schritte (beliebig oder der Reihe nach), Status
„läuft“. Server: Räume (neue Haushalte starten mit fünf), Vorlagen, genau eine aktuelle Runde pro Vorlage,
Aktionen (übernehmen, starten, pausieren, +10 Min., erledigen, Runde überspringen), Schritte abhaken (der
letzte erledigt die Aufgabe), XP-Eskalation bei überfälligen Aufgaben, Live-Updates per WebSocket
(`/households/{id}/live`). App: Heute mit echten Aufgaben und Vorschlägen für den Anfang, Plan · Räume,
Seite „Aufgabe läuft“ (Zeit zählt hoch, Schritte mit Funkeln, Konfetti am Ende), Blatt „Neue Aufgabe“,
freundliche Nachfrage nach der geplanten Dauer (Erledigt / +10 Min. / Später). Das M2-Szenario ist als
Server-Test und in der App geprüft. Systembenachrichtigungen für die Nachfrage kommen mit M5; bis dahin
erscheint sie, sobald die App offen ist. Punkte werden ab M3 ins Punktebuch gebucht.

- Vorlagen mit Punkten min/default/max, Räume als Gliederung
- Rhythmus: fester Termin (Wochentage, Monatstag) oder „alle X Tage nach dem letzten Mal“
- Zustand frisch → fällig → überfällig, die Punkte-Eskalation folgt dem Zustand
- Aufgabe **starten** (Status „läuft“): Zeit zählt nur hoch (kein Countdown), freundliche lokale
  Nachfrage nach der geplanten Dauer (Erledigt / +10 min / Später)
- **Schritte** innerhalb einer Aufgabe, in Vorlagen speicherbar, der Reihe nach abhaken
- Zuweisung: fest, reihum, wer am wenigsten hat, offen; Live-Updates per WebSocket
- App: Heute mit Zustandsbalken, Ansicht „Aufgabe läuft“ (vergangene Zeit + Schritte), Plan · Räume,
  Schritt abhaken mit Funkeln, Aufgabe erledigen mit kräftigem Konfetti, beides innerhalb der Karte

### M3 Punkte & Bestätigung

**Abgeschlossen am 22.11.2026.** `punkte-core`: Serie (Tage in Folge, der laufende Tag zählt noch nicht
gegen sie), Bestätigung pro Vorlage (wie im Haushalt, immer, nie, mit Foto), Regeln für Bonus und
Übertragung. Server: Punktebuch in Gebrauch (jede Aufgabe wird genau einmal gutgeschrieben), Kinder
reichen ein und ein Erwachsener bestätigt mit angepassten XP im Rahmen und einer Nachricht oder schickt
die Aufgabe mit Nachricht zurück; niemand bestätigt die eigene Aufgabe. Bonus auf Aufgaben wird sofort
reserviert, beim Erledigen ausgezahlt und beim Überspringen zurückgebucht. Übertragungen immer mit
Notiz, Limit pro Monat, Kinder mit Freigabe durch Admins. Konten mit Guthaben, XP des Monats und Serie.
Aufholaufgaben bringen XP, aber kein Guthaben. App: Heute mit echter Serie und „Zu bestätigen“,
Bestätigen-Blatt, Bonus setzen auf der Aufgabenseite, Punkte-Ansicht mit Konto, Übertragen, Freigeben,
Verlauf und Familienübersicht (Mitgliederreihenfolge, keine Rangliste). Das M3-Szenario ist als
Server-Test und in der App geprüft. Bis M4 zählt der Kalendermonat als Periode für XP und das
Übertragungslimit; Perioden im Punktebuch setzt der Monatsabschluss.

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
