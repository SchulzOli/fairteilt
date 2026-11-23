#!/usr/bin/env bash
# Update the server (docs/BETRIEB.md, "Aktualisieren"): back up first, then fetch the
# new version and restart. The server runs new migrations when it starts.
#
#   ./update.sh            image from CI (API_IMAGE in .env), or build here if unset
#   ./update.sh <tag>      pin a specific image in .env, e.g. sha-1a2b3c4 to go back one
#                          version; ./update.sh main follows the newest build again
set -euo pipefail
cd "$(dirname "$0")"

# Back up first; on the very first start there is no database yet.
if [ -n "$(docker compose ps --status running -q db 2>/dev/null)" ]; then
  ./backup.sh
fi
git -C .. pull --ff-only

if [ -n "${1:-}" ]; then
  current=$(grep -E '^API_IMAGE=' .env | cut -d= -f2- || true)
  [ -n "$current" ] || { echo "update: set API_IMAGE in .env to use tags" >&2; exit 1; }
  sed -i "s|^API_IMAGE=.*|API_IMAGE=${current%:*}:$1|" .env
fi

if grep -qE '^API_IMAGE=' .env; then
  docker compose pull api
  docker compose up -d --no-build
else
  docker compose up -d --build
fi

echo "Waiting for the API ..."
for _ in $(seq 1 30); do
  if docker compose exec -T api curl -fsS http://localhost:8080/health >/dev/null 2>&1; then
    echo "$(date -Is) update ok: $(grep -E '^API_IMAGE=' .env | cut -d= -f2- || echo 'built here')"
    exit 0
  fi
  sleep 2
done
echo "update: API is not healthy, see: docker compose logs api" >&2
exit 1
