#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
lsof_path=$(command -v lsof 2>/dev/null || true)
if [ -z "$lsof_path" ]; then
    printf 'privacy check not tested: lsof is required to inspect process sockets\n' >&2
    exit 1
fi
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

lsof_stdout="target/privacy-lsof.stdout"
lsof_stderr="target/privacy-lsof.stderr"
set +e
"$lsof_path" -nP -a -p "$pid" -i >"$lsof_stdout" 2>"$lsof_stderr"
lsof_status=$?
set -e

inspection_failed=false
if [ "$lsof_status" -gt 1 ] || [ -s "$lsof_stderr" ]; then
    inspection_failed=true
fi
if [ "$lsof_status" -eq 1 ] && [ -s "$lsof_stdout" ]; then
    inspection_failed=true
fi
if [ "$inspection_failed" = true ]; then
    printf 'privacy check not tested: lsof inspection failed with status %s\nstdout:\n%s\nstderr:\n%s\n' \
        "$lsof_status" "$(cat "$lsof_stdout")" "$(cat "$lsof_stderr")" >&2
    exit 1
fi

# lsof uses status 1 with empty output for no matches. Any socket listing is a failure.
if [ -s "$lsof_stdout" ]; then
    printf 'privacy check failed: resident application has network sockets:\n%s\n' \
        "$(cat "$lsof_stdout")" >&2
    exit 1
fi

printf 'privacy_smoke=pass resident_pid=%s network_sockets=0 lsof_status=%s\n' \
    "$pid" "$lsof_status"
