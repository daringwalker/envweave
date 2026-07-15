#!/usr/bin/env bash
set -euo pipefail

EXPECTED_TAG="${1:-}"
TAURI_CONFIG="apps/desktop/src-tauri/tauri.conf.json"
FRONTEND_PACKAGE="apps/desktop/package.json"

tauri_version="$(node -p "require('./${TAURI_CONFIG}').version")"
frontend_version="$(node -p "require('./${FRONTEND_PACKAGE}').version")"
workspace_version="$(awk '
  /^\[workspace.package\]$/ { in_package=1; next }
  /^\[/ { in_package=0 }
  in_package && /^version = / { gsub(/[\"[:space:]]/, "", $3); print $3; exit }
' Cargo.toml)"

if [[ -z "$workspace_version" ]]; then
  echo "Cannot read workspace version from Cargo.toml" >&2
  exit 1
fi
if [[ "$tauri_version" != "$workspace_version" || "$frontend_version" != "$workspace_version" ]]; then
  echo "Release versions differ: Cargo=$workspace_version Tauri=$tauri_version frontend=$frontend_version" >&2
  exit 1
fi
if [[ -n "$EXPECTED_TAG" && "$EXPECTED_TAG" != "v$workspace_version" ]]; then
  echo "Tag $EXPECTED_TAG does not match application version v$workspace_version" >&2
  exit 1
fi
if ! grep -Fq "## $workspace_version" CHANGELOG.md; then
  echo "CHANGELOG.md has no entry for $workspace_version" >&2
  exit 1
fi
if [[ ! -f "docs/releases/v${workspace_version}.md" ]]; then
  echo "Missing release notes: docs/releases/v${workspace_version}.md" >&2
  exit 1
fi

echo "Release metadata is consistent: v$workspace_version"
