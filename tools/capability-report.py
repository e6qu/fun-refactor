"""Compare the capability matrix against the cells a run actually drove.

`fr capabilities` computes the matrix from each refactoring's own predicate, so a `✓` means
"this command would accept this language" and not "this has ever worked". The second claim
is what this measures: every capability appends the language it ran against to the log named
by FR_CAPABILITY_LOG, and this reads that against what the matrix advertises.

Usage: capability-report.py <matrix.json> <capability.log>
Exits non-zero, naming the gaps, when a claimed cell was never driven.
"""

import collections
import json
import sys


def main(matrix_path: str, log_path: str) -> int:
    with open(matrix_path) as f:
        matrix = json.load(f)
    claimed = {
        (row["capability"], language)
        for row in matrix
        for language, support in row["languages"]
        if support["support"] == "yes"
    }

    seen = set()
    with open(log_path) as f:
        for line in f:
            cell = tuple(line.rstrip("\n").split("\t"))
            if len(cell) == 2:
                seen.add(cell)

    covered = claimed & seen
    missing = sorted(claimed - seen)
    print(f"capability coverage: {len(covered)}/{len(claimed)} "
          f"({100 * len(covered) // len(claimed)}%)")

    if not missing:
        return 0

    print(f"\n{len(missing)} claimed cell(s) the run never drove:")
    for capability, count in collections.Counter(c for c, _ in missing).most_common():
        languages = " ".join(sorted(l for c, l in missing if c == capability))
        print(f"  {capability:24} {count:2}  {languages}")
    print("\nA cell nothing drives is a claim nothing checks. Add a driver to "
          "tests/capability_claims.rs, or change the cell.")
    return 1


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1], sys.argv[2]))
