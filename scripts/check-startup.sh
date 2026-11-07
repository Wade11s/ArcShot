#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

configuration=debug
expected_identifier=dev.arcshot.app.debug
expected_name='ArcShot Debug'
if [ "$#" -eq 1 ] && [ "$1" = "--release" ]; then
    configuration=release
    expected_identifier=dev.arcshot.app
    expected_name=ArcShot
elif [ "$#" -ne 0 ]; then
    printf 'usage: %s [--release]\n' "$0" >&2
    exit 2
fi

build_output=$(scripts/build-app.sh ${1:-})
printf '%s\n' "$build_output"
app_dir="target/bundle/$configuration/ArcShot.app"
actual_identifier=$(plutil -extract CFBundleIdentifier raw "$app_dir/Contents/Info.plist")
actual_name=$(plutil -extract CFBundleName raw "$app_dir/Contents/Info.plist")
if [ "$actual_identifier" != "$expected_identifier" ] || [ "$actual_name" != "$expected_name" ]; then
    printf 'startup check failed: bundle identity mismatch: %s / %s\n' "$actual_identifier" "$actual_name" >&2
    exit 1
fi
printf 'bundle_identity_smoke=pass configuration=%s identifier=%s name=%s\n' "$configuration" "$actual_identifier" "$actual_name"
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
case "$output" in
    *'clipboard_pngf_smoke=pass'*) ;;
    *)
        printf 'startup check failed: expected private-pasteboard PNGf roundtrip, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
case "$output" in
    *'save_file_smoke=pass'*) ;;
    *)
        printf 'startup check failed: expected a verified explicit-save file roundtrip, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
case "$output" in
    *'capture_menu_smoke=pass queued_commands=6'*) ;;
    *)
        printf 'startup check failed: expected six dispatched status-menu commands, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
case "$output" in
    *'menu_items_smoke=pass count=7'*) ;;
    *)
        printf 'startup check failed: expected a seven-item status menu, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
case "$output" in
    *'preference_items_smoke=pass'*'density_selector=toggleDensity:'*'state_from_system=true'*) ;;
    *)
        printf 'startup check failed: expected all three preference menu items in the real menu, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
case "$output" in
    *'density_smoke=pass'*'restored=true'*) ;;
    *)
        printf 'startup check failed: expected a restored option-density preference roundtrip, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
case "$output" in
    *'login_item_status_smoke=pass'*'read_only=true'*) ;;
    *)
        printf 'startup check failed: expected a read-only SMAppService status, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
case "$output" in
    *'preference_smoke=pass'*'restored=true'*) ;;
    *)
        printf 'startup check failed: expected a restored completion-sound preference roundtrip, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
printf '%s\n' "$output"
