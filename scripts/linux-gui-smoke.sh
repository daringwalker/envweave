#!/usr/bin/env bash
set -euo pipefail

BINARY="${1:?application binary is required}"
ARTIFACTS="${2:?artifact directory is required}"
DISPLAY_NUMBER="${ENVWEAVE_TEST_DISPLAY:-:99}"
mkdir -p "$ARTIFACTS/logs"
export DISPLAY="$DISPLAY_NUMBER"
export GDK_BACKEND=x11
export WEBKIT_DISABLE_COMPOSITING_MODE=1

Xvfb "$DISPLAY" -screen 0 1440x900x24 -nolisten tcp >"$ARTIFACTS/logs/xvfb.log" 2>&1 &
xvfb_pid=$!
trap 'kill "$xvfb_pid" 2>/dev/null || true' EXIT
sleep 1

# Start D-Bus only after DISPLAY is exported. Activated GTK portal services
# inherit the environment that the D-Bus daemon had when it started.
dbus-run-session -- bash -s -- "$BINARY" "$ARTIFACTS" <<'GUEST'
set -euo pipefail
BINARY="$1"
ARTIFACTS="$2"
trap 'kill "${app_pid:-}" "${wm_pid:-}" 2>/dev/null || true' EXIT
openbox >"$ARTIFACTS/logs/openbox.log" 2>&1 &
wm_pid=$!
"$BINARY" >"$ARTIFACTS/logs/gui-smoke.log" 2>&1 &
app_pid=$!

window=""
for _ in $(seq 1 30); do
  if ! kill -0 "$app_pid" 2>/dev/null; then
    echo "EnvWeave exited before creating a window" >&2
    cat "$ARTIFACTS/logs/gui-smoke.log" >&2
    exit 1
  fi
  window="$(xdotool search --onlyvisible --pid "$app_pid" 2>/dev/null | head -1 || true)"
  if [[ -z "$window" ]]; then
    window="$(xdotool search --onlyvisible --name EnvWeave 2>/dev/null | head -1 || true)"
  fi
  [[ -n "$window" ]] && break
  sleep 1
done

if [[ -z "$window" ]]; then
  echo "EnvWeave did not create a visible window within 30 seconds" >&2
  exit 1
fi
xdotool windowactivate --sync "$window"
sleep 2
import -display "$DISPLAY" -window root "$ARTIFACTS/gui-smoke.png"
test -s "$ARTIFACTS/gui-smoke.png"

# A visible GTK window is not enough: WebKit can create a window while its
# web view remains blank under a headless renderer. Check the stable content
# area for both a light background and non-zero visual variation.
if command -v magick >/dev/null; then
  image_command=(magick)
else
  image_command=(convert)
fi
read -r render_mean render_deviation < <(
  "${image_command[@]}" "$ARTIFACTS/gui-smoke.png" \
    -crop 800x600+400+170 -colorspace gray \
    -format '%[fx:mean] %[fx:standard_deviation]\n' info:
)
if ! awk -v mean="$render_mean" -v deviation="$render_deviation" \
  'BEGIN { exit !(mean > 0.70 && deviation > 0.005) }'; then
  echo "EnvWeave window did not render expected content (mean=$render_mean, deviation=$render_deviation)" >&2
  exit 1
fi
if grep -Eiq 'panic|fatal|segmentation fault' "$ARTIFACTS/logs/gui-smoke.log"; then
  cat "$ARTIFACTS/logs/gui-smoke.log" >&2
  exit 1
fi
echo "GUI smoke test passed; window=$window, mean=$render_mean, deviation=$render_deviation"
GUEST
