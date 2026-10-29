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

# Split once, then reuse the same positional list for validation and emission.
old_ifs=$IFS
IFS=,
set -f
set -- $logs
set +f
IFS=$old_ifs
[ "$#" -gt 0 ] || usage
for log in "$@"; do
    if [ ! -f "$log" ]; then
        printf 'validation recording failed: log does not exist: %s\n' "$log" >&2
        exit 1
    fi
done

capture() {
    if ! captured=$("$@" 2>&1); then
        printf 'validation recording failed: command failed: %s\n%s\n' "$1" "$captured" >&2
        return 1
    fi
    if [ -z "$captured" ]; then
        printf 'validation recording failed: empty command output: %s\n' "$1" >&2
        return 1
    fi
}

require_nonempty() {
    if [ -z "$2" ]; then
        printf 'validation recording failed: missing %s version\n' "$1" >&2
        exit 1
    fi
}

commit_sha=$(git rev-parse --verify HEAD)
if ! status_output=$(git status --porcelain 2>&1); then
    printf 'validation recording failed: git status failed:\n%s\n' "$status_output" >&2
    exit 1
fi
worktree_dirty=false
if [ -n "$status_output" ]; then
    worktree_dirty=true
fi
timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)

capture rustc --version
rustc_version=$captured
capture cargo --version
cargo_version=$captured
capture xcodebuild -version
xcode_output=$captured
xcode_version=$(printf '%s\n' "$xcode_output" | awk 'NR == 1 { print; exit }')
xcode_build=$(printf '%s\n' "$xcode_output" | awk 'NR == 2 { print; exit }')
require_nonempty Xcode "$xcode_version"
require_nonempty 'Xcode build' "$xcode_build"
capture xcrun --sdk macosx --show-sdk-version
sdk_version=$captured
capture xcrun --sdk macosx --show-sdk-build-version
sdk_build=$captured
capture xcrun swift --version
swift_output=$captured
swift_version=$(printf '%s\n' "$swift_output" | awk '{
    start = index($0, "Apple Swift version")
    if (start > 0) {
        print substr($0, start)
        exit
    }
}')
require_nonempty Swift "$swift_version"
capture xcrun -sdk macosx metal --version
metal_output=$captured
metal_version=$(printf '%s\n' "$metal_output" | awk 'NR == 1 { print; exit }')
require_nonempty Metal "$metal_version"
case "$metal_version" in
    *'metal version'*) ;;
    *)
        printf 'validation recording failed: unrecognized Metal version: %s\n' "$metal_version" >&2
        exit 1
        ;;
esac

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
for log in "$@"; do
    printf 'log=%s\n' "$log"
done
