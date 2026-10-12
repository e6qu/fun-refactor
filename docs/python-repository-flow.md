# Python repository flow baseline

The first cohort uses pinned Toolz, funcy and pydash snapshots with the same native analyzer,
frozen at `aad6dc34cb48ff27c01ad964a425c94167053347`. The
[selection record](../tests/agent-eval/python-repository-flow/cohort.json) declares revisions,
source/sink rules, resource budgets and the first held-out opening. These are three independently
maintained utility libraries, not a representative sample of Python applications.

Each authored adapter imports the upstream identity helper through its real package. One entry
passes the selected value through two helper calls to a sink. Another calls the helper, overwrites
the value and sends zero through the helper to the same sink. Independent AST inspection checks
the actual runtime implementation; isolated CPython executions use source values 17 and 31.
The alternate adapter renames its entry and import alias and moves into another package without
changing upstream code or analysis rules. Unknown-call, one-step-budget and invalid-byte-budget
controls run in both forms.

| Result | Count |
| --- | ---: |
| Independently maintained repositories | 3 |
| Original and renamed/relocated cells | 30 |
| Runtime executions per collection | 60 |
| Complete native analyses | 0 |
| Incomplete native analyses | 24 |
| Deliberate CLI refusals | 6 |

Runtime execution agrees with independent inspection. Renaming and relocation preserve the
reported conclusions. The analyzer remains incomplete even for the positive and overwritten
entries in these packages. An empty witness list therefore does not establish no path.
Unknown calls and exhausted budgets stay incomplete; invalid response budgets refuse analysis.
**B.repository-flow remains open**, and roadmap status remains 9/18 demonstrated acceptance items.
These results establish neither general library support nor agent efficiency.

Retained cutoffs include unchecked module effects, cyclic initialization, unsupported import
forms and unresolved standard-library members. Pydash also hits the import-lookup budget and
an unsupported variadic parameter contract. These boundaries need general solutions; discarding
imports or treating an unresolved call as harmless would not establish a complete answer.

Pydash source first opened after coverage passed on the same collector revision. Its first
inspection failed because the harness treated type-only overload declarations as runtime
implementations. General inspector and runtime-dependency corrections followed; the native
analyzer never changed. The record preserves that first opening. Pydash cannot serve as a fresh
held-out project for a later analyzer implementation.

Collection also exposed two harness failures: an invalid requested call depth and an evidence
directory restored by the compiler cache. Preflight validates public CLI budgets, and collection
now writes to a unique `RUNNER_TEMP` directory. The exact failed report and snapshots remain
historical evidence; a pre-download failure records its run and reason. Every timeout stays unchanged.

The hosted retainer verifies archive digests, producer attempts and source bindings, replays
runtime observations, and checks corruptions after rehashing the modified receipts. Snapshots
retain full source inventories and upstream licenses. The
[fixture guide](../tests/agent-eval/python-repository-flow/README.md) describes the collector.

Next, address the captured import and summary boundaries through general language rules while
preserving refusal of unknown effects and exhausted budgets. Broaden project-derived examples
beyond the authored identity adapters. Select an unopened replacement held-out project before
changing the analyzer, freeze that implementation before evaluation, and keep this baseline intact.
