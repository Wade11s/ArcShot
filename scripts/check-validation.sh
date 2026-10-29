#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

if [ "$#" -gt 1 ]; then
    printf 'usage: %s [validation-directory]\n' "$0" >&2
    exit 2
fi
directory=${1:-docs/validation}
if [ ! -d "$directory" ]; then
    printf 'validation check failed: directory not found: %s\n' "$directory" >&2
    exit 1
fi

nonempty='[[:space:]]*[^[:space:]|]+'
require_value() {
    if ! grep -Eq "$1" "$report"; then
        printf 'validation check failed: %s lacks %s\n' "$report" "$2" >&2
        status=1
    fi
}

found=false
status=0
for report in "$directory"/*.md; do
    [ -f "$report" ] || continue
    found=true
    require_value '实测实现/脚本 SHA：`[0-9a-fA-F]{40}`' 'a full tested implementation SHA'
    require_value '[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z' 'a UTC timestamp'
    require_value "(构建配置：|build_configuration=)$nonempty" 'a nonempty build configuration'
    require_value "(Rust/Cargo：|rustc=)$nonempty" 'a nonempty Rust version'
    require_value "(Rust/Cargo：|cargo=)$nonempty" 'a nonempty Cargo version'
    require_value "(Xcode：|xcode=)$nonempty" 'a nonempty Xcode version'
    require_value "(SDK[： ]|macos_sdk=)$nonempty" 'a nonempty SDK version'
    require_value "(Swift：|swift=)$nonempty" 'a nonempty Swift version'
    require_value "(Metal(/AIR-LLD)?：|metal=)$nonempty" 'a nonempty Metal toolchain version'
    require_value "(机器：|machine=)$nonempty" 'a nonempty machine identifier'
    require_value '(macOS[： ]|macos=)[[:space:]]*[0-9]' 'a nonempty macOS build'
    require_value "(显示器：|display=)$nonempty" 'nonempty display geometry and scale'
    require_value "(权限：|permissions=)$nonempty" 'nonempty permission preconditions'
    require_value "(夹具：|fixture=)$nonempty" 'a nonempty fixture version'
    require_value "(日志：|log=)$nonempty" 'nonempty log paths'
    require_value '\| (Pass|Fail|Not tested) \|' 'a stable test result'

    if grep -q 'build_configuration=' "$report" \
        && ! grep -Eq 'build_configuration=(debug|release)[[:space:]]*$' "$report"; then
        printf 'validation check failed: %s has an unsupported build configuration\n' "$report" >&2
        status=1
    fi
    if grep -q '构建配置：' "$report" \
        && ! grep -Eq '构建配置：(Debug|Release|Debug 与 Release)([[:space:]]|。|$)' "$report"; then
        printf 'validation check failed: %s has an unsupported build configuration\n' "$report" >&2
        status=1
    fi
    if grep -Eq 'TBD|PENDING|待提交|待最终|待填' "$report"; then
        printf 'validation check failed: %s contains an unfinished placeholder\n' "$report" >&2
        status=1
    fi
done

if [ "$found" = false ]; then
    printf 'validation check failed: no Markdown reports in %s\n' "$directory" >&2
    exit 1
fi
if [ "$status" -ne 0 ]; then
    exit "$status"
fi
printf 'validation_reports=pass directory=%s\n' "$directory"
