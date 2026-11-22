#!/usr/bin/env bash
# Nightly database dump. Run from cron on the server, e.g.
#   30 2 * * * /opt/punkte/deploy/backup.sh >> /var/log/punkte-backup.log 2>&1
# Ship the dumps off the box with restic (Hetzner Storage Box or the NAS at home).
set -euo pipefail
cd "$(dirname "$0")"
BACKUP_DIR=${BACKUP_DIR:-/var/backups/punkte}
KEEP_DAYS=${KEEP_DAYS:-14}
mkdir -p "$BACKUP_DIR"
file="$BACKUP_DIR/punkte-$(date +%F-%H%M).sql.gz"
docker compose exec -T db pg_dump -U punkte -d punkte --clean --if-exists | gzip > "$file"
find "$BACKUP_DIR" -name 'punkte-*.sql.gz' -mtime +"$KEEP_DAYS" -delete
echo "$(date -Is) backup ok: $file ($(du -h "$file" | cut -f1))"
# Optional off-site copy:
# restic -r "$RESTIC_REPOSITORY" backup "$BACKUP_DIR" && restic forget --keep-daily 7 --keep-weekly 8 --prune
