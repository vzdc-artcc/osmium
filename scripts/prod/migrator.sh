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

if [[ $# -lt 1 ]]; then
  echo "usage: $0 [env-file] <plan|migrate|verify|reset-run> [args]" >&2
  exit 1
fi

command="$1"
shift

case "${command}" in
  plan|migrate|verify|reset-run)
    ;;
  *)
    echo "unsupported command: ${command}" >&2
    exit 1
    ;;
esac

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

cmd=(--profile cutover run --rm migrator)
if (( ${#global_args[@]} > 0 )); then
  cmd+=("${global_args[@]}")
fi
cmd+=("${command}")
if (( ${#passthrough_args[@]} > 0 )); then
  cmd+=("${passthrough_args[@]}")
fi

dc "${cmd[@]}"
