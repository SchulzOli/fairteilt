#!/usr/bin/env bash
# Nightly database dump (docs/BETRIEB.md, "Sichern"). Run from cron on the server, e.g.
#   30 2 * * * /opt/fairteilt/deploy/backup.sh >> /var/log/punkte-backup.log 2>&1
# With RESTIC_REPOSITORY and RESTIC_PASSWORD (or RESTIC_ENV_FILE pointing to a file that sets
# them), the dumps are also copied off the box with restic (Hetzner Storage Box or the NAS).
set -euo pipefail
cd "$(dirname "$0")"
BACKUP_DIR=${BACKUP_DIR:-/var/backups/punkte}
KEEP_DAYS=${KEEP_DAYS:-14}
RESTIC_ENV_FILE=${RESTIC_ENV_FILE:-/root/.restic-env}
umask 077   # dumps hold personal data
mkdir -p "$BACKUP_DIR"
file="$BACKUP_DIR/punkte-$(date +%F-%H%M).sql.gz"
docker compose exec -T db pg_dump -U punkte -d punkte --clean --if-exists | gzip > "$file"
gzip -t "$file"
find "$BACKUP_DIR" -name 'punkte-*.sql.gz' -mtime +"$KEEP_DAYS" -delete
echo "$(date -Is) backup ok: $file ($(du -h "$file" | cut -f1))"

if [ -z "${RESTIC_REPOSITORY:-}" ] && [ -r "$RESTIC_ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090
  . "$RESTIC_ENV_FILE"
  set +a
fi
if [ -n "${RESTIC_REPOSITORY:-}" ]; then
  restic backup --quiet --tag punkte "$BACKUP_DIR"
  restic forget --quiet --tag punkte --keep-daily 7 --keep-weekly 8 --keep-monthly 12 --prune
  echo "$(date -Is) off-site ok: $RESTIC_REPOSITORY"
fi
