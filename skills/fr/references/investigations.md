# Investigations

To explain why input reaches an output, link the calls and source lines. Recheck when those files change.

Call, reference and flow reports identify uses and their source locations in one revision.
Read their byte spans and Unicode line ranges separately from enclosing declaration handles.
Missing or multiple origins remain explicit.

`fr project dataflow HANDLE` follows the declared Python scalar subset through same-file helpers.
Use `--steps`, `--depth` and `--bytes` to bound work and output. External source, sink, propagation
and sanitizer contracts come from `--rules`; `--context` selects the sanitizer context. Check every
cutoff and omission. A report describes explicit value propagation with unchecked path feasibility.
It cannot establish application security. While loops use bounded fixed-point analysis. The default route cuts off recursion.
Add `--summaries` for symbolic summaries of recursive same-file functions, including return, sink and explicit raise effects.
Inspect `function_summaries` and `completion`; convergence does not prove runtime termination.
Aliases, exception handlers and unsupported effects remain incomplete boundaries.

Read `control_flow`, `origins` and `summaries` for graph nodes and context-specific evaluations.
Synthetic exits have absent origins. Edges use local IDs; derivations do not prove path feasibility.
`--inputs-only` fingerprints the defining file, rules, configuration, analyzer and budgets.
Python `fr_ir.flow.FlowCache` stores complete results in a verified Merkle store and revalidates
inputs before reuse. Keep its trusted root; stale inputs recompute and tampered objects refuse.
Reuse renews occurrence handles, never mutation reviews. Measure overhead first.

`fr project investigate --from PLAN` revalidates a local plan and returns updated states without
writing or executing actions. Use `--transition STEP:start`, `STEP:satisfy`, `STEP:block` or
`STEP:reset` for explicit transitions. Preserve hypotheses, open questions and required checks.
Evidence binds input digests and task criteria. Changed inputs invalidate dependent work. Use a
workspace dependency unless narrower dependency coverage is complete. References record reported
checks and proof kinds; they do not attest execution or authorize mutation.

Python `fr_ir.investigation.TaskPlan` saves typed plans in the verified Merkle store.
Keep it outside the analyzed workspace. Resume restored plans before relying on their evidence.

For Python body provenance, use `project semantic HANDLE --body --origins --origin-limit 40`.
Follow `origins.continuation` or select one `--origin-pointer`. Missing mappings remain explicit.
`SemanticOrigins` links those records to authoring pointers and same-revision flow occurrences.
Use `TaskStep.from_guide` to retain ready guide input. `TaskStep.checked` declares the input scope
for `investigation_checks.run_checks`; inspect `checks --toolchain` before executing its declarations.

`fr project identities` pages declaration content digests beside current handles. Pass a retained
report with `--from` to inspect matched, ambiguous and missing correspondence.
These candidates never renew a stale mutation review. Follow [Task](task.md) and [Workflow](workflow.md)
for checked changes, reversal and patch delivery.

For bounded explanations, follow the `analysis` action from a top-level Python exploration row.
`project flow-facts HANDLE --rules rules.json --context html --limit 2` returns fact headers;
each fact has a bounded explanation action. Follow returned continuations to keep all input options.
Read analysis, disclosure and mapping coverage separately. A full explanation covers one fact.
Missing witnesses establish model absence only with a complete catalogue and complete analysis.
Occurrences link by exact spans to semantic and authoring pointers; normalized nodes can lack origins.
These links are syntax evidence and do not prove source correspondence or feasible paths.
`fr_ir.flow_facts.FlowFacts` validates pages and persists them without refreshing stale actions.
