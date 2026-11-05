#!/bin/sh
# Sample one running ArcShot process for the T-04 resource gates.
#
# RSS is polled with `ps` at a fixed cadence (<= 100 ms by default). The
# physical footprint is cross-checked with the macOS `footprint` tool as fast
# as that tool can run; its own duration is reported so the sampling interval
# is never overstated. Each footprint query also reads the kernel's
# phys_footprint_peak high-water mark, and the full category table is emitted
# once plus at each 1 GiB threshold crossing for the native/GPU breakdown.
# Text-only; no pixels. All maxima are observed peaks, not a proof about
# every instantaneous spike.
#
# usage: resource-footprint.sh <pid> [rss_interval_secs] [duration_secs]
set -eu

pid=${1:?usage: resource-footprint.sh <pid> [rss_interval_secs] [duration_secs]}
rss_interval=${2:-0.1}
duration=${3:-30}

ps -p "$pid" -o pid= >/dev/null 2>&1 || { printf 'no such pid: %s\n' "$pid" >&2; exit 2; }

# Convert a "value KB|MB|GB" pair to integer KiB. Empty input yields nothing.
to_kb() {
    printf '%s %s\n' "$1" "$2" | awk '
        $2 == "KB" { printf "%d\n", $1 }
        $2 == "MB" { printf "%d\n", $1 * 1024 }
        $2 == "GB" { printf "%d\n", $1 * 1024 * 1024 }
    '
}
# Extract "value unit" from one `footprint` output line pattern.
extract_pair() {
    printf '%s\n' "$1" | sed -n "s/.*$2: \\([0-9.]*\\) \\([KMG]B\\).*/\\1 \\2/p" | head -1
}

peak_rss=0
peak_footprint=0
peak_phys_footprint_kb=0
samples=0
footprint_samples=0
footprint_seconds=0
detail_level=0
start=$(date +%s)
index=0

summary() {
    now=$(date +%s)
    printf 'peak_rss_kb=%d peak_footprint_kb=%d peak_phys_footprint_kb=%d rss_samples=%d footprint_samples=%d footprint_total_s=%s elapsed_s=%d duration_s=%s rss_interval_s=%s observed_mean_interval_s=%s\n' \
        "$peak_rss" "$peak_footprint" "$peak_phys_footprint_kb" "$samples" "$footprint_samples" "$footprint_seconds" "$((now - start))" "$duration" "$rss_interval" \
        "$(awk -v e="$((now - start))" -v n="$samples" 'BEGIN { if (n == 0) print "-"; else printf "%.4f", e / n }')"
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
    phys_footprint_peak_kb=
    # `footprint` is the cross-check, not the 100 ms RSS cadence: it is far
    # slower than `ps`, so it runs once every ~1 s while RSS stays at 100 ms.
    if [ $((samples % 10)) -eq 0 ]; then
        started=$(date +%s%N)
        detail=$(footprint -p "$pid" 2>/dev/null || true)
        ended=$(date +%s%N)
        pair=$(extract_pair "$detail" 'Footprint')
        if [ -n "$pair" ]; then
            footprint_kb=$(to_kb $(printf '%s' "$pair" | awk '{print $1}') $(printf '%s' "$pair" | awk '{print $2}'))
        fi
        peak_pair=$(extract_pair "$detail" 'phys_footprint_peak')
        if [ -n "$peak_pair" ]; then
            phys_footprint_peak_kb=$(to_kb $(printf '%s' "$peak_pair" | awk '{print $1}') $(printf '%s' "$peak_pair" | awk '{print $2}'))
        fi
        if [ -n "$footprint_kb" ]; then
            [ "$footprint_kb" -gt "$peak_footprint" ] && peak_footprint=$footprint_kb
            footprint_samples=$((footprint_samples + 1))
            footprint_seconds=$(awk -v a="$footprint_seconds" -v b="$((ended - started))" 'BEGIN { printf "%.3f", a + b / 1e9 }')
            # Full category table once, and again at each 1 GiB threshold
            # crossing, so the peak's native/GPU breakdown is recorded.
            level=$((footprint_kb / 1048576))
            if [ "$detail_level" -eq 0 ] || [ "$level" -gt "$detail_level" ]; then
                printf '%s\n' "$detail" | while IFS= read -r row; do
                    printf 'footprint_detail_at_sample=%d line=%s\n' "$index" "$row"
                done
                detail_level=$level
            fi
        fi
        if [ -n "$phys_footprint_peak_kb" ] && [ "$phys_footprint_peak_kb" -gt "$peak_phys_footprint_kb" ]; then
            peak_phys_footprint_kb=$phys_footprint_peak_kb
        fi
    fi
    printf 'sample=%d rss_kb=%s footprint_kb=%s phys_footprint_peak_kb=%s\n' \
        "$index" "$rss" "${footprint_kb:--}" "${phys_footprint_peak_kb:--}"
    samples=$((samples + 1))
    index=$((index + 1))
    sleep "$rss_interval"
done
summary
