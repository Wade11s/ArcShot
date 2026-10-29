#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
scripts/build-app.sh >/dev/null

log="target/resident-smoke.log"
"target/bundle/debug/ArcShot.app/Contents/MacOS/arcshot" >"$log" 2>&1 &
pid=$!
cleanup() {
    if kill -0 "$pid" 2>/dev/null; then
        kill -TERM "$pid" 2>/dev/null || true
    fi
    wait "$pid" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

sleep 0.5
if ! kill -0 "$pid" 2>/dev/null; then
    printf 'privacy check failed: application did not remain resident: %s\n' "$(cat "$log")" >&2
    exit 1
fi

sockets=$(lsof -nP -a -p "$pid" -i 2>/dev/null || true)
if [ -n "$sockets" ]; then
    printf 'privacy check failed: resident application has network sockets:\n%s\n' "$sockets" >&2
    exit 1
fi

printf 'privacy_smoke=pass resident_pid=%s network_sockets=0\n' "$pid"
