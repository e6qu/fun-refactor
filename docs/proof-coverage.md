# Write, check and deliver proofs

The [roadmap](../PLAN.md#pr-3-complete-proofs-and-implementation-connections) keeps proof authoring and source connections in one work package.
This guide separates current product checks from historical agent outcomes.

| Workflow | Checks and boundary |
| --- | --- |
| Select a function and state a property | [Typed language cases](../tests/formal_kernel.rs) cover Rust, Python, Go, Java, TypeScript, TSX, Zig and Lean |
| Inspect a goal and select library facts | [Proof state](../tests/proof_state.rs) and [fact selection](../src/spec/library_facts.rs) check bounded goals and operation-based facts |
| Write, repair, check and save | [Proof CLI](../tests/proof_cli.rs) covers drafts, rejected saves and one-call delivery |
| Preserve the claim and existing work | [Formal kernels](../tests/formal_kernel.rs) check protected claims, regeneration and handwritten proof regions |
| Detect stale inputs | [Proof CLI](../tests/proof_cli.rs) checks receipts; [scalar connections](../tests/rust_scalar_model.rs) check source and dependency changes |
| Connect integer behavior to a model | [Rust scalar connections](agent-formalization.md#rust-scalar-connections) add quantified arithmetic obligations for page counting and window bounds |
| Deliver a patch | [Supplied-proof delivery](../tests/agent-eval/proof-delivery/README.md) separates delivery mechanics from proof discovery |

The [focused hosted run](https://github.com/e6qu/fun-refactor/actions/runs/44271887240) passed all 31 tests:
8 scalar connections, 8 proof CLI checks, 2 proof-state checks and 13 formal-kernel checks.
The scalar suite proves page counting and window bounds over their complete input ranges.
It also compares Rust and Lean on 174 boundary cases, with Rust compiled at two optimization levels.
It checks rejected saves, exact undo/redo, fresh-workspace patch replay and stale proof evidence.
The [M3 verification run](https://github.com/e6qu/fun-refactor/actions/runs/44272707747) rechecked all 36 original outcomes.
All 39 current-code, source-execution, client and control jobs passed.
Its completion check still fails because GLM and Kimi lack accepted page-count proofs within the original 150 seconds.
Neither workflow makes live model calls or repeats unsuccessful study slots.

## Agent outcomes remain separate

| Evidence | Limit | Result and remaining obligation |
| --- | --- | --- |
| [Original M3 comparison](../tests/agent-eval/proof-writing/local-results/README.md) | 150 seconds per complete attempt | 28 of 36 accepted deliveries; GLM and Kimi lack accepted page-count proofs using fr |
| [Later page-count study](../tests/agent-eval/proof-page-count/local-results/summary.json) | 300 seconds per complete attempt | Four accepted proofs under the separate protocol; the original M3 gate stays open |
| Product-order fact selection from #619 | No new live comparison | Hosted checks cover the feature; its effect on agent delivery remains unmeasured |

A passing implementation check cannot replace a missing agent outcome.
Any new live comparison needs its own declared question, controls, limits and decision rule before calls.
The full product roadmap remains open until its feature and qualification gates pass.
