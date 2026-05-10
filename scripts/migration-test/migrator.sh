#!/usr/bin/env bash

set -eo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
compose_file="${repo_root}/docker-compose.migration-test.yml"

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <plan|migrate|verify|reset-run> [global options]" >&2
  exit 1
fi

command="$1"
shift

global_args=()
passthrough_args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-url|--target-url|--run-id|--domain)
      global_args+=("$1")
      shift
      if [[ $# -eq 0 ]]; then
        echo "missing value for ${global_args[-1]}" >&2
        exit 1
      fi
      global_args+=("$1")
      ;;
    --dry-run|--resume|--strict|--abort-on-warning|--json)
      global_args+=("$1")
      ;;
    *)
      passthrough_args+=("$1")
      ;;
  esac
  shift
done

docker compose -f "${compose_file}" build migrator >/dev/null
cmd=(docker compose -f "${compose_file}" run --rm migrator)
cmd+=("${global_args[@]}")
cmd+=("${command}")
cmd+=("${passthrough_args[@]}")
"${cmd[@]}"
