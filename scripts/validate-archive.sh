#!/usr/bin/env bash
# Validate a pre-built libchdman_rs static archive.
#
# Usage:
#   scripts/validate-archive.sh <path-to-archive>
#
# Checks:
#   1. File exists and is a non-empty ar archive (Linux/macOS) or COFF
#      archive (Windows .lib).
#   2. Contains the expected externally-visible chd_shim_* symbols.
#   3. Prints `file` output as a best-effort target hint.
#
# Exit codes:
#   0  All checks pass
#   1  File missing or wrong format
#   2  Required symbol missing — almost certainly a broken build

set -euo pipefail

archive="${1:?usage: $0 <archive-path>}"

if [ ! -s "$archive" ]; then
  echo "FAIL: $archive missing or empty"
  exit 1
fi

ext="${archive##*.}"
case "$ext" in
  a)
    file "$archive" | grep -q 'archive' || {
      echo "FAIL: $archive is not an ar archive"; exit 1
    }
    ;;
  lib)
    file "$archive" | grep -qi 'archive' || {
      echo "FAIL: $archive is not a Windows archive (.lib)"; exit 1
    }
    ;;
  *)
    echo "FAIL: unknown archive extension .$ext"; exit 1
    ;;
esac

# Sentinel symbols every libchdman_rs build must export. Keep this small
# and stable — update when the wrapped C API gains or loses entry points.
required_symbols=(
  "chd_shim_alloc"
  "chd_shim_free"
  "chd_shim_open_file"
  "chd_shim_close"
  "chd_shim_read_bytes"
  "chd_shim_version"
)

list_symbols() {
  if [ "$ext" = "a" ]; then
    nm --defined-only --extern-only "$archive" 2>/dev/null \
      || nm -g "$archive" 2>/dev/null \
      || nm "$archive"
  else
    if command -v dumpbin >/dev/null 2>&1; then
      # Use the dash form. Under MSYS bash, a leading-slash arg like
      # `/symbols` is path-converted into a Windows path and dumpbin
      # then sees garbage instead of the option.
      dumpbin -symbols "$archive"
    elif command -v llvm-nm >/dev/null 2>&1; then
      llvm-nm "$archive"
    else
      echo "WARN: no tool available to inspect .lib symbols; install llvm or dumpbin" >&2
      return 1
    fi
  fi
}

symbols=$(list_symbols || true)
missing=0
# Use a herestring rather than `echo | grep`: under `set -o pipefail`, grep -q
# closing its stdin early after a match triggers SIGPIPE on echo, which
# pipefail then propagates as a pipeline failure — causing every symbol to
# be reported missing on hosts that flush small enough for the race.
for sym in "${required_symbols[@]}"; do
  if ! grep -q -- "$sym" <<< "$symbols"; then
    echo "FAIL: missing required symbol: $sym"
    missing=1
  fi
done
if [ "$missing" -ne 0 ]; then
  echo "FAIL: one or more required symbols missing — archive is likely broken"
  exit 2
fi

size=$(wc -c < "$archive")
echo "OK: $archive ($((size / 1024)) KiB)"
echo "OK: all ${#required_symbols[@]} required symbols present"
echo "INFO: $(file "$archive" | head -1)"
exit 0
