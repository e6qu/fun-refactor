#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

(
    cd kernels
    lake build --wfail
    lake exe fr-edit-kernel >/dev/null
    lake exe fr-position-kernel >/dev/null
    lake exe fr-history-kernel >/dev/null
    lake exe fr-history-kernel patch-modes >/dev/null
    lake exe fr-history-kernel patch-basis >/dev/null
    lake exe fr-history-kernel owner-executable >/dev/null
    lake exe fr-project-kernel >/dev/null
    lake exe fr-project-kernel line-ranges >/dev/null
    lake exe fr-project-kernel call-selection >/dev/null
    lake exe fr-project-kernel staging-transition >/dev/null
    lake exe fr-project-kernel commit-basis >/dev/null
    lake exe fr-project-kernel worktree-budget >/dev/null
    lake exe fr-project-kernel worktree-removal >/dev/null
    lake exe fr-project-kernel worktree-removal-resume >/dev/null
    lake exe fr-project-kernel worktree-branch-selection >/dev/null
    lake exe fr-project-kernel worktree-archive-compaction >/dev/null
    lake exe fr-project-kernel worktree-recovery >/dev/null
    lake exe fr-project-kernel patterns >/dev/null
    lake exe fr-project-kernel confidence >/dev/null
    lake exe fr-project-kernel membership >/dev/null
    lake exe fr-project-kernel closure >/dev/null
)
