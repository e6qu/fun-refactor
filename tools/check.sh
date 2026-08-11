#!/usr/bin/env bash
#
# Everything CI's `check` job runs, in the order it runs it.
#
# CI calls this script instead of listing the commands itself, so there is one
# definition of what passing means. A local run that checks a subset — tests and
# clippy but not formatting, say — reports green for a branch CI will reject, and
# the difference is only discovered after the push.
#
# The browser API is a separate feature set that neither default clippy nor the
# default test run compiles, so it gets its own pass.

set -euo pipefail

cd "$(dirname "$0")/.."

run() {
    printf '\n\033[1m==> %s\033[0m\n' "$*"
    "$@"
}

# The capability matrix advertises what each command supports, and a `✓` there is
# computed from a predicate — it says the command would accept the language, not that
# anything ever ran it. The test run records what it actually drove, and the report below
# fails when a claimed cell was never touched. Folded into the run that happens anyway,
# because measuring it with a second full `cargo test` would double the wall clock.
log="$(mktemp)"
matrix="$(mktemp)"
trap 'rm -f "$log" "$matrix"' EXIT

run cargo fmt --all --check
run cargo clippy --all-targets -- -D warnings
FR_CAPABILITY_LOG="$log" run cargo test --all-targets
run cargo clippy --all-targets --features wasm -- -D warnings
run cargo test --all-targets --features wasm

printf '\n\033[1m==> capability coverage\033[0m\n'
cargo run --quiet --features cli --bin fr -- capabilities --json > "$matrix"
python3 tools/capability-report.py "$matrix" "$log"

printf '\n\033[1mAll checks passed.\033[0m\n'
