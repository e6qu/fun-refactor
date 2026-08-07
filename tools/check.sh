#!/usr/bin/env bash
#
# Everything CI's `check` job runs, in the order it runs it.
#
# CI calls this script rather than listing the commands itself, so there is one
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

run cargo fmt --all --check
run cargo clippy --all-targets -- -D warnings
run cargo test --all-targets
run cargo clippy --all-targets --features wasm -- -D warnings
run cargo test --all-targets --features wasm

printf '\n\033[1mAll checks passed.\033[0m\n'
