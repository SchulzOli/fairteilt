# Fairteilt

Familien-App für Haushaltsaufgaben mit einstellbarem Punktesystem, periodischem Ausgleich,
Belohnungen/Aufholaufgaben, Erinnerungen und Kalender-/Canvas-Planung.
Tauri 2 (Android, iOS, Desktop) + SvelteKit, Backend in Rust (axum + PostgreSQL) auf Hetzner.

Konzept und Regeln: **[docs/KONZEPT.md](docs/KONZEPT.md)** · Roadmap: **[docs/ROADMAP.md](docs/ROADMAP.md)**

## Aufbau

| Ordner | Inhalt |
| --- | --- |
| `crates/core` | `punkte-core`: Punkte-, Status- und Ausgleichsregeln, rein und getestet. Läuft auf Server und in der App |
| `server` | `punkte-server`: REST-API, Job-Scheduler, Migrationen (`server/migrations`) |
| `app` | SvelteKit-Oberfläche (mobil zuerst) |
| `app/src-tauri` | Tauri-Kern: Server-Zugriff, Offline-Vorschau mit `punkte-core` |
| `deploy` | Dockerfile, Compose (Produktion + Entwicklung), Caddy, Skripte für Sichern, Wiederherstellen und Aktualisieren |

## Entwicklung

Voraussetzungen: Rust (unter Windows mit dem **MSVC**-Toolchain), Node 22+, pnpm, Docker.

```powershell
# einmalig unter Windows, falls der GNU-Toolchain Standard ist
rustup override set stable-x86_64-pc-windows-msvc
```

```powershell
cargo test -p punkte-core
```

```powershell
docker compose -f deploy/docker-compose.dev.yml up -d
```

```powershell
Copy-Item server/.env.example server/.env; cargo run -p punkte-server
```

Tests (Integrationstests brauchen die Entwicklungsdatenbank):

```powershell
$env:DATABASE_URL = 'postgres://punkte:punkte-dev@127.0.0.1:5432/punkte'; cargo test --workspace
```

`127.0.0.1` statt `localhost`: Unter Windows versucht `localhost` zuerst IPv6 und kostet so rund 2 Sekunden pro Datenbankverbindung.

```powershell
pnpm --dir app install; pnpm --dir app tauri dev
```

In der App unter **Mehr → Server** `http://localhost:8080` eintragen und „Verbindung testen“.
Die Vorschau „Abschluss ausprobieren“ unter Regeln rechnet den Ausgleich offline mit `punkte-core` (im Tauri-Kern).

### API (Stand M8)

| Route | Zweck |
| --- | --- |
| `GET /health` | Status + Datenbankverbindung |
| `GET /api/v1/meta` | Version und Standardregeln für neue Haushalte |
| `POST /api/v1/preview/settlement` | Ausgleich durchrechnen: `{ policy, members }` → `Settlement` |
| `POST /api/v1/auth/register` · `/login` | Konto anlegen / anmelden → Zugriffstoken (15 min) + Refresh-Token |
| `POST /api/v1/auth/refresh` · `/logout` | Refresh-Token tauschen (rotierend) / Gerät abmelden |
| `GET /api/v1/me` · `DELETE /api/v1/me` | eigenes Konto und Haushalte; Konto löschen (`{ password }`) |
| `GET /api/v1/me/export` | alle Daten zum eigenen Konto als JSON-Datei |
| `POST /api/v1/households` | Haushalt mit Standardregeln anlegen (Ersteller wird Admin) |
| `POST /api/v1/households/{id}/invites` | Einladungscode erzeugen (nur Admins, 7 Tage gültig) |
| `POST /api/v1/invites/accept` | mit Code beitreten, Tier und Farbe wählen |
| `POST /api/v1/invites/join` | Kind ohne E-Mail tritt mit Kinder-Code auf eigenem Gerät bei |
| `GET /api/v1/households/{id}/members` · `PATCH` · `DELETE …/members/{mid}` | Mitglieder; Tier/Farbe selbst, Rolle/Gewicht als Admin; entfernen (Admins) |
| `GET /api/v1/households/{id}/export` · `POST …/delete` | Haushalt exportieren, löschen (`{ confirm_name }`), nur Admins |
| `POST /api/v1/households/{id}/children` · `PUT …/members/{mid}/pin` | Kinderprofil anlegen, PIN setzen |
| `POST /api/v1/auth/pin` | Profilwechsel per PIN, nur von einem Gerät des Haushalts (Sperre nach 5 Fehlversuchen) |
| `GET` · `POST /api/v1/households/{id}/rooms` · `PATCH` · `DELETE …/rooms/{rid}` | Räume (Kinder nur lesen) |
| `GET` · `POST /api/v1/households/{id}/templates` | Vorlagen mit Rhythmus, Zuweisung, Schritten; legt die erste Runde an (außer „auf Abruf“) |
| `POST /api/v1/templates/{tid}/trigger` | „Ist wieder nötig“: öffnet eine Runde einer Vorlage „auf Abruf“ |
| `GET /api/v1/households/{id}/shopping` | Einkaufslisten mit Artikeln, offener Runde und Vorschlägen |
| `POST /api/v1/households/{id}/lists` · `PATCH` · `DELETE /api/v1/lists/{lid}` | Listen (Erwachsene), jede mit eigener Aufgabe |
| `POST /api/v1/lists/{lid}/items` · `PATCH` · `DELETE /api/v1/items/{iid}` | Aufschreiben (ohne XP, gleiche Artikel zusammengefasst), Notiz, Abteilung, dringend |
| `POST /api/v1/items/{iid}/check` | in den Wagen (startet den Einkauf) oder „nebenbei mitgebracht“ |
| `GET /api/v1/households/{id}/tasks` · `GET /api/v1/tasks/{tid}` | aktuelle Runden mit Zustand, XP und Schritten |
| `POST /api/v1/tasks/{tid}/action` | `claim` · `unclaim` · `start` · `pause` · `snooze` · `done` · `cancel` (nur Admins) |
| `POST /api/v1/tasks/{tid}/steps/{sid}/check` | Schritt abhaken; der letzte erledigt die Aufgabe |
| `POST /api/v1/tasks/{tid}/review` | Erwachsene bestätigen (XP im Rahmen anpassbar, Nachricht) oder schicken zurück |
| `POST /api/v1/tasks/{tid}/bounty` | Bonus aus eigenem Guthaben setzen (sofort reserviert) |
| `GET /api/v1/households/{id}/accounts` | Guthaben, XP des Monats und Serie aller Mitglieder |
| `GET /api/v1/households/{id}/ledger` | Verlauf des eigenen Kontos (Admins: `?member=`), `before` + `limit` zum Blättern |
| `POST /api/v1/households/{id}/ledger/{eid}/reverse` | Admins nehmen eine Gutschrift per Gegenbuchung zurück (mit Begründung) |
| `GET` · `POST /api/v1/households/{id}/transfers` · `POST /api/v1/transfers/{xid}/decide` | Guthaben übertragen, Freigabe für Kinder |
| `GET /api/v1/households/{id}/period` | laufender Zeitraum mit Rennen gegen Krümel und den letzten Abschlüssen |
| `GET /api/v1/households/{id}/settlements` · `POST /api/v1/settlements/{sid}/approve` | Abschlüsse, Freigabe durch Admins |
| `GET` · `PUT /api/v1/households/{id}/rules` · `PUT …/goal` | Regeln des Haushalts (Punkte, Abschluss, Familienziel); Familienziel allein |
| `GET` · `POST /api/v1/households/{id}/rewards` · `PATCH` · `DELETE …/rewards/{rid}` | Belohnungskatalog |
| `POST /api/v1/rewards/{rid}/redeem` · `GET …/households/{id}/redemptions` · `POST /api/v1/redemptions/{xid}/decide` | Einlösen, Freigabe |
| `GET /api/v1/households/{id}/inbox` · `…/inbox/unread` · `POST …/inbox/read` | Postfach, ungelesene, als gelesen markieren |
| `GET` · `PUT /api/v1/households/{id}/notification-prefs` | Push-Kategorien und Ruhezeit je Person |
| `POST` · `DELETE /api/v1/devices` | Push-Token des Geräts an- und abmelden |
| `PUT /api/v1/tasks/{tid}/schedule` · `PUT /api/v1/tasks/{tid}/assign` | einplanen (Kalender), zuweisen (Board) |
| `GET /api/v1/households/{id}/calendar?from=&to=` | aktuelle, erledigte und kommende Runden (höchstens 62 Tage) |
| `GET` · `PUT /api/v1/households/{id}/board` | Kartenpositionen auf dem gemeinsamen Board |
| `GET` · `POST /api/v1/households/{id}/views` · `PUT` · `DELETE /api/v1/views/{vid}` | eigene Ansichten |
| Kopf `Idempotency-Key` | bei jedem Schreibaufruf erlaubt: Wiederholungen bekommen die gespeicherte Antwort (`idempotent-replay: true`), 7 Tage |
| `GET /api/v1/households/{id}/live?token=` | WebSocket: `tasks_changed` / `rooms_changed` / `points_changed` / `list_changed`; persönlich (`member`): `credited`, `returned`, `received`, `transfer_declined`, `reversed` |

## Push einrichten (Firebase)

Ohne Firebase funktioniert alles außer Push aufs Handy: Postfach, Live-Updates und lokale
Erinnerungen der App. Für Push:

1. In der [Firebase-Konsole](https://console.firebase.google.com/) ein Projekt anlegen und eine
   Android-App mit dem Paketnamen `eu.olitech.fairteilt` hinzufügen.
2. `google-services.json` herunterladen und nach `app/src-tauri/gen/android/app/` legen
   (steht in `.gitignore`, kommt nie ins Repo).
3. Unter Projekteinstellungen → Dienstkonten einen privaten Schlüssel erzeugen und als
   `deploy/secrets/firebase-service-account.json` auf den Server legen; in `deploy/.env`
   `FCM_SERVICE_ACCOUNT=/run/secrets/firebase-service-account.json` setzen. Lokal genügt
   `$env:FCM_SERVICE_ACCOUNT = '<Pfad>'` vor `cargo run`.

Der Server meldet beim Start „push over FCM enabled“. Die Android-App liest `google-services.json`
beim Bauen selbst ein (ohne Googles Gradle-Plugin), holt sich ein FCM-Token und meldet es nach der
Anmeldung über `POST /api/v1/devices`. Ohne die Datei baut die App trotzdem, nur ohne Push.

## App-Icon

Quelle: `app/icon-src/` (geteilter Keks). Nach Änderungen alle Größen neu erzeugen:

```powershell
pnpm --dir app tauri icon icon-src/icon.json
```

Danach `app/src-tauri/icons/64x64.png` als `app/static/favicon.png` übernehmen.

## Android

Auf diesem PC eingerichtet (Benutzer-Umgebungsvariablen):

| Variable | Wert |
| --- | --- |
| `JAVA_HOME` | `D:\Android\jdk-21` |
| `ANDROID_HOME` | `D:\Android\Sdk` |
| `NDK_HOME` | `D:\Android\Sdk\ndk\30.0.16248370` |

Rust-Targets für Android sind im MSVC-Toolchain installiert, das Gradle-Projekt liegt in
`app/src-tauri/gen/android`.

```powershell
pnpm --dir app tauri android dev
```

```powershell
pnpm --dir app tauri android build --debug --apk --target aarch64
```

Auf einem neuen Rechner: SDK, NDK (Side by side) und JDK 17+ installieren, die Variablen oben setzen,
`rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`.

Ab Werk spricht die App mit `https://fairteilt.oli-tech.eu` (im `pnpm dev` mit `http://localhost:8080`);
`VITE_SERVER_URL` beim Bauen überschreibt das, in der App lässt sich der Server auf dem Anmeldebildschirm ändern.

**Release:** Der Workflow **Apps** (Actions → Run workflow oder Tag `v*`) baut AAB und APK, signiert mit
dem Upload-Schlüssel aus den Secrets, und iOS für den Simulator. Lokal signiert
`app/src-tauri/gen/android/app/keystore.properties` (nie einchecken). Schritte bis zum Play Store:
[docs/store/PLAY-STORE.md](docs/store/PLAY-STORE.md). iOS braucht für mehr als den Simulator ein
Apple-Developer-Konto.

## Deployment (Hetzner)

Ausführlich mit Sichern, Wiederherstellen und Zurückgehen: **[docs/BETRIEB.md](docs/BETRIEB.md)**. Kurz:

1. Server mit Debian 13 (x86), Docker, Firewall nur 22/80/443, SSH nur mit Schlüssel
2. DNS `fairteilt.oli-tech.eu` (A/AAAA oder Wildcard) auf den Server; im zentralen Caddy ein Block
   `reverse_proxy 127.0.0.1:8791` (Server nur für Fairteilt: `COMPOSE_PROFILES=caddy`)
3. Repo nach `/opt/fairteilt` holen, dann in `deploy/`:
   ```bash
   sudo cp .env.example .env   # POSTGRES_PASSWORD und JWT_SECRET eintragen
   sudo ./update.sh            # holt das Image aus der CI (Workflow Server-Image) und startet
   ```
4. Prüfen mit `curl https://fairteilt.oli-tech.eu/health`
5. Cron: `backup.sh` und `update.sh` nachts (Updates kommen so automatisch nach grüner CI), `restore.sh --test` wöchentlich

## Lizenz

Copyright (C) 2026 SchulzOli

Fairteilt ist freie Software unter der [GNU Affero General Public License v3.0 oder später](LICENSE).
Wer den Server verändert und anderen als Dienst anbietet, muss den geänderten Quellcode ebenfalls
bereitstellen.
