#!/usr/bin/env bash
# Build and run tests/osd_queue.cpp against sys/minimal_osd.cpp
#
#   scripts/osd-queue-harness.sh check [thread|address]   # default: thread
#   scripts/osd-queue-harness.sh bench
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
mode="${1:-check}"
sanitizer="${2:-thread}"
mame="$root/mame-chd-src/src"

case "$mode" in
  check) flags=(-O1 -g "-fsanitize=$sanitizer") ;;
  bench) flags=(-O2) ;;
  *) echo "usage: $0 check [thread|address] | bench" >&2; exit 2 ;;
esac

bin="$root/target/osd-queue-harness/$mode"
mkdir -p "$(dirname "$bin")"
"${CXX:-c++}" -std=c++20 "${flags[@]}" -pthread -DLSB_FIRST -DCRLF=2 \
  -I"$mame/osd" -I"$mame/lib/util" \
  "$root/tests/osd_queue.cpp" "${OSD_SRC:-$root/sys/minimal_osd.cpp}" "$mame/lib/util/strformat.cpp" \
  -o "$bin"

export TSAN_OPTIONS="halt_on_error=1 ${TSAN_OPTIONS:-}" ASAN_OPTIONS="halt_on_error=1 ${ASAN_OPTIONS:-}"
# TSan can't start under Linux's default high mmap randomisation; disable
# ASLR for this process where allowed (not under Docker's default seccomp).
if [ "$mode $sanitizer" = "check thread" ] && [ "$(uname -s)" = Linux ] && setarch "$(uname -m)" -R true 2>/dev/null; then
  exec setarch "$(uname -m)" -R "$bin" "$mode"
fi
exec "$bin" "$mode"
