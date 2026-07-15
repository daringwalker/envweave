#!/usr/bin/env bash
set -euo pipefail

required=(
  APPLE_CERTIFICATE
  APPLE_CERTIFICATE_PASSWORD
  APPLE_SIGNING_IDENTITY
  APPLE_ID
  APPLE_PASSWORD
  APPLE_TEAM_ID
)
missing=()
for name in "${required[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    missing+=("$name")
  fi
done
if ((${#missing[@]})); then
  echo "Stable release requires macOS signing and notarization secrets: ${missing[*]}" >&2
  exit 1
fi
echo "macOS stable-release signing policy is satisfied"
