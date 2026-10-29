#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

configuration=debug
if [ "$#" -eq 1 ] && [ "$1" = "--release" ]; then
    configuration=release
elif [ "$#" -ne 0 ]; then
    printf 'usage: %s [--release]\n' "$0" >&2
    exit 2
fi

build_output=$(scripts/build-app.sh ${1:-})
printf '%s\n' "$build_output"
log="target/startup-smoke-$configuration.log"
"target/bundle/$configuration/ArcShot.app/Contents/MacOS/arcshot" --smoke-test >"$log" 2>&1 &
pid=$!
ticks=0
while kill -0 "$pid" 2>/dev/null; do
    ticks=$((ticks + 1))
    if [ "$ticks" -ge 50 ]; then
        kill -TERM "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
        printf 'startup check failed: timed out waiting for native quit action\n' >&2
        exit 1
    fi
    sleep 0.1
done

set +e
wait "$pid"
status=$?
set -e
output=$(cat "$log")
if [ "$status" -ne 0 ]; then
    printf 'startup check failed: application exited %s: %s\n' "$status" "$output" >&2
    exit 1
fi
case "$output" in
    *'startup_smoke=pass'*'quit_action=terminate:'*) ;;
    *)
        printf 'startup check failed: expected native status item and quit action, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
printf '%s\n' "$output"
