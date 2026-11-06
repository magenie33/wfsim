#!/usr/bin/env bash
# Hosted inventory scans keep their captured inputs and classifier scratch.
set -euo pipefail

fail() { echo "::error::inventory memory: $*" >&2; exit 1; }
if [ "${GITHUB_ACTIONS:-}" != true ] || [ "${INVENTORY_RUNNER_ENVIRONMENT:-}" != github-hosted ] \
  || [ "${INVENTORY_RUNNER_OS:-}" != Linux ] || [ "$(uname -s)" != Linux ]; then
  fail "requires a GitHub-hosted Linux runner"
fi
case "${RUNNER_TEMP:-}" in /*) ;; *) fail "RUNNER_TEMP must be an absolute directory" ;; esac
[ -d "$RUNNER_TEMP" ] && [ ! -L "$RUNNER_TEMP" ] || fail "RUNNER_TEMP must be a real directory"
swapfile="$RUNNER_TEMP/wfsim-inventory.swap"
[ ! -e "$swapfile" ] && [ ! -L "$swapfile" ] || fail "refusing to overwrite a swap path"
swap_bytes=$((24 * 1024 * 1024 * 1024))
reserve_kib=$((60 * 1024 * 1024))

space() {
  local path device available required
  for path in "$PWD" "${TMPDIR:-/tmp}"; do
    read -r device available < <(df -Pk -- "$path" | awk 'NR == 2 { print $1, $4 }')
    case "$available" in ''|*[!0-9]*) fail "cannot determine scratch space" ;; esac
    required=$reserve_kib
    [ "$device" != "$swap_device" ] || required=$((required + $1))
    [ "$available" -ge "$required" ] || return 1
  done
  available=$(df -Pk -- "$RUNNER_TEMP" | awk 'NR == 2 { print $4 }')
  case "$available" in ''|*[!0-9]*) fail "cannot determine swap space" ;; esac
  [ "$available" -ge "$(($1 + 1024 * 1024))" ] || return 1
}

free -b
swapon --show --bytes
df -Pk -- "$PWD" "${TMPDIR:-/tmp}" "$RUNNER_TEMP"
swap_device=$(df -Pk -- "$RUNNER_TEMP" | awk 'NR == 2 { print $1 }')
[ -n "$swap_device" ] || fail "cannot determine swap filesystem"
if ! space "$((swap_bytes / 1024))"; then
  echo "inventory memory: releasing unused hosted Android, .NET and GHC SDKs"
  sudo rm -rf -- /usr/local/lib/android /usr/share/dotnet /opt/ghc
  space "$((swap_bytes / 1024))" || fail "insufficient disk space for swap and complete inventory scratch"
fi
(umask 077; set -o noclobber; : > "$swapfile") || fail "swap path already exists"
chmod 600 "$swapfile"
sudo fallocate -l "$swap_bytes" -- "$swapfile"
[ "$(stat -c '%s' -- "$swapfile")" = "$swap_bytes" ] || fail "swap file size mismatch"
sudo mkswap -- "$swapfile"
sudo swapon -- "$swapfile"
swapon --show=NAME --noheadings --raw | awk -v name="$swapfile" '$0 == name { found = 1 } END { exit !found }' \
  || fail "new inventory swap is not active"
space 0 || fail "complete inventory scratch reserve was not retained"
free -b
swapon --show --bytes
df -Pk -- "$PWD" "${TMPDIR:-/tmp}" "$RUNNER_TEMP"
echo "inventory memory: additional swap_bytes=$swap_bytes; scratch_reserve_kib=$reserve_kib"
