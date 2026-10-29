#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

log="target/validation-tools-test.log"
mkdir -p target
printf 'validation tool test log\n' >"$log"
output=$(scripts/record-validation.sh --configuration debug --logs "$log")
expected_sha=$(git rev-parse HEAD)

require() {
    case "$output" in
        *"$1"*) ;;
        *)
            printf 'validation tools test failed: missing %s\noutput:\n%s\n' "$1" "$output" >&2
            exit 1
            ;;
    esac
}

require "commit_sha=$expected_sha"
require 'worktree_dirty='
require 'timestamp_utc='
require 'build_configuration=debug'
require 'rustc=rustc '
require 'cargo=cargo '
require 'xcode=Xcode '
require 'macos_sdk='
require 'swift='
require 'metal='
require "log=$log"

if scripts/record-validation.sh --configuration unsupported --logs "$log" >/dev/null 2>&1; then
    printf 'validation tools test failed: invalid configuration was accepted\n' >&2
    exit 1
fi

temporary=$(mktemp -d "${TMPDIR:-/tmp}/arcshot-validation.XXXXXX")
cleanup() {
    rm -rf "$temporary"
    rm -f "$log"
}
trap cleanup EXIT HUP INT TERM
mkdir "$temporary/good" "$temporary/missing-sha" "$temporary/placeholder"
cp docs/validation/19-startup.md "$temporary/good/report.md"
scripts/check-validation.sh "$temporary/good" >/dev/null

cat >"$temporary/missing-sha/report.md" <<'EOF'
# Invalid evidence
| ID | Actual | Result |
| --- | --- | --- |
| TEST-01 | ran | Pass |
EOF
if scripts/check-validation.sh "$temporary/missing-sha" >/dev/null 2>&1; then
    printf 'validation tools test failed: missing SHA was accepted\n' >&2
    exit 1
fi

cat >"$temporary/placeholder/report.md" <<'EOF'
# Invalid evidence
- 实测实现/脚本 SHA：`0123456789abcdef0123456789abcdef01234567`
- Result: TBD
EOF
if scripts/check-validation.sh "$temporary/placeholder" >/dev/null 2>&1; then
    printf 'validation tools test failed: placeholder was accepted\n' >&2
    exit 1
fi

printf 'validation_tools=pass\n'
