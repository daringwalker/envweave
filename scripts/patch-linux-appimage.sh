#!/usr/bin/env bash
set -euo pipefail

APPIMAGE="${1:?AppImage path is required}"
if [[ ! -f "$APPIMAGE" ]]; then
  echo "AppImage does not exist: $APPIMAGE" >&2
  exit 1
fi
APPIMAGE="$(cd "$(dirname "$APPIMAGE")" && pwd)/$(basename "$APPIMAGE")"

TAURI_CACHE="${TAURI_CACHE_DIR:-$HOME/.cache/tauri}"
PACKAGER="$TAURI_CACHE/linuxdeploy-plugin-appimage.AppImage"
if [[ ! -x "$PACKAGER" ]]; then
  echo "Tauri AppImage packager is not available: $PACKAGER" >&2
  exit 1
fi

TEMPORARY="$(mktemp -d)"
trap 'rm -rf "$TEMPORARY"' EXIT
(
  cd "$TEMPORARY"
  "$APPIMAGE" --appimage-extract >/dev/null
)
APPDIR="$TEMPORARY/squashfs-root"
mv "$APPDIR/AppRun" "$APPDIR/AppRun.envweave"
cat >"$APPDIR/AppRun" <<'APP_RUN'
#!/usr/bin/env bash
set -e

this_dir="$(readlink -f "$(dirname "$0")")"
if [[ -r /etc/os-release ]]; then
  # shellcheck disable=SC1091
  source /etc/os-release
  distro_family=" ${ID:-} ${ID_LIKE:-} "
  if [[ "$distro_family" == *" arch "* ]]; then
    for library in \
      /usr/lib/libwayland-client.so.0 \
      /usr/lib64/libwayland-client.so.0 \
      /usr/lib/x86_64-linux-gnu/libwayland-client.so.0; do
      if [[ -r "$library" ]]; then
        export LD_PRELOAD="$library${LD_PRELOAD:+:$LD_PRELOAD}"
        break
      fi
    done
  fi
fi

exec "$this_dir/AppRun.envweave" "$@"
APP_RUN
chmod +x "$APPDIR/AppRun"

rm -f "$TEMPORARY"/*.AppImage
(
  cd "$TEMPORARY"
  ARCH=x86_64 "$PACKAGER" --appimage-extract-and-run --appdir="$APPDIR"
)
mapfile -t generated < <(find "$TEMPORARY" -maxdepth 1 -type f -name '*.AppImage')
if ((${#generated[@]} != 1)); then
  echo "Expected one repacked AppImage, found ${#generated[@]}" >&2
  exit 1
fi
chmod +x "${generated[0]}"
mv "${generated[0]}" "$APPIMAGE"
echo "Patched AppImage launcher: $APPIMAGE"
