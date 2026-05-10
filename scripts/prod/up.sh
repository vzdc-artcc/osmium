#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

load_env_file ".env.prod" "${1:-}"
require_command docker
require_non_placeholder_image
ensure_runtime_paths

log "starting steady-state production services"
dc up -d postgres api

log "next checks:"
echo "  docker compose --env-file ${OSMIUM_ENV_FILE} -f ${compose_file} ps"
echo "  curl -s http://127.0.0.1:${OSMIUM_API_HOST_PORT:-3000}/health"
echo "  curl -s http://127.0.0.1:${OSMIUM_API_HOST_PORT:-3000}/ready"
