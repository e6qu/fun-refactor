#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -lt 4 ] || [ "$#" -gt 5 ]; then
    echo "usage: $0 URL OUTPUT SHA256 SIZE [PARTS]" >&2
    exit 2
fi

url="$1"
output="$2"
expected_sha="$3"
size="$4"
parts="${5:-8}"

case "$size" in
    ''|0|*[!0-9]*) echo "SIZE must be a positive integer" >&2; exit 2 ;;
esac
case "$parts" in
    ''|0|*[!0-9]*) echo "PARTS must be a positive integer" >&2; exit 2 ;;
esac
if [ "$parts" -gt 64 ]; then
    echo "PARTS must be at most 64" >&2
    exit 2
fi
if [[ ! "$expected_sha" =~ ^[0-9a-fA-F]{64}$ ]]; then
    echo "SHA256 must contain 64 hexadecimal characters" >&2
    exit 2
fi

prefix="${output}.part.$$"
assembled="${output}.assembled.$$"
pids=()
cleanup() {
    local pid
    for pid in $(jobs -pr); do
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    done
    rm -f "${prefix}."* "$assembled"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

fetch_part() {
    local part="$1" start="$2" end="$3" expected="$4"
    local attempt actual offset received range curl_pid=""
    trap - EXIT
    trap 'if [ -n "$curl_pid" ]; then kill "$curl_pid" 2>/dev/null || true; wait "$curl_pid" 2>/dev/null || true; fi; exit 130' INT TERM
    : > "${prefix}.${part}"
    for attempt in 1 2 3 4 5; do
        actual=$(wc -c < "${prefix}.${part}")
        if [ "$actual" -eq "$expected" ]; then return 0; fi
        offset=$((start + actual))
        : > "${prefix}.${part}.chunk"
        curl -fsSL --connect-timeout 10 --max-time 45 \
            --header 'Accept-Encoding: identity' --range "${offset}-${end}" \
            --max-filesize "$((expected - actual))" \
            --dump-header "${prefix}.${part}.headers" --write-out '%{http_code}' \
            -o "${prefix}.${part}.chunk" "$url" > "${prefix}.${part}.status" &
        curl_pid=$!
        wait "$curl_pid" || true
        curl_pid=""
        range=$(awk 'tolower($1) == "content-range:" {sub(/^[^:]*: */, ""); sub(/\r$/, ""); value=$0} END {print value}' "${prefix}.${part}.headers")
        received=$(wc -c < "${prefix}.${part}.chunk")
        if [ "$(cat "${prefix}.${part}.status")" = 206 ] && \
           [ "$range" = "bytes ${offset}-${end}/${size}" ] && \
           [ "$received" -le "$((expected - actual))" ]; then
            cat "${prefix}.${part}.chunk" >> "${prefix}.${part}"
        fi
        actual=$(wc -c < "${prefix}.${part}")
        if [ "$actual" -eq "$expected" ]; then return 0; fi
        if [ "$attempt" -lt 5 ]; then sleep 2; fi
    done
    echo "range ${start}-${end} incomplete after bounded resume attempts" >&2
    return 1
}

if [ "$parts" -gt "$size" ]; then parts="$size"; fi
for ((part = 0; part < parts; part++)); do
    start=$((size * part / parts))
    end=$((size * (part + 1) / parts - 1))
    expected=$((end - start + 1))
    fetch_part "$part" "$start" "$end" "$expected" &
    pids+=("$!")
done

failed=0
for pid in "${pids[@]}"; do
    if ! wait "$pid"; then
        failed=1
    fi
done
if [ "$failed" -ne 0 ]; then
    echo "could not fetch $url in verified ranges" >&2
    exit 1
fi

: > "$assembled"
for ((part = 0; part < parts; part++)); do
    cat "${prefix}.${part}" >> "$assembled"
done

if command -v sha256sum >/dev/null 2>&1; then
    actual_sha=$(sha256sum "$assembled" | cut -d ' ' -f 1)
else
    actual_sha=$(shasum -a 256 "$assembled" | cut -d ' ' -f 1)
fi
actual_sha=$(printf '%s' "$actual_sha" | tr '[:upper:]' '[:lower:]')
expected_sha=$(printf '%s' "$expected_sha" | tr '[:upper:]' '[:lower:]')
if [ "$actual_sha" != "$expected_sha" ]; then
    echo "SHA-256 mismatch for $url" >&2
    exit 1
fi

mv "$assembled" "$output"
