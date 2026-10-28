#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
# Never disable the one-shot native completion bound for this experiment.
export SCREENCAPTUREKIT_COMPLETION_TIMEOUT_SECS=30
cargo build --locked
app="$PWD/target/ArcShotScrollPrototype.app"
binary="$app/Contents/MacOS/arcshot-scroll-prototype"
mkdir -p "$app/Contents/MacOS"
if [[ ! -f "$binary" || target/debug/arcshot-scroll-prototype -nt "$binary" || Info.plist -nt "$app/Contents/Info.plist" ]]; then
    cp target/debug/arcshot-scroll-prototype "$binary"
    cp Info.plist "$app/Contents/Info.plist"
    codesign --force --sign - --timestamp=none --identifier dev.arcshot.scroll-prototype "$app"
fi
# Keep textual diagnostics in this terminal. No screen images or video are written.
exec "$binary" "$@"
