#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${APPLE_CERTIFICATE:-}" ]]; then
  echo "No Developer ID certificate configured; using an ad-hoc signature for this prerelease build"
  echo "APPLE_SIGNING_IDENTITY=-" >> "$GITHUB_ENV"
  exit 0
fi
: "${APPLE_CERTIFICATE_PASSWORD:?APPLE_CERTIFICATE_PASSWORD is required when APPLE_CERTIFICATE is set}"
: "${KEYCHAIN_PASSWORD:?KEYCHAIN_PASSWORD is required when APPLE_CERTIFICATE is set}"
: "${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY is required when APPLE_CERTIFICATE is set}"

certificate="$RUNNER_TEMP/envweave-certificate.p12"
keychain="$RUNNER_TEMP/envweave-signing.keychain-db"
printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$certificate"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$keychain"
security import "$certificate" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security list-keychains -d user -s "$keychain" login.keychain-db
security default-keychain -d user -s "$keychain"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$keychain"
security find-identity -v -p codesigning "$keychain"
printf 'APPLE_SIGNING_IDENTITY=%s\n' "$APPLE_SIGNING_IDENTITY" >> "$GITHUB_ENV"
