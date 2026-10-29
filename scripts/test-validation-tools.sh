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
require 'swift=Apple Swift version'
require 'metal=Apple metal version'
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
mkdir "$temporary/fail-bin"
printf '#!/bin/sh\nexit 1\n' >"$temporary/fail-bin/xcodebuild"
chmod +x "$temporary/fail-bin/xcodebuild"
if PATH="$temporary/fail-bin:$PATH" scripts/record-validation.sh \
    --configuration debug --logs "$log" >/dev/null 2>&1; then
    printf 'validation tools test failed: toolchain command failure was accepted\n' >&2
    exit 1
fi
printf '#!/bin/sh\nexit 0\n' >"$temporary/fail-bin/xcodebuild"
if PATH="$temporary/fail-bin:$PATH" scripts/record-validation.sh \
    --configuration debug --logs "$log" >/dev/null 2>&1; then
    printf 'validation tools test failed: empty toolchain output was accepted\n' >&2
    exit 1
fi
rm "$temporary/fail-bin/xcodebuild"
real_git=$(command -v git)
cat >"$temporary/fail-bin/git" <<EOF
#!/bin/sh
if [ "\$1" = status ]; then
    exit 128
fi
exec $real_git "\$@"
EOF
chmod +x "$temporary/fail-bin/git"
if PATH="$temporary/fail-bin:$PATH" scripts/record-validation.sh \
    --configuration debug --logs "$log" >/dev/null 2>&1; then
    printf 'validation tools test failed: git status failure was accepted as clean\n' >&2
    exit 1
fi
rm "$temporary/fail-bin/git"

mkdir "$temporary/good" "$temporary/complete" "$temporary/missing-sha" "$temporary/placeholder"
cp docs/validation/19-startup.md "$temporary/good/report.md"
scripts/check-validation.sh "$temporary/good" >/dev/null

cat >"$temporary/complete/report.md" <<EOF
# Complete evidence
- 实测实现/脚本 SHA：\`$expected_sha\`
timestamp_utc=2026-10-29T00:00:00Z
build_configuration=release
rustc=rustc 1.99.1
cargo=cargo 1.99.1
xcode=Xcode 27.0
macos_sdk=27.0
swift=Apple Swift version 6.4
metal=Apple metal version 32023.921
machine=Mac14,2
macos=27.2 (26B123)
display=1470x956 scale=2.0
permissions=not requested
fixture=icon-v1
log=target/test.log
| ID | Actual | Result |
| --- | --- | --- |
| TEST-01 | passed | Pass |
EOF
scripts/check-validation.sh "$temporary/complete" >/dev/null

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

mkdir "$temporary/missing-field"
for field in timestamp_utc build_configuration rustc cargo xcode macos_sdk swift metal machine macos display permissions fixture log; do
    grep -v "^$field=" "$temporary/complete/report.md" >"$temporary/missing-field/report.md"
    if scripts/check-validation.sh "$temporary/missing-field" >/dev/null 2>&1; then
        printf 'validation tools test failed: missing field %s was accepted\n' "$field" >&2
        exit 1
    fi
done
grep -v '| Pass |' "$temporary/complete/report.md" >"$temporary/missing-field/report.md"
if scripts/check-validation.sh "$temporary/missing-field" >/dev/null 2>&1; then
    printf 'validation tools test failed: missing test result was accepted\n' >&2
    exit 1
fi

for field in build_configuration rustc cargo xcode macos_sdk swift metal machine macos display permissions fixture log; do
    sed "s|^$field=.*|$field=|" "$temporary/complete/report.md" >"$temporary/missing-field/report.md"
    if scripts/check-validation.sh "$temporary/missing-field" >/dev/null 2>&1; then
        printf 'validation tools test failed: empty field %s was accepted\n' "$field" >&2
        exit 1
    fi
done
sed 's/^build_configuration=release/build_configuration=unsupported/' \
    "$temporary/complete/report.md" >"$temporary/missing-field/report.md"
if scripts/check-validation.sh "$temporary/missing-field" >/dev/null 2>&1; then
    printf 'validation tools test failed: unsupported build configuration was accepted\n' >&2
    exit 1
fi
grep -v -e '^xcode=' -e '^macos_sdk=' -e '^swift=' -e '^metal=' \
    "$temporary/complete/report.md" >"$temporary/missing-field/report.md"
printf 'Xcode SDK Swift Metal\n' >>"$temporary/missing-field/report.md"
if scripts/check-validation.sh "$temporary/missing-field" >/dev/null 2>&1; then
    printf 'validation tools test failed: incidental toolchain prose was accepted\n' >&2
    exit 1
fi

printf 'validation_tools=pass\n'
