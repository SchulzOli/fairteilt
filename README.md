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
| `deploy` | Dockerfile, Compose (Produktion + Entwicklung), Caddy, Backup-Skript |

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
Die Punkte-Ansicht rechnet den Ausgleich offline mit `punkte-core`.

### API (Stand M2)

| Route | Zweck |
| --- | --- |
| `GET /health` | Status + Datenbankverbindung |
| `GET /api/v1/meta` | Version und Standardregeln für neue Haushalte |
| `POST /api/v1/preview/settlement` | Ausgleich durchrechnen: `{ policy, members }` → `Settlement` |
| `POST /api/v1/auth/register` · `/login` | Konto anlegen / anmelden → Zugriffstoken (15 min) + Refresh-Token |
| `POST /api/v1/auth/refresh` · `/logout` | Refresh-Token tauschen (rotierend) / Gerät abmelden |
| `GET /api/v1/me` | eigenes Konto und Haushalte |
| `POST /api/v1/households` | Haushalt mit Standardregeln anlegen (Ersteller wird Admin) |
| `POST /api/v1/households/{id}/invites` | Einladungscode erzeugen (nur Admins, 7 Tage gültig) |
| `POST /api/v1/invites/accept` | mit Code beitreten, Tier und Farbe wählen |
| `POST /api/v1/invites/join` | Kind ohne E-Mail tritt mit Kinder-Code auf eigenem Gerät bei |
| `GET /api/v1/households/{id}/members` · `PATCH …/members/{mid}` | Mitglieder; Tier/Farbe selbst, Rolle/Gewicht als Admin |
| `POST /api/v1/households/{id}/children` · `PUT …/members/{mid}/pin` | Kinderprofil anlegen, PIN setzen |
| `POST /api/v1/auth/pin` | Profilwechsel per PIN, nur von einem Gerät des Haushalts (Sperre nach 5 Fehlversuchen) |
| `GET` · `POST /api/v1/households/{id}/rooms` · `PATCH` · `DELETE …/rooms/{rid}` | Räume (Kinder nur lesen) |
| `GET` · `POST /api/v1/households/{id}/templates` | Vorlagen mit Rhythmus, Zuweisung, Schritten; legt die erste Runde an |
| `GET /api/v1/households/{id}/tasks` · `GET /api/v1/tasks/{tid}` | aktuelle Runden mit Zustand, XP und Schritten |
| `POST /api/v1/tasks/{tid}/action` | `claim` · `unclaim` · `start` · `pause` · `snooze` · `done` · `cancel` (nur Admins) |
| `POST /api/v1/tasks/{tid}/steps/{sid}/check` | Schritt abhaken; der letzte erledigt die Aufgabe |
| `GET /api/v1/households/{id}/live?token=` | WebSocket: `tasks_changed` / `rooms_changed`, wenn sich etwas ändert |

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

iOS braucht macOS mit Xcode. Später über einen macOS-Runner in GitHub Actions.

## Deployment (Hetzner)

1. Cloud-Server (Debian, kleinste Instanz reicht), Firewall nur 22/80/443, SSH nur mit Schlüssel
2. DNS-Eintrag `fairteilt.oli-tech.eu` (A/AAAA) auf die Server-IP
3. Repo auf den Server holen, dann in `deploy/`:
   ```bash
   cp .env.example .env   # POSTGRES_PASSWORD eintragen (DOMAIN ist vorbelegt)
   docker compose up -d --build
   ```
4. Caddy holt das TLS-Zertifikat selbst. Prüfen mit `curl https://fairteilt.oli-tech.eu/health`
5. Backups: `deploy/backup.sh` per Cron, Kopie außer Haus mit restic (Storage Box oder NAS)

## Lizenz

Copyright (C) 2026 SchulzOli

Fairteilt ist freie Software unter der [GNU Affero General Public License v3.0 oder später](LICENSE).
Wer den Server verändert und anderen als Dienst anbietet, muss den geänderten Quellcode ebenfalls
bereitstellen.
