# Osmium task runner. Recipes wrap the commands in AGENTS.md §5; the plain cargo commands keep
# working.
set shell := ["bash", "-cu"]

import 'just/secrets.just'

# List recipes
default:
    @just --list

# Install the repo's git hooks: pre-commit secret scan and formatting, commit-msg attribution check. Once per clone.
install-hooks:
    git config core.hooksPath .githooks
    @echo "hooks installed: core.hooksPath=$(git config core.hooksPath)"

# Tests of the git hooks in .githooks/
hooks-check:
    bash .githooks/tests/test_commit_msg.sh
