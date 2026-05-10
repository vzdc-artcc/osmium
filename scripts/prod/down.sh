#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

load_env_file ".env.prod" "${1:-}"
require_command docker

log "stopping production services without deleting volumes"
dc --profile cutover down --remove-orphans

log "destructive reset is intentionally not the default"
echo "  If you truly need to delete persistent volumes, inspect and remove them manually."
