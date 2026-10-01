#!/usr/bin/env bash
#
# Everything CI's check jobs run, and the one definition of what passing means.
#
# CI calls this script to share one gate definition with developer checks.
#
# The browser API is a separate feature set that neither default clippy nor the
# default test run compiles, so it gets its own pass. CI distributes the native
# tests through ci-shards.py and runs the static and wasm slices alongside them.
# With no argument the script runs the complete PR gate; `check.sh deep` runs the exhaustive self-audits after merge and nightly.
#
# `check-prose.py` counts the writing habits listed in `docs/style.md` against the
# numbers in `tools/PROSE-DEBT`. It fails when a count rises, and when a count falls
# without the number being lowered.

set -euo pipefail

cd "$(dirname "$0")/.."

slice="${1:-all}"
case "$slice" in
    all|default|static|wasm|deep|study) ;;
    *)
        echo "unknown slice: $slice (default, static, wasm, deep, study, or no argument for the PR gate)" >&2
        exit 1
        ;;
esac

if [ "$slice" = study ]; then
    python3 tools/agent_eval/test_study.py
    python3 tools/agent_eval/test_host.py
    python3 tools/agent_eval/test_gateway.py
    python3 tools/agent_eval/test_isolated_grade.py
    python3 tools/agent_eval/test_study_runner.py
    python3 tools/agent_eval/test_source_disclosure.py
    python3 tools/agent_eval/test_container_resources.py
    python3 tools/agent_eval/test_opencode_rehearsal.py
    python3 tools/agent_eval/test_rehearsal_evidence.py
    python3 tools/agent_eval/test_opencode_export.py
    python3 tools/agent_eval/test_external_environment.py
    python3 tools/test_fetch_verified_ranges.py
    python3 tools/agent_eval/test_codex_runner.py
    python3 tests/agent-eval/test_python_repository_basis.py
    python3 tools/roadmap-status.py --check
    python3 tools/check-prose.py
    exit 0
fi

if [ "$slice" = all ] || [ "$slice" = default ]; then
    bash tools/check.sh study
fi

if [ "$slice" != wasm ]; then
    source tools/lean-resources.sh
    fr_configure_lean_resources
fi

run() {
    printf '\n\033[1m==> %s\033[0m\n' "$*"
    "$@"
}

# Compiler caches can default to account directories outside a sandboxed checkout.
# Keep them under `target` unless the caller supplies a cache path.
zig_cache="${ZIG_GLOBAL_CACHE_DIR:-$PWD/target/zig-cache}"
go_cache="${GOCACHE:-$PWD/target/go-cache}"
mkdir -p "$zig_cache" "$go_cache"
export GOCACHE="$go_cache"

if [ "$slice" = all ] || [ "$slice" = default ] || [ "$slice" = static ]; then
    # The capability matrix advertises what each command supports, and a `✓` there is
    # computed from a predicate — it says the command would accept the language, not that
    # anything ever ran it. The test run records what it actually drove, and the report below
    # fails when a claimed cell was never touched. Folded into the run that happens anyway,
    # because measuring it with a second full `cargo test` would double the wall clock.
    log="$(mktemp)"
    matrix="$(mktemp)"
    trap 'rm -f "$log" "$matrix"' EXIT

    run python3 tools/test-ci-shards.py
    run cargo fmt --all --check
    run cargo clippy --all-targets -- -D warnings
    run cargo test --test vfs_choke_point --test virtual_model_workspaces \
        --test resolution_measurements -- --test-threads 1
    run bash tools/check-kernels.sh
    if [ "$slice" != static ]; then
        ZIG_GLOBAL_CACHE_DIR="$zig_cache" FR_CAPABILITY_LOG="$log" run cargo test --all-targets \
            -- --test-threads "$FR_LEAN_JOBS"
    fi

    PYTHONPATH="$PWD/sdk/python/src:$PWD/tools${PYTHONPATH:+:$PYTHONPATH}" \
        run ty check sdk/python/src tools/representative-acceptance.py tools/native-intent-context.py tools/source-bodies-context.py tools/upstream-read-agent.py tools/upstream-rename-agent.py tools/upstream-react-agent.py tools/upstream-css-agent.py tools/upstream-tsx-body-agent.py tools/upstream-multibody-agent.py tools/upstream-cross-crate-bodies-agent.py tools/application-migration-agent.py tools/proof-authoring-agent.py tools/upstream-mermaid-agent.py tools/investigation-agent.py tools/investigation-cohort.py tools/outcome-acceptance.py tools/change-scope-acceptance.py tools/structural-change-acceptance.py tools/refinement-acceptance.py tools/agent_eval/investigation.py tools/agent_eval/investigation_run.py tools/agent_eval/investigation_prompt.py

    printf '\n\033[1m==> writing\033[0m\n'
    python3 tools/check-prose.py

    if [ "$slice" != static ]; then
        printf '\n\033[1m==> capability coverage\033[0m\n'
        # The integration suite uses CARGO_BIN_EXE_fr, so Cargo has already built this
        # exact default-feature binary. Asking `cargo run` for it again cost almost a
        # minute on a clean CI runner even though the executable was ready to run.
        test -x target/debug/fr
        target/debug/fr capabilities --json > "$matrix"
        python3 tools/capability-report.py "$matrix" "$log"
    fi
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
        --test self_translation \
        -- --test-threads "$FR_LEAN_JOBS"
    run cargo test --test lean_kernels -- --include-ignored \
        --test-threads "$FR_LEAN_JOBS"
    run bash tools/check-external-replays.sh
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
