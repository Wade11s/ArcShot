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

require_pattern() {
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
    require_pattern '实测实现/脚本 SHA：`[0-9a-fA-F]{40}`' 'a full tested implementation SHA'
    require_pattern '[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z' 'a UTC timestamp'
    require_pattern '构建配置：|build_configuration=' 'a build configuration'
    require_pattern 'Rust/Cargo：|rustc=' 'a Rust version'
    require_pattern 'Rust/Cargo：|cargo=' 'a Cargo version'
    require_pattern 'Xcode|xcode=' 'an Xcode version'
    require_pattern 'SDK|macos_sdk=' 'an SDK version'
    require_pattern 'Swift|swift=' 'a Swift version'
    require_pattern 'Metal|metal=' 'a Metal toolchain version'
    require_pattern '机器：|machine=' 'a machine identifier'
    require_pattern 'macOS|macos=' 'a macOS build'
    require_pattern '显示器|display=' 'display geometry and scale'
    require_pattern '权限：|permissions=' 'permission preconditions'
    require_pattern '夹具：|fixture=' 'a fixture version'
    require_pattern '日志|log=' 'log paths'
    require_pattern '\| (Pass|Fail|Not tested) \|' 'a stable test result'
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
