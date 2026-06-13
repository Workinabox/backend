#!/usr/bin/env bash
# Start (or reuse) a local Postgres in Docker, then run the backend against it.
# The binary's built-in defaults already point at this container, so no env vars
# are needed. Extra args are forwarded to `cargo run` (e.g. `-- --persistence memory`).
set -euo pipefail

CONTAINER=wiab-pg
IMAGE=postgres:16

if [ -z "$(docker ps -q -f name="^${CONTAINER}$")" ]; then
  if [ -n "$(docker ps -aq -f name="^${CONTAINER}$")" ]; then
    echo "starting existing ${CONTAINER}…"
    docker start "${CONTAINER}" >/dev/null
  else
    echo "creating ${CONTAINER}…"
    docker run -d --name "${CONTAINER}" \
      -e POSTGRES_USER=wiab \
      -e POSTGRES_PASSWORD=wiab \
      -e POSTGRES_DB=wiab \
      -p 5432:5432 \
      "${IMAGE}" >/dev/null
  fi
fi

echo "waiting for postgres…"
until docker exec "${CONTAINER}" pg_isready -U wiab -d wiab >/dev/null 2>&1; do
  sleep 0.5
done
echo "postgres ready on localhost:5432 (wiab/wiab, db wiab)"

cd "$(dirname "$0")/.."
exec cargo run "$@"
