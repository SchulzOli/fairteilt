# Fairteilt – Roadmap

Stand: 23.11.2026 (nach M8, Betrieb vorbereitet). Details zu Regeln und Datenmodell: [KONZEPT.md](KONZEPT.md),
Gestaltung: [KONZEPT.md §7.5](KONZEPT.md#75-gestaltung-design-interview-22112026).

Jeder Meilenstein endet mit etwas, das man in der Hand hat. Nach M3 ist die App zum ersten Mal
im Familienalltag nutzbar (Familien-Alpha). Umfang grob: S / M / L.

## Nächste Schritte

Erledigt: CI, TypeScript-Typen aus `punkte-core` und Server, Design-Grundlage in der App, **M1** bis **M8**, Push über Firebase eingerichtet.

| Wer | Schritt |
| --- | --- |
| Betrieb | Server nach [BETRIEB.md](BETRIEB.md) einrichten, Wiederherstellung einmal echt proben |
| Konto | Play-Console-Konto, Upload-Schlüssel erzeugen und als GitHub-Secrets hinterlegen ([Anleitung](store/PLAY-STORE.md)) |
| Konto | Apple-Developer-Konto für TestFlight und Push auf iOS |
| Konto | Google-OAuth-Clients (Web + Android) anlegen, Client-ID auf Server und als GitHub-Variable eintragen ([Anleitung](GOOGLE-LOGIN.md)) |
| Entscheidung | Datenschutzerklärung ([Entwurf](DATENSCHUTZ.md)) ausfüllen und veröffentlichen; Zielgruppe im Play Store (18+ oder Familienrichtlinie) |
| Betrieb | Hetzner-Cloud-Server (x86) bestellen, DNS `fairteilt.oli-tech.eu` setzen (bis zur Alpha) |
| Betrieb | Tablet für die Wandanzeige festlegen |

## Meilensteine

| | Meilenstein | Ziel („fertig, wenn …“) | Umfang |
| --- | --- | --- | --- |
| ✅ | **M0 Gerüst** | Konzept, `punkte-core`, Server, App-Hülle, Android-Build, Designrichtung | – |
| ✅ | **M1 Konten & Haushalt** | Ein Admin legt den Haushalt an, ein zweiter tritt per QR-Code bei, ein Kind sucht sich ein Tier aus und meldet sich per PIN an | L |
| ✅ | **M2 Aufgaben, Räume & Zustand** | „Bad putzen, alle 7 Tage“ wird gelb und rot; Alex startet es, hakt fünf Schritte ab, und Fairteilt fragt freundlich nach, falls er hängen bleibt | L |
| ✅ | **M3 Punkte & Bestätigung** | Kind erledigt, Elternteil bestätigt mit angepassten Punkten, die Serie zählt hoch | M |
| ▶ | **α Familien-Alpha** | Läuft auf Hetzner mit Backups, signierte APK, zwei Wochen Alltag ohne Datenverlust | S |
| ✅ | **M4 Ausgleich, Familienziel & Belohnungen** | Monatsabschluss läuft automatisch, Rennen gegen Krümel entscheidet über die Familienbelohnung | M |
| ✅ | **M5 Erinnerungen & Push** | Lokale Erinnerungen beim Kippen auf „fällig“, Postfach, Push über FCM | M |
| ✅ | **M6 Plan & Wandanzeige** | Kalender, Board, eigene Ansichten, Tablet-Wandanzeige mit PIN-Abhaken | L |
| ✅ | **M7 Offline & Stores** | Offline-Warteschlange, Play Store, iOS über CI, Datenexport und Löschen | L |
| ✅ | **M8 Einkaufsliste** | Alle setzen Artikel auf die Liste (ohne XP); dadurch erscheint „Einkaufen“ als Aufgabe auf Abruf, die XP bringt und nie neu eingerichtet werden muss | M |

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
Verlauf und Familienübersicht (Mitgliederreihenfolge, keine Rangliste). Nachgereicht am 23.11.2026:
Hinweise mit Klang, wenn jemand etwas für dich tut (bestätigt, zurückgegeben, Guthaben geschickt,
zurückgenommen) und wenn deine Serie wächst; Admins sehen den Verlauf aller und nehmen Gutschriften per
Gegenbuchung mit Begründung zurück (Guthaben höchstens bis 0, ein Bonus bleibt, die Serie zählt den Tag
nicht mehr). Das M3-Szenario ist als
Server-Test und in der App geprüft. Bis M4 zählt der Kalendermonat als Periode für XP und das
Übertragungslimit; Perioden im Punktebuch setzt der Monatsabschluss.

- Punktebuch (nur anhängen), Bestätigen/Ablehnen mit Anpassung im Rahmen und Nachricht ans Kind
- Bonus auf Aufgaben (Treuhand), Übertragungen, Serien pro Person
- App: Bestätigen-Blatt, Punkte-Ansicht mit Verlauf

### α Familien-Alpha

**Vorbereitet am 23.11.2026**, es fehlen der Server und die signierte APK (beides braucht Konten).
Anleitung für Einrichtung, Sicherung, Wiederherstellung und Updates: [BETRIEB.md](BETRIEB.md).
Der Workflow **Server-Image** baut bei jeder Server-Änderung auf `main` das Image
`ghcr.io/schulzoli/fairteilt-server` (Tags `main`, `sha-<commit>`, Versionen), der Server baut kein
Rust mehr selbst. Das Image entsteht erst nach grüner CI und nur, wenn sich am Server etwas geändert
hat; `deploy/update.sh` holt es nachts per Cron (wie bei Linkhaven), sichert vorher und kann auf ein
älteres Image zurück.
`deploy/backup.sh` schiebt die nächtliche Sicherung mit restic außer Haus, `deploy/restore.sh --test`
spielt sie wöchentlich probeweise in eine Wegwerf-Datenbank. Die echte Wiederherstellung sichert
erst den aktuellen Stand und läuft in einer Transaktion: Eine kaputte Sicherung ändert nichts. Lokal
mit dem kompletten Stack (Caddy, API, Postgres) geprüft: erster Start, Sicherung, Probe,
Wiederherstellung, abgebrochene und fehlgeschlagene Wiederherstellung, Update.

### M4 Ausgleich, Familienziel & Belohnungen

**Abgeschlossen am 23.11.2026.** `punkte-core`: Zeiträume in der Zeitzone des Haushalts (Monat ab Tag X,
Woche ab Wochentag, alle X Tage), Krümels Lauf (gleichmäßig bis zum Ziel am Ende, Schub durch überfällige
Aufgaben), Entscheidung des Rennens, Zielvorschlag aus dem letzten Zeitraum. Server: Jeder Haushalt hat
einen laufenden Zeitraum mit Familienziel; der Scheduler schließt ihn am Ende automatisch als Entwurf ab
(Ziel, Leistung, Abweichung, Übertrag je Mitglied, Abwesenheiten zählen), entscheidet das Rennen und
öffnet den nächsten. Admins geben frei, sonst geschieht das nach der eingestellten Zeit: Bonus, Abzug
(nie unter 0), Nullsummen-Ausgleich und Aufholaufgaben aus den Vorlagen mit Frist von 7 Tagen. Das Rennen
wird bei jeder Gutschrift geprüft: Wer zuerst ankommt, entscheidet über die Familienbelohnung.
Belohnungskatalog mit Freigabe, Bestand und Abkühlzeit; abgelehnte Einlösungen bekommen ihr Guthaben
zurück. Haushaltsregeln (Punkte, Guthaben, Abschluss, Familienziel) lesen alle, ändern Admins. App:
echtes Rennen in Heute mit Klängen beim Überholen und beim Sieg, Familienziel-Blatt für Admins, Abschluss
in Punkte („Wer lag wo?“, Freigabe), Seite Belohnungen, Seite Regeln. Das M4-Szenario ist als
Server-Test und in der App geprüft. Die Warnung vor dem Ende des Zeitraums und Benachrichtigungen kommen
mit M5; Abwesenheiten werden berücksichtigt, haben aber noch keine Oberfläche.

- Periodenabschluss mit Entwurf, Freigabe, Übertrag und Aufholaufgaben
- Familienziel pro Monat, Krümel rückt gleichmäßig bis zum Ziel am Monatsende vor
- Belohnungskatalog inklusive Familienbelohnung

### M5 Erinnerungen & Push

**Abgeschlossen am 23.11.2026.** Push-Empfang in der Android-App am selben Tag nachgereicht (siehe M6).
Server: Postfach für alles, was jemanden betrifft (neue Aufgabe, bitte bestätigen, bestätigt oder
zurückgegeben, Bonus, Guthaben, Übertragung und Belohnung zur Freigabe, Abschluss fertig und
freigegeben, Aufholaufgabe, Familienziel, Zurücknahme). Erinnerungen per Job alle 5 Minuten: „bald
fällig“ bei 75 % der Runde, „überfällig“, nach einem Tag an die Admins; Warnung 48 Stunden vor dem
Abschluss an alle, die hinter ihrem Anteil liegen. Push über FCM (HTTP v1, Dienstkonto) mit Kategorien
je Person und Ruhezeiten; tote Tokens werden vergessen. Ohne Dienstkonto läuft alles außer Push. App:
Postfach mit Glocke und Zähler, Einstellungen (Push-Kategorien, Ruhezeit), lokale Erinnerungen auf dem
Gerät über das offizielle Tauri-Plugin: „wird bald fällig“ und die Nachfrage bei gestarteten Aufgaben
mit den Knöpfen Erledigt / +10 Min. / Später (am Handy geplant, am Rechner solange die App läuft).
Server-Seite als Test geprüft (auch Push gegen einen nachgebauten FCM-Endpunkt), Postfach in der App.
Lokale Benachrichtigungen und Knöpfe sind gebaut, aber noch nicht auf einem Gerät ausprobiert.

### M6 Plan & Wandanzeige

**Abgeschlossen am 23.11.2026.** `punkte-core`: kommende Runden aus dem Rhythmus (angenommen, jede wird
pünktlich erledigt). Server: Aufgaben in den Kalender einplanen (Planer alles, alle ihre eigenen),
zuweisen per Board (mit Postfach-Eintrag), Kalender-Abfrage mit aktuellen, erledigten und kommenden
Runden samt Reihum-Folge, Kartenpositionen auf dem gemeinsamen Board (wiederkehrende Aufgaben behalten
ihren Platz), eigene Ansichten (Filter nach Person, Raum, Stand, Art, Bonus, Fälligkeit; Liste, Kanban,
Kalender, Fläche; privat oder geteilt). App: Plan mit Räume · Kalender (Tag, Woche, Monat, Agenda;
ziehen oder antippen zum Einplanen; Farbe = Person) · Board (Fläche mit Personen- und Tageszonen, ziehen
zum Zuweisen und Einplanen, verschieben und zoomen; als Liste auf schmalen Bildschirmen) · Ansichten.
Wandanzeige `/wand` (Querformat): Uhr, Familienziel, eine Spalte pro Person mit den Aufgaben von heute,
abhaken per Antippen → Tier → PIN (kurze Sitzung für die Person, das Tablet bleibt angemeldet),
Bildschirm bleibt an, nachts gedimmt und leise. Push-Empfang (aus M5): Firebase im Android-Projekt,
Token an den Server, Hinweise im System, wenn die App im Hintergrund ist. Server-Seite als Test
geprüft, Plan und Wandanzeige in der App, Push mit dem Firebase-Projekt (siehe unten).

**Überarbeitet am 23.11.2026:** Kalender und Board im Stil der Aufgabenkarten. Ein gemeinsamer Eintrag
(`PlanNode`) für alle Plan-Ansichten: Symbol-Kachel in der Farbe der Person, Tier, XP, Zustände wie auf
den Karten (läuft, überfällig, erledigt, kommt noch gestrichelt). Woche als Tageskarten mit Datums-Plakette,
Tag mit Stundenraster und Jetzt-Linie, Monat mit farbigen Balken, Agenda nach Tagen. **Ziehen und Ablegen**
mit Maus und Finger (am Handy kurz drücken, dann ziehen): Einträge auf Tag oder Stunde verschieben, aus
„Noch ohne Termin“ einplanen, auf „Ohne Uhrzeit“ austragen; Board-Liste: auf Person oder Tag ziehen.
Board mit breiteren Zonen und Zoom-Knöpfen. **„Wann?“** auf jeder Aufgabe (und auf der Einkaufsliste):
Tag und Uhrzeit wählen; bei Aufgaben „auf Abruf“ wie Einkaufen wird der gewählte Tag auch die Fälligkeit.

**Ergänzt am 23.11.2026:** **Einrichtung ändern**: Stift an jeder Aufgabe unter Plan · Räume öffnet das
Aufgabenformular mit der bestehenden Einrichtung (Name, Raum, Symbol, Rhythmus, XP, Zuweisung, Schritte,
Bestätigung, Dauer) und „Aufgabe löschen“. Name, Symbol und Raum gelten sofort, der Rest für die offene
Runde, solange sie niemand begonnen hat, sonst ab der nächsten. Aufgaben per Ziehen in einen anderen Raum,
**Räume löschen** (Aufgaben bleiben, ohne Raum). **Heute ohne direktes Abhaken:** „+“ übernimmt, Antippen
öffnet die Aufgabe, dort erledigen, Schritte abhaken oder „Abgeben“. **Mit Google anmelden** (Android über
den Credential Manager, im Browser über Google Identity Services, Server prüft das ID-Token; Einrichtung:
[GOOGLE-LOGIN.md](GOOGLE-LOGIN.md)).

**Vereinfacht am 23.11.2026:** nur noch XP (Guthaben, Übertragungen und Belohnungskatalog entfernt,
Monatsziel bleibt und ist unter Punkte änderbar). XP-Bonus auf Aufgaben anderer als Geschenk mit
Monatsbudget pro Person (Regeln). Mehrere Personen pro Aufgabe („+“ = mitmachen), XP und Bonus werden
geteilt. Kein Timer mehr: kein Starten, keine Pause, keine Dauer, keine Nachfrage. Zustand in vier Stufen
(Gut, Hat Zeit, Fällig, Überfällig) mit Farbverlauf Grün bis Rot; fremde Aufgaben auf Heute ausgegraut.

### M7 Offline & Stores

**Abgeschlossen am 23.11.2026** (soweit ohne Store-Konten möglich). Offline: Die App startet ohne
Verbindung mit dem letzten bekannten Stand (Konto und Aufgaben je Person auf dem Gerät), zeigt oben
„Offline“, und Abhaken, Starten, Übernehmen, Schritte usw. landen in einer Warteschlange, die auch
einen Neustart übersteht. Sobald der Server wieder antwortet (Prüfung alle 15 s, Live-Verbindung),
geht alles in Reihenfolge raus; jede Änderung trägt einen `Idempotency-Key`, der Server merkt sich die
Antwort 7 Tage und bucht nichts doppelt. Lehnt der Server eine Änderung ab, erfährt man es per Hinweis.
Datenschutz: „Meine Daten herunterladen“ (alles zum eigenen Konto als JSON), „Haushalt exportieren“
(Admins), Konto löschen (mit Passwort; Punktebuch bleibt als „Gelöschtes Konto“ für die anderen),
Haushalt löschen (Name eintippen), Mitglieder entfernen; beim Abmelden verschwindet die Offline-Kopie.
Release: Server-Adresse ab Werk `https://fairteilt.oli-tech.eu` (`VITE_SERVER_URL` überschreibt),
Release-Signierung über `keystore.properties` oder Secrets, Workflow **Apps** baut AAB + APK und
iOS für den Simulator. Store-Texte und Datensicherheit in [store/PLAY-STORE.md](store/PLAY-STORE.md),
Datenschutzerklärung als [Entwurf](DATENSCHUTZ.md). **Offen, braucht Konten:** Play-Console-Konto und
Upload-Schlüssel, Apple-Developer-Konto (signiertes iOS, APNs).

### M8 Einkaufsliste

**Abgeschlossen am 23.11.2026** (Entwurf auf der Roadmap-Seite abgesegnet, alle Vorschläge übernommen).
Neuer Rhythmus **„auf Abruf“** (`Schedule::OnCall { due_after_days }`): Die Vorlage hat keine offene
Runde, bis sie ausgelöst wird, von Hand („Ist wieder nötig“ unter Plan · Räume) oder von ihrer Liste.
Je Vorlage höchstens eine Runde; fällig nach X Tagen (Einkaufen: 2), „dringend“ macht sofort fällig.
Jeder Haushalt hat die Liste „Supermarkt“ mit der Aufgabe „Einkaufen“, Erwachsene legen weitere an
(jede mit eigener Aufgabe „… einkaufen“). **Aufschreiben darf jeder, auch Kinder, ohne XP und ohne
Serie**; gleiche Artikel werden zusammengefasst, Vorschläge kommen aus dem Verlauf (mit Abteilung).
Kinder ändern nur, was sie selbst aufgeschrieben haben. Der erste Artikel ruft „Einkaufen“. Abhaken
legt in den Wagen und startet den Einkauf für die Person, die dran ist (oder für jeden, wenn offen);
wer nicht dran ist, hat es nebenbei mitgebracht: Der Artikel verschwindet ohne XP, und eine leere
Liste schließt die Runde still. **„Einkauf fertig“** ist das normale Erledigen: feste XP der Vorlage
(Kinder mit Okay), der Wagen wandert in den Verlauf, Übriges ruft sofort die nächste Runde. Ohne
abgehakte Artikel gibt es kein „fertig“. Neues auf der Liste während des Einkaufs: Postfach/Push
„Mia: Bananen noch mit?“ an die einkaufende Person und live markiert. App: Navigationspunkt
**Liste** (fünf unten, mit Zähler), Einkaufsmodus auf der Aufgabenseite (nach Abteilungen, Bildschirm
bleibt an), Schild „auf Abruf“ auf den Karten, Kachel auf der Wandanzeige (Aufschreiben per Tier, ohne
PIN). Alles läuft über die Offline-Warteschlange (Artikel-IDs vergibt die App). Export enthält Listen
und Verlauf. Server-Seite als Test geprüft, Liste, Einkauf, „auf Abruf“ und Wandanzeige in der App.
Keine Preise, kein Haushaltsbuch.

## Design (Kurzfassung, Details in KONZEPT §7.5)

- **Wir gegen Krümel:** Das Familienziel ist ein gemeinsames Rennen gegen ein Maskottchen. Es gibt
  keine Ranglisten und keine Abzeichen, nur Serien.
- **Zustand statt Zettel:** Zustandsbalken grün → honig → rot pro Aufgabe und Raum (Vorbild Tody)
- **Aufholen statt Strafe:** Im UI heißt es immer „Aufholaufgabe“
- **Warm & familiär:** Indigo/Petrol/Honig auf warmem Grund, Tier-Avatare, eigene Icon-Familie,
  Bricolage Grotesque + Atkinson Hyperlegible Next
- **Klänge:** warm & hölzern, leise an, Lautlos wird respektiert. Schritt < Aufgabe < Familienziel
- **Kinder wachsen hinein:** dieselbe App ohne Admin-Funktionen. Dazu eine Wandanzeige für das Tablet
