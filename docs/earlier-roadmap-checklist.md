# Earlier qualification checklist

These checks belong to the roadmap used before the focus on smaller agents, refactoring, code changes and Lean proof writing.
They remain auditable. An unfinished item stays unfinished; moving it here does not complete it.
The [current roadmap](../PLAN.md) sets priorities. The [retained-results report](roadmap-status.md) checks the original acceptance conditions and counts.

### A. Find relevant code and explain the evidence

- [x] Report exact declaration, reference and call locations.
- [x] Link analyzed expressions to source and expose missing or ambiguous locations.
- [x] Explain findings with their inputs, assumptions and omitted information.
- [x] Retain compiler inputs, versions and disagreements with fr.
- [x] Save requested outcomes, checked facts and remaining questions.
- [x] Run the initial bug and feature comparison without supplying edit locations.

### B. Explain how values move through code

- [ ] Specify reusable language rules connecting source, analysis and edits, with independent examples and explicit limits.
- [x] Represent supported assignments, branches, loops, returns and explicit exceptions.
- [x] Describe supported argument passing, returns and actions, including recursive calls.
- [x] Trace user-declared inputs, outputs and cleaning functions with source locations.
- [x] Distinguish possible calls and value paths from statements established by a proof.

### C. Resume work and reuse correct answers

- [x] Distinguish current code references, saved-content fingerprints and candidate matches after edits.
- [x] Track source, import searches, settings and analyzer inputs behind answers.
- [x] Verify saved answers and analyze again when reuse cannot be established.
- [x] Resume saved tasks after discarding outdated findings.
- [x] Invalidate check and proof results after relevant input changes.
- [x] Measure first use, reuse and rechecking after edits on three independent projects.

### D. Make checked changes and useful proofs

- [ ] Discover all relevant callers, contracts, configuration and tests for the declared change.
- [x] Add general language rules needed by observed structural changes.
- [x] Translate a function under explicit arithmetic, error and evaluation-order rules.
- [ ] Connect proofs to an existing implementation and to a new implementation developed with its specification.
- [ ] Retain proof inputs for edits and translations, rejecting outdated claims.
- [x] Check small models governing edits, task transitions and interrupted-write recovery.

The [acceptance catalog](../tests/agent-eval/roadmap.json) retains the original requirements and evidence references.
The [previous roadmap](https://github.com/e6qu/fun-refactor/blob/c923d3cde7/PLAN.md) records its former work sequence.
