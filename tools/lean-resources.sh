#!/usr/bin/env bash

# Keep Lean validation responsive on developer machines. The same bound limits
# Rust test cases that launch Lean and the worker pool inside each Lean process.
fr_configure_lean_resources() {
    FR_LEAN_JOBS="${FR_LEAN_JOBS:-2}"
    LEAN_NUM_THREADS="${LEAN_NUM_THREADS:-$FR_LEAN_JOBS}"

    case "$FR_LEAN_JOBS" in
        ''|0|*[!0-9]*)
            echo "FR_LEAN_JOBS must be a positive integer" >&2
            return 2
            ;;
    esac
    case "$LEAN_NUM_THREADS" in
        ''|0|*[!0-9]*)
            echo "LEAN_NUM_THREADS must be a positive integer" >&2
            return 2
            ;;
    esac

    export FR_LEAN_JOBS LEAN_NUM_THREADS
}
