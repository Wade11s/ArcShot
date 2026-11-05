#!/bin/sh
# Sample one running ArcShot process for the T-04 resource gates.
#
# RSS is polled with `ps` at a fixed cadence (<= 100 ms by default). The
# physical footprint is cross-checked with the macOS `footprint` tool as fast
# as that tool can run; its own duration is reported so the sampling interval
# is never overstated. Text-only; no pixels.
#
# usage: resource-footprint.sh <pid> [rss_interval_secs] [duration_secs]
set -eu

pid=${1:?usage: resource-footprint.sh <pid> [rss_interval_secs] [duration_secs]}
rss_interval=${2:-0.1}
duration=${3:-30}

ps -p "$pid" -o pid= >/dev/null 2>&1 || { printf 'no such pid: %s\n' "$pid" >&2; exit 2; }

peak_rss=0
peak_footprint=0
samples=0
footprint_samples=0
footprint_seconds=0
start=$(date +%s)
index=0

summary() {
    printf 'peak_rss_kb=%d peak_footprint_kb=%d rss_samples=%d footprint_samples=%d footprint_total_s=%s duration_s=%s\n' \
        "$peak_rss" "$peak_footprint" "$samples" "$footprint_samples" "$footprint_seconds" "$duration"
}
# A measurement driver terminates the sampler when its work is done; the
# summary must still be printed so the observed peak is never lost.
trap 'summary; exit 0' TERM INT HUP

while :; do
    now=$(date +%s)
    [ $((now - start)) -lt "$duration" ] || break
    rss=$(ps -p "$pid" -o rss= | tr -d ' ')
    [ -n "$rss" ] || break
    [ "$rss" -gt "$peak_rss" ] && peak_rss=$rss
    footprint_kb=
    # `footprint` is the cross-check, not the 100 ms RSS cadence: it is far
    # slower than `ps`, so it runs once every ~1 s while RSS stays at 100 ms.
    if [ $((samples % 10)) -eq 0 ]; then
        started=$(date +%s%N)
        line=$(footprint -p "$pid" 2>/dev/null | sed -n 's/.*Footprint: \([0-9.]*\) \([KMG]B\).*/\1 \2/p' | head -1)
        ended=$(date +%s%N)
        if [ -n "$line" ]; then
            value=$(printf '%s' "$line" | awk '{print $1}')
            unit=$(printf '%s' "$line" | awk '{print $2}')
            footprint_kb=$(printf '%s %s' "$value" "$unit" | awk '
                $2 == "KB" { print $1 }
                $2 == "MB" { printf "%d\n", $1 * 1024 }
                $2 == "GB" { printf "%d\n", $1 * 1024 * 1024 }
            ')
            if [ -n "$footprint_kb" ] && [ "$footprint_kb" -gt "$peak_footprint" ]; then
                peak_footprint=$footprint_kb
            fi
            footprint_samples=$((footprint_samples + 1))
            footprint_seconds=$(awk -v a="$footprint_seconds" -v b="$((ended - started))" 'BEGIN { printf "%.3f", a + b / 1e9 }')
        fi
    fi
    printf 'sample=%d rss_kb=%s footprint_kb=%s\n' "$index" "$rss" "${footprint_kb:--}"
    samples=$((samples + 1))
    index=$((index + 1))
    sleep "$rss_interval"
done
summary
