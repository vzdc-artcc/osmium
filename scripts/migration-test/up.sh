#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
compose_file="${repo_root}/docker-compose.migration-test.yml"
dump_file="${repo_root}/dev-data/mock-prod/prod.sql"

if [[ ! -f "${dump_file}" ]]; then
  echo "missing dump file at ${dump_file}" >&2
  exit 1
fi

docker compose -f "${compose_file}" down -v --remove-orphans
docker compose -f "${compose_file}" build api migrator
docker compose -f "${compose_file}" up --build -d mock-prod-postgres osmium-postgres
docker compose -f "${compose_file}" run --rm mock-prod-seed
docker compose -f "${compose_file}" up -d api
