#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

output=$(cargo run --locked --quiet -p arcshot-app -- --check-text-backend)
case "$output" in
    *'text_backend=pass'*) ;;
    *)
        printf 'text backend check failed: expected real font metrics, got: %s\n' "$output" >&2
        exit 1
        ;;
esac
printf '%s\n' "$output"
