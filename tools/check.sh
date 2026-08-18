#!/usr/bin/env bash
#
# Everything CI's check jobs run, and the one definition of what passing means.
#
# CI calls this script instead of listing the commands itself. A local run that
# checks a subset — tests and clippy but not formatting, say — reports green for
# a branch CI will reject, and the difference is only discovered after the push.
#
# The browser API is a separate feature set that neither default clippy nor the
# default test run compiles, so it gets its own pass. The two passes share no
# compilation and were nearly all of the gate's wall clock in sequence, so CI
# runs `check.sh default` and `check.sh wasm` as parallel jobs. With no argument
# the script runs both, which is the full gate for a laptop before a push.
#
# `check-prose.py` counts the writing habits listed in `docs/style.md` against the
# numbers in `tools/PROSE-DEBT`. It fails when a count rises, and when a count falls
# without the number being lowered.

set -euo pipefail

cd "$(dirname "$0")/.."

slice="${1:-all}"
case "$slice" in
    all|default|wasm) ;;
    *)
        echo "unknown slice: $slice (default, wasm, or no argument for both)" >&2
        exit 1
        ;;
esac

run() {
    printf '\n\033[1m==> %s\033[0m\n' "$*"
    "$@"
}

if [ "$slice" != wasm ]; then
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

    printf '\n\033[1m==> writing\033[0m\n'
    python3 tools/check-prose.py

    printf '\n\033[1m==> capability coverage\033[0m\n'
    cargo run --quiet --features cli --bin fr -- capabilities --json > "$matrix"
    python3 tools/capability-report.py "$matrix" "$log"
fi

if [ "$slice" != default ]; then
    run cargo clippy --all-targets --features wasm -- -D warnings
    run cargo test --all-targets --features wasm
    # The browser build compiles without the cli feature. With defaults on, an
    # import only the CLI uses looks used, and the deploy is where the unused
    # warning finally fails. Same host target, so no wasm clang is needed.
    run cargo clippy --lib --no-default-features --features wasm,lang-all -- -D warnings
fi

printf '\n\033[1mAll checks passed.\033[0m\n'
