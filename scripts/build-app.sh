#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

configuration=debug
if [ "$#" -eq 1 ] && [ "$1" = "--release" ]; then
    configuration=release
    cargo build --locked --release -p arcshot-app
elif [ "$#" -ne 0 ]; then
    printf 'usage: %s [--release]\n' "$0" >&2
    exit 2
else
    cargo build --locked -p arcshot-app
fi

app_dir="target/bundle/$configuration/ArcShot.app"
rm -rf "$app_dir"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp "target/$configuration/arcshot" "$app_dir/Contents/MacOS/arcshot"
cp packaging/Info.plist "$app_dir/Contents/Info.plist"
if [ "$configuration" = debug ]; then
    plutil -replace CFBundleIdentifier -string dev.arcshot.app.debug "$app_dir/Contents/Info.plist"
    plutil -replace CFBundleName -string 'ArcShot Debug' "$app_dir/Contents/Info.plist"
    plutil -insert CFBundleDisplayName -string 'ArcShot Debug' "$app_dir/Contents/Info.plist"
fi
plutil -lint "$app_dir/Contents/Info.plist"
# The stable "ArcShot Dev" identity keeps the designated requirement (and the
# TCC Screen Recording grant) valid across rebuilds; ad-hoc is cdhash-bound and
# invalidates the grant on every build. Run scripts/create-dev-signing-identity.sh
# once to create it; without it this falls back to ad-hoc.
signing_identity='ArcShot Dev'
if security find-identity -v -p codesigning 2>/dev/null | grep -q "\"$signing_identity\""; then
    codesign --force --sign "$signing_identity" --timestamp=none "$app_dir"
    printf 'signing=identity name=%s\n' "$signing_identity"
else
    codesign --force --sign - --timestamp=none "$app_dir"
    printf 'signing=adhoc\n'
fi
printf 'app_bundle=%s\n' "$app_dir"
