#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

export CARGO_HOME="$PWD/target/cargo-home"
mkdir -p "$CARGO_HOME"

scratch="$(mktemp -d "${TMPDIR:-/tmp}/fr-regex-deps.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
dependency_root="$scratch/workspace"
python3 tools/regex-workspace-check.py unpack "$dependency_root"

if [ "${1:-}" != "--offline" ]; then
    cargo fetch --locked
    cargo fetch --manifest-path "$dependency_root/Cargo.toml" --locked
fi

CARGO_NET_OFFLINE=true cargo test --test agent_acceptance \
    recorded_workspace_patches_pass_checks_oracles_and_exact_reversal \
    -- --ignored
