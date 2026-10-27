#!/usr/bin/env bash
set -euo pipefail

build=${1:?build mode required}
llvm_version=${2:?LLVM version required}
[[ "$llvm_version" =~ ^[0-9]+$ ]] && [ "$llvm_version" -ge 19 ] || exit 2
packages=(fonts-noto-cjk "lld-$llvm_version")
case "$build" in
  true) ;;
  false) packages+=(libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev) ;;
  *) echo "unknown site-tools mode: $build" >&2; exit 2 ;;
esac

missing=()
for package in "${packages[@]}"; do
  if [ "$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null || true)" != 'install ok installed' ]; then
    missing+=("$package")
  fi
done
if [ "${#missing[@]}" -eq 0 ]; then
  echo 'site dependencies already installed'
  exit 0
fi

log_dir=$(mktemp -d)
trap 'rm -rf "$log_dir"' EXIT
apt_step() {
  local stage=$1; shift
  echo "::group::$stage"
  if "$@" > "$log_dir/$stage.log" 2>&1; then
    tail -n 20 "$log_dir/$stage.log"
  else
    local status=$?
    cat "$log_dir/$stage.log"
    echo "::endgroup::"
    exit "$status"
  fi
  echo "::endgroup::"
}
apt_step update sudo timeout 180s apt-get -o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 update -qq
apt_step install sudo env DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=a timeout 300s apt-get \
  -o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 \
  install -y --no-install-recommends "${missing[@]}"
