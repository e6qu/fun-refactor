#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

(
    cd kernels
    lake build --wfail
    lake exe fr-edit-kernel >/dev/null
    lake exe fr-position-kernel >/dev/null
)
