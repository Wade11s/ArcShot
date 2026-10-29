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

found=false
status=0
for report in "$directory"/*.md; do
    [ -f "$report" ] || continue
    found=true
    if ! grep -Eq '实测实现/脚本 SHA：`[0-9a-fA-F]{40}`' "$report"; then
        printf 'validation check failed: %s lacks a full tested implementation SHA\n' "$report" >&2
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
