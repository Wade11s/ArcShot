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
codesign --force --sign - --timestamp=none "$app_dir"
printf 'app_bundle=%s\n' "$app_dir"
