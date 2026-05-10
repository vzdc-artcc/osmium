#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

env_arg=""
if [[ $# -gt 0 ]]; then
  candidate_env="$(make_absolute_path "$1")"
  if [[ -f "${candidate_env}" ]]; then
    env_arg="$1"
    shift
  fi
fi

load_env_file ".env.cutover" "${env_arg}"
require_command docker
require_non_placeholder_image
ensure_runtime_paths

dump_file="${OSMIUM_DUMP_DIR}/prod.sql"
[[ -f "${dump_file}" ]] || die "missing dump file at ${dump_file}"

trap 'print_recovery_guidance' ERR

log "validating compose configuration"
dc config >/dev/null
dc --profile cutover config >/dev/null

log "starting target postgres"
dc up -d postgres
wait_for_service_health postgres 120

log "bootstrapping target schema with internal migration service"
dc --profile cutover up -d target-init
wait_for_service_health target-init 120
dc --profile cutover stop target-init >/dev/null 2>&1 || true
dc --profile cutover rm -fsv target-init >/dev/null 2>&1 || true

log "starting temporary legacy source postgres"
dc --profile cutover up -d legacy-postgres
wait_for_service_health legacy-postgres 120

log "loading legacy dump into temporary source database"
dc --profile cutover run --rm legacy-seed

log "running migrator plan"
"${repo_root}/scripts/prod/migrator.sh" "${OSMIUM_ENV_FILE}" plan

log "running migrator migrate"
"${repo_root}/scripts/prod/migrator.sh" "${OSMIUM_ENV_FILE}" migrate

log "running migrator verify"
"${repo_root}/scripts/prod/migrator.sh" "${OSMIUM_ENV_FILE}" verify

log "starting api after successful migration"
dc up -d api
wait_for_service_health api 120
wait_for_api_endpoint /health 120

log "current readiness response"
dc exec -T api curl -fsS http://127.0.0.1:3000/ready
echo

log "tearing down temporary cutover services"
dc --profile cutover stop legacy-postgres >/dev/null 2>&1 || true
dc --profile cutover rm -fsv legacy-seed legacy-postgres >/dev/null 2>&1 || true
docker volume rm osmium-prod-legacy-postgres-data >/dev/null 2>&1 || true

log "cutover completed"
cat <<EOF
Next steps:
  1. Copy ${repo_root}/.env.prod.example to ${repo_root}/.env.prod and fill in steady-state values.
  2. Ensure worker flags are enabled in ${repo_root}/.env.prod:
     STATS_SYNC_ENABLED=true
     ROSTER_SYNC_ENABLED=true
     EMAIL_WORKER_ENABLED=true
     EMAIL_ENABLED=true
  3. Recreate the steady-state api with the steady-state env:
     scripts/prod/up.sh ${repo_root}/.env.prod
  4. Verify:
     curl -s http://127.0.0.1:${OSMIUM_API_HOST_PORT:-3000}/health
     curl -s http://127.0.0.1:${OSMIUM_API_HOST_PORT:-3000}/ready
EOF
