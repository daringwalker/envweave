#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ACTION="${1:-all}"
CPUS="${ENVWEAVE_VM_CPUS:-8}"
MEMORY="${ENVWEAVE_VM_MEMORY_GIB:-8}"
DISK="${ENVWEAVE_VM_DISK_GIB:-60}"

usage() {
  echo "Usage: $0 [all|ubuntu|arch|status|stop|destroy]"
}

ensure_runtime() {
  if ! command -v brew >/dev/null; then
    echo "Homebrew is required to install Lima and QEMU." >&2
    exit 1
  fi
  command -v limactl >/dev/null || brew install lima
  if ! command -v qemu-system-x86_64 >/dev/null; then
    install_qemu
  fi
}

install_qemu() {
  local macos_major
  macos_major="$(sw_vers -productVersion | cut -d. -f1)"
  if (( macos_major >= 13 )); then
    brew install qemu
    return
  fi

  # QEMU 9+ requires Apple Clang 15. macOS 12 ships Clang 14, so extract the
  # last Homebrew formula known to build on Monterey from the trusted core tap.
  local tap="${USER}/envweave"
  local formula="$(brew --repository)/Library/Taps/${USER}/homebrew-envweave/Formula/qemu@8.2.2.rb"
  brew tap | grep -Fxq "$tap" || brew tap-new "$tap"
  brew tap | grep -Fxq homebrew/core || brew tap --force homebrew/core
  if [[ ! -f "$formula" ]]; then
    brew extract \
      --version=8.2.2 \
      --git-revision=13950206061360b5207eaffaa49cd3fd3c7dcab0 \
      qemu "$tap"
  fi
  HOMEBREW_NO_AUTO_UPDATE=1 brew install "$tap/qemu@8.2.2"
  brew link --force qemu@8.2.2
}

instance_exists() {
  limactl list --format '{{.Name}}' 2>/dev/null | grep -Fxq "$1"
}

instance_status() {
  limactl list --format '{{.Name}} {{.Status}}' 2>/dev/null \
    | awk -v name="$1" '$1 == name { print $2; exit }'
}

start_instance() {
  local distro="$1"
  local name="envweave-${distro}"
  local template
  case "$distro" in
    ubuntu) template="ubuntu-24.04" ;;
    arch) template="archlinux" ;;
    *) echo "Unsupported distribution: $distro" >&2; exit 1 ;;
  esac

  if instance_exists "$name"; then
    if [[ "$(instance_status "$name")" != "Running" ]]; then
      limactl start --yes --timeout 20m "$name"
    fi
  else
    limactl start --yes \
      --name "$name" \
      --vm-type qemu \
      --cpus "$CPUS" \
      --memory "$MEMORY" \
      --disk "$DISK" \
      --containerd none \
      --mount-only "$ROOT" \
      --timeout 20m \
      "template:${template}"
  fi
}

run_distribution() {
  local distro="$1"
  local name="envweave-${distro}"
  local guest_archive="/tmp/envweave-${distro}-artifacts.tar.gz"
  local guest_source="/tmp/envweave-${distro}-source.tar.gz"
  local guest_runner="/tmp/envweave-linux-guest-test.sh"
  local output="$ROOT/artifacts/linux/$distro"
  local host_source
  host_source="$(mktemp -t "envweave-${distro}-source.XXXXXX")"
  trap 'rm -f "$host_source"' RETURN

  start_instance "$distro"
  tar -czf "$host_source" \
    --exclude .git \
    --exclude target \
    --exclude node_modules \
    --exclude dist \
    --exclude artifacts \
    -C "$ROOT" .
  # Copy an immutable snapshot instead of executing through the live virtiofs
  # mount. Older macOS/QEMU combinations can briefly expose a partially
  # updated file while the host editor replaces it atomically.
  limactl copy "$host_source" "$name:$guest_source"
  limactl copy "$ROOT/scripts/linux-guest-test.sh" "$name:$guest_runner"
  limactl shell "$name" -- bash "$guest_runner" "$distro" "$guest_source"
  mkdir -p "$output"
  limactl copy "$name:$guest_archive" "$output/results.tar.gz"
  tar -xzf "$output/results.tar.gz" -C "$output"
  echo "[$distro] results: $output"
}

run_ubuntu_appimage_on_arch() {
  local name="envweave-arch"
  local appimage
  appimage="$(find "$ROOT/artifacts/linux/ubuntu/bundles" -type f -name '*.AppImage' -print -quit)"
  if [[ -z "$appimage" ]]; then
    echo "Ubuntu AppImage is required for the Arch portability test" >&2
    exit 1
  fi
  local guest_appimage="/tmp/envweave-ubuntu-build.AppImage"
  local guest_smoke="/tmp/envweave-linux-gui-smoke.sh"
  local guest_results="/tmp/envweave-cross-distro-smoke"
  local output="$ROOT/artifacts/linux/cross-distro"
  limactl copy "$appimage" "$name:$guest_appimage"
  limactl copy "$ROOT/scripts/linux-gui-smoke.sh" "$name:$guest_smoke"
  limactl shell "$name" -- bash -lc \
    "chmod +x '$guest_appimage'; rm -rf '$guest_results'; bash '$guest_smoke' '$guest_appimage' '$guest_results'"
  rm -rf "$output"
  mkdir -p "$output"
  limactl copy "$name:$guest_results/." "$output"
  echo "[ubuntu -> arch] AppImage results: $output"
}

manage_instances() {
  local operation="$1"
  local name
  for name in envweave-ubuntu envweave-arch; do
    if instance_exists "$name"; then
      case "$operation" in
        stop)
          if [[ "$(instance_status "$name")" == "Running" ]]; then
            limactl stop "$name"
          fi
          ;;
        destroy) limactl delete --force "$name" ;;
      esac
    fi
  done
}

case "$ACTION" in
  ubuntu|arch)
    ensure_runtime
    run_distribution "$ACTION"
    ;;
  all)
    ensure_runtime
    run_distribution ubuntu
    run_distribution arch
    run_ubuntu_appimage_on_arch
    ;;
  status)
    command -v limactl >/dev/null && limactl list || echo "Lima is not installed"
    ;;
  stop|destroy)
    command -v limactl >/dev/null && manage_instances "$ACTION"
    ;;
  *)
    usage
    exit 2
    ;;
esac
