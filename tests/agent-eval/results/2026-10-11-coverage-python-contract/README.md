# Python behavior contract across helpers and files

[Hosted collection](https://github.com/e6qu/fun-refactor/actions/runs/38124983615) passed
252 original/renamed cases and 36 integrity tests. It also passed the focused SDK suites
for expressions, imports, binding, defaults and recursive summaries.

The 126 authored programs cover fourteen declared rule families. Runtime observations,
possible-flow answers and documented differences remain separate. Imported programs run
in isolated Python processes. The manifest identifies the source commit, binary and exact artifact.

The [failed initial comparison](../2026-10-11-coverage-python-contract-diagnostic/README.md)
is preserved separately. Its ten checker discrepancies were corrected without rewriting
the original report or weakening incomplete results into absence claims.

B.contract remains open for discovery-depth limits, invalid syntax and CLI refusals,
import ambiguity and shadowing, and module/binding-chain limits. This finite result does
not establish whole-language correctness, executable paths or agent-efficiency gains.
