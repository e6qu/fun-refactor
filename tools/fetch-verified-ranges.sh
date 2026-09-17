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
cleanup() {
    rm -f "${prefix}."* "$assembled"
}
trap cleanup EXIT

pids=()
for ((part = 0; part < parts; part++)); do
    start=$((size * part / parts))
    end=$((size * (part + 1) / parts - 1))
    expected=$((end - start + 1))
    (
        curl -fsSL --retry 3 --retry-delay 5 --retry-connrefused \
            --connect-timeout 20 --max-time 180 --header 'Accept-Encoding: identity' \
            --range "${start}-${end}" -o "${prefix}.${part}" "$url"
        actual=$(wc -c < "${prefix}.${part}")
        if [ "$actual" -ne "$expected" ]; then
            echo "range ${start}-${end} returned ${actual} bytes, expected ${expected}" >&2
            exit 1
        fi
    ) &
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
