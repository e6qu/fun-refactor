#!/usr/bin/env bash
#
# Which cells of the capability matrix does the test suite actually exercise?
#
# `fr capabilities` computes the matrix from each refactoring's own predicate, so a `✓`
# means "this command would accept this language" and not "this has ever worked". This
# measures the second claim: every capability records the language it ran against when
# FR_CAPABILITY_LOG is set, the suite runs with it set, and the result is compared against
# what the matrix advertises.
#
# `tests/capability_claims.rs` drives all of them, so a healthy run reports 100%. A number
# below that means a capability was added without a driver there, and the report names it.

set -euo pipefail

cd "$(dirname "$0")/.."

log="$(mktemp)"
matrix="$(mktemp)"
report="$(mktemp)"
trap 'rm -f "$log" "$matrix" "$report"' EXIT

cat > "$report" <<'PYTHON'
import collections
import json
import sys

with open(sys.argv[1]) as f:
    matrix = json.load(f)
claimed = {
    (row["capability"], language)
    for row in matrix
    for language, support in row["languages"]
    if support["support"] == "yes"
}

seen = set()
with open(sys.argv[2]) as f:
    for line in f:
        cell = tuple(line.rstrip("\n").split("\t"))
        if len(cell) == 2:
            seen.add(cell)

covered = claimed & seen
missing = sorted(claimed - seen)
print(f"exercised: {len(covered)}/{len(claimed)} ({100 * len(covered) // len(claimed)}%)")
if missing:
    print(f"\nnot exercised ({len(missing)}):")
    for capability, count in collections.Counter(c for c, _ in missing).most_common():
        languages = " ".join(sorted(l for c, l in missing if c == capability))
        print(f"  {capability:24} {count:2}  {languages}")
    sys.exit(1)
print("\nEvery cell the matrix claims was driven by the suite.")
PYTHON

printf '\033[1m==> running the suite with capability recording on\033[0m\n'
FR_CAPABILITY_LOG="$log" cargo test --all-targets >/dev/null

cargo run --quiet --features cli --bin fr -- capabilities --json > "$matrix"
python3 "$report" "$matrix" "$log"
