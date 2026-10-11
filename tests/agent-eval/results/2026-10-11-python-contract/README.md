# Scalar Python behavior contract

Collected in [run 38099566700](https://github.com/e6qu/fun-refactor/actions/runs/38099566700).
The manifest retains the source commit, binary identity, artifact identity and exact report hash.

Twelve rule families are checked through 36 independently written programs and their renamed,
relocated equivalents: 72 cases in total. CPython outcomes and possible-flow answers remain
separate. Ten cases document model/runtime differences; twelve require incomplete analysis.
Every witness is checked against exact AST coordinates and the expected source-to-sink pair.

The 27 corruption controls reject missing examples, altered runtime outcomes, incorrect source
locations, changed caller rules and overstated completeness or path claims.
Replay them with `python3 -B tools/test-python-contract.py` under the local resource guard.
Full builds and report collection belong on GitHub runners.

This is a finite first coverage pass, not complete Python correctness, execution attestation,
path feasibility, exception safety or an agent-efficiency comparison. B.contract remains open
for recursive summaries, static imports and the full admitted call-binding inventory.
See the [behavior contract](../../../../docs/python-contract.md) for rules and limits.
