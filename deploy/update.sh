#!/usr/bin/env bash
# Update the server (docs/BETRIEB.md, "Aktualisieren"). Runs nightly from cron, works by hand too:
# fetches the repository and the image; only if something is new, it backs up first and restarts.
# The server runs new migrations when it starts.
#
#   ./update.sh            image from CI (API_IMAGE in .env), or build here if unset
#   ./update.sh <tag>      pin a specific image in .env, e.g. sha-1a2b3c4 to go back one
#                          version; ./update.sh main follows the newest build again
set -euo pipefail
cd "$(dirname "$0")"

if [ -n "${1:-}" ]; then
  current=$(grep -E '^API_IMAGE=' .env | cut -d= -f2- || true)
  [ -n "$current" ] || { echo "update: set API_IMAGE in .env to use tags" >&2; exit 1; }
  sed -i "s|^API_IMAGE=.*|API_IMAGE=${current%:*}:$1|" .env
fi

before=$(git -C .. rev-parse HEAD)
git -C .. pull --ff-only --quiet
after=$(git -C .. rev-parse HEAD)

prebuilt=$(grep -E '^API_IMAGE=' .env | cut -d= -f2- || true)
running=$(docker compose ps --status running -q api 2>/dev/null || true)
if [ -n "$prebuilt" ]; then
  docker compose pull --quiet api
  new_image=$(docker image inspect -f '{{.Id}}' "$prebuilt")
  old_image=$([ -n "$running" ] && docker inspect -f '{{.Image}}' "$running" || true)
  if [ -n "$running" ] && [ "$new_image" = "$old_image" ] && [ "$before" = "$after" ] && [ -z "${1:-}" ]; then
    echo "$(date -Is) nothing new ($prebuilt, ${after:0:7})"
    exit 0
  fi
fi

# Back up first; on the very first start there is no database yet.
if [ -n "$(docker compose ps --status running -q db 2>/dev/null)" ]; then
  ./backup.sh
fi

if [ -n "$prebuilt" ]; then
  docker compose up -d --no-build --remove-orphans
else
  docker compose up -d --build --remove-orphans
fi

echo "Waiting for the API ..."
for _ in $(seq 1 30); do
  if docker compose exec -T api curl -fsS -m 5 http://localhost:8080/health >/dev/null 2>&1; then
    echo "$(date -Is) update ok: ${prebuilt:-built here} (repository ${before:0:7} -> ${after:0:7})"
    docker image prune -f --filter "label=org.opencontainers.image.title=fairteilt-server" >/dev/null || true
    exit 0
  fi
  sleep 2
done
echo "$(date -Is) update: API is not healthy, see: docker compose logs api" >&2
exit 1
