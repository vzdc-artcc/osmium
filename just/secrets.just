gitleaks_version := "8.30.1"

# Uses the gitleaks binary if installed, otherwise Docker. Config: .gitleaks.toml; reviewed false
# positives: .gitleaksignore.
# Scan for committed secrets: tree (default), staged, or history
secrets-scan target="tree":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ target }}" in
      tree) args=(dir .) ;;
      staged) args=(git --pre-commit --staged .) ;;
      history) args=(git --log-opts=--all .) ;;
      *) echo "usage: just secrets-scan [tree|staged|history]" >&2; exit 2 ;;
    esac
    if command -v gitleaks >/dev/null 2>&1; then
      gitleaks "${args[@]}" --redact --no-banner
    elif command -v docker >/dev/null 2>&1; then
      docker run --rm -v "$PWD:/repo" -w /repo \
        ghcr.io/gitleaks/gitleaks:v{{ gitleaks_version }} "${args[@]}" --redact --no-banner
    else
      echo "secrets-scan needs gitleaks or Docker" >&2
      exit 2
    fi
