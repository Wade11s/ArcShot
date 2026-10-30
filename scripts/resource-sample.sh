#!/bin/sh
# Poll resident memory footprint of a running ArcShot process while the
# user performs manual capture steps. Text output only; no screen pixels.
# usage: resource-sample.sh <pid> [interval_secs] [count]
set -eu

pid=${1:?usage: resource-sample.sh <pid> [interval_secs] [count]}
interval=${2:-1}
count=${3:-30}

ps -p "$pid" -o pid= >/dev/null 2>&1 || { printf 'no such pid: %s\n' "$pid" >&2; exit 2; }

peak_rss=0
i=0
while [ "$i" -le "$count" ]; do
    rss=$(ps -p "$pid" -o rss= | tr -d ' ')
    [ "$rss" -gt "$peak_rss" ] && peak_rss=$rss
    printf 'sample=%d rss_kb=%s\n' "$i" "$rss"
    [ "$i" -eq "$count" ] && break
    sleep "$interval"
    i=$((i + 1))
done
printf 'peak_rss_kb=%d samples=%d interval_s=%s\n' "$peak_rss" "$((count + 1))" "$interval"
