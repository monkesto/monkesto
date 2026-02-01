#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOKS_DIR="$PROJECT_ROOT/.git/hooks"

chmod +x "$SCRIPT_DIR/pre-commit"

if [ -f "$HOOKS_DIR/pre-commit" ]; then
    mv "$HOOKS_DIR/pre-commit" "$HOOKS_DIR/pre-commit.backup"
fi

ln -sf "../../scripts/pre-commit" "$HOOKS_DIR/pre-commit"
