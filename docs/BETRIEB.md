# Fairteilt – Betrieb (Familien-Alpha)

So läuft der Server auf Hetzner: einrichten, sichern, wiederherstellen, aktualisieren.
Grundsätze aus [KONZEPT §9.3](KONZEPT.md#93-sicherheit): nur 80/443 offen, SSH nur mit Schlüssel,
nächtliche Sicherung mit Kopie außer Haus.

Fairteilt läuft in zwei Containern ([docker-compose.yml](../deploy/docker-compose.yml)): `db` (Postgres 18)
und `api` (der Rust-Server, fertig gebaut von der CI). Die API lauscht nur auf `127.0.0.1:8791`; davor
sitzt ein Reverse Proxy, der HTTPS macht. Der Server spielt neue Migrationen beim Start selbst ein.

Zwei Varianten:

- **A. Gemeinsamer Server** (Standard): Auf dem Server läuft schon ein zentraler Caddy für mehrere
  Apps unter eigenen Subdomains. Fairteilt bekommt dort einen Block mehr.
- **B. Server nur für Fairteilt:** `COMPOSE_PROFILES=caddy` in `.env` startet den mitgelieferten
  Caddy auf 80/443 (dann Schritt 3 überspringen).

Alle Befehle mit `sudo`: Repository, Sicherungen und Cron gehören root.

## 1. Server einrichten (einmalig)

1. **Voraussetzungen:** Debian 13 auf **x86** (das Image aus der CI gibt es nur für `linux/amd64`),
   Docker mit Compose, SSH nur mit Schlüssel, Firewall nur TCP 22/80/443 und UDP 443, automatische
   Sicherheitsupdates. Ein Port, den Docker ohne `127.0.0.1` veröffentlicht, wäre trotz `ufw` offen;
   die Compose-Datei bindet die API deshalb nur lokal.
2. **DNS:** `fairteilt.oli-tech.eu` muss auf den Server zeigen (A und AAAA, oder ein Wildcard-Eintrag
   `*.oli-tech.eu`). Prüfen: `dig +short A fairteilt.oli-tech.eu`.
3. **Zentraler Caddy** (Variante A): Block in `/etc/caddy/Caddyfile` ergänzen:

   ```
   fairteilt.oli-tech.eu {
   	encode zstd gzip
   	header {
   		Strict-Transport-Security "max-age=31536000"
   		X-Content-Type-Options "nosniff"
   		Referrer-Policy "no-referrer"
   		-Server
   	}
   	reverse_proxy 127.0.0.1:8791
   }
   ```

   ```bash
   sudo caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile && sudo systemctl reload caddy
   ```

   Ist Port 8791 schon belegt (`ss -ltn | grep 8791`), einen anderen nehmen und in `.env` als
   `API_PORT` eintragen. Live-Updates laufen über WebSocket, das reicht Caddy von selbst durch.

4. **Werkzeuge:** `sudo apt install -y git restic`
5. **Fairteilt holen und einrichten:**

   ```bash
   sudo git clone https://github.com/SchulzOli/fairteilt.git /opt/fairteilt
   cd /opt/fairteilt/deploy
   sudo cp .env.example .env && sudo chmod 600 .env
   openssl rand -base64 32   # → POSTGRES_PASSWORD in .env
   openssl rand -base64 48   # → JWT_SECRET in .env
   sudo nano .env
   ```

   Beide Werte zusätzlich im Passwortmanager ablegen. Das Datenbank-Passwort gilt nur beim
   allerersten Start; später ändern heißt auch in Postgres ändern (`ALTER USER punkte PASSWORD …`).
   `JWT_SECRET` neu setzen meldet alle Geräte nach 15 Minuten ab.

6. **Push (optional):** Das Firebase-Dienstkonto auf den Server kopieren (nie committen, nie in
   einen Chat kopieren), z. B. vom PC mit `scp deploy/secrets/firebase-service-account.json <server>:`,
   dann auf dem Server:

   ```bash
   sudo mkdir -p secrets && sudo mv ~/firebase-service-account.json secrets/
   sudo chown 10001:10001 secrets/firebase-service-account.json && sudo chmod 400 secrets/firebase-service-account.json
   ```

   Der Server läuft im Container als Benutzer 10001 und muss die Datei lesen können. In `.env`
   `FCM_SERVICE_ACCOUNT=/run/secrets/firebase-service-account.json` einkommentieren.

7. **Starten:** `sudo ./update.sh` holt das Image und startet (beim ersten Mal gibt es noch nichts zu
   sichern). Danach muss `curl https://fairteilt.oli-tech.eu/health` mit `"database":true` antworten;
   `sudo docker compose logs api` zeigt „push over FCM enabled“, wenn Push eingerichtet ist.

8. **Sicherung einplanen:** `/etc/cron.d/fairteilt` anlegen (`sudo nano /etc/cron.d/fairteilt`):

   ```cron
   30 2 * * * root /opt/fairteilt/deploy/backup.sh >> /var/log/fairteilt-backup.log 2>&1
   15 4 * * 0 root /opt/fairteilt/deploy/restore.sh --test >> /var/log/fairteilt-backup.log 2>&1
   ```

   Die zweite Zeile spielt sonntags die neueste Sicherung in eine Wegwerf-Datenbank und zählt die
   Zeilen. So fällt eine kaputte Sicherung auf, bevor man sie braucht. Updates laufen bewusst nicht
   automatisch: Neue Versionen bringen Migrationen mit, die spielt man lieber selbst ein.

## 2. Sichern

[backup.sh](../deploy/backup.sh) legt jede Nacht einen `pg_dump` nach `/var/backups/punkte` (nur für
root lesbar, 14 Tage). Ohne Kopie außer Haus ist er beim Verlust des Servers mit weg, darum:

Ein Server-Backup bei Hetzner (Schnappschuss der ganzen Platte) ist eine gute Ergänzung, liegt aber
beim selben Anbieter und lässt sich nicht für eine einzelne App zurückspielen.

**Kopie außer Haus mit restic** (verschlüsselt, z. B. Hetzner Storage Box per SFTP oder das NAS), als
root (`sudo -i`):

```bash
cat > /root/.restic-env <<'EOF'
RESTIC_REPOSITORY=sftp:u123456@u123456.your-storagebox.de:/fairteilt
RESTIC_PASSWORD=<langes Passwort, zusätzlich im Passwortmanager ablegen>
EOF
chmod 600 /root/.restic-env
set -a; . /root/.restic-env; set +a; restic init
```

Ist `/root/.restic-env` da, schiebt `backup.sh` jede Sicherung hinterher (7 tägliche, 8 wöchentliche,
12 monatliche). Ohne das restic-Passwort sind die Kopien wertlos, also nicht nur auf dem Server
aufbewahren. Prüfen: `sudo -i`, dann `set -a; . /root/.restic-env; set +a; restic snapshots`. Für SFTP braucht root einen
SSH-Schlüssel, den die Storage Box kennt.

## 3. Wiederherstellen

[restore.sh](../deploy/restore.sh) im Ordner `/opt/fairteilt/deploy`, jeweils mit `sudo`:

| Befehl | Wirkung |
| --- | --- |
| `./restore.sh --test` | neueste Sicherung probeweise in `punkte_restore_test`, Zeilen zählen, wieder löschen. Die echte Datenbank bleibt unberührt |
| `./restore.sh --test /var/backups/punkte/punkte-2026-12-01-0230.sql.gz` | dasselbe für eine bestimmte Sicherung |
| `./restore.sh /var/backups/punkte/punkte-2026-12-01-0230.sql.gz` | echte Datenbank ersetzen. Fragt nach („wiederherstellen“ eintippen), sichert vorher den aktuellen Stand als `punkte-before-restore-….sql.gz`, hält die API an und spielt alles in einer Transaktion ein. Scheitert etwas, bleibt der alte Stand |

**Aus restic** (Server weg): neuen Server nach Abschnitt 1 einrichten (bis Schritt 7), dann als root

```bash
set -a; . /root/.restic-env; set +a
restic restore latest --tag punkte --target /
cd /opt/fairteilt/deploy && ./restore.sh "$(ls -1t /var/backups/punkte/punkte-2*.sql.gz | head -n 1)"
```

Die Apps merken davon nichts, solange `.env` (vor allem `JWT_SECRET`) gleich bleibt. Mit neuem
Geheimnis melden sich alle einmal neu an. Was zwischen Sicherung und Ausfall geändert wurde, ist
verloren; Handys, die in der Zeit offline waren, schicken ihre Warteschlange danach noch einmal.

## 4. Aktualisieren

Jeder Push auf `main`, der den Server betrifft, baut im Workflow **Server-Image** ein neues Image
(`ghcr.io/schulzoli/fairteilt-server`, Tags `main` und `sha-<commit>`, bei `v*`-Tags die Version).

```bash
cd /opt/fairteilt/deploy && sudo ./update.sh
```

[update.sh](../deploy/update.sh) sichert zuerst, holt Repository und Image, startet neu und wartet,
bis `/health` antwortet. **Zurück auf eine ältere Version:** `sudo ./update.sh sha-1a2b3c4` (Tag aus der
Paketliste auf GitHub) setzt `API_IMAGE` in `.env` fest; `sudo ./update.sh main` folgt wieder dem
neuesten Stand. Hat die neuere Version schon Migrationen eingespielt, die alte Version passt nicht
mehr dazu: dann zusätzlich die Sicherung von vor dem Update zurückspielen (Abschnitt 3).

Ist das Paket auf GitHub privat (Einstellung des Pakets), braucht der Server vorher einmal
`docker login ghcr.io` mit einem Token, das nur `read:packages` darf. Öffentlich geht es ohne.

## 5. Nachsehen

In `/opt/fairteilt/deploy` (`.env` ist nur für root lesbar, darum `sudo`):

| Frage | Befehl |
| --- | --- |
| Läuft alles? | `sudo docker compose ps`, `curl -fsS https://fairteilt.oli-tech.eu/health` |
| Fehler der API | `sudo docker compose logs --since 1h api` |
| Kommt der Proxy durch? | `curl -fsS http://127.0.0.1:8791/health`, `journalctl -u caddy -n 50` |
| Letzte Sicherungen | `tail /var/log/fairteilt-backup.log` |
| Platz | `df -h`, `docker system df` (alte Images: `docker image prune`) |

## 6. Familien-Alpha: Abnahme

Fertig, wenn (ROADMAP α): der Server läuft so wie oben, die Wiederherstellung einmal echt auf einem
Wegwerf-Server geprobt ist, die Familie die signierte APK nutzt und zwei Wochen Alltag ohne
Datenverlust laufen.
