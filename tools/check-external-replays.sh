#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cohort=""
if [ "${1:-}" = --cohort ]; then
    cohort="${2:?--cohort requires a retained cohort name}"
    case "$cohort" in
        2026-09-08-regex|2026-09-08-coordinated|2026-09-09-structural-authoring|2026-09-11-context-v3|2026-09-11-workflow-v4|unknown-target) ;;
        *) echo "unknown external replay cohort: $cohort" >&2; exit 2 ;;
    esac
    shift 2
fi

export CARGO_HOME="$PWD/target/cargo-home"
mkdir -p "$CARGO_HOME"

scratch="$(mktemp -d "${TMPDIR:-/tmp}/fr-regex-deps.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
dependency_root="$scratch/workspace"
python3 tools/regex-workspace-check.py unpack "$dependency_root"

if [ "${1:-}" != "--offline" ]; then
    if [ -z "$cohort" ]; then
        cargo fetch --locked
    fi
    cargo fetch --manifest-path "$dependency_root/Cargo.toml" --locked
fi

if [ -n "$cohort" ]; then
    if [ "$cohort" = unknown-target ]; then
        python3 tools/external-eval.py tools/investigation-cohort.py --replay
    else
        python3 tools/external-eval.py tools/agent-eval.py replay "tests/agent-eval/results/$cohort"
    fi
    exit 0
fi

CARGO_NET_OFFLINE=true cargo test --test agent_acceptance \
    recorded_workspace_patches_pass_checks_oracles_and_exact_reversal \
    -- --ignored

CARGO_NET_OFFLINE=true cargo test --test agent_acceptance \
    unknown_target_agent_patches_replay_after_interrupted_investigations \
    -- --ignored --exact
