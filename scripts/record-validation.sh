#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

usage() {
    printf 'usage: %s --configuration debug|release --logs path[,path...]\n' "$0" >&2
    exit 2
}

configuration=
logs=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --configuration)
            [ "$#" -ge 2 ] || usage
            configuration=$2
            shift 2
            ;;
        --logs)
            [ "$#" -ge 2 ] || usage
            logs=$2
            shift 2
            ;;
        *)
            usage
            ;;
    esac
done

case "$configuration" in
    debug|release) ;;
    *) usage ;;
esac
[ -n "$logs" ] || usage

for command_name in git date rustc cargo xcodebuild xcrun sw_vers sysctl uname awk; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
        printf 'validation recording failed: required command not found: %s\n' "$command_name" >&2
        exit 1
    fi
done

old_ifs=$IFS
IFS=,
set -f
for log in $logs; do
    if [ ! -f "$log" ]; then
        printf 'validation recording failed: log does not exist: %s\n' "$log" >&2
        exit 1
    fi
done
set +f
IFS=$old_ifs

commit_sha=$(git rev-parse --verify HEAD)
worktree_dirty=false
if [ -n "$(git status --porcelain)" ]; then
    worktree_dirty=true
fi
timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)
rustc_version=$(rustc --version)
cargo_version=$(cargo --version)
xcode_version=$(xcodebuild -version | awk 'NR == 1 { print; exit }')
xcode_build=$(xcodebuild -version | awk 'NR == 2 { print; exit }')
sdk_version=$(xcrun --sdk macosx --show-sdk-version)
sdk_build=$(xcrun --sdk macosx --show-sdk-build-version)
swift_version=$(xcrun swift --version 2>&1 | awk '/Apple Swift version/ { print; exit }')
metal_version=$(xcrun -sdk macosx metal --version 2>&1 | awk 'NR == 1 { print; exit }')
machine=$(sysctl -n hw.model)
macos_version=$(sw_vers -productVersion)
macos_build=$(sw_vers -buildVersion)
architecture=$(uname -m)

printf 'commit_sha=%s\n' "$commit_sha"
printf 'worktree_dirty=%s\n' "$worktree_dirty"
printf 'timestamp_utc=%s\n' "$timestamp_utc"
printf 'build_configuration=%s\n' "$configuration"
printf 'machine=%s\n' "$machine"
printf 'architecture=%s\n' "$architecture"
printf 'macos=%s (%s)\n' "$macos_version" "$macos_build"
printf 'rustc=%s\n' "$rustc_version"
printf 'cargo=%s\n' "$cargo_version"
printf 'xcode=%s (%s)\n' "$xcode_version" "$xcode_build"
printf 'macos_sdk=%s (%s)\n' "$sdk_version" "$sdk_build"
printf 'swift=%s\n' "$swift_version"
printf 'metal=%s\n' "$metal_version"

old_ifs=$IFS
IFS=,
set -f
for log in $logs; do
    printf 'log=%s\n' "$log"
done
set +f
IFS=$old_ifs
