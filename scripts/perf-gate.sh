#!/bin/sh
# Aggregate T-04 performance samples from a product stderr log and evaluate
# the release gates. Text-only; never reads pixels.
#
# Metric seams (all in microseconds unless noted):
#   PERF-INTERACT       interact_commit_us
#   PERF-REGION         capture_wake_window_submit_us
#   PERF-SCROLL-START   scroll_start_us
#   PERF-FRAME-PREVIEW  frame_preview_us              (max gate too)
#   PERF-EVENT-PREVIEW  event_preview_us              (max gate too)
#   PERF-SCK            capture_request_us
#   PERF-STITCH         scroll_stitch_verify_us
#   PERF-EXPORT-2M/6M/24M  output_encode_us= pixels=WxH (bucketed by pixels)
#   PERF-CLIPBOARD      clipboard_accept_us
#
# usage: perf-gate.sh <log> [--min-samples N]
set -eu

log=${1:?usage: perf-gate.sh <log> [--min-samples N]}
shift
min=30
while [ "$#" -gt 0 ]; do
    case "$1" in
        --min-samples)
            [ "$#" -ge 2 ] || { printf 'missing N\n' "$2" >&2; exit 2; }
            min=$2
            shift 2
            ;;
        *)
            printf 'unknown option: %s\n' "$1" >&2
            exit 2
            ;;
    esac
done
[ -f "$log" ] || { printf 'no such log: %s\n' "$log" >&2; exit 2; }

# Print "label n median p95 max" for one metric's samples on stdin.
stats() {
    label=$1
    vals=$(sort -n)
    n=$(printf '%s\n' "$vals" | grep -c . || true)
    if [ "$n" -eq 0 ]; then
        printf '%s 0 - - -\n' "$label"
        return
    fi
    printf '%s\n' "$vals" | awk -v n="$n" -v label="$label" '
        { vals[NR] = $1 }
        END {
            median = vals[int((n + 1) / 2)]
            rank = int(n * 0.95 + 0.999999)
            if (rank < 1) rank = 1
            if (rank > n) rank = n
            printf "%s %d %d %d %d\n", label, n, median, vals[rank], vals[n]
        }
    '
}

# Print the same for a metric whose samples are positional arguments.
read_stats() {
    label=$1
    shift
    printf '%s\n' "$@" | stats "$label"
}

# Evaluate one p95 gate. Args: label n median p95 max gate_us [max_gate_us]
evaluate() {
    label=$1
    n=$2
    median=$3
    p95=$4
    max=$5
    gate=$6
    max_gate=${7:-}

    if [ "$n" -lt "$min" ]; then
        printf '| %s | %d | %s | %s | %s | %s | Not tested |\n' \
            "$label" "$n" "$median" "$p95" "$max" "$gate"
        return
    fi
    result=Pass
    if [ "$p95" -gt "$gate" ]; then
        result=Fail
    fi
    if [ -n "$max_gate" ] && [ "$max" -gt "$max_gate" ]; then
        result=Fail
    fi
    printf '| %s | %d | %s | %s | %s | %s | %s |\n' \
        "$label" "$n" "$median" "$p95" "$max" "$gate" "$result"
    [ "$result" = Pass ] || failed=1
}

# Evaluate PERF-EXPORT-24M: five consecutive runs, each <= gate_us.
evaluate_runs() {
    label=$1
    gate=$2
    shift 2
    n=$#
    if [ "$n" -lt 5 ]; then
        printf '| %s | %d | - | - | - | %s (5 runs) | Not tested |\n' \
            "$label" "$n" "$gate"
        return
    fi
    result=Pass
    for value in "$@"; do
        if [ "$value" -gt "$gate" ]; then
            result=Fail
        fi
    done
    printf '| %s | %d | - | - | - | %s (5 runs) | %s |\n' \
        "$label" "$n" "$gate" "$result"
    [ "$result" = Pass ] || failed=1
}

extract() {
    grep -o "$1=[0-9]*" "$log" | cut -d= -f2 | tr '\n' ' '
}

interact=$(extract 'interact_commit_us')
region=$(extract 'capture_wake_window_submit_us')
scroll_start=$(extract 'scroll_start_us')
frame_preview=$(extract 'frame_preview_us')
event_preview=$(extract 'event_preview_us')
sck=$(extract 'capture_request_us')
stitch=$(extract 'scroll_stitch_verify_us')
clipboard=$(extract 'clipboard_accept_us')

# Export samples carry their composed pixel size; bucket by the T-04 classes.
export_lines=$(grep 'output_encode_us=' "$log" | sed -n 's/.*output_encode_us=\([0-9]*\) pixels=\([0-9]*\)x\([0-9]*\).*/\1 \2 \3/p')
export_2m=$(printf '%s\n' "$export_lines" | awk '$2 * $3 <= 2000000 { print $1 }' | tr '\n' ' ')
export_6m=$(printf '%s\n' "$export_lines" | awk '$2 * $3 > 2000000 && $2 * $3 <= 6000000 { print $1 }' | tr '\n' ' ')
export_24m=$(printf '%s\n' "$export_lines" | awk '$2 * $3 > 6000000 && $2 * $3 <= 24000000 { print $1 }' | tr '\n' ' ')

printf '| Metric | n | median_us | p95_us | max_us | Gate | Result |\n'
printf '| --- | --- | --- | --- | --- | --- | --- |\n'

failed=0
set -- $(read_stats PERF-INTERACT $interact)
evaluate PERF-INTERACT "$2" "$3" "$4" "$5" 100000
set -- $(read_stats PERF-REGION $region)
evaluate PERF-REGION "$2" "$3" "$4" "$5" 300000
set -- $(read_stats PERF-SCROLL-START $scroll_start)
evaluate PERF-SCROLL-START "$2" "$3" "$4" "$5" 1000000
set -- $(read_stats PERF-FRAME-PREVIEW $frame_preview)
evaluate PERF-FRAME-PREVIEW "$2" "$3" "$4" "$5" 200000 500000
set -- $(read_stats PERF-EVENT-PREVIEW $event_preview)
evaluate PERF-EVENT-PREVIEW "$2" "$3" "$4" "$5" 400000 750000
set -- $(read_stats PERF-SCK $sck)
evaluate PERF-SCK "$2" "$3" "$4" "$5" 150000
set -- $(read_stats PERF-STITCH $stitch)
evaluate PERF-STITCH "$2" "$3" "$4" "$5" 100000
set -- $(read_stats PERF-EXPORT-2M $export_2m)
evaluate PERF-EXPORT-2M "$2" "$3" "$4" "$5" 2000000
set -- $(read_stats PERF-EXPORT-6M $export_6m)
evaluate PERF-EXPORT-6M "$2" "$3" "$4" "$5" 5000000
evaluate_runs PERF-EXPORT-24M 20000000 $export_24m
set -- $(read_stats PERF-CLIPBOARD $clipboard)
evaluate PERF-CLIPBOARD "$2" "$3" "$4" "$5" 500000

if [ "$failed" -ne 0 ]; then
    printf 'perf_gate=FAIL log=%s\n' "$log" >&2
    exit 1
fi
printf 'perf_gate=PASS log=%s\n' "$log"
