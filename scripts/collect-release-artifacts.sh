#!/usr/bin/env bash
set -euo pipefail

PLATFORM="${1:?platform is required}"
ARCH="${2:?architecture is required}"
OUTPUT="${3:?output directory is required}"
BUNDLE_ROOT="target/release/bundle"
VERSION="$(node -p "require('./apps/desktop/src-tauri/tauri.conf.json').version")"

mkdir -p "$OUTPUT"
found=0
while IFS= read -r -d '' artifact; do
  case "$artifact" in
    *.AppImage) extension="AppImage" ;;
    *.deb) extension="deb" ;;
    *.dmg) extension="dmg" ;;
    *) continue ;;
  esac
  destination="$OUTPUT/EnvWeave_${VERSION}_${PLATFORM}_${ARCH}.${extension}"
  if [[ -e "$destination" ]]; then
    echo "More than one .$extension artifact was produced for $PLATFORM/$ARCH" >&2
    exit 1
  fi
  cp "$artifact" "$destination"
  found=$((found + 1))
done < <(find "$BUNDLE_ROOT" -type f \( \
  -name "*_${VERSION}_*.AppImage" -o \
  -name "*_${VERSION}_*.deb" -o \
  -name "*_${VERSION}_*.dmg" \
\) -print0)

if ((found == 0)); then
  echo "No release artifacts found below $BUNDLE_ROOT" >&2
  exit 1
fi

{
  echo "version=$VERSION"
  echo "platform=$PLATFORM"
  echo "architecture=$ARCH"
  echo "commit=${GITHUB_SHA:-local}"
  echo "built_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$OUTPUT/BUILD-INFO-${PLATFORM}-${ARCH}.txt"
