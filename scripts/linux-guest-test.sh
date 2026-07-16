#!/usr/bin/env bash
set -euo pipefail

DISTRO="${1:?distribution is required}"
SOURCE="${2:?source directory or archive is required}"
SOURCE_TREE="$SOURCE"
SOURCE_TEMP=""
if [[ -f "$SOURCE" ]]; then
  SOURCE_TEMP="$(mktemp -d)"
  trap 'rm -rf "$SOURCE_TEMP"' EXIT
  tar -xzf "$SOURCE" -C "$SOURCE_TEMP"
  SOURCE_TREE="$SOURCE_TEMP"
fi
NODE_VERSION="$(tr -d '[:space:]' < "$SOURCE_TREE/.node-version")"
PNPM_VERSION="11.7.0"
BASE="$HOME/.cache/envweave-linux/$DISTRO"
WORK="$BASE/source"
ARTIFACTS="$BASE/artifacts"
LOGS="$ARTIFACTS/logs"
NODE_HOME="$HOME/.local/node-v$NODE_VERSION"
export PATH="$NODE_HOME/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export CARGO_TERM_COLOR=always
export npm_config_registry="https://registry.npmmirror.com"
export RUSTUP_DIST_SERVER="https://rsproxy.cn"
export RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"

install_ubuntu_dependencies() {
  sudo apt-get update
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \
    build-essential curl ca-certificates git rsync file pkg-config patchelf \
    libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
    xvfb dbus-x11 openbox xdotool imagemagick flatpak
}

install_arch_dependencies() {
  sudo pacman -Syu --noconfirm --needed \
    base-devel curl ca-certificates git rsync file pkgconf patchelf \
    gtk3 webkit2gtk-4.1 libayatana-appindicator librsvg \
    xorg-server-xvfb dbus openbox xdotool imagemagick fuse2 wqy-zenhei flatpak

  # gdk-pixbuf 2.44 no longer ships external loader modules, while the
  # current linuxdeploy GTK plugin still expects its legacy directory.
  # An empty compatibility directory lets it generate a valid empty cache.
  local gdk_pixbuf_binarydir
  gdk_pixbuf_binarydir="$(pkgconf --variable=gdk_pixbuf_binarydir gdk-pixbuf-2.0)"
  sudo mkdir -p "$gdk_pixbuf_binarydir/loaders"
}

install_toolchains() {
  if [[ ! -x "$NODE_HOME/bin/node" ]]; then
    local archive="node-v${NODE_VERSION}-linux-x64.tar.xz"
    local temporary
    temporary="$(mktemp -d)"
    curl -fL "https://npmmirror.com/mirrors/node/v${NODE_VERSION}/${archive}" -o "$temporary/$archive"
    mkdir -p "$NODE_HOME"
    tar -xJf "$temporary/$archive" --strip-components=1 -C "$NODE_HOME"
    rm -rf "$temporary"
  fi
  if ! command -v cargo >/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  fi
  rustup component add rustfmt clippy
  if ! command -v pnpm >/dev/null || [[ "$(pnpm --version)" != "$PNPM_VERSION" ]]; then
    npm install --global --prefix "$HOME/.local" "pnpm@$PNPM_VERSION"
  fi
  npm config set registry "$npm_config_registry"
}

prepare_source() {
  # Results from a previous version can make a failed run look successful.
  # Keep compilation caches, but start every distro run with empty artifacts.
  rm -rf "$ARTIFACTS"
  mkdir -p "$WORK" "$LOGS"
  rsync -a --delete \
    --exclude .git \
    --exclude target \
    --exclude node_modules \
    --exclude dist \
    --exclude artifacts \
    "$SOURCE_TREE/" "$WORK/"
}

run_logged() {
  local name="$1"
  shift
  echo "==> $name"
  "$@" 2>&1 | tee "$LOGS/$name.log"
}

package_application() {
  # Tauri/linuxdeploy does not reliably replace every symlink in an AppDir
  # left by an earlier version. Keep the Rust compilation cache, but always
  # assemble release bundles from an empty directory.
  rm -rf target/release/bundle
  case "$DISTRO" in
    ubuntu)
      run_logged package pnpm --dir apps/desktop tauri build --bundles deb,appimage
      ;;
    arch)
      # linuxdeploy bundles an older strip that does not understand the RELR
      # ELF sections emitted by current Arch packages. Keep those libraries
      # unstripped; the system linker can load them normally.
      run_logged package env NO_STRIP=1 pnpm --dir apps/desktop tauri build --bundles appimage
      ;;
  esac
}

collect_results() {
  mkdir -p "$ARTIFACTS/bundles"
  find target/release/bundle -type f \( -name '*.deb' -o -name '*.AppImage' \) \
    -exec cp {} "$ARTIFACTS/bundles/" \;
  {
    echo "distribution=$DISTRO"
    echo "kernel=$(uname -srmo)"
    echo "node=$(node --version)"
    echo "pnpm=$(pnpm --version)"
    echo "rustc=$(rustc --version)"
    echo "cargo=$(cargo --version)"
    if command -v pacman >/dev/null; then
      echo "pacman_packages=$(pacman -Qq | wc -l | tr -d ' ')"
    fi
    echo "completed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } > "$ARTIFACTS/report.txt"
  tar -czf "/tmp/envweave-${DISTRO}-artifacts.tar.gz" -C "$ARTIFACTS" .
}

case "$DISTRO" in
  ubuntu) install_ubuntu_dependencies ;;
  arch) install_arch_dependencies ;;
  *) echo "Unsupported distribution: $DISTRO" >&2; exit 2 ;;
esac
install_toolchains
prepare_source
cd "$WORK"
run_logged rust-format cargo fmt --all -- --check
run_logged rust-clippy cargo clippy --workspace --all-targets -- -D warnings
run_logged rust-tests cargo test --workspace
if [[ "$DISTRO" == "arch" ]]; then
  run_logged pacman-live-scan env ENVWEAVE_LIVE_PACKAGE_MANAGER=pacman \
    cargo test -p envweave-packages scans_live_pacman_inventory_when_requested
fi
run_logged flatpak-live-scan env ENVWEAVE_LIVE_PACKAGE_MANAGER=flatpak \
  cargo test -p envweave-packages scans_live_flatpak_inventory_when_requested
run_logged pnpm-install pnpm install --frozen-lockfile
run_logged frontend pnpm check
package_application
appimage="$(find "$WORK/target/release/bundle" -type f -name '*.AppImage' -print -quit)"
if [[ -z "$appimage" ]]; then
  echo "Packaged AppImage was not found for launcher patching" >&2
  exit 1
fi
bash scripts/patch-linux-appimage.sh "$appimage"
bash scripts/linux-gui-smoke.sh "$WORK/target/release/envweave-desktop" "$ARTIFACTS"
bash scripts/linux-gui-smoke.sh "$appimage" "$ARTIFACTS/appimage-smoke"
collect_results
