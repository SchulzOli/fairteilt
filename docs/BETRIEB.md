# Fairteilt – Betrieb (Familien-Alpha)

So läuft der Server auf Hetzner: einrichten, sichern, wiederherstellen, aktualisieren.
Grundsätze aus [KONZEPT §9.3](KONZEPT.md#93-sicherheit): nur 80/443 offen, SSH nur mit Schlüssel,
nächtliche Sicherung mit Kopie außer Haus.

Alles läuft in drei Containern ([docker-compose.yml](../deploy/docker-compose.yml)): `db` (Postgres 18),
`api` (der Rust-Server, fertig gebaut von der CI) und `caddy` (HTTPS mit Let's-Encrypt-Zertifikat).
Der Server spielt neue Migrationen beim Start selbst ein.

## 1. Server einrichten (einmalig)

1. **Server bestellen:** Hetzner Cloud, Debian 13, **x86** (z. B. CX22: 2 vCPU, 4 GB). Das
   Image aus der CI gibt es nur für x86 (`linux/amd64`). Beim Bestellen den eigenen SSH-Schlüssel
   hinterlegen.
2. **Firewall in der Hetzner Cloud** (nicht `ufw`: Docker umgeht `ufw`): eingehend TCP 22, 80, 443
   und UDP 443 (HTTP/3), alles andere zu. SSH nach Möglichkeit nur von der eigenen IP oder über
   Tailscale.
3. **DNS:** A-Eintrag (und AAAA für IPv6) `fairteilt.oli-tech.eu` auf die Server-IP.
4. **Auf dem Server** (als root):

   ```bash
   # SSH nur mit Schlüssel
   sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication no/' /etc/ssh/sshd_config && systemctl reload ssh
   # Sicherheitsupdates automatisch
   apt update && apt install -y unattended-upgrades git restic
   # Docker aus dem offiziellen Paket (Anleitung: docs.docker.com/engine/install/debian)
   ```

5. **Fairteilt holen und einrichten:**

   ```bash
   git clone https://github.com/SchulzOli/fairteilt.git /opt/fairteilt
   cd /opt/fairteilt/deploy
   cp .env.example .env && chmod 600 .env
   openssl rand -base64 32   # → POSTGRES_PASSWORD in .env
   openssl rand -base64 48   # → JWT_SECRET in .env
   nano .env                 # DOMAIN prüfen, API_IMAGE=ghcr.io/schulzoli/fairteilt-server:main einkommentieren
   ```

   Das Datenbank-Passwort gilt nur beim allerersten Start. Später ändern heißt: auch in Postgres
   ändern (`ALTER USER punkte PASSWORD …`). `JWT_SECRET` neu setzen meldet alle Geräte nach 15 Minuten ab.

6. **Push (optional):** Das Firebase-Dienstkonto als `deploy/secrets/firebase-service-account.json`
   hochladen (nie committen, nie in einen Chat kopieren). Der Server läuft im Container als Benutzer
   10001 und muss die Datei lesen können:

   ```bash
   chown 10001:10001 secrets/firebase-service-account.json && chmod 400 secrets/firebase-service-account.json
   ```

   In `.env` dann `FCM_SERVICE_ACCOUNT=/run/secrets/firebase-service-account.json` einkommentieren.

7. **Starten:** `./update.sh` holt das Image und startet (beim ersten Mal gibt es noch nichts zu sichern).
   Danach muss `https://fairteilt.oli-tech.eu/health` mit `"database":true` antworten; `docker compose logs api`
   zeigt beim Start „push over FCM enabled“, wenn Push eingerichtet ist.

8. **Sicherung einplanen** (`crontab -e` als root):

   ```cron
   30 2 * * * /opt/fairteilt/deploy/backup.sh >> /var/log/punkte-backup.log 2>&1
   15 4 * * 0 /opt/fairteilt/deploy/restore.sh --test >> /var/log/punkte-backup.log 2>&1
   ```

   Die zweite Zeile spielt sonntags die neueste Sicherung in eine Wegwerf-Datenbank und zählt die
   Zeilen. So fällt eine kaputte Sicherung auf, bevor man sie braucht.

## 2. Sichern

[backup.sh](../deploy/backup.sh) legt jede Nacht einen `pg_dump` nach `/var/backups/punkte` (nur für
root lesbar, 14 Tage). Ohne Kopie außer Haus ist er beim Verlust des Servers mit weg, darum:

**Kopie außer Haus mit restic** (verschlüsselt, z. B. Hetzner Storage Box per SFTP oder das NAS):

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
aufbewahren. Prüfen: `restic snapshots`.

## 3. Wiederherstellen

[restore.sh](../deploy/restore.sh) im Ordner `/opt/fairteilt/deploy`:

| Befehl | Wirkung |
| --- | --- |
| `./restore.sh --test` | neueste Sicherung probeweise in `punkte_restore_test`, Zeilen zählen, wieder löschen. Die echte Datenbank bleibt unberührt |
| `./restore.sh --test /var/backups/punkte/punkte-2026-12-01-0230.sql.gz` | dasselbe für eine bestimmte Sicherung |
| `./restore.sh /var/backups/punkte/punkte-2026-12-01-0230.sql.gz` | echte Datenbank ersetzen. Fragt nach („wiederherstellen“ eintippen), sichert vorher den aktuellen Stand als `punkte-before-restore-….sql.gz`, hält die API an und spielt alles in einer Transaktion ein. Scheitert etwas, bleibt der alte Stand |

**Aus restic** (Server weg): neuen Server nach Abschnitt 1 einrichten (bis Schritt 7), dann

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
cd /opt/fairteilt/deploy && ./update.sh
```

[update.sh](../deploy/update.sh) sichert zuerst, holt Repository und Image, startet neu und wartet,
bis `/health` antwortet. **Zurück auf eine ältere Version:** `./update.sh sha-1a2b3c4` (Tag aus der
Paketliste auf GitHub) setzt `API_IMAGE` in `.env` fest; `./update.sh main` folgt wieder dem
neuesten Stand. Hat die neuere Version schon Migrationen eingespielt, die alte Version passt nicht
mehr dazu: dann zusätzlich die Sicherung von vor dem Update zurückspielen (Abschnitt 3).

Ist das Paket auf GitHub privat (Einstellung des Pakets), braucht der Server vorher einmal
`docker login ghcr.io` mit einem Token, das nur `read:packages` darf. Öffentlich geht es ohne.

## 5. Nachsehen

| Frage | Befehl |
| --- | --- |
| Läuft alles? | `docker compose ps`, `curl -fsS https://fairteilt.oli-tech.eu/health` |
| Fehler der API | `docker compose logs --since 1h api` |
| Letzte Sicherungen | `tail /var/log/punkte-backup.log`, `restic snapshots` |
| Platz | `df -h`, `docker system df` (alte Images: `docker image prune`) |

## 6. Familien-Alpha: Abnahme

Fertig, wenn (ROADMAP α): der Server läuft so wie oben, die Wiederherstellung einmal echt auf einem
Wegwerf-Server geprobt ist, die Familie die signierte APK nutzt und zwei Wochen Alltag ohne
Datenverlust laufen.
