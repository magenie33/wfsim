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
    return "$status"
  fi
  echo "::endgroup::"
}
apt_options=(-o Acquire::Retries=0 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20
  -o APT::Update::Error-Mode=any)
download() {
  local stage=$1 seconds=$2
  apt_step "$stage-update" sudo timeout 90s apt-get "${apt_options[@]}" update -qq || return $?
  apt_step "$stage-download" sudo env DEBIAN_FRONTEND=noninteractive timeout "${seconds}s" apt-get \
    "${apt_options[@]}" install -y --no-install-recommends --download-only "${missing[@]}"
}
if ! download primary 120; then
  echo '::warning::runner Ubuntu mirror failed; retrying downloads against the official archive'
  python3 - "${WFSIM_APT_SOURCE_DIR:-/etc/apt}" "$log_dir/mirror" <<'PY'
from pathlib import Path
import re
import sys

source, dest = map(Path, sys.argv[1:])
(dest / 'sources.list.d').mkdir(parents=True)
files = [source / 'sources.list', *(source / 'sources.list.d').glob('*.list'),
         *(source / 'sources.list.d').glob('*.sources')]
changed = 0
for path in files:
    if not path.is_file():
        continue
    text, mirrors = re.subn(r'mirror\+file:/+etc/apt/apt-mirrors(-security)?\.txt\b',
        lambda m: 'https://' + ('security' if m[1] else 'archive') + '.ubuntu.com/ubuntu', path.read_text())
    text, count = re.subn(r'(?:https?://azure\.archive|http://archive)\.ubuntu\.com/ubuntu\b',
                         'https://archive.ubuntu.com/ubuntu', text)
    changed += count + mirrors
    (dest / path.relative_to(source)).write_text(text)
if not changed:
    sys.exit('No supported Ubuntu runner mirror found; retaining the setup failure')
PY
  apt_options+=(-o "Dir::Etc::sourcelist=$log_dir/mirror/sources.list"
    -o "Dir::Etc::sourceparts=$log_dir/mirror/sources.list.d")
  download fallback 180
fi
apt_step install sudo env DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=a timeout 300s apt-get \
  "${apt_options[@]}" install -y --no-install-recommends --no-download "${missing[@]}"
