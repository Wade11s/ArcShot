#!/bin/sh
# Aggregate capture timing diagnostics from a text log produced by a
# manual capture session. PERF-SCK uses capture_request_us; the window
# submit metric is context only and is NOT first presentation.
# usage: perf-samples.sh <log> [--min-samples N]  (default 30)
set -eu

log=${1:?usage: perf-samples.sh <log> [--min-samples N]}
min=30
[ "${2:-}" = "--min-samples" ] && min=${3:?missing N}

[ -f "$log" ] || { printf 'no such log: %s\n' "$log" >&2; exit 2; }

stats() {
    field=$1
    # shellcheck disable=SC2016
    vals=$(printf '%s\n' "$2" | tr ' ' '\n' | grep -E '^[0-9]+$' | sort -n)
    n=$(printf '%s\n' "$vals" | grep -c .)
    printf '%s samples=%d' "$field" "$n"
    [ "$n" -eq 0 ] && { printf '\n'; return; }
    median=$(printf '%s\n' "$vals" | awk -v n="$n" 'NR==int((n+1)/2){print; exit}')
    p95=$(printf '%s\n' "$vals" | awk -v n="$n" 'NR==int(n*0.95+0.5){print; exit}')
    max=$(printf '%s\n' "$vals" | tail -1)
    printf ' median_us=%s p95_us=%s max_us=%s\n' "$median" "$p95" "$max"
}

req=$(grep -o 'capture_request_us=[0-9]*' "$log" | cut -d= -f2 | tr '\n' ' ')
wake=$(grep -o 'capture_wake_window_submit_us=[0-9]*' "$log" | cut -d= -f2 | tr '\n' ' ')

stats capture_request_us "$req"
stats capture_wake_window_submit_us "$wake"

n=$(printf '%s' "$req" | wc -w | tr -d ' ')
if [ "$n" -lt "$min" ]; then
    printf 'insufficient_samples=%d required=%d\n' "$n" "$min" >&2
    exit 1
fi
