# Preserve the first dictionary corpus failure

The [first collection](https://github.com/e6qu/fun-refactor/actions/runs/46024820276)
passed 2,523 SDK checks but rejected four corpus queries. Two new examples, each
with original and moved names, selected the builtin model without enabling
import analysis. The CLI correctly refused the invalid arguments. This was a
fixture configuration error, not accepted analysis evidence.

`artifact.zip` preserves the original GitHub artifact bytes. `provenance.json`
records its artifact ID, source revision and GitHub digest. The retention tests
check that digest and all four original refusals. The corrected collection adds
the required import-analysis selection; it does not alter this report.

The [source-snapshot retention job](https://github.com/e6qu/fun-refactor/actions/runs/46027146720)
stopped at its existing extraction limit. The complete bound native source tree
exceeds 64 MiB, including the generated Lean parser. That limit remains unchanged.
This record retains the report, embedded case sources and source-input hashes.
It does not claim that the complete historical validator was replayed.
