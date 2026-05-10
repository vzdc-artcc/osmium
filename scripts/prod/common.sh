#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
compose_file="${repo_root}/docker-compose.prod.yml"

log() {
  echo "[osmium-prod] $*"
}

die() {
  echo "[osmium-prod] error: $*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

make_absolute_path() {
  local path="$1"
  if [[ "${path}" = /* ]]; then
    printf '%s\n' "${path}"
  else
    printf '%s/%s\n' "${repo_root}" "${path#./}"
  fi
}

load_env_file() {
  local default_env="$1"
  local requested_env="${2:-${default_env}}"

  OSMIUM_ENV_FILE="$(make_absolute_path "${requested_env}")"
  [[ -f "${OSMIUM_ENV_FILE}" ]] || die "missing env file at ${OSMIUM_ENV_FILE}"

  set -a
  # shellcheck disable=SC1090
  source "${OSMIUM_ENV_FILE}"
  set +a

  export OSMIUM_ENV_FILE

  if [[ -n "${OSMIUM_FILES_DIR:-}" ]]; then
    OSMIUM_FILES_DIR="$(make_absolute_path "${OSMIUM_FILES_DIR}")"
  fi
  if [[ -n "${OSMIUM_DUMP_DIR:-}" ]]; then
    OSMIUM_DUMP_DIR="$(make_absolute_path "${OSMIUM_DUMP_DIR}")"
  fi

  export OSMIUM_FILES_DIR="${OSMIUM_FILES_DIR:-${repo_root}/dev-data/prod-files}"
  export OSMIUM_DUMP_DIR="${OSMIUM_DUMP_DIR:-${repo_root}/dev-data/mock-prod}"
}

require_non_placeholder_image() {
  [[ -n "${OSMIUM_IMAGE:-}" ]] || die "OSMIUM_IMAGE must be set in ${OSMIUM_ENV_FILE}"
  [[ "${OSMIUM_IMAGE}" != *"<"* ]] || die "OSMIUM_IMAGE still contains placeholder text: ${OSMIUM_IMAGE}"
}

ensure_runtime_paths() {
  mkdir -p "${OSMIUM_FILES_DIR}"
  mkdir -p "${OSMIUM_DUMP_DIR}"
}

dc() {
  docker compose --env-file "${OSMIUM_ENV_FILE}" -f "${compose_file}" "$@"
}

wait_for_service_health() {
  local service="$1"
  local timeout="${2:-120}"
  local elapsed=0

  while (( elapsed < timeout )); do
    local container_id
    container_id="$(dc ps -q "${service}" 2>/dev/null || true)"
    if [[ -n "${container_id}" ]]; then
      local status
      status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${container_id}" 2>/dev/null || true)"
      case "${status}" in
        healthy|running)
          return 0
          ;;
        unhealthy|exited|dead)
          dc logs "${service}" || true
          die "service ${service} is ${status}"
          ;;
      esac
    fi
    sleep 2
    elapsed=$((elapsed + 2))
  done

  dc logs "${service}" || true
  die "timed out waiting for ${service} to become healthy"
}

wait_for_api_endpoint() {
  local path="$1"
  local timeout="${2:-120}"
  local elapsed=0

  while (( elapsed < timeout )); do
    if dc exec -T api curl -fsS "http://127.0.0.1:3000${path}" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
    elapsed=$((elapsed + 2))
  done

  dc logs api || true
  die "timed out waiting for api endpoint ${path}"
}

print_recovery_guidance() {
  cat >&2 <<'EOF'
Recovery suggestions:
  - rerun verification: scripts/prod/migrator.sh verify
  - resume migration: scripts/prod/migrator.sh migrate --resume
  - intentionally restart a run: scripts/prod/migrator.sh reset-run --run-id <run-id>
EOF
}
