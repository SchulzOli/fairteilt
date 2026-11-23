#!/usr/bin/env bash
# Restore a dump made by backup.sh (docs/BETRIEB.md, "Wiederherstellen").
#
#   ./restore.sh --test [dump.sql.gz]   restore into a scratch database and count rows;
#                                       production is not touched (default: newest dump)
#   ./restore.sh dump.sql.gz            replace the production database with the dump
#
# Before replacing anything, the current state is saved as punkte-before-restore-*.sql.gz,
# the API is stopped, and the dump is applied in one transaction: if anything fails,
# the old data stays as it was. The server runs pending migrations when it starts again.
set -euo pipefail
cd "$(dirname "$0")"
BACKUP_DIR=${BACKUP_DIR:-/var/backups/punkte}

die() { echo "restore: $*" >&2; exit 1; }
psql_db() { docker compose exec -T db psql -v ON_ERROR_STOP=1 -U punkte -q "$@"; }

test_mode=0
if [ "${1:-}" = "--test" ]; then
  test_mode=1
  shift
fi
file=${1:-}
if [ -z "$file" ]; then
  [ "$test_mode" = 1 ] || die "which dump? usage: ./restore.sh [--test] FILE.sql.gz"
  file=$(ls -1t "$BACKUP_DIR"/punkte-2*.sql.gz 2>/dev/null | head -n 1 || true)
  [ -n "$file" ] || die "no dump found in $BACKUP_DIR"
fi
[ -f "$file" ] || die "file not found: $file"
gzip -t "$file" || die "$file is not a valid gzip file"

if [ "$test_mode" = 1 ]; then
  scratch=punkte_restore_test
  echo "Restoring $file into the scratch database $scratch ..."
  psql_db -d postgres -c "DROP DATABASE IF EXISTS $scratch" -c "CREATE DATABASE $scratch OWNER punkte"
  gunzip -c "$file" | psql_db -d "$scratch" --single-transaction >/dev/null
  psql_db -d "$scratch" -At -F ' ' <<'SQL'
SELECT 'households ' || count(*) FROM households;
SELECT 'members    ' || count(*) FROM memberships;
SELECT 'tasks      ' || count(*) FROM tasks;
SELECT 'ledger     ' || count(*) FROM ledger_entries;
SELECT 'migrations ' || count(*) || ' (newest ' || max(version) || ')' FROM _sqlx_migrations;
SQL
  psql_db -d postgres -c "DROP DATABASE $scratch"
  echo "$(date -Is) restore test ok: $file"
  exit 0
fi

echo "This replaces ALL data in the production database with:"
echo "  $file ($(du -h "$file" | cut -f1), $(date -r "$file" '+%F %H:%M'))"
read -r -p 'Type "wiederherstellen" to continue: ' answer
[ "$answer" = "wiederherstellen" ] || die "cancelled, nothing changed"

mkdir -p "$BACKUP_DIR"
before="$BACKUP_DIR/punkte-before-restore-$(date +%F-%H%M%S).sql.gz"
echo "Saving the current state to $before ..."
docker compose exec -T db pg_dump -U punkte -d punkte --clean --if-exists | gzip > "$before"

echo "Stopping the API ..."
docker compose stop api
restart_api() {
  status=$?
  [ "$status" = 0 ] || echo "restore: FAILED, the database was not changed (rolled back)" >&2
  echo "Starting the API ..."
  docker compose start api
  exit "$status"
}
trap restart_api EXIT

echo "Restoring ..."
gunzip -c "$file" | psql_db -d punkte --single-transaction >/dev/null

echo "$(date -Is) restore ok: $file (previous state: $before)"
