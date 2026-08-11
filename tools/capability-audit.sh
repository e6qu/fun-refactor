#!/usr/bin/env bash
#
# The capability coverage figure on its own, without the rest of `check.sh`.
#
# `check.sh` measures this on the test run it already does, so nothing here is needed to
# defend the number — this is for asking the question directly while working on it, and for
# seeing the report when the suite is otherwise green.
#
# The reporting itself lives in `capability-report.py`, used by both, so the two cannot
# start disagreeing about what counts as covered.

set -euo pipefail

cd "$(dirname "$0")/.."

log="$(mktemp)"
matrix="$(mktemp)"
trap 'rm -f "$log" "$matrix"' EXIT

printf '\033[1m==> running the suite with capability recording on\033[0m\n'
FR_CAPABILITY_LOG="$log" cargo test --all-targets >/dev/null

cargo run --quiet --features cli --bin fr -- capabilities --json > "$matrix"
python3 tools/capability-report.py "$matrix" "$log"
