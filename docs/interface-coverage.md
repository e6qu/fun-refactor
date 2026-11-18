# Public interfaces and recovery

Use the CLI or Python SDK for native project checks, proofs and Git operations.
Use the browser to inspect loaded source, review refactorings and export patches.
A browser patch needs native project checks before delivery.

| Workflow | CLI and Python SDK | WASM and playground | Checks |
| --- | --- | --- | --- |
| Code and call graphs | `fr graph --json`, DOT export and project relationship queries; Python uses `FrClient.call("graph")` and `project(...)` | `graph().details` exports nodes, call sites, source digests and uncertainty; the playground displays them | [Graph exports](../tests/graph_export.rs), [browser API](../tests/wasm_native.rs) |
| Local call graph | Incoming/outgoing project calls and native call trees | `graph_around(...)` carries depth truncation, unresolved calls and workspace coverage alongside the drawing | [Browser review and graph checks](../web/test/review.mjs) |
| Flow and saved findings | Native analysis and Python `FlowCache`, `TaskPlan` and `InvestigationSession` retain dependencies and reject stale results | Source flow queries rebuild from the loaded workspace; no native dependency cache or compiler | [Cache versus fresh analysis](../sdk/python/tests/test_flow.py), [saved investigations](../sdk/python/tests/test_investigation.py) |
| Reviewed edits | Native previews and checked writes; Python `review` then `execute` | `preview` then `apply_review` checks the request, source, history and proposed output again | [Task changes](../sdk/python/tests/test_ir.py), [browser restart and stale reviews](../tests/wasm_native.rs) |
| Failed project checks | Inspect the applied transaction, repair the prerequisite or source, then resume the reviewed workflow | Export the patch and run native checks in the complete checkout | [Recovery procedure](workflow-recovery.md), [delivery receipts](../sdk/python/tests/test_investigation_delivery.py) |
| Interrupted writes | Native journal recovery preserves conflicting edits and reports pending work | Private review copies protect live state; storage retains a complete previous or next checkpoint | [Host recovery](../tests/host_recovery.rs), [browser storage](../web/test/session.mjs) |
| History and delivery | Native undo/redo, patches, staging and worktree recovery; Python preserves structured failure reports | Bounded undo/redo, compaction, session restoration and Git text patches | [History](../tests/history_cli.rs), [Git staging](../tests/git_stage_history_cli.rs), [patch replay](../web/test/patch.mjs) |
| Proofs | CLI and Python call the native Lean workflow, including checked save and patch delivery | No Lean runtime in the browser | [Proof coverage](proof-coverage.md) |
| Agent and integration use | The shipped fr skill uses the same CLI; Python object stores verify saved content on restoration | JavaScript bindings expose browser reports and session envelopes | [Runtime SDK](agent-runtime-sdk.md), [context storage](agent-context-workspace.md), [skill](../skills/fr/SKILL.md) |

## Review and handoff

The browser review uses the existing refactoring planners and memory-history implementation.
Its private workspace allows syntax checks and indexing to finish before replacing live state.
It also checks that the result fits a restorable session. It does not claim native acceptance checks ran.
Read [browser history](browser-history.md) for request names, size limits and restart behavior.

The hosted [interface workflow](../.github/workflows/recovery-interfaces.yml) exercises native Rust
and the shipped WASM module. It checks read-only previews, altered reviews, source and history changes,
compaction, restart, repeated apply, undo/redo and refused operations.
It applies a reviewed browser patch in a fresh native checkout, runs the resulting Python program,
and compares the native graph through the Python SDK with the browser graph.
A conflicting receiver must refuse the patch.

[Run 44312841042](https://github.com/e6qu/fun-refactor/actions/runs/44312841042) passed 24 native tests,
88 browser API cases, reviewed translation, patch replay, checkpoint recovery and the refactoring sweep.

The full CI gate also runs the existing native interruption matrix, SDK recovery and dependency-reuse tests,
installable artifact checks and examples. Successful fixture results do not establish arbitrary recovery
after power loss or semantic completeness for every language.
The [analysis](analysis-coverage.md), [refactoring](refactoring-coverage.md) and [proof](proof-coverage.md)
guides retain those boundaries and remaining obligations.

## Reading incomplete results

The graph describes indexed source. Unresolved calls, dispatch candidates, omitted files and syntax gaps
remain visible. External callers and runtime dispatch stay unknown.
The playground limits long lists to 50 displayed entries and shows their full counts.
The exported report retains every entry.

A source digest binds bytes; it does not prove a graph complete.
A saved answer needs its dependency checks before reuse.
A review checks proposed edits; native check receipts describe checks that actually ran.
Browser and native history have different storage contracts, but neither may silently overwrite a conflict.
