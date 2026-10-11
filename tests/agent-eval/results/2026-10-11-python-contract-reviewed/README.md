# Reviewed scalar Python behavior contract

Collected in [run 38102873522](https://github.com/e6qu/fun-refactor/actions/runs/38102873522)
after correcting the static-import documentation link. The earlier capture remains unchanged.
The manifest retains source, binary and artifact identities plus the exact report hash.

All 72 cases and 27 audit tests pass. Twelve rule families have 36 independent programs and
renamed/relocated equivalents. Ten cases document model/runtime differences; twelve require
incomplete analysis. Witnesses retain exact AST coordinates and expected source-to-sink pairs.

Replay the audit with `python3 -B tools/test-python-contract.py` under the local resource guard.
Full builds and collection belong on GitHub runners.

This finite corpus does not close B.contract or establish full Python correctness, execution
attestation, exception safety or agent efficiency. Recursive summaries, static imports and the
full admitted call-binding inventory remain to be connected to the explicit rule inventory.
See the [behavior contract](../../../../docs/python-contract.md) for rules and limits.
