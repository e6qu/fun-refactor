#!/usr/bin/env bash
#
# Everything CI's check jobs run, and the one definition of what passing means.
#
# CI calls this script instead of listing the commands itself. A local run that
# checks a subset — tests and clippy but not formatting, say — reports green for
# a branch CI will reject, and the difference is only discovered after the push.
#
# The browser API is a separate feature set that neither default clippy nor the
# default test run compiles, so it gets its own pass. CI runs `check.sh default`
# and `check.sh wasm` as parallel jobs. With no argument the script runs the PR
# gate; `check.sh deep` runs the exhaustive self-audits after merge and nightly.
#
# `check-prose.py` counts the writing habits listed in `docs/style.md` against the
# numbers in `tools/PROSE-DEBT`. It fails when a count rises, and when a count falls
# without the number being lowered.

set -euo pipefail

cd "$(dirname "$0")/.."

slice="${1:-all}"
case "$slice" in
    all|default|wasm|deep) ;;
    *)
        echo "unknown slice: $slice (default, wasm, deep, or no argument for the PR gate)" >&2
        exit 1
        ;;
esac

run() {
    printf '\n\033[1m==> %s\033[0m\n' "$*"
    "$@"
}

# Zig defaults to a cache in the account home, which may be read-only in a
# sandboxed checkout. Keeping the compiler cache under `target` makes the test
# command self-contained and lets callers still override it when they need to.
zig_cache="${ZIG_GLOBAL_CACHE_DIR:-$PWD/target/zig-cache}"
mkdir -p "$zig_cache"

if [ "$slice" = all ] || [ "$slice" = default ]; then
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
    ZIG_GLOBAL_CACHE_DIR="$zig_cache" FR_CAPABILITY_LOG="$log" run cargo test --all-targets

    printf '\n\033[1m==> writing\033[0m\n'
    python3 tools/check-prose.py

    printf '\n\033[1m==> capability coverage\033[0m\n'
    cargo run --quiet --features cli --bin fr -- capabilities --json > "$matrix"
    python3 tools/capability-report.py "$matrix" "$log"

    printf '\n\033[1m==> Lean kernels\033[0m\n'
    bash tools/check-kernels.sh
fi

if [ "$slice" = deep ]; then
    # These test crates index or translate the repository as a whole. Together they
    # dominated PR latency, but each is still run against every merged revision and
    # on the nightly audit. The feature removes them from the interactive test pass
    # without weakening the complete post-merge validation.
    ZIG_GLOBAL_CACHE_DIR="$zig_cache" run cargo test --features full-audit \
        --test commands_agree \
        --test conformance \
        --test round_trip \
        --test self_translation
    run cargo test --test lean_kernels -- --include-ignored
fi

if [ "$slice" = all ] || [ "$slice" = wasm ]; then
    # `wasm_native` exercises the exported browser API. Running all integration
    # tests with this feature repeated the default suite while adding no wasm
    # coverage, so this lane compiles the library and runs its native API tests.
    run cargo clippy --lib --features wasm -- -D warnings
    run cargo clippy --test wasm_native --features wasm -- -D warnings
    run cargo test --lib --features wasm
    run cargo test --test wasm_native --test wasm_api --features wasm
    # The browser build compiles without the cli feature. With defaults on, an
    # import only the CLI uses looks used, and the deploy is where the unused
    # warning finally fails. Same host target, so no wasm clang is needed.
    run cargo clippy --lib --no-default-features --features wasm,lang-all -- -D warnings
fi

printf '\n\033[1mAll checks passed.\033[0m\n'
